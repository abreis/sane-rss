use rss::Item;
use std::collections::{HashMap, HashSet, VecDeque};
use std::ops::Deref;
use std::sync::Arc;
use tokio::sync::RwLock;

pub struct StoredFeed {
    pub title: String,
    pub description: String,
    pub items: VecDeque<Item>,
}

#[derive(Clone)]
pub struct FeedStorage {
    inner: Arc<RwLock<FeedStorageInner>>,
}

pub struct FeedStorageInner {
    /// A list of items we're serving to the user.
    pub feeds: HashMap<String, StoredFeed>,

    /// How many items we can keep in each feed.
    max_items: usize,

    /// A list of items we've seen before (and might have filtered out).
    ///
    /// Note: not limited by `max_items`.
    pub known_items: HashMap<String, HashSet<String>>,
}

impl Deref for FeedStorage {
    type Target = RwLock<FeedStorageInner>;
    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

impl FeedStorage {
    pub fn new(max_items: usize) -> Self {
        Self {
            inner: Arc::new(RwLock::new(FeedStorageInner {
                feeds: HashMap::new(),
                max_items,
                known_items: HashMap::new(),
            })),
        }
    }
}

impl FeedStorageInner {
    /// Store an item to be served in our filtered feeds.
    pub fn store_filtered_item(&mut self, feed_name: &str, item: rss::Item) {
        let feed = self
            .feeds
            .get_mut(feed_name)
            .expect("Tried to record an item in an unknown feed");

        feed.items.push_back(item);

        // Remove oldest items if we exceed the limit.
        while feed.items.len() > self.max_items {
            feed.items.pop_front();
        }
    }

    /// Adds a new empty channel to our storage if it does not exist.
    pub fn add_channel(&mut self, feed_name: &str, title: &str, description: &str) {
        if !self.feeds.contains_key(feed_name) {
            self.feeds.insert(
                feed_name.to_owned(),
                StoredFeed {
                    title: title.to_owned(),
                    description: description.to_owned(),
                    items: VecDeque::new(),
                },
            );
        }
    }

    /// Returns whether an item in a given feed has been seen before.
    pub fn is_known(&self, feed_name: &str, item: &rss::Item) -> bool {
        let item_guid = Self::item_to_guid(item);

        if let Some(known_feed_items) = self.known_items.get(feed_name) {
            known_feed_items.contains(&item_guid)
        } else {
            false
        }
    }

    /// Records a new item in a feed as known.
    ///
    /// Returns false if the item already existed.
    pub fn record_as_known(&mut self, feed_name: impl Into<String>, item: &rss::Item) -> bool {
        let item_guid = Self::item_to_guid(item);
        self.known_items
            .entry(feed_name.into())
            .or_default()
            .insert(item_guid)
    }

