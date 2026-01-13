# Twitter News Summary

A Rust application that fetches tweets from a Twitter list via RSS feeds, summarizes them using OpenAI, and sends the summary via Telegram.

**Why This Works:** Twitter's Free API tier is extremely limited (~1 request/month for lists), so this app uses a smarter approach:
1. Extract list members once using a browser console script (free, unlimited)
2. Fetch tweets via Nitter RSS feeds (free, unlimited)
3. No Twitter API credentials needed!

## Features

- 📰 Fetches recent tweets from Twitter list members via RSS
- 🚀 **Avoids Twitter API limits** - uses free Nitter RSS feeds for tweets
- 🔧 **No API required** - extract list members via browser console script
- 🤖 Generates concise summaries using OpenAI GPT
- 📱 Delivers summaries via Telegram
- ⏰ Runs automatically twice daily via GitHub Actions
- 🦀 Written in Rust for reliability and performance

## Prerequisites

### Extract Twitter List Members

**Why Browser Script?** Twitter's Free tier API has severe restrictions:
- The `/lists/{id}/members` endpoint allows only **~1 request per month**
- Not practical for regular use or testing
- The browser script is free, unlimited, and works instantly

**How to Extract List Members (Browser Console Script):**

1. **Find your list ID** from the Twitter list URL:
   - Go to your list on Twitter/X
   - URL format: `https://x.com/i/lists/YOUR_LIST_ID`
   - Example: `https://x.com/i/lists/1990645556884955140` → List ID is `1990645556884955140`

2. **Navigate to the members page:**
   - Go to: `https://x.com/i/lists/YOUR_LIST_ID/members`
   - Replace `YOUR_LIST_ID` with your actual list ID

3. **Load ALL members:**
   - **IMPORTANT:** Scroll down slowly until you see all list members
   - Twitter lazy-loads content, so you must scroll to load everyone
   - You'll know you've reached the end when no more users appear

4. **Open Developer Tools:**
   - Press `F12` (Windows/Linux) or `Cmd+Option+I` (Mac)
   - Or right-click → "Inspect" → click on "Console" tab

5. **Run the extraction script:**
   Paste the entire script into the Console and press Enter:

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
              members.add(text.substring(1)); // Remove @ symbol
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
            members.add(text.substring(1)); // Remove @ symbol
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

6. **Check the output:**
   - The script will automatically copy the usernames to your clipboard
   - You'll see output like:
   ```
   Copied 34 members to clipboard:
   ['AIatMeta', 'AnthropicAI', 'DeepLearningAI', 'LangChain', 'OpenAI', 'PyTorch', ...]
   ```
   - The usernames are now in your clipboard WITHOUT the @ symbol

7. **Save to file:**
   - The usernames are already in your clipboard (copied automatically by the script)
   - Create/open the file `data/usernames.txt` in your project
   - Paste the usernames (Cmd+V or Ctrl+V)
   - They should appear one per line, WITHOUT @ symbols

**Alternative: Twitter API (Not Recommended)**

