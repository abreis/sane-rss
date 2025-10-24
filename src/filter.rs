//! LLM-based feed filter.

use crate::config::Config;
use anyhow::{Context, bail};
use llm::{
    LLMProvider,
    builder::{LLMBackend, LLMBuilder},
    chat::ChatMessage,
};
use serde::Deserialize;

pub struct LLMFilter {
    llm: Box<dyn LLMProvider>,
    config: Config,
}

/// A result from the LLM filter query.
#[derive(Debug, Deserialize)]
struct FilterResponse {
    accept: bool,
    reject: bool,
}

impl LLMFilter {
    pub fn new(config: Config) -> anyhow::Result<Self> {
        let backend = match config.llm.provider.as_str() {
            "anthropic" => LLMBackend::Anthropic,
            "gemini" => LLMBackend::Google,
            "openai" => LLMBackend::OpenAI,
            _ => bail!("Invalid LLM provider in configuration"),
        };

        let llm = LLMBuilder::new()
            .backend(backend)
            .api_key(&config.llm.api_key)
            .model(&config.llm.model)
            .build()
            .unwrap();

        Ok(Self { llm, config })
    }

    /// Sends the item to the LLM for filtering.
    ///
    /// Returns true if the item should be accepted.
    pub async fn accepts(&self, feed_name: &str, item: &rss::Item) -> bool {
        tracing::debug!(
            "Asking LLM if it accepts item from feed {feed_name}: {:?}",
            item.title()
        );

        //
        // Prepare the list of accepted and rejected topics for this item.
        let feed_config = self.config.feeds.get(feed_name).expect("Unknown feed name");

        let mut accept_topics = Vec::new();
        accept_topics.extend(self.config.global_filters.accept.clone());
        accept_topics.extend(feed_config.filters.accept.clone());

        let mut reject_topics = Vec::new();
        reject_topics.extend(self.config.global_filters.reject.clone());
        reject_topics.extend(feed_config.filters.reject.clone());

        if accept_topics.is_empty() && reject_topics.is_empty() {
            tracing::debug!("No topics to accept or reject, auto-accepting");
            return true;
        }

        // Prepare a prompt.
        let prompt = self.prepare_prompt(item, accept_topics, reject_topics);

        // Call the LLM.
        match self.call_llm(prompt).await {
            Err(error) => {
                tracing::warn!("Failed to chat with the LLM, auto-accepting item: {error}");
                true
            }

            Ok(response) => {
                if response.reject {
                    tracing::info!("LLM filter rejected '{:?}'", item.title());
                }
                tracing::debug!("LLM filter decisions: {:?}", response);

                response.accept || !response.reject
            }
        }
    }

    async fn call_llm(&self, prompt: String) -> anyhow::Result<FilterResponse> {
        tracing::debug!("Sending prompt to the LLM");
        let message = ChatMessage::user().content(prompt).build();
        let messages = vec![message];

        let response = self.llm.chat(&messages).await?;
        let content = response.text().context("No text content in response")?;
        tracing::trace!(response_content = content);

        // Strip markdown JSON code fences if present.
        let content = content
            .trim()
            .strip_prefix("```json")
            .and_then(|s| s.strip_suffix("```"))
            .unwrap_or(&content)
            .to_string();

        // Parse the LLM response.
        let filter_response: FilterResponse =
            serde_json::from_str(&content).context("Failed to parse JSON response from LLM")?;

        Ok(filter_response)
    }

    /// Takes an RSS item and a list of filters, and prepares a prompt for the LLM.
    fn prepare_prompt(
        &self,
        item: &rss::Item,
        accept_topics: Vec<String>,
        reject_topics: Vec<String>,
    ) -> String {
        let mut accept_topics = accept_topics.join("; ");
        let mut reject_topics = reject_topics.join("; ");

        // Try to get a summarized content excerpt from the feed.
        let mut content_excerpt = extract_content_text(item);

        // Always give something to the LLM rather than empty strings.
        let title = item.title().unwrap_or("none");
        let description = item.description().unwrap_or("none");
        if content_excerpt.is_empty() {
            content_excerpt = "none".to_string();
        };
        if accept_topics.is_empty() {
            accept_topics = "none".to_string();
        };
        if reject_topics.is_empty() {
            reject_topics = "none".to_string();
        };

        // Hydrate the prompt template.
        let prompt = self
            .config
            .llm
            .prompt
            .replace("{title}", title)
            .replace("{description}", description)
            .replace("{content_excerpt}", &content_excerpt)
            .replace("{accept_topics}", &accept_topics)
            .replace("{reject_topics}", &reject_topics);

        prompt
    }
}

