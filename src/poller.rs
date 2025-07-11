use crate::{
    config::Config,
    feed::{item_to_guid, FeedFetcher},
    llm::LlmFilter,
    storage::FeedStorage,
};
use futures::stream::{self, StreamExt};
use std::{sync::Arc, time::Duration};
use tokio::time;
use tracing::{debug, info};

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

        // The first tick completes immediately, skip it to avoid immediate polling.
        interval.tick().await;

        loop {
            interval.tick().await;
            debug!("Starting feed polling cycle");
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
                            self.storage.store_items(feed_name.clone(), vec![item], None, None, self.config.max_items_per_feed()).await;
                            info!("Added filtered item to feed {}", feed_name);
                        } else {
                            info!("Item rejected by filter");
                        }
                    }
                }
            }
        }
    }

    pub async fn initial_fetch(&self) -> Result<(), String> {
        info!("Performing initial feed retrieval");

        let feeds: Vec<_> = self.config.feeds.iter().collect();
        let max_items = self.config.max_items_per_feed();

        let fetch_tasks = feeds.into_iter().map(|(feed_name, feed_config)| {
            let fetcher = &self.fetcher;
            let storage = &self.storage;

            async move {
                match fetcher.fetch_feed(feed_name, feed_config).await {
                    Some(channel) => {
                        let title = channel.title().to_string();
                        let description = channel.description().to_string();
                        let items = channel.into_items();

                        storage
                            .store_items(
                                feed_name.clone(),
                                items,
                                Some(title),
                                Some(description),
                                max_items,
                            )
                            .await;
                        Ok(())
                    }
                    None => Err(format!("Failed to fetch feed: {}", feed_name)),
                }
            }
        });

        let results: Vec<Result<(), String>> = stream::iter(fetch_tasks)
            .buffer_unordered(5)
            .collect()
            .await;

        for result in results {
            result?;
        }

        Ok(())
    }
}
