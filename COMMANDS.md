# Makefile Commands Reference

This document provides detailed information about all available `make` commands for the Twitter News Summary project.

## Quick Reference

```bash
make help           # Show all available commands
make export         # Export Twitter list members (one-time)
make run            # Run the summary job locally
make preview        # Preview summary without sending (fetches tweets, saves cache)
make preview-cached # Preview with cached tweets (fast iteration)
make trigger        # Trigger summary on Fly.io production
make build          # Build release binary
make check          # Check code without building
make test           # Run all tests
make clean          # Clean build artifacts
```

---

## Development Commands

### `make export`

**Purpose**: Export Twitter list members to `data/usernames.txt`

**When to use**: One-time setup to fetch list members from Twitter API

**Requirements**:
- `TWITTER_BEARER_TOKEN` in `.env` (OAuth 2.0 App-Only token)
- `TWITTER_LIST_ID` in `.env` (numeric ID from list URL)

**Example**:
```bash
make export
```

**Output**: Creates/updates `data/usernames.txt` with list members

**Note**: This is optional! You can also use the browser console script (see README.md) to avoid needing Twitter API credentials.

---

### `make run`

**Purpose**: Run the news summary job locally in service mode

**When to use**:
- Testing the full application flow locally
- Running a local web server with scheduler
- Development and debugging

**Requirements**:
- All environment variables in `.env` (see `.env.example`)
- `DATABASE_URL` pointing to PostgreSQL database
- `data/usernames.txt` with Twitter usernames

**Example**:
```bash
make run
```

**What it does**:
1. Starts web server on port specified by `PORT` (default: 8080)
2. Starts scheduler for times in `SCHEDULE_TIMES`
3. Provides `/health`, `/webhook`, `/trigger`, `/subscribers` endpoints

**Endpoints available**:
- `GET /health` - Health check
- `POST /webhook` - Telegram webhook handler
- `POST /trigger` - Manual summary trigger (requires `X-API-Key` header)
- `GET /subscribers` - List subscribers (requires `X-API-Key` header)

---

### `make preview`

**Purpose**: Generate and display a summary preview without sending to Telegram

**When to use**:
- Testing summary generation
- Previewing output format
- Checking OpenAI integration
- Validating tweet filtering logic

**Requirements**:
- `OPENAI_API_KEY` in `.env`
- `NITTER_INSTANCE` in `.env`
- `data/usernames.txt` with Twitter usernames

**Example**:
```bash
make preview
```

**Output**:
- Prints the formatted summary to stdout (MarkdownV2 format)
- Saves summary to `run-history/{timestamp}.md`
- Saves tweets to `run-history/tweets_cache.json` for reuse

**Tip**: Use this before `make run` to validate your summary looks good!

---

### `make preview-cached`

**Purpose**: Generate summary using previously cached tweets (fast iteration on formatting)

**When to use**:
- Iterating on OpenAI prompt changes
- Testing message formatting changes
- Quick iteration without waiting for tweet fetches
- Experimenting with different summary styles

**Requirements**:
- Previous run of `make preview` (to generate the cache)
- `OPENAI_API_KEY` in `.env`

**Example**:
```bash
# First run: fetch tweets and cache them
make preview

# Subsequent runs: use cached tweets (much faster!)
make preview-cached
```

**Advanced usage**:
```bash
# You can also use the flag directly
cargo run --bin preview -- --use-cached
```

**Benefits**:
- ⚡ **Much faster** - No RSS fetching delay (saves ~30-60 seconds)
- 💰 **Saves Nitter API calls** - Uses previously fetched tweets
- 🔄 **Perfect for iteration** - Test prompt changes without re-fetching
- 📊 **Consistent baseline** - Same tweets = easier comparison

**Output**: Same as `make preview` but uses cached tweets instead of fresh fetch

**Tip**: Great for A/B testing different OpenAI prompts or formatting styles!

---

### `make build`

**Purpose**: Build optimized release binary

**When to use**:
- Creating production-ready binary
- Before deployment
- Performance testing

**Example**:
```bash
make build
```

**Output**: Creates `target/release/twitter-news-summary` binary

**Build optimizations** (see `Cargo.toml`):
- Size optimization (`opt-level = "z"`)
- Link-time optimization (`lto = true`)
- Stripped debug symbols (`strip = true`)

---

### `make check`

**Purpose**: Type-check and validate code without building

**When to use**:
- Quick validation during development
- Pre-commit checks
- Faster than full build

**Example**:
```bash
make check
```

**Tip**: Much faster than `make build` for catching errors!

---

### `make test`

**Purpose**: Run all unit and integration tests

**When to use**:
- Before committing changes
- After modifying core logic
- Validating bug fixes

**Example**:
```bash
make test
```

**Test coverage includes**:
- Config validation
- RSS feed parsing
- Telegram message formatting
- Database operations
- Webhook handling
- Auto-removal of blocked subscribers

---

### `make clean`

**Purpose**: Remove all build artifacts and cached dependencies

**When to use**:
- Fixing build issues
- Freeing disk space
- Fresh rebuild needed

**Example**:
```bash
make clean
```

**What it removes**:
- `target/` directory
- Compiled binaries
- Build cache

---

## Production Commands

### `make trigger`

**Purpose**: Manually trigger a summary on the Fly.io production instance

**When to use**:
- Testing production deployment
- Sending an immediate summary to subscribers
- Recovering from a failed scheduled run
- Testing after configuration changes

**Requirements**:
- `API_KEY` in `.env` file OR exported as environment variable
- Production app running on Fly.io at `https://twitter-summary-bot.fly.dev`

**Example**:
```bash
# Using API_KEY from .env file
make trigger

# Or with explicit API_KEY
API_KEY=your_key_here make trigger
```

