# Deploy Nitter to Fly.io (Easiest Option)

## Why Fly.io?

- ✅ **$5/month credit forever** (recurring, not one-time!)
- ✅ Super easy deployment (literally 3 commands)
- ✅ Built-in TLS/SSL certificates
- ✅ Global CDN and load balancing
- ✅ Automatic health checks and restarts

**Cost:**
- Nitter needs ~256MB RAM = $1.94/month
- Your $5 credit covers this with $3.06 to spare!
- **Net cost: $0/month** ✅

## Prerequisites

- Credit card (for $5/month credit allocation)
- 10-15 minutes for setup

## Step 1: Install Fly CLI

**macOS:**
```bash
brew install flyctl
```

**Linux:**
```bash
curl -L https://fly.io/install.sh | sh
```

**Windows:**
```powershell
iwr https://fly.io/install.ps1 -useb | iex
```

## Step 2: Sign Up and Authenticate

```bash
# Sign up (opens browser)
flyctl auth signup

# Or login if you already have an account
flyctl auth login
```

Follow the prompts to:
1. Create your account
2. Add payment method (for $5/month credit)
3. Verify email

## Step 3: Create Session Tokens

**On your local machine:**

```bash
# Clone Nitter to get session creation tool
git clone https://github.com/zedeus/nitter.git
cd nitter/tools

# Install dependencies
pip install -r requirements.txt

# Create session tokens (replace with YOUR Twitter credentials)
python3 create_session_browser.py YOUR_USERNAME YOUR_PASSWORD --append ../sessions.jsonl

# Verify it was created
ls -la ../sessions.jsonl
```

## Step 4: Prepare Deployment Files

```bash
# Go back to twitter-news-summary project
cd /path/to/twitter-news-summary

# Create fly deployment directory
mkdir -p nitter-fly && cd nitter-fly

# Copy necessary files
cp ../nitter-selfhost/docker-compose.yml .
cp ../nitter-selfhost/nitter.conf .
cp /path/to/nitter/sessions.jsonl .
```

## Step 5: Create fly.toml Configuration

**Make sure you're in the nitter-fly directory first:**
```bash
cd /path/to/twitter-news-summary/nitter-fly
```

**Then copy and paste this entire command** (including the EOF lines) into your terminal and press Enter:

```bash
cat > fly.toml << 'EOF'
app = ""
primary_region = "sjc"

[build]
  image = "zedeus/nitter:latest"

[env]
  PORT = "8080"

[[services]]
  protocol = "tcp"
  internal_port = 8080

  [[services.ports]]
    port = 80
    handlers = ["http"]
    force_https = true

  [[services.ports]]
    port = 443
    handlers = ["tls", "http"]

  [services.concurrency]
    type = "connections"
    hard_limit = 25
    soft_limit = 20

  [[services.http_checks]]
    interval = "30s"
    timeout = "5s"
    grace_period = "10s"
    method = "get"
    path = "/"

[mounts]
  source = "nitter_data"
  destination = "/data"

[[vm]]
  cpu_kind = "shared"
  cpus = 1
  memory_mb = 256
EOF
```

**What this does:**
- Creates a file named `fly.toml` with Fly.io configuration
- Sets your app to use 256MB RAM (~$1.94/month, covered by $5 credit)
- Configures HTTP/HTTPS on ports 80/443 with automatic SSL
- Sets up health checks to keep your app running

**Verify it worked:**
```bash
ls -la fly.toml  # Should show the file was created
```

## Step 6: Launch the App

```bash
# Initialize the app
flyctl launch --no-deploy

# When prompted:
# - App name: Choose a unique name (e.g., "my-nitter-instance")
# - Region: Choose closest to you (or press Enter for default)
# - Copy configuration? Type "y"
# - Tweak settings? Type "N"
```

## Step 7: Create Redis Volume

```bash
# Create a volume for Redis persistence
flyctl volumes create nitter_data --region sjc --size 1
```

## Step 8: Set Configuration Secrets

Update your `nitter.conf` to use environment variables, or upload it as a file.

**Option A: Simple approach - Mount config files**

```bash
# Create a Dockerfile to include config files
cat > Dockerfile << 'EOF'
FROM zedeus/nitter:latest

# Copy configuration files
COPY nitter.conf /src/nitter.conf
COPY sessions.jsonl /src/sessions.jsonl

EXPOSE 8080
CMD ["nitter"]
EOF

# Update fly.toml to use local Dockerfile
sed -i '' 's/image = "zedeus\/nitter:latest"/# image = "zedeus\/nitter:latest"/' fly.toml

cat >> fly.toml << 'EOF'

[build]
  dockerfile = "Dockerfile"
EOF
```

