use serde::{Deserialize, Serialize};
use std::{collections::HashMap, path::PathBuf};

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Config {
    pub llm: LlmConfig,
    pub global_filters: Option<Filters>,
    pub feeds: HashMap<String, FeedConfig>,
    pub server_host: Option<String>,
    pub server_port: Option<u16>,
    pub polling_interval_seconds: Option<u64>,
    pub max_items_per_feed: Option<usize>,
    pub guid_cache_file: Option<PathBuf>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct LlmConfig {
    pub api_key: String,
    pub model: String,
    pub prompt: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Filters {
    pub accept: Option<Vec<String>>,
    pub reject: Option<Vec<String>>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct FeedConfig {
    pub url: String,
    pub filters: Option<Filters>,
}

impl Config {
    pub fn from_path(path: &PathBuf) -> Result<Self, Box<dyn std::error::Error>> {
        let content = std::fs::read_to_string(path)?;
        let mut config: Config = toml::from_str(&content)?;

        // Place the guid cache file in the config directory.
        if let Some(guid_filename) = &config.guid_cache_file {
            let mut guid_file_path = path.clone();
            guid_file_path.set_file_name(guid_filename);
            config.guid_cache_file = Some(guid_file_path);
        }

        Ok(config)
    }

    pub fn polling_interval_seconds(&self) -> u64 {
        self.polling_interval_seconds.unwrap_or(300) // Default to 5 minutes
    }

    pub fn server_host(&self) -> &str {
        self.server_host.as_deref().unwrap_or("127.0.0.1")
    }

    pub fn server_port(&self) -> u16 {
        self.server_port.unwrap_or(8080)
    }

    pub fn max_items_per_feed(&self) -> usize {
        self.max_items_per_feed.unwrap_or(60) // Default to 60 items
    }
}
