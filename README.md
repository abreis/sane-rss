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

## Systemd Service Installation (Debian/Ubuntu)

### 1. Build and Install the Binary

```bash
# Build the release binary
cargo build --release

# Copy the binary to a system location
sudo cp target/release/sane-rss /usr/local/bin/
sudo chmod +x /usr/local/bin/sane-rss
```

### 2. Create Configuration Directory

```bash
# Create directory for the configuration file
sudo mkdir -p /etc/sane-rss

# Copy your config file to the system location
sudo cp config/config.toml /etc/sane-rss/config.toml
sudo chmod 644 /etc/sane-rss/config.toml
```

### 3. Install Systemd Service

```bash
# Copy the systemd service file
sudo cp build/sane-rss.service /etc/systemd/system/
sudo chmod 644 /etc/systemd/system/sane-rss.service

# Reload systemd to recognize the new service
sudo systemctl daemon-reload
```

### 4. Configure the Service

Edit the service file to specify the config file location:

```bash
sudo systemctl edit sane-rss
```

Add the following override:

```
[Service]
ExecStart=
ExecStart=/usr/local/bin/sane-rss /etc/sane-rss/config.toml
```

### 5. Start and Enable the Service

```bash
# Start the service
sudo systemctl start sane-rss

# Enable the service to start on boot
sudo systemctl enable sane-rss

# Check service status
sudo systemctl status sane-rss

# View logs
sudo journalctl -u sane-rss -f
```

### 6. Optional: Create a Dedicated User

For improved security, create a dedicated user:

```bash
# Create a system user for the service
sudo useradd -r -s /bin/false sane-rss

# Create a data directory if needed
sudo mkdir -p /var/lib/sane-rss
sudo chown sane-rss:sane-rss /var/lib/sane-rss

# Uncomment the User and Group lines in the systemd service file
sudo systemctl edit sane-rss
```

Add:
```
[Service]
User=sane-rss
Group=sane-rss
```

### Managing the Service

```bash
# Stop the service
sudo systemctl stop sane-rss

# Restart the service
sudo systemctl restart sane-rss

# Disable the service from starting on boot
sudo systemctl disable sane-rss

# View service logs
sudo journalctl -u sane-rss --since "1 hour ago"
```

### Configuration File Location

The systemd service expects the configuration file at `/etc/sane-rss/config.toml`. Make sure to update this file with your RSS feeds and API credentials.

## Notes

- Posts are only filtered when they are new (not on initial launch)
- All data is stored in memory and will be lost on restart
- The service uses RSS 2.0 format for output feeds
