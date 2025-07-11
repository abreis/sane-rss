use crate::feed::item_to_guid;
use rss::Item;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::info;

// TODO
// #[derive(Clone)]
pub struct Feed {
    pub title: Option<String>,
    pub description: Option<String>,
    pub items: Vec<Item>,
}

#[derive(Clone)]
pub struct FeedStorage {
    pub feeds: Arc<RwLock<HashMap<String, Feed>>>,
    pub seen_guids: Arc<RwLock<HashMap<String, HashSet<String>>>>,
}

impl FeedStorage {
    pub fn new() -> Self {
        Self {
            feeds: Arc::new(RwLock::new(HashMap::new())),
            seen_guids: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    pub async fn store_items(
        &self,
        feed_name: String,
        items: Vec<Item>,
        title: Option<String>,
        description: Option<String>,
    ) {
        use std::collections::hash_map::Entry;

        let mut feeds = self.feeds.write().await;
        let mut seen = self.seen_guids.write().await;

        info!(
            "Storing {} items for feed {}",
            items.len(),
            feed_name
        );

        let mut guids = HashSet::new();
        for item in &items {
            guids.insert(item_to_guid(item));
        }

        match feeds.entry(feed_name.clone()) {
            Entry::Occupied(mut entry) => {
                let feed = entry.get_mut();
                if title.is_some() {
                    feed.title = title;
                }
                if description.is_some() {
                    feed.description = description;
                }
                feed.items.extend(items);
            }
            Entry::Vacant(entry) => {
                entry.insert(Feed {
                    title,
                    description,
                    items,
                });
            }
        }

        match seen.entry(feed_name) {
            Entry::Occupied(mut entry) => {
                entry.get_mut().extend(guids);
            }
            Entry::Vacant(entry) => {
                entry.insert(guids);
            }
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
}
