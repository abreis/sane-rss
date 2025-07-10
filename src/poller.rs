use crate::{
    config::Config,
    feed::{item_to_guid, FeedFetcher},
    llm::LlmFilter,
    storage::FeedStorage,
};
use std::{collections::HashSet, sync::Arc, time::Duration};
use tokio::time;
use tracing::info;

pub struct FeedPoller {
    config: Arc<Config>,
    storage: FeedStorage,
    fetcher: FeedFetcher,
    filter: Arc<LlmFilter>,
}

impl FeedPoller {
    pub fn new(config: Arc<Config>, storage: FeedStorage, filter: Arc<LlmFilter>) -> Self {
        Self {
            config,
            storage,
            fetcher: FeedFetcher::new(),
            filter,
        }
    }

    pub async fn start(self) {
        let interval_duration = Duration::from_secs(self.config.polling_interval_seconds());
        let mut interval = time::interval(interval_duration);

        info!(
            "Starting feed poller with interval of {} seconds",
            self.config.polling_interval_seconds()
        );

        // Skip the first tick to avoid immediate polling
        interval.tick().await;

        loop {
            interval.tick().await;
            info!("Starting feed polling cycle");
            self.poll_all_feeds().await;
        }
    }

    async fn poll_all_feeds(&self) {
        for (feed_name, feed_config) in &self.config.feeds {
            if let Some(channel) = self.fetcher.fetch_feed(feed_name, feed_config).await {
                let items = channel.into_items();
                for item in items {
                    let guid = item_to_guid(&item);

                    if self.storage.is_new_item(feed_name, &guid).await {
                        info!(
                            "Found new item in feed {}: {}",
                            feed_name,
                            item.title().unwrap_or("No title")
                        );

                        let should_accept = self
                            .filter
                            .should_accept_item(
                                &item,
                                &self.config.global_filters,
                                &feed_config.filters,
                            )
                            .await;

                        if should_accept {
                            self.storage.add_filtered_item(feed_name, item, guid).await;
                        } else {
                            info!("Item rejected by filter");
                        }
                    }
                }
            }
        }
    }

    pub async fn initial_fetch(&self) {
        info!("Performing initial feed retrieval");

        for (feed_name, feed_config) in &self.config.feeds {
            if let Some(channel) = self.fetcher.fetch_feed(feed_name, feed_config).await {
                let items = channel.into_items();
                let mut guids = HashSet::new();

                for item in &items {
                    guids.insert(item_to_guid(item));
                }

                self.storage
                    .store_initial_items(feed_name.clone(), items, guids)
                    .await;
            }
        }
    }
}