**What it does**:
1. Reads `API_KEY` from `.env` or environment
2. Sends POST request to `https://twitter-summary-bot.fly.dev/trigger`
3. Waits for response (takes ~2-3 minutes)
4. Displays success or error message

**Expected output**:
```
🚀 Triggering summary on Fly.io...
Summary sent
```

**Troubleshooting**:
- `401 Unauthorized` - Invalid or missing API_KEY
- `500 Internal Server Error` - Check Fly.io logs: `flyctl logs --app twitter-summary-bot`

**Security note**: The API_KEY is transmitted via HTTPS header (`X-API-Key`) and validated server-side using constant-time comparison.

---

## Environment Variables

All commands that interact with external services require environment variables. See `.env.example` for a complete template.

### Core Variables (Required)
```bash
OPENAI_API_KEY=sk-...              # OpenAI API key
NITTER_INSTANCE=https://...        # Your self-hosted Nitter instance
TELEGRAM_BOT_TOKEN=123:ABC...      # Telegram bot token (from @BotFather)
DATABASE_URL=postgres://...        # PostgreSQL connection string
```

### Service Mode Variables (Required for `make run` and `make trigger`)
```bash
API_KEY=...                        # API key for /trigger endpoint
TELEGRAM_WEBHOOK_SECRET=...        # Webhook secret for security
SCHEDULE_TIMES=08:00,20:00        # Comma-separated HH:MM times
PORT=8080                          # Server port
```

### Optional Variables
```bash
NITTER_API_KEY=...                 # API key for secured Nitter instance
OPENAI_MODEL=gpt-4o-mini          # OpenAI model (default: gpt-4o-mini)
MAX_TWEETS=50                      # Max tweets per run (default: 50)
HOURS_LOOKBACK=12                  # Time window in hours (default: 12)
RUST_LOG=info                      # Log level (default: info)
```

### Twitter API Variables (Optional - only for `make export`)
```bash
TWITTER_BEARER_TOKEN=...           # OAuth 2.0 App-Only token
TWITTER_LIST_ID=...                # Numeric list ID
```

---

## Common Workflows

### Initial Setup
```bash
# 1. Copy environment template
cp .env.example .env

# 2. Edit .env with your credentials
# (Add OPENAI_API_KEY, NITTER_INSTANCE, TELEGRAM_BOT_TOKEN, etc.)

# 3. Export list members (choose one method)
make export                        # Using Twitter API
# OR use browser console script (see README.md)

# 4. Preview the summary
make preview

# 5. Run locally to test
make run
```

### Development Cycle
```bash
# Make code changes...

# Check for errors
make check

# Run tests
make test

# Preview output
make preview

# Test full flow locally
make run
```

### Iterating on Formatting/Prompts
```bash
# First run: fetch tweets and cache them
make preview

# Iterate on formatting changes (much faster!)
# Edit src/openai.rs or src/telegram.rs...
make preview-cached  # Uses cached tweets from previous run

# Keep iterating until satisfied
make preview-cached
make preview-cached

# When ready, test with fresh data
make preview
```

### Deployment Cycle
```bash
# Build release binary
make build

# Run tests one more time
make test

# Deploy to Fly.io
flyctl deploy --app twitter-summary-bot

# Trigger manually to test
make trigger

# Monitor logs
flyctl logs --app twitter-summary-bot
```

### Troubleshooting Production
```bash
# Check Fly.io logs
flyctl logs --app twitter-summary-bot --no-tail

# Trigger manual summary
make trigger

# Check subscriber list (requires API_KEY)
curl -H "X-API-Key: $API_KEY" https://twitter-summary-bot.fly.dev/subscribers

# Restart app if needed
flyctl apps restart twitter-summary-bot
```

---

## Tips and Best Practices

### Before Committing
```bash
make check    # Fast validation
make test     # Run all tests
make preview  # Verify output format
```

### Testing Changes Locally
```bash
make preview  # Test without sending messages
make run      # Test full flow with local database
```

### Production Testing
```bash
make trigger  # Manual trigger to test live system
```

### Cleaning Up
```bash
make clean    # When build cache gets corrupted
rm -f twitter_news_summary.db  # Remove local SQLite if created
```

### Performance Optimization
- Use `make check` instead of `make build` for faster feedback
- Use `make preview` to test without database/Telegram overhead
- Cache Cargo dependencies in CI/CD for faster builds

---

## Error Messages

### "API_KEY not found in environment or .env file"
**Solution**: Add `API_KEY=your_key_here` to your `.env` file

### "Failed to trigger summary"
**Possible causes**:
- Invalid API_KEY (check `.env` file)
- Fly.io app not running (check `flyctl status --app twitter-summary-bot`)
- Network connectivity issues

### "TWITTER_BEARER_TOKEN not set"
**Solution**: Only needed for `make export`. Either:
1. Add `TWITTER_BEARER_TOKEN` to `.env`, OR
2. Use browser console script instead (see README.md)

### "No tweets found in the last X hours"
**Normal behavior** when:
- No tweets in the time window (adjust `HOURS_LOOKBACK`)
- Nitter instance has delays (RSS feeds can lag 5-10 minutes)
- Usernames file is empty

---

## See Also

- **README.md** - Project overview and setup guide
- **CLAUDE.md** - AI assistant guidance for this project
- **.env.example** - Complete environment variable reference
- **Cargo.toml** - Rust dependencies and build configuration
- **SECURITY.md** - Security considerations and best practices

---

## Contributing

When adding new Makefile commands:

1. Add to `.PHONY` declaration
2. Add help text in `make help`
3. Document in this file with:
   - Purpose
   - When to use
   - Requirements
   - Examples
   - Expected output

Keep commands simple, well-documented, and following the existing patterns!
