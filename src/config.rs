use serde::{Deserialize, Serialize};
use std::{collections::HashMap, path::PathBuf};

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Config {
    pub llm: LLMConfig,
    pub global_filters: Filters,
    pub feeds: HashMap<String, FeedConfig>,
    pub server_host: String,
    pub server_port: u16,
    pub polling_interval_seconds: u64,
    pub max_items_per_feed: usize,
    pub known_items_file: PathBuf,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct LLMConfig {
    pub provider: String,
    pub api_key: String,
    pub model: String,
    pub prompt: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Filters {
    pub accept: Vec<String>,
    pub reject: Vec<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct FeedConfig {
    pub url: String,
    pub filters: Filters,
}
