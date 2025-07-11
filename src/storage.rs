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

    pub async fn store_initial_items(
        &self,
        feed_name: String,
        items: Vec<Item>,
        guids: HashSet<String>,
        title: Option<String>,
        description: Option<String>,
    ) {
        let mut feeds = self.feeds.write().await;
        let mut seen = self.seen_guids.write().await;

        info!(
            "Storing {} initial items for feed {}",
            items.len(),
            feed_name
        );

        let feed = Feed {
            title,
            description,
            items,
        };

        feeds.insert(feed_name.clone(), feed);
        seen.insert(feed_name, guids);
    }

    pub async fn add_filtered_item(&self, feed_name: &str, item: Item, guid: String) {
        let mut feeds = self.feeds.write().await;
        let mut seen = self.seen_guids.write().await;

        if let Some(feed) = feeds.get_mut(feed_name) {
            feed.items.push(item);
            if let Some(feed_guids) = seen.get_mut(feed_name) {
                feed_guids.insert(guid);
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
