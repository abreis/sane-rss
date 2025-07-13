use crate::config::FeedConfig;
use rss::{Channel, Item};
use std::time::Duration;
use tracing::{debug, error, warn};
use url::Url;

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
        debug!("Fetching feed: {} from {}", feed_name, config.url);

        match self.client.get(&config.url).send().await {
            Ok(response) => match response.text().await {
                Ok(content) => match Channel::read_from(content.as_bytes()) {
                    Ok(channel) => {
                        debug!(
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

    pub async fn fetch_favicon(&self, feed_url: &str) -> Option<Vec<u8>> {
        // Parse the feed URL to get the base domain
        let url = match Url::parse(feed_url) {
            Ok(url) => url,
            Err(e) => {
                warn!("Failed to parse feed URL {}: {}", feed_url, e);
                return None;
            }
        };

        // Construct favicon URL at the root of the domain (scheme + authority)
        let root_url = format!("{}://{}", url.scheme(), url.authority());
        let favicon_url = match Url::parse(&root_url).and_then(|u| u.join("/favicon.ico")) {
            Ok(url) => url,
            Err(e) => {
                warn!("Failed to construct favicon URL: {}", e);
                return None;
            }
        };

        debug!("Fetching favicon from: {}", favicon_url);

        // Try to fetch the favicon
        match self.client.get(favicon_url.as_str()).send().await {
            Ok(response) => {
                if response.status().is_success() {
                    match response.bytes().await {
                        Ok(bytes) => {
                            debug!("Successfully fetched favicon ({} bytes)", bytes.len());
                            Some(bytes.to_vec())
                        }
                        Err(e) => {
                            debug!("Failed to read favicon response: {}", e);
                            None
                        }
                    }
                } else {
                    debug!("Favicon request returned status: {}", response.status());
                    None
                }
            }
            Err(e) => {
                debug!("Failed to fetch favicon: {}", e);
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
