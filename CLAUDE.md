# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

This is a Rust application that uses a **hybrid approach**:
1. **One-time setup**: Exports Twitter list members via Twitter API v2
2. **Ongoing**: Fetches tweets from Nitter RSS feeds (no API quota limits)
3. Summarizes them using OpenAI's Chat Completions API
4. Sends the summary via Telegram using Telegram Bot API
5. Runs automatically twice daily via GitHub Actions (8am and 6pm UTC)

**Key Benefit**: Only uses Twitter API once, then relies on free RSS feeds forever!

## Common Commands

### Development

**Using Makefile (recommended):**
```bash
make help        # Show all available commands
make export      # Export Twitter list members (one-time)
make run         # Run the news summary job
make build       # Build release binary
make check       # Check code
```

**Using Cargo directly:**
```bash
cargo run --bin export    # Export Twitter list members
cargo run                  # Run the main application
cargo build --release      # Build release binary
cargo check                # Check code
cargo fmt                  # Format code
cargo clippy               # Run linter
```

### Environment Setup
```bash
# Copy environment template and fill in credentials
cp .env.example .env

# Required environment variables (see .env.example for formats):
# One-time setup only:
# - TWITTER_BEARER_TOKEN (OAuth 2.0 App-Only, only for list export)
# - TWITTER_LIST_ID (numeric ID from list URL, only for list export)
#
# Main application:
# - NITTER_INSTANCE (defaults to https://nitter.net) - RSS feed source
# - USERNAMES_FILE (defaults to data/usernames.txt) - Exported usernames
# - OPENAI_API_KEY
# - OPENAI_MODEL (defaults to gpt-4o-mini)
# - TELEGRAM_BOT_TOKEN (from @BotFather on Telegram)
# - TELEGRAM_CHAT_ID (numeric chat/user ID)
# - MAX_TWEETS (defaults to 50) - Maximum tweets to return per run
# - HOURS_LOOKBACK (defaults to 12) - Time window for tweet filtering
# - RUST_LOG (defaults to info)
```

### GitHub Actions
```bash
# Manual trigger from Actions tab: "Run workflow" button
# Or push to main to test the workflow file changes

# The workflow runs automatically on schedule (see .github/workflows/summarize.yml)
# All credentials must be configured as GitHub Secrets
```

## Architecture

### Module Structure
- `main.rs` - Entry point with 3-step orchestration flow
- `config.rs` - Environment variable loading and validation
- `rss.rs` - Nitter RSS feed fetcher (main tweet source)
- `twitter.rs` - Twitter API v2 client (only used by export binary)
- `openai.rs` - OpenAI chat completions for summarization
- `telegram.rs` - Telegram Bot API messaging
- `bin/fetch_list_members.rs` - One-time "export" binary to export list members

### Execution Flow (src/main.rs)
1. Load configuration from environment variables
2. Fetch tweets from RSS feeds (via `rss::fetch_tweets_from_rss`)
   - Reads usernames from `data/usernames.txt`
   - Fetches RSS feed for each username from Nitter
   - Filters to last `HOURS_LOOKBACK` (default 12) hours
   - Returns up to `MAX_TWEETS` (default 50) tweets
3. If tweets exist, generate summary with OpenAI
4. Send summary via Telegram with timestamp header

### RSS Integration (src/rss.rs) - Main Tweet Source
- Reads usernames from `data/usernames.txt` file
- Fetches RSS feeds from Nitter instance for each username
- Uses parallel fetching with `futures::join_all` for performance
- Parses RSS XML using `rss` crate
- Converts RSS items to `Tweet` struct (compatible with existing code)
- Client-side filtering by `HOURS_LOOKBACK` time window
- Sorts tweets by date (newest first)
- Returns up to `MAX_TWEETS` tweets
- **No API quota limits** - unlimited free fetches

### Twitter Integration (src/twitter.rs) - One-Time Export Only
- **Only used by `bin/fetch_list_members.rs`** binary
- Uses Twitter API v2 `/lists/{id}/members` endpoint
- Requires Bearer Token (OAuth 2.0 App-Only authentication)
- Fetches all list members with pagination support
- Extracts usernames and saves to `data/usernames.txt`
- **Not used by main application** - only for initial setup

### OpenAI Summarization (src/openai.rs)
- Uses Chat Completions API (`/v1/chat/completions`)
- System prompt instructs to group topics, use bullet points, limit to 500 words
- Sends numbered list of tweets (with author prefixes from Twitter module)
- Temperature 0.7, max_tokens 1000
- Returns plain text summary suitable for Telegram

### Telegram Delivery (src/telegram.rs)
- Uses Telegram Bot API `/sendMessage` endpoint
- Formats message with header: "📰 *Twitter Summary*" + UTC timestamp
- Uses Markdown parse mode for formatting
- Bot token embedded in URL, no separate authentication needed
- Supports both personal chats and group chats (bot must be added to group)

### Error Handling
- Uses `anyhow::Result` for error propagation throughout
- All external API calls include `.context()` for clear error messages
- Non-2xx responses from APIs are converted to errors with status + body
- If no tweets found, gracefully exits without sending Telegram message

### GitHub Actions Workflow (.github/workflows/summarize.yml)
- Builds release binary with caching (speed optimization)
- All secrets passed as environment variables to the binary
- OPENAI_MODEL uses GitHub Variables (not Secrets) for easy modification
- Can be manually triggered via workflow_dispatch

## Important Notes

### Twitter API Requirements (One-Time Only)
- Bearer Token must have read access to lists
- List must be public OR use OAuth 2.0 User Context for private lists
- List ID is numeric and found in URL: `twitter.com/i/lists/{id}`
- **Only needed once** to export list members via `make export` or `cargo run --bin export`

### Nitter/RSS Setup
- Primary instance: `https://nitter.net`
- Alternative instances: `https://nitter.poast.org`, `https://nitter.1d4.us`
- Can be changed via `NITTER_INSTANCE` environment variable
- RSS feeds may be 5-10 minutes delayed (normal and acceptable)
- No API keys or authentication required
- Unlimited free fetches

### Telegram Bot Setup
- Create bot via @BotFather on Telegram (send `/newbot`)
- Must start a conversation with the bot before it can send messages
- Get chat ID from `https://api.telegram.org/bot<TOKEN>/getUpdates` after sending a message
- For groups: add bot to group and use the group's chat ID (negative number)

### Cargo Release Profile
The release profile is highly optimized for binary size:
- `opt-level = "z"` (optimize for size)
- `lto = true` (link-time optimization)
- `codegen-units = 1` (better optimization, slower compile)
- `strip = true` (remove debug symbols)

### Dependencies
- `tokio` - async runtime with full features
- `reqwest` - HTTP client with JSON support
- `serde` + `serde_json` - serialization/deserialization
- `anyhow` + `thiserror` - error handling
- `chrono` - timestamp formatting
- `tracing` + `tracing-subscriber` - structured logging
- `dotenvy` - .env file loading
- `rss` - RSS/Atom feed parsing
- `futures` - parallel async operations
