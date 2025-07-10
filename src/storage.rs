use rss::Item;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::info;

#[derive(Clone)]
pub struct FeedStorage {
    feeds: Arc<RwLock<HashMap<String, Vec<Item>>>>,
    seen_guids: Arc<RwLock<HashMap<String, HashSet<String>>>>,
}

impl FeedStorage {
    pub fn new() -> Self {
        Self {
            feeds: Arc::new(RwLock::new(HashMap::new())),
            seen_guids: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    pub async fn store_initial_items(&self, feed_name: String, items: Vec<Item>, guids: HashSet<String>) {
        let mut feeds = self.feeds.write().await;
        let mut seen = self.seen_guids.write().await;
        
        info!("Storing {} initial items for feed {}", items.len(), feed_name);
        feeds.insert(feed_name.clone(), items);
        seen.insert(feed_name, guids);
    }

    pub async fn add_filtered_item(&self, feed_name: &str, item: Item, guid: String) {
        let mut feeds = self.feeds.write().await;
        let mut seen = self.seen_guids.write().await;
        
        if let Some(feed_items) = feeds.get_mut(feed_name) {
            feed_items.push(item);
            if let Some(feed_guids) = seen.get_mut(feed_name) {
                feed_guids.insert(guid);
            }
            info!("Added filtered item to feed {}", feed_name);
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

    pub async fn get_feed_items(&self, feed_name: &str) -> Option<Vec<Item>> {
        let feeds = self.feeds.read().await;
        feeds.get(feed_name).cloned()
    }

    pub async fn get_all_feeds(&self) -> HashMap<String, Vec<Item>> {
        let feeds = self.feeds.read().await;
        feeds.clone()
    }
}