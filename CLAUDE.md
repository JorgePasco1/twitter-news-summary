# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

This is a Rust application that:
1. Fetches tweets from a Twitter list using Twitter API v2
2. Summarizes them using OpenAI's Chat Completions API
3. Sends the summary via Telegram using Telegram Bot API
4. Runs automatically twice daily via GitHub Actions (8am and 6pm UTC)

## Common Commands

### Development
```bash
# Run the application locally
cargo run

# Build release binary (optimized for size)
cargo build --release

# Check code without building
cargo check

# Format code
cargo fmt

# Run linter
cargo clippy
```

### Environment Setup
```bash
# Copy environment template and fill in credentials
cp .env.example .env

# Required environment variables (see .env.example for formats):
# - TWITTER_BEARER_TOKEN (OAuth 2.0 App-Only from Twitter Developer Portal)
# - TWITTER_LIST_ID (numeric ID from list URL: twitter.com/i/lists/[ID])
# - OPENAI_API_KEY
# - OPENAI_MODEL (defaults to gpt-4o-mini)
# - TELEGRAM_BOT_TOKEN (from @BotFather on Telegram)
# - TELEGRAM_CHAT_ID (numeric chat/user ID)
# - MAX_TWEETS (defaults to 50) - Maximum tweets to fetch per run
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
- `twitter.rs` - Twitter API v2 client for fetching list tweets
- `openai.rs` - OpenAI chat completions for summarization
- `telegram.rs` - Telegram Bot API messaging

### Execution Flow (src/main.rs)
1. Load configuration from environment variables
2. Fetch up to `MAX_TWEETS` (default 50) tweets from last `HOURS_LOOKBACK` (default 12) hours
3. If tweets exist, generate summary with OpenAI
4. Send summary via Telegram with timestamp header

### Twitter Integration (src/twitter.rs)
- Uses Twitter API v2 `/lists/{id}/tweets` endpoint
- Requires Bearer Token (OAuth 2.0 App-Only authentication)
- Time-based filtering: calculates `start_time` as `now - HOURS_LOOKBACK` hours
- Fetches up to `MAX_TWEETS` with expansions for author data
- Uses ISO 8601 format for `start_time` parameter (e.g., `2024-01-01T00:00:00Z`)
- Enriches tweet text with author info: "@username (Full Name): tweet text"
- Returns `Vec<Tweet>` with id, text, author_id, created_at fields

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

### Twitter API Requirements
- Bearer Token must have read access to lists
- List must be public OR use OAuth 2.0 User Context for private lists
- List ID is numeric and found in URL: `twitter.com/i/lists/{id}`

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
