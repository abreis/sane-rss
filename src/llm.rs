use crate::config::{Filters, LlmConfig};
use rss::Item;
use scraper::{Html, Selector};
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
        let content_excerpt = self.extract_content_text(item);

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
            return true;
        }

        let prompt = self
            .config
            .prompt
            .replace("{title}", title)
            .replace("{description}", description)
            .replace("{content_excerpt}", &content_excerpt)
            .replace("{accept_topics}", &accept_topics.join("; "))
            .replace("{reject_topics}", &reject_topics.join("; "));

        match self.call_anthropic_api(&prompt).await {
            Ok(response) => {
                if response.reject {
                    info!(
                        "LLM filter rejected '{}': accept={}, reject={}",
                        title, response.accept, response.reject
                    );
                }

                response.accept || !response.reject
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

        tracing::debug!(content);

        let filter_response: FilterResponse = serde_json::from_str(content)?;
        Ok(filter_response)
    }

    fn extract_content_text(&self, item: &Item) -> String {
        let content = item.content().unwrap_or("");
        if content.is_empty() {
            return String::new();
        }

        let document = Html::parse_document(content);
        let selector = Selector::parse("p").unwrap();
        let mut extracted_text = String::new();
        let max_chars: usize = 1000;

        for element in document.select(&selector) {
            let text = element.text().collect::<String>().trim().to_string();
            if !text.is_empty() {
                let remaining = max_chars.saturating_sub(extracted_text.len());
                if remaining == 0 {
                    break;
                }

                if extracted_text.len() + text.len() <= max_chars {
                    if !extracted_text.is_empty() {
                        extracted_text.push(' ');
                    }
                    extracted_text.push_str(&text);
                } else {
                    let truncated = &text[..remaining.min(text.len())];
                    if !extracted_text.is_empty() {
                        extracted_text.push(' ');
                    }
                    extracted_text.push_str(truncated);
                    break;
                }
            }
        }

        extracted_text
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rss::Channel;

    fn test_prompt() -> &'static str {
        r#"You are an RSS feed filter. Analyze the following RSS post and determine if it matches any of the provided topics.

        Post title: {title}
        Post description: {description}
        Post content excerpt: {content_excerpt}

        Accept topics: {accept_topics}
        Reject topics: {reject_topics}

        Return a JSON response with two boolean fields:
        - "accept": true if the post matches any accept topics, otherwise false
        - "reject": true if the post matches any reject topics, otherwise false

        Both fields can be true at the same time. If both fields are true, the post will be accepted.

        You must respond with valid JSON in exactly this format: {"accept": true/false, "reject": true/false}"#
    }

    fn test_llmconfig_noapi() -> LlmConfig {
        LlmConfig {
            api_key: "no_key".to_string(),
            model: "no_model".to_string(),
            prompt: "no_prompt".to_string(),
        }
    }
    fn test_llmconfig() -> LlmConfig {
        LlmConfig {
            api_key: std::env::var("ANTHROPIC_API_KEY").expect("unset ANTHROPIC_API_KEY"),
            model: "claude-sonnet-4-20250514".to_string(),
            prompt: test_prompt().to_string(),
        }
    }

    fn test_feed_channel() -> rss::Channel {
        let feed_xml = r#"<?xml version="1.0" encoding="UTF-8"?><rss version="2.0"><channel><title><![CDATA[Astral Codex Ten]]></title><description><![CDATA[P(A|B) = [P(A)*P(B|A)]/P(B), all the rest is commentary.]]></description><item><title><![CDATA[Practically-A-Book Review: Byrnes on Trance]]></title><description><![CDATA[...]]></description><link>https://www.astralcodexten.com/p/practically-a-book-review-byrnes</link><guid isPermaLink="false">https://www.astralcodexten.com/p/practically-a-book-review-byrnes</guid><dc:creator><![CDATA[Scott Alexander]]></dc:creator><pubDate>Wed, 09 Jul 2025 11:28:42 GMT</pubDate><enclosure url="https://substack-post-media.s3.amazonaws.com/public/images/f0b86839-2368-4b49-9211-592283ae668a_336x279.png" length="0" type="image/jpeg"/><content:encoded><![CDATA[<p>Steven Byrnes is a physicist/AI researcher/amateur neuroscientist; needless to say, he blogs on Less Wrong. I finally got around to reading <strong><a href="https://www.lesswrong.com/s/qhdHbCJ3PYesL9dde">his 2024 series giving a predictive processing perspective on intuitive self-models</a></strong>. If that sounds boring, it shouldn&#8217;t: Byrnes charges head-on into some of the toughest subjects in psychology, including trance, amnesia, and multiple personalities. I found his perspective enlightening (no pun intended; meditation is another one of his topics) and thought I would share. </p><p>It all centers around this picture:</p><div class="captioned-image-container"><figure><a class="image-link image2" target="_blank" href="https://substackcdn.com/image/fetch/$s_!v7ZB!,f_auto,q_auto:good,fl_progressive:steep/https%3A%2F%2Fsubstack-post-media.s3.amazonaws.com%2Fpublic%2Fimages%2F39854132-188a-4637-9b79-99b055ea5e89_287x234.png" data-component-name="Image2ToDOM"><div class="image2-inset"><picture><source type="image/webp" srcset="https://substackcdn.com/image/fetch/$s_!v7ZB!,w_424,c_limit,f_webp,q_auto:good,fl_progressive:steep/https%3A%2F%2Fsubstack-post-media.s3.amazonaws.com%2Fpublic%2Fimages%2F39854132-188a-4637-9b79-99b055ea5e89_287x234.png 424w, https://substackcdn.com/image/fetch/$s_!v7ZB!,w_848,c_limit,f_webp,q_auto:good,fl_progressive:steep/https%3A%2F%2Fsubstack-post-media.s3.amazonaws.com%2Fpublic%2Fimages%2F39854132-188a-4637-9b79-99b055ea5e89_287x234.png 848w, https://substackcdn.com/image/fetch/$s_!v7ZB!,w_1272,c_limit,f_webp,q_auto:good,fl_progressive:steep/https%3A%2F%2Fsubstack-post-media.s3.amazonaws.com%2Fpublic%2Fimages%2F39854132-188a-4637-9b79-99b055ea5e89_287x234.png 1272w, https://substackcdn.com/image/fetch/$s_!v7ZB!,w_1456,c_limit,f_webp,q_auto:good,fl_progressive:steep/https%3A%2F%2Fsubstack-post-media.s3.amazonaws.com%2Fpublic%2Fimages%2F39854132-188a-4637-9b79-99b055ea5e89_287x234.png 1456w" sizes="100vw"><img src="https://substackcdn.com/image/fetch/$s_!v7ZB!,w_1456,c_limit,f_auto,q_auto:good,fl_progressive:steep/https%3A%2F%2Fsubstack-post-media.s3.amazonaws.com%2Fpublic%2Fimages%2F39854132-188a-4637-9b79-99b055ea5e89_287x234.png" width="287" height="234" data-attrs="{&quot;src&quot;:&quot;https://substack-post-media.s3.amazonaws.com/public/images/39854132-188a-4637-9b79-99b055ea5e89_287x234.png&quot;,&quot;srcNoWatermark&quot;:null,&quot;fullscreen&quot;:null,&quot;imageSize&quot;:null,&quot;height&quot;:234,&quot;width&quot;:287,&quot;resizeWidth&quot;:null,&quot;bytes&quot;:11117,&quot;alt&quot;:null,&quot;title&quot;:null,&quot;type&quot;:&quot;image/png&quot;,&quot;href&quot;:null,&quot;belowTheFold&quot;:false,&quot;topImage&quot;:true,&quot;internalRedirect&quot;:&quot;https://www.astralcodexten.com/i/166402303?img=https%3A%2F%2Fsubstack-post-media.s3.amazonaws.com%2Fpublic%2Fimages%2F39854132-188a-4637-9b79-99b055ea5e89_287x234.png&quot;,&quot;isProcessing&quot;:false,&quot;align&quot;:null,&quot;offset&quot;:false}" class="sizing-normal" alt="" srcset="https://substackcdn.com/image/fetch/$s_!v7ZB!,w_424,c_limit,f_auto,q_auto:good,fl_progressive:steep/https%3A%2F%2Fsubstack-post-media.s3.amazonaws.com%2Fpublic%2Fimages%2F39854132-188a-4637-9b79-99b055ea5e89_287x234.png 424w, https://substackcdn.com/image/fetch/$s_!v7ZB!,w_848,c_limit,f_auto,q_auto:good,fl_progressive:steep/https%3A%2F%2Fsubstack-post-media.s3.amazonaws.com%2Fpublic%2Fimages%2F39854132-188a-4637-9b79-99b055ea5e89_287x234.png 848w, https://substackcdn.com/image/fetch/$s_!v7ZB!,w_1272,c_limit,f_auto,q_auto:good,fl_progressive:steep/https%3A%2F%2Fsubstack-post-media.s3.amazonaws.com%2Fpublic%2Fimages%2F39854132-188a-4637-9b79-99b055ea5e89_287x234.png 1272w, https://substackcdn.com/image/fetch/$s_!v7ZB!,w_1456,c_limit,f_auto,q_auto:good,fl_progressive:steep/https%3A%2F%2Fsubstack-post-media.s3.amazonaws.com%2Fpublic%2Fimages%2F39854132-188a-4637-9b79-99b055ea5e89_287x234.png 1456w" sizes="100vw" fetchpriority="high"></picture><div></div></div></a></figure></div><p>But first: some excruciatingly obvious philosophical preliminaries.</p><p>We don&#8217;t directly perceive the external world. Every philosopher has their own way of saying exactly what it is we <em>do</em> perceive, but the predictive processing interpretation is that we perceive our models of the world. To be very naive and hand-wavey, lower-level brain centers get sense-data, make a guess about what produced that sense data, then &#8220;show&#8221; &#8220;us&#8221; that guess. If the guess is wrong, too bad - we see the incorrect guess, not the reality. </p>]]></content:encoded></item></channel></rss>"#;

        Channel::read_from(feed_xml.as_bytes()).expect("Failed to parse RSS feed")
    }

    #[test]
    fn test_extract_content_text() {
        let channel = test_feed_channel();
        let item = channel.items().first().expect("No items in feed");

        // Create a mock LLM config for testing
        let config = test_llmconfig_noapi();
        let llm_filter = LlmFilter::new(config);

        // Test the extract_content_text function
        let extracted_text = llm_filter.extract_content_text(item);

        let expected_text = r#"Steven Byrnes is a physicist/AI researcher/amateur neuroscientist; needless to say, he blogs on Less Wrong. I finally got around to reading his 2024 series giving a predictive processing perspective on intuitive self-models. If that sounds boring, it shouldn’t: Byrnes charges head-on into some of the toughest subjects in psychology, including trance, amnesia, and multiple personalities. I found his perspective enlightening (no pun intended; meditation is another one of his topics) and thought I would share. It all centers around this picture: But first: some excruciatingly obvious philosophical preliminaries. We don’t directly perceive the external world. Every philosopher has their own way of saying exactly what it is we do perceive, but the predictive processing interpretation is that we perceive our models of the world. To be very naive and hand-wavey, lower-level brain centers get sense-data, make a guess about what produced that sense data, then “show” “us” that guess. "#;

        assert_eq!(extracted_text, expected_text)
    }

    #[tokio::test]
    async fn test_should_accept_item() {
        // Get test RSS channel and first item
        let channel = test_feed_channel();
        let item = channel.items().first().expect("No items in feed");

        let llm_config = test_llmconfig();
        let llm_filter = LlmFilter::new(llm_config);

        let global_filters: Option<Filters> = None;
        let local_filters: Option<Filters> = Some(Filters {
            accept: None,
            reject: Some(vec![
                "title is about: open thread, or hidden open thread".to_string(),
                "main topic is: ACX grants, or directly related to it".to_string(),
            ]),
        });

        let result = llm_filter
            .should_accept_item(item, &global_filters, &local_filters)
            .await;

        assert!(result);
    }
}
