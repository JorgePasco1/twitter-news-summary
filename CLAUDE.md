# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

This is a Rust application that fetches and summarizes Twitter list tweets:
1. **List members**: Extracted via browser console script (saved to `data/usernames.txt`)
2. **Tweet fetching**: Uses Nitter RSS feeds (unlimited, no API quota)
3. Summarizes them using OpenAI's Chat Completions API
4. Sends the summary via Telegram using Telegram Bot API
5. Runs automatically twice daily via GitHub Actions (8am and 6pm UTC)

**Key Benefit**: No Twitter API required! Extract usernames once, use free RSS feeds for tweets.

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

### Git Hooks Setup
```bash
# Configure git to use the project's hooks (runs fmt, clippy, tests before push)
git config core.hooksPath .githooks
```

### Environment Setup
```bash
# Copy environment template and fill in credentials
cp .env.example .env

# Required environment variables (see .env.example for formats):
# - OPENAI_API_KEY
# - TELEGRAM_BOT_TOKEN (from @BotFather on Telegram)
# - TELEGRAM_CHAT_ID (numeric chat/user ID)
# - NITTER_INSTANCE (self-hosted instance URL, see nitter-selfhost/FLY_IO_SETUP.md)
#
# Optional (with defaults):
# - USERNAMES_FILE (defaults to data/usernames.txt)
# - NITTER_API_KEY (if your Nitter instance requires authentication)
# - OPENAI_MODEL (defaults to gpt-4o-mini)
# - MAX_TWEETS (defaults to 50) - Maximum tweets to return per run
# - HOURS_LOOKBACK (defaults to 12) - Time window for tweet filtering
# - RUST_LOG (defaults to info)
#
# Optional - Only for `make export` command:
# - TWITTER_BEARER_TOKEN (OAuth 2.0 App-Only)
# - TWITTER_LIST_ID (numeric ID from list URL)
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
- `rss.rs` - Nitter RSS feed fetcher (reads from usernames file, main tweet source)
- `openai.rs` - OpenAI chat completions for summarization
- `telegram.rs` - Telegram Bot API messaging
- `twitter.rs` - Twitter API v2 client (only used by export binary, optional)
- `bin/fetch_list_members.rs` - Optional "export" binary to fetch list members via Twitter API

### Execution Flow (src/main.rs)
1. Load configuration from environment variables
2. Fetch tweets from RSS feeds (via `rss::fetch_tweets_from_rss`)
   - Reads usernames from `data/usernames.txt` file
   - Fetches RSS feed for each username from Nitter in parallel
   - Filters to last `HOURS_LOOKBACK` (default 12) hours
   - Returns up to `MAX_TWEETS` (default 50) tweets
3. If tweets exist, generate summary with OpenAI
4. Send summary via Telegram with timestamp header

### RSS Integration (src/rss.rs) - Main Tweet Source
- Reads usernames from `USERNAMES_FILE` (defaults to `data/usernames.txt`)
- Fetches RSS feeds from Nitter instance for each username
- Supports optional API key authentication via `NITTER_API_KEY` for secured instances
- Sends `X-API-Key` header when API key is configured
- Uses sequential fetching with 3s delays to avoid rate limiting
- Parses RSS XML using `rss` crate
- Converts RSS items to `Tweet` struct
- Client-side filtering by `HOURS_LOOKBACK` time window
- Sorts tweets by date (newest first)
- Returns up to `MAX_TWEETS` tweets
- **No API quota limits** - unlimited free fetches

### Twitter Integration (src/twitter.rs) - Optional Export Tool
- **Only used by `make export` binary** (optional)
- Uses Twitter API v2 `/lists/{id}/members` endpoint
- Requires Bearer Token (OAuth 2.0 App-Only authentication)
- Fetches all list members with pagination support
- Returns Vec of usernames
- Export binary saves results to `data/usernames.txt`
- **Not used by main application** - main app reads from file

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

### Extracting List Members

**Option 1: Browser Console Script (Recommended)**
1. Go to your Twitter list page: `https://x.com/i/lists/YOUR_LIST_ID/members`
2. Scroll down to load all members
3. Open DevTools Console (F12)
4. Run this script (see README.md for full version with validation):
```javascript
const usernames = [...document.querySelectorAll('[data-testid^="UserAvatar-Container-"]')]
  .map(el => el.getAttribute('data-testid').replace('UserAvatar-Container-', ''))
  .filter(u => u && u !== 'unknown');
const currentUserElement = document.querySelector('[data-testid="SideNav_AccountSwitcher_Button"] img');
const currentUser = currentUserElement ? currentUserElement.getAttribute('alt').replace('@', '') : null;
const uniqueUsernames = [...new Set(usernames)]
  .filter(u => u !== currentUser)
  .sort();
console.log('Found ' + uniqueUsernames.length + ' users:\n');
console.log(uniqueUsernames.join('\n'));
```
5. Copy output (usernames only) and save to `data/usernames.txt`

**Option 2: Twitter API (Optional)**
- Only needed if you want to use `make export` command
- Requires Bearer Token with read access to lists
- List must be public OR use OAuth 2.0 User Context for private lists
- List ID is numeric and found in URL: `twitter.com/i/lists/{id}`
- Run `make export` to save usernames to file

### Nitter/RSS Setup
- **Self-hosted instance required** (public instances are unreliable)
- See `nitter-selfhost/FLY_IO_SETUP.md` for free hosting on Fly.io
- Set via `NITTER_INSTANCE` environment variable
- RSS feeds may be 5-10 minutes delayed (normal and acceptable)
- Unlimited free fetches (no Twitter API quotas)

**Security (Optional):**
- Protect your Nitter instance with API key authentication
- Generate key: `openssl rand -hex 32`
- Configure your reverse proxy (nginx/Caddy) to require `X-API-Key` header
- Set `NITTER_API_KEY` environment variable to match
- Application automatically sends the key in all RSS requests

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
