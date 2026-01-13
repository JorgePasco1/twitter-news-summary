# Self-Hosting Nitter - Complete Setup Guide

## 🚀 Quick Start - Choose Your Hosting Option

**Need 24/7 hosting for GitHub Actions?** You have FREE options:

| Option | Cost | Setup Time | Difficulty | Best For |
|--------|------|------------|-----------|----------|
| [**Fly.io**](./FLY_IO_SETUP.md) ⚡ | $0 (free credit) | 15 min | Easy | Easiest setup, auto HTTPS |
| [**Oracle Cloud**](./ORACLE_CLOUD_SETUP.md) 🏆 | $0 forever | 60 min | Medium | Truly free, more resources |
| **Local Docker** (this guide) | $0 (electricity) | 5 min | Easiest | Testing, if computer runs 24/7 |

**[📊 Detailed Comparison →](./HOSTING_COMPARISON.md)**

---

## Why Self-Host?

Public Nitter instances have anti-bot protection that blocks automated RSS access. By self-hosting, you'll have:
- Unlimited RSS feed access for your own use
- No rate limits or bot detection
- Complete control over your instance
- **No Twitter API keys needed** - just a regular Twitter account!

## Requirements

✅ **What you need:**
- A free Twitter account (no API plan required!)
- Docker and Docker Compose installed
- Basic terminal knowledge

❌ **What you DON'T need:**
- Twitter API keys
- Developer Console access
- Paid Twitter API tier

## Setup Steps

### Step 1: Create a Twitter Account (if needed)

1. Go to https://twitter.com/i/flow/signup
2. Create a new account (use a throwaway email if you want)
3. **Do NOT enable 2FA** - it makes the token extraction harder
4. Complete the signup process

### Step 2: Get Session Tokens

You need to extract session tokens from your Twitter account. There are two methods:

#### Method A: Browser Automation (Recommended)

```bash
# Clone the Nitter repository to get the session creation tool
git clone https://github.com/zedeus/nitter.git
cd nitter/tools

# Install Python dependencies
pip install -r requirements.txt

# Create session tokens (replace with your credentials)
python3 create_session_browser.py YOUR_USERNAME YOUR_PASSWORD --append ../sessions.jsonl
```

**If you have 2FA enabled:**
```bash
# Get your TOTP secret from your authenticator app settings
python3 create_session_browser.py YOUR_USERNAME YOUR_PASSWORD YOUR_TOTP_SECRET --append ../sessions.jsonl
```

This will create a `sessions.jsonl` file with your credentials.

#### Method B: HTTP Requests (Faster, but may trigger bot detection)

```bash
cd nitter/tools
pip install -r requirements.txt
python3 create_session_curl.py YOUR_USERNAME YOUR_PASSWORD --append ../sessions.jsonl
```

### Step 3: Copy Session File

```bash
# Copy the generated sessions.jsonl to the nitter-selfhost directory
cp nitter/sessions.jsonl /path/to/twitter-news-summary/nitter-selfhost/
```

### Step 4: Start Nitter

```bash
cd /path/to/twitter-news-summary/nitter-selfhost

# Start the containers
docker-compose up -d

# Check logs to verify it's working
docker-compose logs -f nitter
```

You should see output like:
```
nitter  | Config loaded from nitter.conf
nitter  | Connected to Redis
nitter  | Loaded 1 session(s)
nitter  | Starting Nitter on port 8080
```

### Step 5: Test Your Instance

**Web Interface:**
```bash
# Open in browser
open http://localhost:8080

# Or test with curl
curl http://localhost:8080
```

**RSS Feed:**
```bash
# Test RSS feed for a user (e.g., OpenAI)
curl http://localhost:8080/OpenAI/rss
```

You should get actual RSS XML output!

## Update Your App Configuration

Now update your main app to use your local Nitter instance:

Edit `.env`:
```bash
NITTER_INSTANCE=http://localhost:8080
```

Then test your app:
```bash
cd /path/to/twitter-news-summary
make run
```

## Important Notes

### Personal Use Only
- This setup is for **personal use only**
- Don't make your instance public or share it
- Twitter may ban accounts used for large-scale automated access

### Session Token Expiration
- Session tokens can expire after a while
- If RSS feeds stop working, regenerate your session tokens:
  ```bash
  cd nitter/tools
  python3 create_session_browser.py YOUR_USERNAME YOUR_PASSWORD --append ../sessions.jsonl
  ```

### Multiple Accounts (Optional)
- For better reliability, you can add multiple Twitter accounts
- Run the session creation script multiple times with different accounts
- The `--append` flag adds new sessions to existing ones

### Security
- Change the `hmacKey` in `nitter.conf` to a random string
- Don't expose port 8080 publicly (use reverse proxy with auth if needed)
- Keep your `sessions.jsonl` file private

## Troubleshooting

### "Failed to parse RSS"
- Check if Nitter is running: `docker-compose ps`
- Check logs: `docker-compose logs nitter`
- Verify session is loaded: look for "Loaded X session(s)" in logs

### "Cannot connect to Redis"
- Make sure `redisHost = "nitter-redis"` in nitter.conf
- Restart containers: `docker-compose restart`

### "403 Forbidden" or "Rate Limited"
- Your session token may have expired
- Regenerate tokens using the browser automation script
- Consider adding more Twitter accounts for redundancy

### Session Creation Fails
- Make sure you're using the correct username/password
- Try the browser automation method instead of curl method
- Check if Twitter is asking for additional verification

## Maintenance

### Update Nitter
```bash
docker-compose pull
docker-compose up -d
```

### View Logs
```bash
docker-compose logs -f nitter
```

### Stop Nitter
```bash
docker-compose down
```

### Restart Nitter
```bash
docker-compose restart
```

## Alternative: Cloud Hosting

If you don't want to run locally, you can deploy to:
- **Fly.io** (free tier available) - See: https://github.com/sekai-soft/guide-nitter-self-hosting/blob/master/docs/fly-io.md
- **VPS** (DigitalOcean, Linode, etc.)
- **Home server / NAS**

## Resources

- [Official Nitter Repository](https://github.com/zedeus/nitter)
- [Self-Hosting Guide](https://github.com/sekai-soft/guide-nitter-self-hosting)
- [Creating Session Tokens Wiki](https://github.com/zedeus/nitter/wiki/Creating-session-tokens)
- [Docker Compose Guide](https://github.com/zedeus/nitter/blob/master/docker-compose.yml)

## Summary

**The Bottom Line:**
1. ✅ No Twitter API keys needed
2. ✅ Just a free Twitter account
3. ✅ Simple Docker setup
4. ✅ Unlimited RSS access for personal use
5. ✅ Works perfectly with your news summary app

Once set up, your app will fetch RSS feeds from `http://localhost:8080` with zero restrictions!