/// Attempts to parse an HTML content section and turn it into plain text.
fn extract_content_text(item: &rss::Item) -> String {
    let Some(content) = item.content() else {
        return String::new();
    };

    // Limit the size of the resulting content block.
    let max_chars: usize = 1000;

    let document = scraper::Html::parse_document(content);
    let selector = scraper::Selector::parse("p").unwrap();
    let mut extracted_text = String::new();

    // LLM magic.
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

#[cfg(test)]
mod tests {
    fn test_feed_channel() -> rss::Channel {
        let feed_xml = r#"<?xml version="1.0" encoding="UTF-8"?><rss version="2.0"><channel><title><![CDATA[Astral Codex Ten]]></title><description><![CDATA[P(A|B) = [P(A)*P(B|A)]/P(B), all the rest is commentary.]]></description><item><title><![CDATA[Practically-A-Book Review: Byrnes on Trance]]></title><description><![CDATA[...]]></description><link>https://www.astralcodexten.com/p/practically-a-book-review-byrnes</link><guid isPermaLink="false">https://www.astralcodexten.com/p/practically-a-book-review-byrnes</guid><dc:creator><![CDATA[Scott Alexander]]></dc:creator><pubDate>Wed, 09 Jul 2025 11:28:42 GMT</pubDate><enclosure url="https://substack-post-media.s3.amazonaws.com/public/images/f0b86839-2368-4b49-9211-592283ae668a_336x279.png" length="0" type="image/jpeg"/><content:encoded><![CDATA[<p>Steven Byrnes is a physicist/AI researcher/amateur neuroscientist; needless to say, he blogs on Less Wrong. I finally got around to reading <strong><a href="https://www.lesswrong.com/s/qhdHbCJ3PYesL9dde">his 2024 series giving a predictive processing perspective on intuitive self-models</a></strong>. If that sounds boring, it shouldn&#8217;t: Byrnes charges head-on into some of the toughest subjects in psychology, including trance, amnesia, and multiple personalities. I found his perspective enlightening (no pun intended; meditation is another one of his topics) and thought I would share. </p><p>It all centers around this picture:</p><div class="captioned-image-container"><figure><a class="image-link image2" target="_blank" href="https://substackcdn.com/image/fetch/$s_!v7ZB!,f_auto,q_auto:good,fl_progressive:steep/https%3A%2F%2Fsubstack-post-media.s3.amazonaws.com%2Fpublic%2Fimages%2F39854132-188a-4637-9b79-99b055ea5e89_287x234.png" data-component-name="Image2ToDOM"><div class="image2-inset"><picture><source type="image/webp" srcset="https://substackcdn.com/image/fetch/$s_!v7ZB!,w_424,c_limit,f_webp,q_auto:good,fl_progressive:steep/https%3A%2F%2Fsubstack-post-media.s3.amazonaws.com%2Fpublic%2Fimages%2F39854132-188a-4637-9b79-99b055ea5e89_287x234.png 424w, https://substackcdn.com/image/fetch/$s_!v7ZB!,w_848,c_limit,f_webp,q_auto:good,fl_progressive:steep/https%3A%2F%2Fsubstack-post-media.s3.amazonaws.com%2Fpublic%2Fimages%2F39854132-188a-4637-9b79-99b055ea5e89_287x234.png 848w, https://substackcdn.com/image/fetch/$s_!v7ZB!,w_1272,c_limit,f_webp,q_auto:good,fl_progressive:steep/https%3A%2F%2Fsubstack-post-media.s3.amazonaws.com%2Fpublic%2Fimages%2F39854132-188a-4637-9b79-99b055ea5e89_287x234.png 1272w, https://substackcdn.com/image/fetch/$s_!v7ZB!,w_1456,c_limit,f_webp,q_auto:good,fl_progressive:steep/https%3A%2F%2Fsubstack-post-media.s3.amazonaws.com%2Fpublic%2Fimages%2F39854132-188a-4637-9b79-99b055ea5e89_287x234.png 1456w" sizes="100vw"><img src="https://substackcdn.com/image/fetch/$s_!v7ZB!,w_1456,c_limit,f_auto,q_auto:good,fl_progressive:steep/https%3A%2F%2Fsubstack-post-media.s3.amazonaws.com%2Fpublic%2Fimages%2F39854132-188a-4637-9b79-99b055ea5e89_287x234.png" width="287" height="234" data-attrs="{&quot;src&quot;:&quot;https://substack-post-media.s3.amazonaws.com/public/images/39854132-188a-4637-9b79-99b055ea5e89_287x234.png&quot;,&quot;srcNoWatermark&quot;:null,&quot;fullscreen&quot;:null,&quot;imageSize&quot;:null,&quot;height&quot;:234,&quot;width&quot;:287,&quot;resizeWidth&quot;:null,&quot;bytes&quot;:11117,&quot;alt&quot;:null,&quot;title&quot;:null,&quot;type&quot;:&quot;image/png&quot;,&quot;href&quot;:null,&quot;belowTheFold&quot;:false,&quot;topImage&quot;:true,&quot;internalRedirect&quot;:&quot;https://www.astralcodexten.com/i/166402303?img=https%3A%2F%2Fsubstack-post-media.s3.amazonaws.com%2Fpublic%2Fimages%2F39854132-188a-4637-9b79-99b055ea5e89_287x234.png&quot;,&quot;isProcessing&quot;:false,&quot;align&quot;:null,&quot;offset&quot;:false}" class="sizing-normal" alt="" srcset="https://substackcdn.com/image/fetch/$s_!v7ZB!,w_424,c_limit,f_auto,q_auto:good,fl_progressive:steep/https%3A%2F%2Fsubstack-post-media.s3.amazonaws.com%2Fpublic%2Fimages%2F39854132-188a-4637-9b79-99b055ea5e89_287x234.png 424w, https://substackcdn.com/image/fetch/$s_!v7ZB!,w_848,c_limit,f_auto,q_auto:good,fl_progressive:steep/https%3A%2F%2Fsubstack-post-media.s3.amazonaws.com%2Fpublic%2Fimages%2F39854132-188a-4637-9b79-99b055ea5e89_287x234.png 848w, https://substackcdn.com/image/fetch/$s_!v7ZB!,w_1272,c_limit,f_auto,q_auto:good,fl_progressive:steep/https%3A%2F%2Fsubstack-post-media.s3.amazonaws.com%2Fpublic%2Fimages%2F39854132-188a-4637-9b79-99b055ea5e89_287x234.png 1272w, https://substackcdn.com/image/fetch/$s_!v7ZB!,w_1456,c_limit,f_auto,q_auto:good,fl_progressive:steep/https%3A%2F%2Fsubstack-post-media.s3.amazonaws.com%2Fpublic%2Fimages%2F39854132-188a-4637-9b79-99b055ea5e89_287x234.png 1456w" sizes="100vw" fetchpriority="high"></picture><div></div></div></a></figure></div><p>But first: some excruciatingly obvious philosophical preliminaries.</p><p>We don&#8217;t directly perceive the external world. Every philosopher has their own way of saying exactly what it is we <em>do</em> perceive, but the predictive processing interpretation is that we perceive our models of the world. To be very naive and hand-wavey, lower-level brain centers get sense-data, make a guess about what produced that sense data, then &#8220;show&#8221; &#8220;us&#8221; that guess. If the guess is wrong, too bad - we see the incorrect guess, not the reality. </p>]]></content:encoded></item></channel></rss>"#;

        rss::Channel::read_from(feed_xml.as_bytes()).expect("Failed to parse RSS feed")
    }

    #[test]
    fn test_extract_content_text() {
        let channel = test_feed_channel();
        let item = channel.items().first().expect("No items in feed");

        // Test the extract_content_text function
        let extracted_text = super::extract_content_text(item);

        let expected_text = r#"Steven Byrnes is a physicist/AI researcher/amateur neuroscientist; needless to say, he blogs on Less Wrong. I finally got around to reading his 2024 series giving a predictive processing perspective on intuitive self-models. If that sounds boring, it shouldn’t: Byrnes charges head-on into some of the toughest subjects in psychology, including trance, amnesia, and multiple personalities. I found his perspective enlightening (no pun intended; meditation is another one of his topics) and thought I would share. It all centers around this picture: But first: some excruciatingly obvious philosophical preliminaries. We don’t directly perceive the external world. Every philosopher has their own way of saying exactly what it is we do perceive, but the predictive processing interpretation is that we perceive our models of the world. To be very naive and hand-wavey, lower-level brain centers get sense-data, make a guess about what produced that sense data, then “show” “us” that guess. "#;

        assert_eq!(extracted_text, expected_text)
    }
}
