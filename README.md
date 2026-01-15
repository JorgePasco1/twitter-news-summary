# Twitter News Summary Bot

A Rust-based Telegram bot that fetches tweets from Twitter lists via RSS feeds, summarizes them using OpenAI, and delivers personalized summaries to subscribers on a schedule.

**🤖 Try it:** [https://t.me/twitter_news_summary_bot](https://t.me/twitter_news_summary_bot)

**Why This Works:** Twitter's Free API tier is extremely limited (~1 request/month for lists), so this bot uses a smarter approach:
1. Extract list members once using a browser console script (free, unlimited)
2. Fetch tweets via self-hosted Nitter RSS feeds (free, unlimited)
3. Summarize with OpenAI and deliver via Telegram
4. No Twitter API credentials needed!

## Features

- 🤖 **Telegram Bot Subscription Service** - Users can subscribe/unsubscribe via bot commands
- 📰 Fetches recent tweets from Twitter list members via RSS
- 🚀 **Avoids Twitter API limits** - uses self-hosted Nitter RSS feeds
- 🔧 **No Twitter API required** - extract list members via browser console script
- 🤖 Generates concise summaries using OpenAI GPT
- 📱 Delivers summaries to all subscribers via Telegram
- ⏰ Runs automatically twice daily (customizable schedule)
- 🔒 Secure webhook authentication with secret tokens
- 💾 PostgreSQL database for subscriber management (hosted on Neon.tech)
- 🦀 Written in Rust for reliability and performance
- 🚢 Deployed on Fly.io
- 🔄 CI/CD pipeline with GitHub Actions

## Architecture

**Service Mode (Current):**
```
┌─────────────────────────────────────────────────┐
│              Fly.io (All-in-One)                │
│                                                 │
│  ┌───────────────────────────────────────────┐ │
│  │  Twitter Summary Bot Service              │ │
│  │                                           │ │
│  │  • Telegram Webhook Handler              │ │
│  │  • Scheduler (8am & 8pm Peru time)       │ │
│  │  • PostgreSQL Database (Neon.tech)        │ │
│  │  • RSS Fetcher → OpenAI → Telegram       │ │
│  └───────────────────────────────────────────┘ │
│                                                 │
│  ┌───────────────────────────────────────────┐ │
│  │  Self-hosted Nitter Instance              │ │
│  │  (RSS feeds with API key auth)            │ │
│  └───────────────────────────────────────────┘ │
└─────────────────────────────────────────────────┘
```

## Bot Commands

| Command | Description |
|---------|-------------|
| `/start` | Welcome message and help |
| `/subscribe` | Subscribe to receive summaries |
| `/unsubscribe` | Unsubscribe from summaries |
| `/status` | Check your subscription status |

**Admin-only features:**
- See total subscriber count in `/status`
- Receive notifications when message delivery fails

## Quick Start

### 1. Self-Host Nitter Instance

**⚠️ REQUIRED:** You must self-host your own Nitter instance for reliable RSS access.

| Option | Cost | Setup Time | Best For |
|--------|------|------------|----------|
| [**Fly.io**](./nitter-selfhost/FLY_IO_SETUP.md) ⚡ | $0 (free credit) | 15 min | Easiest, automatic HTTPS |
| [**Oracle Cloud**](./nitter-selfhost/ORACLE_CLOUD_SETUP.md) 🏆 | $0 forever | 60 min | Truly free, more resources |

See [nitter-selfhost/README.md](./nitter-selfhost/README.md) for setup instructions.

### 2. Extract Twitter List Members

Use the browser console script to extract your Twitter list members (one-time):

1. Go to `https://x.com/i/lists/YOUR_LIST_ID/members`
2. Scroll down to load all members
3. Open Developer Tools (F12) → Console tab
4. Run this script:

```javascript
(async function() {
  const regions = document.querySelectorAll('[role="region"]');
  let listRegion = null;

  for (let reg of regions) {
    if (reg.textContent.includes('List members')) {
      listRegion = reg;
      break;
    }
  }

  if (listRegion) {
    let scrollable = listRegion;
    while (scrollable && scrollable.scrollHeight === scrollable.clientHeight) {
      scrollable = scrollable.parentElement;
    }

    if (scrollable && scrollable.scrollHeight > scrollable.clientHeight) {
      const members = new Set();

      scrollable.scrollTop = 0;
      await new Promise(r => setTimeout(r, 200));

      const step = 200;
      let pos = 0;

      while (pos < scrollable.scrollHeight) {
        scrollable.scrollTop = pos;
        await new Promise(r => setTimeout(r, 150));

        const buttons = listRegion.querySelectorAll('button');
        buttons.forEach(btn => {
          const links = btn.querySelectorAll('a');
          if (links.length >= 3) {
            const handleLink = links[2];
            const text = handleLink.textContent.trim();
            if (text.startsWith('@')) {
              members.add(text.substring(1));
            }
          }
        });

        pos += step;
      }

      scrollable.scrollTop = scrollable.scrollHeight;
      await new Promise(r => setTimeout(r, 200));

      const buttons = listRegion.querySelectorAll('button');
      buttons.forEach(btn => {
        const links = btn.querySelectorAll('a');
        if (links.length >= 3) {
          const handleLink = links[2];
          const text = handleLink.textContent.trim();
          if (text.startsWith('@')) {
            members.add(text.substring(1));
          }
        }
      });

      const memberList = Array.from(members).sort();
      const text = memberList.join('\\n');
      navigator.clipboard.writeText(text);
      console.log('Copied ' + memberList.length + ' members to clipboard:');
      console.log(memberList);
    }
  }
})();
```

5. Paste the output into `data/usernames.txt` (one username per line)
6. Commit the file: `git add data/usernames.txt && git commit -m "Add usernames"`

### 3. Deploy to Fly.io

```bash
# Install Fly CLI
curl -L https://fly.io/install.sh | sh

# Login to Fly.io
flyctl auth login

# Create app
flyctl launch

# Generate secrets
openssl rand -hex 32  # For TELEGRAM_WEBHOOK_SECRET
openssl rand -hex 32  # For API_KEY

# Set secrets
flyctl secrets set \
  TELEGRAM_BOT_TOKEN=<your_bot_token> \
  TELEGRAM_WEBHOOK_SECRET=<generated_webhook_secret> \
  TELEGRAM_CHAT_ID=<your_chat_id> \
  OPENAI_API_KEY=<your_openai_key> \
  NITTER_INSTANCE=https://your-nitter-instance.fly.dev \
  NITTER_API_KEY=<your_nitter_api_key> \
  API_KEY=<generated_api_key> \
  DATABASE_URL=<your_postgresql_connection_string>

# Deploy
flyctl deploy

# Configure Telegram webhook
curl -X POST "https://api.telegram.org/bot<YOUR_BOT_TOKEN>/setWebhook" \
  -H "Content-Type: application/json" \
  -d '{
    "url": "https://your-app.fly.dev/webhook",
    "secret_token": "<your_webhook_secret>"
  }'
```

### 4. GitHub Actions CI/CD (Optional)

Add `FLY_API_TOKEN` to GitHub Secrets for automatic deployments:

```bash
# Get your Fly API token
flyctl tokens create

# Add to GitHub: Settings → Secrets → Actions → New secret
# Name: FLY_API_TOKEN
# Value: <your token>
```

Every push to `main` will automatically deploy after tests pass.

## Configuration

### Environment Variables

**Required:**
```bash
TELEGRAM_BOT_TOKEN=<from @BotFather>
TELEGRAM_WEBHOOK_SECRET=<generate with: openssl rand -hex 32>
TELEGRAM_CHAT_ID=<your chat ID for admin notifications>
OPENAI_API_KEY=<from platform.openai.com>
NITTER_INSTANCE=https://your-nitter-instance.fly.dev
DATABASE_URL=<PostgreSQL connection string, e.g., from Neon.tech>
```

**Optional:**
```bash
NITTER_API_KEY=<if your Nitter instance requires auth>
API_KEY=<for /trigger and /subscribers endpoints>
OPENAI_MODEL=gpt-4o-mini
MAX_TWEETS=50
HOURS_LOOKBACK=12
SCHEDULE_TIMES=08:00,20:00  # Peru time (UTC-5)
PORT=8080
RUST_LOG=info
```

### Schedule Customization

The bot runs at **8:00 AM and 8:00 PM Peru time (UTC-5)** by default.

To change the schedule:
```bash
flyctl secrets set SCHEDULE_TIMES=07:00,19:00 --app your-app-name
```

Times are in **Peru timezone (UTC-5)** and automatically converted to UTC for the scheduler.

## API Endpoints

| Endpoint | Method | Auth | Description |
|----------|--------|------|-------------|
| `/health` | GET | None | Health check |
| `/webhook` | POST | Telegram Secret | Telegram webhook handler |
| `/trigger` | POST | API Key | Manually trigger summary |
| `/subscribers` | GET | API Key | List subscribers (admin) |

**Manual trigger example:**
```bash
curl -X POST https://your-app.fly.dev/trigger \
  -H "X-API-Key: your_api_key"
```

## Local Development

### Setup

```bash
# Clone repository
git clone https://github.com/yourusername/twitter-news-summary.git
cd twitter-news-summary

# Copy environment template
cp .env.example .env

# Edit .env with your credentials
vim .env

# Create usernames file
mkdir -p data
# Paste usernames from browser script into data/usernames.txt
```

### Run Locally

```bash
# Run the service
cargo run

# Or use make
make run

# Run tests
cargo test
```

The service will start on `http://localhost:8080`. You can use ngrok to expose the webhook for Telegram:

```bash
ngrok http 8080
# Update webhook URL to ngrok URL
```

## Project Structure

```
├── .github/workflows/
│   └── deploy.yml           # CI/CD pipeline
├── src/
│   ├── main.rs              # Server setup & orchestration
│   ├── config.rs            # Environment configuration
│   ├── db.rs                # PostgreSQL database layer (async sqlx)
│   ├── scheduler.rs         # Cron scheduler
│   ├── telegram.rs          # Webhook handler & messaging
│   ├── rss.rs               # RSS feed fetcher
│   ├── openai.rs            # OpenAI summarization
│   ├── twitter.rs           # Twitter API (optional export)
│   └── security.rs          # Constant-time comparison
├── data/
│   └── usernames.txt        # Twitter list members
├── nitter-selfhost/         # Nitter deployment guides
├── Cargo.toml
├── Dockerfile
├── fly.toml
└── README.md
```

## Security

- 🔒 **Telegram webhook secret** verification (required)
- 🔒 **Constant-time comparison** for all secrets (prevents timing attacks)
- 🔒 **API key authentication** for admin endpoints
- 🔒 **Subscriber privacy** - only admin sees total count
- 🔒 **Database encryption** via PostgreSQL SSL (Neon.tech)

See [SECURITY.md](./SECURITY.md) for security best practices.

## Troubleshooting

### Deployment Issues

**Cargo.lock not found:**
- Make sure `Cargo.lock` is committed (not in `.gitignore`)

**Database connection errors:**
- Verify `DATABASE_URL` is set correctly in Fly.io secrets
- Check PostgreSQL host is accessible (Neon.tech status)

### Bot Not Responding

**Webhook not working:**
- Verify webhook is set: `curl https://api.telegram.org/bot<TOKEN>/getWebhookInfo`
- Check webhook secret matches in both Fly.io and Telegram
- View logs: `flyctl logs --app your-app-name`

**Commands not recognized:**
- Make sure you've started a conversation with the bot first
- Send `/start` to initialize

### RSS/Nitter Issues

**All RSS fetches fail:**
- Test your Nitter instance: `curl https://your-instance/OpenAI/rss`
- Check if it's running: `flyctl status --app your-nitter-app`
- View Nitter logs: `flyctl logs --app your-nitter-app`

**API key errors:**
- Verify `NITTER_API_KEY` matches your Nitter instance configuration
- Check that your Nitter instance is configured to require the key

### Scheduler Not Running

**No summaries at scheduled time:**
- Check logs: `flyctl logs --app your-app-name`
- Verify schedule: Look for "Scheduling job for XX:XX" in startup logs
- Test manually: `curl -X POST https://your-app.fly.dev/trigger -H "X-API-Key: your_key"`

## Development

**Available commands:**
```bash
make help        # Show all commands
make run         # Run the service
make build       # Build release binary
make test        # Run tests
make check       # Check code
make clean       # Clean build artifacts
```

**Running tests:**
```bash
cargo test --all-features
# 213 tests covering all modules
```

## Deployment

**GitHub Actions automatically deploys on push to `main`:**

1. Tests run first
2. If tests pass, deploys to Fly.io
3. Zero-downtime rolling updates
4. Documentation changes don't trigger deployment

**Manual deployment:**
```bash
flyctl deploy --app your-app-name
```

## Contributing

Contributions are welcome! Please:
1. Run tests: `cargo test --all-features`
2. Format code: `cargo fmt`
3. Check lints: `cargo clippy`

## License

MIT
