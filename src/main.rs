use sane_rss::{
    config::Config, llm::LlmFilter, poller::FeedPoller, server::create_router, storage::FeedStorage,
};
use std::{env, sync::Arc};
use tracing::{error, info};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

#[tokio::main]
async fn main() {
    // Initialize tracing
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "sane_rss=info".into()),
        )
        .with(tracing_subscriber::fmt::layer().without_time())
        .init();

    info!("Starting sane-rss");

    // Load configuration
    let config_path = env::args()
        .nth(1)
        .unwrap_or_else(|| "config.toml".to_string());

    let config_path = match std::fs::canonicalize(&config_path) {
        Ok(path) => path,
        Err(e) => {
            error!("Failed to resolve config path '{}': {}", config_path, e);
            std::process::exit(1);
        }
    };

    let config = match Config::from_path(&config_path) {
        Ok(cfg) => Arc::new(cfg),
        Err(e) => {
            error!("Failed to load configuration from {:?}: {}", config_path, e);
            std::process::exit(1);
        }
    };

    info!("Configuration loaded successfully");

    // Initialize components
    let storage = {
        let storage = FeedStorage::new();

        // Attempt to load previously seen feed items.
        if let Some(cache_file) = &config.guid_cache_file {
            info!("Loading seen feed items from {:?}...", cache_file);
            if let Err(error) = storage.load_seen_guids(cache_file).await {
                error!("failed to load items: {}", error);
            }
        }

        Arc::new(storage)
    };
    let llm_filter = Arc::new(LlmFilter::new(config.llm.clone()));
    let poller = FeedPoller::new(config.clone(), storage.clone(), llm_filter);

    // Perform initial fetch
    if let Err(e) = poller.initial_fetch().await {
        error!("Failed during initial feed fetch: {}", e);
        std::process::exit(1);
    }

    // Start feed polling in background
    let poller_handle = tokio::spawn(async move {
        poller.start().await;
    });

    // Start HTTP server
    let app = create_router(storage.clone());

    let addr = format!("{}:{}", config.server_host(), config.server_port());

    info!("Starting HTTP server on {}", addr);

    let listener = match tokio::net::TcpListener::bind(&addr).await {
        Ok(listener) => listener,
        Err(e) => {
            error!("Failed to bind to {}: {}", addr, e);
            std::process::exit(1);
        }
    };

    let server_handle = tokio::spawn(async move {
        if let Err(e) = axum::serve(listener, app).await {
            error!("Server error: {}", e);
        }
    });

    // Wait for tasks (they should run forever)
    tokio::select! {
        _ = poller_handle => {
            error!("Feed poller stopped unexpectedly");
        }
        _ = server_handle => {
            error!("HTTP server stopped unexpectedly");
        }
        _ = tokio::signal::ctrl_c() => {
            info!("Received shutdown signal, stopping...");

            if let Some(cache_file) = &config.guid_cache_file {
                info!("Storing seen feed items to {:?}...", cache_file);
                if let Err(error) = storage.save_seen_guids(cache_file).await {
                    error!("failed to store items: {}", error);
                }
            }
        }
    }
}
