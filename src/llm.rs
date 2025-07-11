use crate::config::{Filters, LlmConfig};
use rss::Item;
use serde::{Deserialize, Serialize};
use std::time::Duration;
use tracing::{error, info, warn};

#[derive(Debug, Serialize, Deserialize)]
struct FilterResponse {
    accept: bool,
    reject: bool,
}

pub struct LlmFilter {
    client: reqwest::Client,
    config: LlmConfig,
}

impl LlmFilter {
    pub fn new(config: LlmConfig) -> Self {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(60))
            .build()
            .expect("Failed to create HTTP client");

        Self { client, config }
    }

    pub async fn should_accept_item(
        &self,
        item: &Item,
        global_filters: &Option<Filters>,
        local_filters: &Option<Filters>,
    ) -> bool {
        let title = item.title().unwrap_or("No title");
        let description = item.description().unwrap_or("No description");
        let link = item.link().unwrap_or("");

        let mut accept_topics = Vec::new();
        let mut reject_topics = Vec::new();

        if let Some(global) = global_filters {
            if let Some(accept) = &global.accept {
                accept_topics.extend(accept.clone());
            }
            if let Some(reject) = &global.reject {
                reject_topics.extend(reject.clone());
            }
        }

        if let Some(local) = local_filters {
            if let Some(accept) = &local.accept {
                accept_topics.extend(accept.clone());
            }
            if let Some(reject) = &local.reject {
                reject_topics.extend(reject.clone());
            }
        }

        if accept_topics.is_empty() && reject_topics.is_empty() {
            info!("No filters configured, accepting item: {}", title);
            return true;
        }

        let prompt = self
            .config
            .prompt
            .replace("{title}", title)
            .replace("{description}", description)
            .replace("{link}", link)
            .replace("{accept_topics}", &accept_topics.join("; "))
            .replace("{reject_topics}", &reject_topics.join("; "));

        match self.call_anthropic_api(&prompt).await {
            Ok(response) => {
                info!(
                    "LLM filter result for '{}': accept={}, reject={}",
                    title, response.accept, response.reject
                );
                response.accept || (!response.reject && accept_topics.is_empty())
            }
            Err(e) => {
                error!("Failed to filter item '{}': {}", title, e);
                true
            }
        }
    }

    async fn call_anthropic_api(
        &self,
        prompt: &str,
    ) -> Result<FilterResponse, Box<dyn std::error::Error>> {
        let request_body = serde_json::json!({
            "model": self.config.model,
            "max_tokens": 1024,
            "messages": [{
                "role": "user",
                "content": prompt
            }],
        });

        let response = self
            .client
            .post("https://api.anthropic.com/v1/messages")
            .header("x-api-key", &self.config.api_key)
            .header("anthropic-version", "2023-06-01")
            .header("content-type", "application/json")
            .json(&request_body)
            .send()
            .await?;

        if !response.status().is_success() {
            let error_text = response.text().await?;
            warn!("Anthropic API error: {}", error_text);
            return Err(format!("API request failed: {}", error_text).into());
        }

        let api_response: serde_json::Value = response.json().await?;

        let content = api_response["content"][0]["text"]
            .as_str()
            .ok_or("No text content in response")?;

        let filter_response: FilterResponse = serde_json::from_str(content)?;
        Ok(filter_response)
    }
}
