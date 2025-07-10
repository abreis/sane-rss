use crate::config::FeedConfig;
use rss::{Channel, Item};
use std::time::Duration;
use tracing::{error, info, warn};

pub struct FeedFetcher {
    client: reqwest::Client,
}

impl FeedFetcher {
    pub fn new() -> Self {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(30))
            .build()
            .expect("Failed to create HTTP client");

        Self { client }
    }

    pub async fn fetch_feed(&self, feed_name: &str, config: &FeedConfig) -> Option<Channel> {
        info!("Fetching feed: {} from {}", feed_name, config.url);

        match self.client.get(&config.url).send().await {
            Ok(response) => match response.text().await {
                Ok(content) => match Channel::read_from(content.as_bytes()) {
                    Ok(channel) => {
                        info!(
                            "Successfully fetched feed: {} with {} items",
                            feed_name,
                            channel.items().len()
                        );
                        Some(channel)
                    }
                    Err(e) => {
                        error!("Failed to parse RSS feed {}: {}", feed_name, e);
                        None
                    }
                },
                Err(e) => {
                    error!("Failed to read response from {}: {}", config.url, e);
                    None
                }
            },
            Err(e) => {
                warn!("Failed to fetch feed {}: {}", feed_name, e);
                None
            }
        }
    }
}

pub fn item_to_guid(item: &Item) -> String {
    if let Some(guid) = item.guid() {
        guid.value().to_string()
    } else if let Some(link) = item.link() {
        link.to_string()
    } else if let Some(title) = item.title() {
        format!("{}-{}", title, item.pub_date().unwrap_or("no-date"))
    } else {
        format!("unknown-{}", chrono::Utc::now().timestamp())
    }
}