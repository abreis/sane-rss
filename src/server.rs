use crate::storage::FeedStorage;
use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::get,
    Router,
};
use rss::ChannelBuilder;
use std::sync::Arc;
use tracing::info;

pub struct ServerState {
    pub storage: FeedStorage,
}

pub fn create_router(storage: FeedStorage) -> Router {
    let state = Arc::new(ServerState { storage });

    Router::new()
        .route("/:feed_name", get(serve_feed))
        .route("/", get(list_feeds))
        .with_state(state)
}

async fn serve_feed(
    Path(feed_name): Path<String>,
    State(state): State<Arc<ServerState>>,
) -> Response {
    match state.storage.feeds.read().await.get(&feed_name) {
        Some(feed) => {
            let title = feed
                .title
                .clone()
                .unwrap_or_else(|| format!("[F] {}", feed_name));
            let description = feed
                .description
                .clone()
                .unwrap_or_else(|| format!("Filtered RSS feed for {}", feed_name));
            let link = format!("http://localhost:8080/{}", feed_name);

            let channel = ChannelBuilder::default()
                .title(title)
                .link(link)
                .description(description)
                .items(feed.items.iter().cloned().collect::<Vec<_>>())
                .build();

            let rss_string = channel.to_string();
            info!(
                "Serving feed: {} with {} items",
                feed_name,
                channel.items().len()
            );
            (
                StatusCode::OK,
                [("content-type", "application/rss+xml")],
                rss_string,
            )
                .into_response()
        }
        None => (StatusCode::NOT_FOUND, "Feed not found").into_response(),
    }
}

async fn list_feeds(State(state): State<Arc<ServerState>>) -> Response {
    let feeds = state.storage.feeds.read().await;
    let feed_list: Vec<String> = feeds
        .iter()
        .map(|(name, feed)| format!("- /{} ({} items)", name, feed.items.len()))
        .collect();

    if feed_list.is_empty() {
        (StatusCode::OK, "No feeds available yet").into_response()
    } else {
        let response = format!("Available feeds:\n{}", feed_list.join("\n"));
        (StatusCode::OK, response).into_response()
    }
}
