# sane-rss

An RSS filtering service that uses LLMs to intelligently filter feed items based on topics.

## Features

- Subscribe to multiple RSS feeds
- Filter posts using Claude/Anthropic models based on accept/reject topics
- Serve filtered feeds via HTTP endpoints
- Configurable via TOML file
- In-memory storage (no database required)
- Automatic periodic polling of feeds
- Parallel feed fetching on startup for improved performance
- LLM filtering includes article URLs for more accurate content analysis

## Installation

```bash
cargo build --release
```

## Configuration

Create a `config.toml` file (see `config/sample.toml` for an example):


## Usage

Run the service:

```bash
./target/release/sane-rss config.toml
```

Or with cargo:

```bash
cargo run --release -- config.toml
```

The service will:
1. Load existing posts from feeds without filtering (on first launch)
2. Start polling feeds at the configured interval for new posts
3. Filter new posts using the configured LLM
4. Serve filtered feeds at `http://localhost:8080/{feed_name}`

## Accessing Filtered Feeds

- List all available feeds: `http://localhost:8080/`
- Access a specific feed: `http://localhost:8080/{feed_name}`

## Environment Variables

Set the log level:
```bash
RUST_LOG=sane_rss=debug ./target/release/sane-rss config.toml
```

## Notes

- Posts are only filtered when they are new (not on initial launch)
- All data is stored in memory and will be lost on restart
- The service uses RSS 2.0 format for output feeds
