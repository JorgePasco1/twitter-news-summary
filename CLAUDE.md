# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

This is a Rust application that uses a **hybrid approach**:
1. **Each run**: Fetches list members from Twitter API v2 (1-2 API calls)
2. **Tweet fetching**: Uses Nitter RSS feeds (unlimited, no API quota)
3. Summarizes them using OpenAI's Chat Completions API
4. Sends the summary via Telegram using Telegram Bot API
5. Runs automatically twice daily via GitHub Actions (8am and 6pm UTC)

**Key Benefit**: Avoids Twitter's tweet quota limits by using free RSS feeds. List always stays up-to-date!

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
# - TWITTER_BEARER_TOKEN (OAuth 2.0 App-Only, fetches list members each run)
# - TWITTER_LIST_ID (numeric ID from list URL)
# - OPENAI_API_KEY
# - TELEGRAM_BOT_TOKEN (from @BotFather on Telegram)
# - TELEGRAM_CHAT_ID (numeric chat/user ID)
#
# Optional (with defaults):
# - NITTER_INSTANCE (defaults to https://nitter.net) - RSS feed source
# - OPENAI_MODEL (defaults to gpt-4o-mini)
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
- `main.rs` - Entry point with 4-step orchestration flow
- `config.rs` - Environment variable loading and validation
- `twitter.rs` - Twitter API v2 client for fetching list members dynamically
- `rss.rs` - Nitter RSS feed fetcher (main tweet source)
- `openai.rs` - OpenAI chat completions for summarization
- `telegram.rs` - Telegram Bot API messaging
- `bin/fetch_list_members.rs` - Optional "export" binary to cache list members to file

### Execution Flow (src/main.rs)
1. Load configuration from environment variables
2. Fetch list members from Twitter API (via `twitter::fetch_list_members`)
   - Calls Twitter API v2 `/lists/{id}/members` endpoint
   - Handles pagination to get all members
   - Returns Vec of usernames
   - Uses 1-2 API calls per run (well within free tier limits)
3. Fetch tweets from RSS feeds (via `rss::fetch_tweets_from_rss`)
   - Takes usernames from step 2 as input
   - Fetches RSS feed for each username from Nitter in parallel
   - Filters to last `HOURS_LOOKBACK` (default 12) hours
   - Returns up to `MAX_TWEETS` (default 50) tweets
4. If tweets exist, generate summary with OpenAI
5. Send summary via Telegram with timestamp header

### RSS Integration (src/rss.rs) - Main Tweet Source
- Accepts usernames as parameter (from Twitter API fetch)
- Fetches RSS feeds from Nitter instance for each username
- Uses parallel fetching with `futures::join_all` for performance
- Parses RSS XML using `rss` crate
- Converts RSS items to `Tweet` struct (compatible with existing code)
- Client-side filtering by `HOURS_LOOKBACK` time window
- Sorts tweets by date (newest first)
- Returns up to `MAX_TWEETS` tweets
- **No API quota limits** - unlimited free fetches

### Twitter Integration (src/twitter.rs) - List Member Fetching
- **Used by both main application and export binary**
- Uses Twitter API v2 `/lists/{id}/members` endpoint
- Requires Bearer Token (OAuth 2.0 App-Only authentication)
- Fetches all list members with pagination support
- Returns Vec of usernames to be used for RSS fetching
- Main app calls this each run to keep list up-to-date (1-2 API calls)
- Export binary optionally saves results to `data/usernames.txt` for reference

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
- **Required for each run** - fetches current list members dynamically (1-2 API calls per run)
- Well within free tier limits (10,000 requests/month for list members endpoint)
- Optional: Export binary (`make export`) caches members to file for reference

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