If you have paid Twitter API access:
1. Create a [Twitter Developer account](https://developer.twitter.com/en/portal/dashboard)
2. Subscribe to a paid tier (Free tier is too restrictive)
3. Generate a Bearer Token
4. Add `TWITTER_BEARER_TOKEN` and `TWITTER_LIST_ID` to .env
5. Run `make export`

**Note:** This is not recommended due to API costs and rate limits. The browser script is free and works better.

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

### Initial Setup

```bash
# Clone the repository
git clone https://github.com/yourusername/twitter-news-summary.git
cd twitter-news-summary

# Copy environment template
cp .env.example .env

# Edit .env with all credentials
vim .env  # or use your preferred editor
```

### Create Usernames File

Create the `data/` directory and add your usernames:

```bash
mkdir -p data

# Paste the output from the browser console script
cat > data/usernames.txt << 'EOF'
# Paste your usernames here, one per line
AnthropicAI
DeepLearningAI
LangChain
OpenAI
PyTorch
huggingface
EOF
```

### Configure Environment Variables

Edit `.env` and fill in all required credentials:

**Required:**
```bash
# Get from https://platform.openai.com/api-keys
OPENAI_API_KEY=sk-proj-abc123...

# Get from @BotFather on Telegram
TELEGRAM_BOT_TOKEN=123456789:ABCdefGHIjklMNOpqrsTUVwxyz

# Get from https://api.telegram.org/bot<YOUR_BOT_TOKEN>/getUpdates (after messaging your bot)
TELEGRAM_CHAT_ID=123456789
```

**Optional (defaults shown):**
```bash
USERNAMES_FILE=data/usernames.txt
NITTER_INSTANCE=https://nitter.net  # Automatically falls back to alternatives if down
OPENAI_MODEL=gpt-4o-mini
MAX_TWEETS=50
HOURS_LOOKBACK=12
RUST_LOG=info
```

**Note on Nitter instances:** The app automatically tests instances and falls back to alternatives if the primary is down. Built-in fallbacks: `nitter.poast.org`, `nitter.privacydev.net`, `nitter.1d4.us`, `nitter.cz`, `nitter.unixfox.eu`

### Testing the Application

1. **Verify your Telegram bot setup:**
   - Open Telegram and find your bot (search for its username)
   - Send any message to your bot (like `/start` or "Hello")
   - This is required - the bot can only send messages to chats that have initiated contact

2. **Run the application:**
   ```bash
   make run
   # Or: cargo run
   ```

3. **Expected output:**
   ```
   INFO twitter_news_summary: Starting Twitter news summary job
   INFO twitter_news_summary: Fetching tweets from RSS feeds
   INFO twitter_news_summary::rss: Loaded 34 usernames from data/usernames.txt
   INFO twitter_news_summary::rss: Testing primary instance: https://nitter.net
   INFO twitter_news_summary::rss: ✓ Found working instance: https://nitter.net
   INFO twitter_news_summary::rss: Using Nitter instance: https://nitter.net
   INFO twitter_news_summary::rss: Fetching RSS feeds for 34 users
   INFO twitter_news_summary::rss: RSS fetch complete: 34 successful, 0 failed
   INFO twitter_news_summary::rss: Filtered to 27 tweets from last 12 hours
   INFO twitter_news_summary: Fetched 27 tweets
   INFO twitter_news_summary: Generating summary with OpenAI
   INFO twitter_news_summary: Sending summary via Telegram
   INFO twitter_news_summary: Summary sent successfully!
   ```

   **If the primary instance is down, you'll see:**
   ```
   INFO twitter_news_summary::rss: Testing primary instance: https://nitter.net
   WARN twitter_news_summary::rss: Primary instance https://nitter.net is not working, trying fallbacks...
   INFO twitter_news_summary::rss: Testing fallback instance: https://nitter.poast.org
   INFO twitter_news_summary::rss: ✓ Found working instance: https://nitter.poast.org
   INFO twitter_news_summary::rss: Using Nitter instance: https://nitter.poast.org
   ```

4. **Check Telegram:**
   - You should receive a message from your bot
   - Format: "📰 **Twitter Summary**" with timestamp and bullet points

### Troubleshooting

**No tweets found:**
- This is normal if there are no tweets in the last 12 hours
- Try increasing `HOURS_LOOKBACK` in .env (e.g., `HOURS_LOOKBACK=24`)
- Check if the list members are actually tweeting

**File not found error:**
- Make sure `data/usernames.txt` exists
- Run the browser console script to extract usernames
- Or use `make export` if you have Twitter API access

**Telegram not receiving:**
- Ensure you've started a conversation with your bot first
- Verify `TELEGRAM_CHAT_ID` matches your user ID from getUpdates
- Check that `TELEGRAM_BOT_TOKEN` is correct

**Nitter RSS errors:**
- If nitter.net is down, try alternative instances:
  - `NITTER_INSTANCE=https://nitter.poast.org`
  - `NITTER_INSTANCE=https://nitter.1d4.us`

### Update List Members

When your Twitter list changes (new members added/removed), update your usernames file:

**Recommended: Browser Console Script**
1. Go to your list's members page again
2. Scroll down to load all members
3. Run the same console script from the Prerequisites section
4. Copy the new output
5. Replace the contents of `data/usernames.txt` with the new list

**Alternative: Twitter API (if you have paid access)**
```bash
# Make sure TWITTER_BEARER_TOKEN and TWITTER_LIST_ID are set in .env
make export    # Updates data/usernames.txt
```

**Tip:** You can update the list as often as you want with the browser script. It's free and takes less than a minute!

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
| `USERNAMES_FILE` | No | `data/usernames.txt` | Path to file containing Twitter usernames |
| `NITTER_INSTANCE` | No | `https://nitter.net` | Nitter instance URL for RSS feeds |
| `OPENAI_API_KEY` | Yes | - | API key from OpenAI |
| `OPENAI_MODEL` | No | `gpt-4o-mini` | OpenAI model to use for summarization |
| `TELEGRAM_BOT_TOKEN` | Yes | - | Bot token from @BotFather |
| `TELEGRAM_CHAT_ID` | Yes | - | Your chat ID or group chat ID |
| `MAX_TWEETS` | No | `50` | Maximum number of tweets to return per run |
| `HOURS_LOOKBACK` | No | `12` | Only fetch tweets from last N hours |
| `RUST_LOG` | No | `info` | Log level (trace, debug, info, warn, error) |
| `TWITTER_BEARER_TOKEN` | No* | - | *Only for `make export` command |
| `TWITTER_LIST_ID` | No* | - | *Only for `make export` command |

## GitHub Actions Setup

1. **Commit your usernames file** to the repository:
   ```bash
   git add data/usernames.txt
   git commit -m "Add Twitter list usernames"
   git push
   ```

2. Go to your repository Settings → Secrets and variables → Actions

3. Add the following **secrets**:
   - `OPENAI_API_KEY`
   - `TELEGRAM_BOT_TOKEN`
   - `TELEGRAM_CHAT_ID`

4. Optionally add **variables** for customization:
   - `USERNAMES_FILE` (default: `data/usernames.txt`)
   - `OPENAI_MODEL` (default: `gpt-4o-mini`)
   - `MAX_TWEETS` (default: `50`)
   - `HOURS_LOOKBACK` (default: `12`)
   - `NITTER_INSTANCE` (default: `https://nitter.net`)

5. The workflow runs at 8am and 6pm UTC. Adjust the cron schedule in `.github/workflows/summarize.yml` as needed.

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

### Browser Script Issues

**Script returns 0 users or very few:**
- Make sure you scrolled down to load all members first
- Twitter lazy-loads members - you must scroll slowly and wait for them to appear
- Try refreshing the page, scrolling all the way down, then run the script again
- Check that you're on the `/members` page, not the main list page

**Found fewer users than expected:**
- The script will warn you: "Missing X members!"
- Keep scrolling down slowly until no new members appear
- Run the script again - the count should increase
- Repeat until "✓ All members loaded!" appears

**Your account is still included in the list:**
- Make sure you edited `MY_USERNAME` in the script with your actual username
- The username should NOT include the @ symbol
- Example: `const MY_USERNAME = 'JorgePasco1';` not `'@JorgePasco1'`
- Check the output - it should say "Your account filtered out: @YourUsername (1 instance)"

**Can't find the list ID:**
- Open your list on Twitter/X
- Look at the URL bar - it should be `https://x.com/i/lists/NUMBERS`
- The numbers are your list ID

**Console script doesn't work:**
- Make sure you're using Chrome, Firefox, Safari, or Edge (modern browsers)
- Check that JavaScript is enabled
- Try refreshing the page and running the script again

### Twitter API Export Errors (if using `make export`)
- **Rate limit exceeded:** Free tier allows ~1 request/month - wait or use browser script
- Ensure your Bearer Token has read access to lists
- Verify the list is public (private lists require OAuth 2.0 User Context)
- Check that `data/usernames.txt` was created successfully

### Nitter/RSS Errors / All RSS Fetches Failed
- **The app automatically tries fallback instances** if the primary is down
- Built-in fallbacks: `nitter.poast.org`, `nitter.privacydev.net`, `nitter.1d4.us`, `nitter.cz`, `nitter.unixfox.eu`
- If ALL instances fail, check https://status.d420.de/ to find currently working instances
- You can update your `.env` file with a working instance:
  ```bash
  NITTER_INSTANCE=https://nitter.WORKING-DOMAIN
  ```
- The log will show which instance is being tested and used:
  ```
  Testing primary instance: https://nitter.net
  Testing fallback instance: https://nitter.poast.org
  ✓ Found working instance: https://nitter.poast.org
  ```
- RSS feeds may be 5-10 minutes delayed (this is normal when working)

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
