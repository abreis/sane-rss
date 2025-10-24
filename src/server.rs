use crate::storage::FeedStorage;
use axum::{
    Router,
    extract::{Path, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::get,
};
use rss::ChannelBuilder;

pub fn create_router(storage: FeedStorage) -> Router {
    Router::new()
        .route("/feeds", get(list_feeds))
        .route("/{feed_name}", get(serve_feed))
        .with_state(storage)
}

async fn serve_feed(Path(feed_name): Path<String>, State(storage): State<FeedStorage>) -> Response {
    let storage = storage.read().await;

    // Do we have the requested feed?
    match storage.feeds.get(&feed_name) {
        // Nope.
        None => (StatusCode::NOT_FOUND, "Feed not found").into_response(),

        // Yup.
        Some(feed) => {
            tracing::debug!("Serving feed: {feed_name} with {} items", feed.items.len());

            // Prepare a feed to serve.
            let channel = ChannelBuilder::default()
                .title(&feed.title)
                .description(&feed.description)
                .items(feed.items.clone())
                .build();

            // Turn it into RSS XML and serve.
            let rss_string = channel.to_string();
            let rss_content = [("content-type", "application/rss+xml")];
            (StatusCode::OK, rss_content, rss_string).into_response()
        }
    }
}

async fn list_feeds(State(storage): State<FeedStorage>) -> Response {
    let storage = storage.read().await;

    let content = if storage.feeds.is_empty() {
        "No feeds available yet".to_string()
    } else {
        let feed_list: Vec<String> = storage
            .feeds
            .keys()
            .map(|name| format!("- /{name}"))
            .collect();

        format!("Available feeds:\n{}", feed_list.join("\n"))
    };

    (StatusCode::OK, content).into_response()
}