    /// Turns an RSS item into a GUID.
    ///
    /// If the item does not contain a GUID, we use its link or its title as a unique identifier.
    fn item_to_guid(item: &rss::Item) -> String {
        if let Some(guid) = item.guid() {
            guid.value().to_string()
        } else if let Some(link) = item.link() {
            link.to_string()
        } else if let Some(title) = item.title() {
            format!("{}-{}", title, item.pub_date().unwrap_or("no-date"))
        } else {
            unreachable!()
        }
    }
}

// /// Initialize a feed with metadata during first fetch
// pub async fn initialize_feed(
//     &self,
//     feed_name: String,
//     feed_title: String,
//     feed_description: String,
// ) {
//     use std::collections::hash_map::Entry;
//     let mut feeds = self.feeds.write().await;

//     info!("Initializing feed {}", feed_name);

//     match feeds.entry(feed_name) {
//         Entry::Occupied(mut entry) => {
//             let feed = entry.get_mut();
//             feed.title = Some(feed_title);
//             feed.description = Some(feed_description);
//         }
//         Entry::Vacant(entry) => {
//             entry.insert(Feed {
//                 title: Some(feed_title),
//                 description: Some(feed_description),
//                 items: VecDeque::new(),
//                 favicon: None,
//             });
//         }
//     }
// }

// /// Add new items to an existing feed during polling
// /// Only adds items that haven't been seen before (deduplication)
// pub async fn add_items(&self, feed_name: String, items: Vec<Item>, max_items: usize) {
//     // Filter out items we've already seen
//     let mut new_items = Vec::new();
//     for item in items {
//         let guid = item_to_guid(&item);
//         if self.is_new_item(&feed_name, &guid).await {
//             new_items.push(item);
//         }
//     }

//     if new_items.is_empty() {
//         return;
//     }

//     let mut feeds = self.feeds.write().await;

//     info!("Adding {} new items to feed {}", new_items.len(), feed_name);

//     if let Some(feed) = feeds.get_mut(&feed_name) {
//         for item in &new_items {
//             feed.items.push_back(item.clone());

//             // Remove oldest items if we exceed the limit
//             while feed.items.len() > max_items {
//                 feed.items.pop_front();
//             }
//         }
//     } else {
//         // Feed doesn't exist yet, this can't happen.
//         warn!("Tried to add items to a feed that doesn't exist: {feed_name}")
//     }

//     // Drop the write lock before calling record_seen_item
//     drop(feeds);

//     // Mark all new items as seen using the dedicated method
//     for item in &new_items {
//         self.record_seen_item(&feed_name, item_to_guid(item)).await;
//     }
// }

// pub async fn is_new_item(&self, feed_name: &str, guid: &str) -> bool {
//     let seen = self.seen_guids.read().await;
//     if let Some(feed_guids) = seen.get(feed_name) {
//         !feed_guids.contains(guid)
//     } else {
//         true
//     }
// }

// // Tracks items we've already retrieved, so we don't add them repeatedly.
// pub async fn record_seen_item(&self, feed_name: &str, guid: String) {
//     use std::collections::hash_map::Entry;

//     let mut seen = self.seen_guids.write().await;

//     match seen.entry(feed_name.to_string()) {
//         Entry::Occupied(mut entry) => {
//             entry.get_mut().insert(guid);
//         }
//         Entry::Vacant(entry) => {
//             let mut guids = HashSet::new();
//             guids.insert(guid);
//             entry.insert(guids);
//         }
//     }
// }

// pub async fn store_favicon(&self, feed_name: &str, favicon_data: Vec<u8>) {
//     let mut feeds = self.feeds.write().await;
//     if let Some(feed) = feeds.get_mut(feed_name) {
//         feed.favicon = Some(favicon_data);
//         info!("Stored favicon for feed {}", feed_name);
//     }
// }

// pub async fn get_favicon(&self, feed_name: &str) -> Option<Vec<u8>> {
//     let feeds = self.feeds.read().await;
//     feeds.get(feed_name).and_then(|feed| feed.favicon.clone())
// }

// pub async fn save_seen_guids(&self, path: &PathBuf) -> std::io::Result<()> {
//     let seen = self.seen_guids.read().await;
//     let json = serde_json::to_string(&*seen)?;
//     std::fs::write(path, json)?;
//     Ok(())
// }

// pub async fn load_seen_guids(&self, path: &PathBuf) -> std::io::Result<()> {
//     use std::io::ErrorKind;

//     let json = match std::fs::read_to_string(path) {
//         Ok(content) => {
//             if content.is_empty() {
//                 return Ok(());
//             }
//             content
//         }
//         Err(e) if e.kind() == ErrorKind::NotFound => {
//             return Ok(());
//         }
//         Err(e) => return Err(e),
//     };

//     let loaded: HashMap<String, HashSet<String>> = serde_json::from_str(&json)?;
//     let mut seen = self.seen_guids.write().await;
//     *seen = loaded;
//     Ok(())
// }
