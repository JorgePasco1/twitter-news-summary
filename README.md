# Twitter News Summary

A Rust application that fetches tweets from a Twitter list via RSS feeds, summarizes them using OpenAI, and sends the summary via Telegram.

**Hybrid Approach**: One-time Twitter API export + ongoing RSS fetching (no API quota limits!)

## Features

- 📰 Fetches recent tweets from Twitter list members via RSS
- 🚀 **No Twitter API quota limits** - uses free Nitter RSS feeds
- 🤖 Generates concise summaries using OpenAI GPT
- 📱 Delivers summaries via Telegram
- ⏰ Runs automatically twice daily via GitHub Actions
- 🦀 Written in Rust for reliability and performance
- 💰 **Free forever** - only uses Twitter API once to export list members

## Prerequisites

### Twitter API (One-Time Setup Only)
1. Create a [Twitter Developer account](https://developer.twitter.com/en/portal/dashboard)
2. Create a project and app
3. Generate a Bearer Token (OAuth 2.0 App-Only)
4. Find your list ID from the list URL: `twitter.com/i/lists/[LIST_ID]`

**Note**: You only need the Twitter API to export your list members once. After that, the app uses free RSS feeds!

### OpenAI API
1. Create an [OpenAI account](https://platform.openai.com)
2. Generate an [API key](https://platform.openai.com/api-keys)

### Telegram Bot
1. Open Telegram and search for [@BotFather](https://t.me/BotFather)
2. Send `/newbot` and follow the prompts to create a new bot
3. Save the bot token provided by BotFather
4. Start a chat with your new bot and send any message
5. Get your chat ID by visiting: `https://api.telegram.org/bot<YOUR_BOT_TOKEN>/getUpdates`
6. Look for the `"chat":{"id":123456789}` in the response

## Local Development

### Step 1: One-Time List Export

```bash
# Clone the repository
git clone https://github.com/yourusername/twitter-news-summary.git
cd twitter-news-summary

# Copy environment template
cp .env.example .env

# Edit .env with Twitter credentials (for one-time export)
vim .env
# Add: TWITTER_BEARER_TOKEN and TWITTER_LIST_ID

# Export list members to data/usernames.txt
make export
# Or: cargo run --bin export
```

### Step 2: Configure and Run

```bash
# Add remaining credentials to .env
vim .env
# Add: OPENAI_API_KEY, TELEGRAM_BOT_TOKEN, TELEGRAM_CHAT_ID
# Optionally: NITTER_INSTANCE, MAX_TWEETS, HOURS_LOOKBACK

# You can now remove TWITTER_BEARER_TOKEN and TWITTER_LIST_ID from .env

# Run the summary job
make run
# Or: cargo run
```

### Available Commands

```bash
make help        # Show all available commands
make export      # Export Twitter list members (one-time)
make run         # Run the news summary job
make build       # Build release binary
make check       # Check code without building
make test        # Run tests
make clean       # Clean build artifacts
```

### Configuration Options

| Variable | Required | Default | Description |
|----------|----------|---------|-------------|
| `TWITTER_BEARER_TOKEN` | One-time only | - | OAuth 2.0 Bearer Token (only for list export) |
| `TWITTER_LIST_ID` | One-time only | - | Numeric list ID (only for list export) |
| `NITTER_INSTANCE` | No | `https://nitter.net` | Nitter instance URL for RSS feeds |
| `USERNAMES_FILE` | No | `data/usernames.txt` | Path to exported usernames file |
| `OPENAI_API_KEY` | Yes | - | API key from OpenAI |
| `OPENAI_MODEL` | No | `gpt-4o-mini` | OpenAI model to use for summarization |
| `TELEGRAM_BOT_TOKEN` | Yes | - | Bot token from @BotFather |
| `TELEGRAM_CHAT_ID` | Yes | - | Your chat ID or group chat ID |
| `MAX_TWEETS` | No | `50` | Maximum number of tweets to return per run |
| `HOURS_LOOKBACK` | No | `12` | Only fetch tweets from last N hours |
| `RUST_LOG` | No | `info` | Log level (trace, debug, info, warn, error) |

## GitHub Actions Setup

### Important: First Export List Members Locally

Before setting up GitHub Actions, you must:
1. Run `make export` (or `cargo run --bin export`) locally
2. Commit the generated `data/usernames.txt` file to your repository

### Configure GitHub Actions

1. Go to your repository Settings → Secrets and variables → Actions

2. Add the following **secrets**:
   - `OPENAI_API_KEY`
   - `TELEGRAM_BOT_TOKEN`
   - `TELEGRAM_CHAT_ID`

   **Note**: Twitter credentials are NOT needed for GitHub Actions!

3. Optionally add **variables** for customization:
   - `OPENAI_MODEL` (default: `gpt-4o-mini`)
   - `MAX_TWEETS` (default: `50`)
   - `HOURS_LOOKBACK` (default: `12`)
   - `NITTER_INSTANCE` (default: `https://nitter.net`)

4. The workflow runs at 8am and 6pm UTC. Adjust the cron schedule in `.github/workflows/summarize.yml` as needed.

## Schedule Customization

Edit the cron expressions in `.github/workflows/summarize.yml`:

```yaml
on:
  schedule:
    # Format: minute hour day month weekday
    - cron: '0 8 * * *'   # 8:00 AM UTC
    - cron: '0 18 * * *'  # 6:00 PM UTC
```

Use [crontab.guru](https://crontab.guru) to generate custom schedules.

## Project Structure

```
├── .github/
│   └── workflows/
│       └── summarize.yml   # GitHub Actions workflow
├── src/
│   ├── main.rs             # Entry point
│   ├── config.rs           # Environment configuration
│   ├── twitter.rs          # Twitter API client
│   ├── openai.rs           # OpenAI summarization
│   └── telegram.rs         # Telegram Bot client
├── .env.example            # Environment template
├── Cargo.toml              # Dependencies
└── README.md
```

## Troubleshooting

### List Export Errors
- Ensure your Bearer Token has read access to lists
- Verify the list is public (private lists may require OAuth 2.0 User Context)
- Check that `data/usernames.txt` was created successfully

### Nitter/RSS Errors
- If Nitter instance is down, try a different one: `https://nitter.poast.org`, `https://nitter.1d4.us`
- Some instances may rate-limit; switch to another instance in your `.env`
- RSS feeds may be 5-10 minutes delayed (this is normal)

### Telegram Not Receiving Messages
- Ensure you've started a chat with your bot (send any message first)
- Verify the bot token is correct and active
- Check that the chat ID matches your conversation with the bot
- For group chats, make sure the bot is added as a member

### GitHub Actions Not Running
- Check the Actions tab for any errors
- Scheduled workflows may be disabled on forked repos
- Manual trigger with "Run workflow" button to test

## License

MIT
