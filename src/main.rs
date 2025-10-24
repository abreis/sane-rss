mod config;
mod filter;
mod poller;
mod server;
mod storage;

use anyhow::Context;
use filter::LLMFilter;
use futures::StreamExt;
use poller::FeedPoller;
use signal_hook::consts::{SIGINT, SIGQUIT, SIGTERM};
use signal_hook_tokio::Signals;
use storage::FeedStorage;
use tracing_subscriber::prelude::*;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Initialize tracing with env-declared filters.
    let env_filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| "sane_rss=info".into());
    tracing_subscriber::registry()
        .with(env_filter)
        .with(tracing_subscriber::fmt::layer().without_time())
        .init();

    tracing::info!("Starting sane-rss");

    //
    // Load configuration.
    let config = {
        // Get the config file from the first commandline argument.
        let config_path = std::env::args()
            .nth(1)
            .context("Please provide a path to a configuration file")?;

        // Canonicalize the config path so we know it exists and can use it later.
        let config_path =
            std::fs::canonicalize(&config_path).context("Failed to resolve config path")?;

        // Read file and deserialize.
        let content =
            std::fs::read_to_string(&config_path).context("Failed to read config file")?;
        let mut config: config::Config =
            toml::from_str(&content).context("Failed to deserialize config file")?;

        // Place known_items_file in the same directory as the config file.
        let mut known_items_file = config_path;
        known_items_file.set_file_name(&config.known_items_file);
        config.known_items_file = known_items_file;

        config
    };
    tracing::info!("Configuration loaded successfully");

    //
    // Initialize components.
    let storage = FeedStorage::new(config.max_items_per_feed, config.known_items_file.clone());
    let llm_filter = LLMFilter::new(config.clone())?;
    let poller = FeedPoller::new(config.clone(), storage.clone(), llm_filter);

    // Load known items from disk.
    storage.write().await.load_known_items()?;

    //
    // Spawn our polling task.
    let poller_handle = tokio::spawn(async move { poller.launch().await });

    //
    // Launch an HTTP server to serve the filtered feeds.
    let app = server::create_router(storage.clone());
    let addr = format!("{}:{}", config.server_host, config.server_port);

    tracing::info!("Starting HTTP server on {}", addr);
    let listener = tokio::net::TcpListener::bind(&addr)
        .await
        .context("Server failed to bind to given address")?;

    let server_handle = tokio::spawn(async move { axum::serve(listener, app).await });

    //
    // Handle signals.
    let mut signals = Signals::new(&[SIGTERM, SIGINT, SIGQUIT]).unwrap();

    // Sends a message to shutdown_recv if any of the signals are received.
    let (shutdown_send, shutdown_recv) = tokio::sync::oneshot::channel::<()>();
    tokio::spawn(async move {
        while let Some(signal) = signals.next().await {
            match signal {
                SIGTERM | SIGINT | SIGQUIT => {
                    shutdown_send.send(()).unwrap();
                    break;
                }
                _ => unreachable!(),
            }
        }
    });

    //
    // Wait for a signal, or for one of the tasks to exit prematurely (poller, http server);
    tokio::select! {
        _ = shutdown_recv => tracing::info!("Received stop signal, shutting down"),
        _ = server_handle => tracing::error!("HTTP server stopped unexpectedly, shutting down"),
        _ = poller_handle => tracing::error!("Feed poller stopped unexpectedly, shutting down"),
    }

    // Store our list of known items on exit.
    storage.read().await.save_known_items()?;

    Ok(())
}
