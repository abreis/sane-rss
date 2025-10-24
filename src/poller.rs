//! Periodic feed poller.

use anyhow::Context;

use crate::{
    config::{Config, FeedConfig},
    filter::LLMFilter,
    storage::FeedStorage,
};
use std::time::Duration;

pub struct FeedPoller {
    config: Config,
    storage: FeedStorage,
    filter: LLMFilter,
}

impl FeedPoller {
    pub fn new(config: Config, storage: FeedStorage, filter: LLMFilter) -> Self {
        Self {
            config,
            storage,
            filter,
        }
    }

    // Launches the periodic feed poller.
    pub async fn launch(self) {
        let polling_interval = Duration::from_secs(self.config.polling_interval_seconds);
        tracing::info!(
            "Starting feed poller with interval of {} seconds",
            polling_interval.as_secs()
        );

        // An async periodic interval.
        let mut interval = tokio::time::interval(polling_interval);
        // The first tick completes immediately.
        // To avoid immediate polling, uncomment to skip it.
        // interval.tick().await;

        loop {
            interval.tick().await;

            tracing::debug!("[feed_poller]: Polling all feeds");
            self.poll_feeds().await;
        }
    }

    async fn poll_feeds(&self) {
        // Go through every feed.
        'feed_loop: for (feed_name, feed_config) in &self.config.feeds {
            tracing::debug!("Retrieving feed {feed_name}");

            // Retrieve the feed. Don't stop if it fails.
            let channel = match retrieve_feed(feed_config).await {
                Ok(channel) => channel,
                Err(error) => {
                    tracing::warn!("Retrieval error: {error}");
                    continue 'feed_loop;
                }
            };
            tracing::debug!(
                "Retrieved {} items from feed {feed_name}",
                channel.items().len()
            );

            let mut storage = self.storage.write().await;

            // See if our storage knows this channel.
            storage.add_channel(feed_name, channel.title(), channel.description());

            // Strip any items we've already seen from the list.
            let mut items: Vec<rss::Item> = channel.items;
            items.retain(|item| !storage.is_known(&feed_name, item));

            // Record remaining items as seen.
            tracing::debug!("Recording {} items retained as new", items.len());
            for unknown_item in &items {
                storage.record_as_known(feed_name, unknown_item);
            }

            // Don't hold the lock through the (slow) LLM calls.
            drop(storage);

            // Send each item to the LLM for filtering.
            let mut accepted_items = Vec::new();
            for item in items {
                if self.filter.accepts(feed_name, &item).await {
                    accepted_items.push(item);
                }
            }

            // If accepted, place it in our storage.
            tracing::debug!("Filters accepted {} items, storing", accepted_items.len());
            let mut storage = self.storage.write().await;
            for item in accepted_items {
                storage.store_filtered_item(&feed_name, item);
            }
        }

        // At the end of each cycle, write our known items to disk.
        if let Err(error) = self.storage.write().await.save_known_items() {
            tracing::warn!("Failed to write known items to file: {}", error);
        }
    }
}

async fn retrieve_feed(config: &FeedConfig) -> anyhow::Result<rss::Channel> {
    tracing::debug!("Retrieving feed from {}", config.url);

    let response = reqwest::get(&config.url)
        .await
        .context("Failed to HTTP GET feed")?;

    let content = response.text().await.context("No text in response")?;

    let channel =
        rss::Channel::read_from(content.as_bytes()).context("Failed to parse RSS feed")?;

    Ok(channel)
}