**Update nitter.conf:**

```bash
# Edit nitter.conf - change hostname to your fly app name
# hostname = "my-nitter-instance.fly.dev"
```

## Step 9: Deploy!

```bash
flyctl deploy
```

This will:
- Build your Docker image with configs
- Upload to Fly.io
- Deploy to your chosen region
- Set up health checks
- Assign a URL

**Your instance will be at:** `https://YOUR-APP-NAME.fly.dev`

## Step 10: Verify It's Working

```bash
# Check app status
flyctl status

# Check logs
flyctl logs

# Test RSS feed
curl https://YOUR-APP-NAME.fly.dev/OpenAI/rss

# Or open in browser:
# https://YOUR-APP-NAME.fly.dev
```

## Step 11: Update Your App

Edit `.env` in your twitter-news-summary project:

```bash
NITTER_INSTANCE=https://YOUR-APP-NAME.fly.dev
```

Then test:
```bash
cd /path/to/twitter-news-summary
make run
```

## Check Your Costs

```bash
# View your billing dashboard
flyctl dashboard billing

# You should see:
# - $5.00 monthly credit
# - ~$1.94 usage (or less)
# - $0 balance due
```

## Useful Commands

**View logs:**
```bash
flyctl logs
```

**SSH into instance:**
```bash
flyctl ssh console
```

**Restart app:**
```bash
flyctl apps restart
```

**Scale (if needed):**
```bash
# Increase memory (still within free credit)
flyctl scale memory 512

# Check VM size
flyctl scale show
```

**Update Nitter:**
```bash
# Just rebuild and deploy
flyctl deploy
```

**Update session tokens:**
```bash
# Regenerate sessions.jsonl locally
cd /path/to/nitter/tools
python3 create_session_browser.py YOUR_USERNAME YOUR_PASSWORD --append ../sessions.jsonl

# Copy to fly deployment directory
cp ../sessions.jsonl /path/to/nitter-fly/

# Redeploy
cd /path/to/nitter-fly
flyctl deploy
```

## Important Notes

### Monthly Credit

- You get $5/month credit automatically
- Credit doesn't roll over (use it or lose it)
- 256MB instance costs ~$1.94/month (well within budget)
- Stopped apps still consume volume storage (~$0.15/GB/month)

### SSL/TLS

- Fly.io provides free automatic HTTPS! ✅
- Your instance URL: `https://YOUR-APP-NAME.fly.dev`
- Certificates auto-renew

### Regions

Available regions:
- `sjc` - San Jose, California
- `iad` - Ashburn, Virginia
- `lhr` - London
- `fra` - Frankfurt
- `syd` - Sydney
- Many more...

Choose the one closest to you or your GitHub Actions runner.

### Scaling

If you need more resources (still free with credit):
```bash
# 512MB (if you add more Twitter accounts)
flyctl scale memory 512
# Cost: ~$3.88/month (still under $5 credit!)
```

## Troubleshooting

### Deployment fails
```bash
# Check build logs
flyctl logs

# Verify Dockerfile syntax
docker build -t test .
```

### Can't access instance
```bash
# Check app status
flyctl status

# Check if app is running
flyctl apps list

# Restart if needed
flyctl apps restart
```

### Session tokens expired
```bash
# Regenerate locally
cd nitter/tools
python3 create_session_browser.py USER PASS --append ../sessions.jsonl

# Update deployment
cp ../sessions.jsonl /path/to/nitter-fly/
cd /path/to/nitter-fly
flyctl deploy
```

### Over budget
```bash
# Check billing
flyctl dashboard billing

# If over $5, scale down
flyctl scale memory 256
flyctl scale count 1
```

## Alternative: Official Nitter Fly Guide

If you want password protection on your RSS feeds:
- Follow: https://github.com/sekai-soft/guide-nitter-self-hosting/blob/master/docs/fly-io.md
- Adds nginx with HTTP basic auth and RSS password protection

## Resources

- [Fly.io Pricing](https://fly.io/pricing/)
- [Fly.io Documentation](https://fly.io/docs/)
- [Official Nitter Fly.io Guide](https://github.com/sekai-soft/guide-nitter-self-hosting/blob/master/docs/fly-io.md)

## Summary

You now have:
- ✅ Nitter running 24/7 on global infrastructure
- ✅ Accessible at `https://YOUR-APP-NAME.fly.dev`
- ✅ Automatic HTTPS with valid certificates
- ✅ Covered by $5/month credit

**Total monthly cost: $0** (covered by credit) 🎉

**Setup time: ~15 minutes** ⚡
