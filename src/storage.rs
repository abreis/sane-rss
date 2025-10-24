use rss::Item;
use std::collections::{HashMap, VecDeque};
use std::ops::Deref;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::RwLock;

/// Don't keep more than this number of items in the known items cache.
///
/// Limit is per feed. It should be larger than the largest number of items
/// returned by any feed (typically <60).
const KNOWN_ITEMS_LIMIT: usize = 128;

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
    pub known_items: HashMap<String, VecDeque<String>>,

    /// A location to store and load known items.
    known_items_file: PathBuf,
}

impl Deref for FeedStorage {
    type Target = RwLock<FeedStorageInner>;
    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

impl FeedStorage {
    pub fn new(max_items: usize, known_items_file: PathBuf) -> Self {
        Self {
            inner: Arc::new(RwLock::new(FeedStorageInner {
                feeds: HashMap::new(),
                max_items,
                known_items: HashMap::new(),
                known_items_file,
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
    pub fn record_as_known(&mut self, feed_name: impl Into<String>, item: &rss::Item) {
        let item_guid = Self::item_to_guid(item);
        let item_guids = self.known_items.entry(feed_name.into()).or_default();

        item_guids.push_back(item_guid);
        while item_guids.len() > KNOWN_ITEMS_LIMIT {
            item_guids.pop_front();
        }
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

    /// Save our list of known items to a file.
    ///
    /// Overwrites the file's contents.
    pub fn save_known_items(&self) -> std::io::Result<()> {
        let json = serde_json::to_string(&self.known_items)?;
        std::fs::write(&self.known_items_file, json)?;
        Ok(())
    }

    /// Loads our list of known items from a file.
    pub fn load_known_items(&mut self) -> std::io::Result<()> {
        use std::io::ErrorKind;

        match std::fs::read_to_string(&self.known_items_file) {
            // File was read, attempt to deserialize and store.
            Ok(content) => {
                self.known_items = serde_json::from_str(&content)?;
                Ok(())
            }

            // File did not exist: continue.
            Err(error) if error.kind() == ErrorKind::NotFound => Ok(()),

            // Other errors: fail.
            Err(error) => Err(error),
        }
    }
}
