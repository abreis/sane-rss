use crate::feed::item_to_guid;
use rss::Item;
use std::collections::{HashMap, HashSet, VecDeque};
use std::path::PathBuf;
use tokio::sync::RwLock;
use tracing::info;

// TODO
// #[derive(Clone)]
pub struct Feed {
    pub title: Option<String>,
    pub description: Option<String>,
    pub items: VecDeque<Item>,
    pub favicon: Option<Vec<u8>>,
}

pub struct FeedStorage {
    pub feeds: RwLock<HashMap<String, Feed>>,
    pub seen_guids: RwLock<HashMap<String, HashSet<String>>>,
}

impl FeedStorage {
    pub fn new() -> Self {
        Self {
            feeds: RwLock::new(HashMap::new()),
            seen_guids: RwLock::new(HashMap::new()),
        }
    }

    pub async fn store_items(
        &self,
        feed_name: String,
        items: Vec<Item>,
        title: Option<String>,
        description: Option<String>,
        max_items: usize,
    ) {
        use std::collections::hash_map::Entry;

        let mut feeds = self.feeds.write().await;

        info!("Storing {} items for feed {}", items.len(), feed_name);

        match feeds.entry(feed_name.clone()) {
            Entry::Occupied(mut entry) => {
                let feed = entry.get_mut();
                if title.is_some() {
                    feed.title = title;
                }
                if description.is_some() {
                    feed.description = description;
                }

                // Add new items using push_back
                for item in &items {
                    feed.items.push_back(item.clone());

                    // Remove oldest items if we exceed the limit
                    while feed.items.len() > max_items {
                        feed.items.pop_front();
                    }
                }
            }
            Entry::Vacant(entry) => {
                let mut deque = VecDeque::with_capacity(max_items);
                for item in &items {
                    deque.push_back(item.clone());

                    // Ensure we don't exceed the limit even on initial insert
                    while deque.len() > max_items {
                        deque.pop_front();
                    }
                }

                entry.insert(Feed {
                    title,
                    description,
                    items: deque,
                    favicon: None,
                });
            }
        }

        // Drop the write lock before calling mark_item_as_seen
        drop(feeds);

        // Mark all items as seen using the dedicated method
        for item in &items {
            self.mark_item_as_seen(&feed_name, item_to_guid(item)).await;
        }
    }

    pub async fn is_new_item(&self, feed_name: &str, guid: &str) -> bool {
        let seen = self.seen_guids.read().await;
        if let Some(feed_guids) = seen.get(feed_name) {
            !feed_guids.contains(guid)
        } else {
            true
        }
    }

    pub async fn store_favicon(&self, feed_name: &str, favicon_data: Vec<u8>) {
        let mut feeds = self.feeds.write().await;
        if let Some(feed) = feeds.get_mut(feed_name) {
            feed.favicon = Some(favicon_data);
            info!("Stored favicon for feed {}", feed_name);
        }
    }

    pub async fn get_favicon(&self, feed_name: &str) -> Option<Vec<u8>> {
        let feeds = self.feeds.read().await;
        feeds.get(feed_name).and_then(|feed| feed.favicon.clone())
    }

    pub async fn mark_item_as_seen(&self, feed_name: &str, guid: String) {
        use std::collections::hash_map::Entry;

        let mut seen = self.seen_guids.write().await;

        match seen.entry(feed_name.to_string()) {
            Entry::Occupied(mut entry) => {
                entry.get_mut().insert(guid);
            }
            Entry::Vacant(entry) => {
                let mut guids = HashSet::new();
                guids.insert(guid);
                entry.insert(guids);
            }
        }
    }
}

impl FeedStorage {
    pub async fn save_seen_guids(&self, path: &PathBuf) -> std::io::Result<()> {
        let seen = self.seen_guids.read().await;
        let json = serde_json::to_string(&*seen)?;
        std::fs::write(path, json)?;
        Ok(())
    }

    pub async fn load_seen_guids(&self, path: &PathBuf) -> std::io::Result<()> {
        use std::io::ErrorKind;

        let json = match std::fs::read_to_string(path) {
            Ok(content) => {
                if content.is_empty() {
                    return Ok(());
                }
                content
            }
            Err(e) if e.kind() == ErrorKind::NotFound => {
                return Ok(());
            }
            Err(e) => return Err(e),
        };

        let loaded: HashMap<String, HashSet<String>> = serde_json::from_str(&json)?;
        let mut seen = self.seen_guids.write().await;
        *seen = loaded;
        Ok(())
    }
}
