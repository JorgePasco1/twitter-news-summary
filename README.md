# Twitter News Summary

A Rust application that fetches tweets from a Twitter list, summarizes them using OpenAI, and sends the summary via WhatsApp (Twilio).

## Features

- 📰 Fetches recent tweets from any Twitter list
- 🤖 Generates concise summaries using OpenAI GPT
- 📱 Delivers summaries via WhatsApp
- ⏰ Runs automatically twice daily via GitHub Actions
- 🦀 Written in Rust for reliability and performance

## Prerequisites

### Twitter API
1. Create a [Twitter Developer account](https://developer.twitter.com/en/portal/dashboard)
2. Create a project and app
3. Generate a Bearer Token (OAuth 2.0 App-Only)
4. Find your list ID from the list URL: `twitter.com/i/lists/[LIST_ID]`

### OpenAI API
1. Create an [OpenAI account](https://platform.openai.com)
2. Generate an [API key](https://platform.openai.com/api-keys)

### Twilio WhatsApp
1. Create a [Twilio account](https://console.twilio.com)
2. Set up the [WhatsApp Sandbox](https://console.twilio.com/us1/develop/sms/try-it-out/whatsapp-learn) for testing
3. Send the join code from your phone to activate
4. Note your Account SID, Auth Token, and sandbox number

## Local Development

```bash
# Clone the repository
git clone https://github.com/yourusername/twitter-news-summary.git
cd twitter-news-summary

# Copy environment template
cp .env.example .env

# Edit .env with your credentials
vim .env

# Build and run
cargo run
```

## GitHub Actions Setup

1. Go to your repository Settings → Secrets and variables → Actions

2. Add the following **secrets**:
   - `TWITTER_BEARER_TOKEN`
   - `TWITTER_LIST_ID`
   - `OPENAI_API_KEY`
   - `TWILIO_ACCOUNT_SID`
   - `TWILIO_AUTH_TOKEN`
   - `TWILIO_WHATSAPP_FROM` (format: `whatsapp:+14155238886`)
   - `WHATSAPP_TO` (format: `whatsapp:+1234567890`)

3. Optionally add a **variable**:
   - `OPENAI_MODEL` (default: `gpt-4o-mini`)

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
│   └── whatsapp.rs         # Twilio WhatsApp client
├── .env.example            # Environment template
├── Cargo.toml              # Dependencies
└── README.md
```

## Troubleshooting

### Twitter API Errors
- Ensure your Bearer Token has read access to lists
- Verify the list is public, or use OAuth 2.0 User Context for private lists

### WhatsApp Not Receiving Messages
- Confirm you've joined the Twilio sandbox by sending the join code
- Sandbox sessions expire after 72 hours of inactivity

### GitHub Actions Not Running
- Check the Actions tab for any errors
- Scheduled workflows may be disabled on forked repos
- Manual trigger with "Run workflow" button to test

## License

MIT
