# Deploy Nitter to Oracle Cloud (Always Free Tier)

## Why Oracle Cloud?

- ✅ **Completely FREE forever** (not a trial!)
- ✅ Up to 4 ARM CPUs + 24GB RAM (more than enough for Nitter)
- ✅ 200GB storage included
- ✅ No time limits, no surprise charges
- ✅ Can host multiple apps on same instance

**Cost:** $0/month forever

## Prerequisites

- Credit card (for identity verification - won't be charged)
- 30-60 minutes for initial setup

## Step 1: Create Oracle Cloud Account

1. Go to https://www.oracle.com/cloud/free/
2. Click **"Start for free"**
3. Fill in your details and verify email
4. Add payment method (for verification only - free tier never charges)
5. Complete signup process

## Step 2: Create a VM Instance

1. **Log into Oracle Cloud Console**
   - Go to https://cloud.oracle.com/

2. **Create a Compute Instance**
   - Click **"Create a VM instance"** or navigate to:
     - Menu → Compute → Instances → Create Instance

3. **Configure Instance**

   **Name:** `nitter-server`

   **Placement:** (default is fine)

   **Image:**
   - Click **"Change Image"**
   - Select **"Canonical Ubuntu 22.04"** (or latest)
   - Click **"Select Image"**

   **Shape:**
   - Click **"Change Shape"**
   - Select **"Ampere"** (ARM-based)
   - Choose **"VM.Standard.A1.Flex"**
   - Set **2 OCPUs** and **12 GB RAM** (or more if you want)
   - This is FREE tier eligible! ✅

   **Networking:**
   - Use default VCN settings
   - Check **"Assign a public IPv4 address"** ✅

   **SSH Keys:**
   - **IMPORTANT:** Download the private key pair (you'll need this!)
   - Save as `oracle-nitter-key.pem` or similar

4. **Click "Create"**

5. **Wait for provisioning** (~2-3 minutes)
   - Status will change to **"Running"** with a green indicator

6. **Note your public IP address** - you'll see it in the instance details

## Step 3: Configure Firewall (Security Lists)

**Open port 8080 for Nitter:**

1. On your instance page, click on the **VCN name** (under "Primary VNIC")

2. Click **"Security Lists"** → Click your security list name

3. Click **"Add Ingress Rules"**

4. Configure the rule:
   - **Source CIDR:** `0.0.0.0/0` (or your IP for more security)
   - **IP Protocol:** `TCP`
   - **Destination Port Range:** `8080`
   - **Description:** `Nitter web access`

5. Click **"Add Ingress Rules"**

## Step 4: Connect to Your VM

```bash
# Make key file secure (required)
chmod 400 oracle-nitter-key.pem

# Connect via SSH (replace with YOUR public IP)
ssh -i oracle-nitter-key.pem ubuntu@YOUR_PUBLIC_IP
```

You should now be connected to your Ubuntu VM!

## Step 5: Install Docker and Docker Compose

```bash
# Update system
sudo apt update && sudo apt upgrade -y

# Install Docker
curl -fsSL https://get.docker.com -o get-docker.sh
sudo sh get-docker.sh

# Add your user to docker group (to run without sudo)
sudo usermod -aG docker ubuntu

# Install Docker Compose
sudo apt install docker-compose-plugin -y

# Verify installation
docker --version
docker compose version

# Log out and back in for group changes to take effect
exit
```

**Reconnect:**
```bash
ssh -i oracle-nitter-key.pem ubuntu@YOUR_PUBLIC_IP
```

## Step 6: Configure Ubuntu Firewall

```bash
# Allow port 8080
sudo iptables -I INPUT 6 -m state --state NEW -p tcp --dport 8080 -j ACCEPT

# Save firewall rules
sudo netfilter-persistent save

# If netfilter-persistent not installed:
sudo apt install iptables-persistent -y
sudo netfilter-persistent save
```

## Step 7: Deploy Nitter

```bash
# Create nitter directory
mkdir ~/nitter && cd ~/nitter

# Create docker-compose.yml
cat > docker-compose.yml << 'EOF'
version: "3"

services:
  nitter:
    image: zedeus/nitter:latest-arm64
    container_name: nitter
    ports:
      - "8080:8080"
    volumes:
      - ./nitter.conf:/src/nitter.conf:Z,ro
      - ./sessions.jsonl:/src/sessions.jsonl:Z,ro
    depends_on:
      - nitter-redis
    restart: unless-stopped
    healthcheck:
      test: wget -nv --tries=1 --spider http://127.0.0.1:8080/Jack/status/20 || exit 1
      interval: 30s
      timeout: 5s
      retries: 2

  nitter-redis:
    image: redis:6-alpine
    container_name: nitter-redis
    command: redis-server --save 60 1 --loglevel warning
    volumes:
      - nitter-redis:/data
    restart: unless-stopped
    healthcheck:
      test: redis-cli ping
      interval: 30s
      timeout: 5s
      retries: 2

volumes:
  nitter-redis:
EOF

# Create nitter.conf
cat > nitter.conf << 'EOF'
[Server]
hostname = "YOUR_PUBLIC_IP"
title = "nitter"
address = "0.0.0.0"
port = 8080
https = false
httpMaxConnections = 100
staticDir = "./public"

[Cache]
listMinutes = 240
rssMinutes = 10
redisHost = "nitter-redis"
redisPort = 6379
redisPassword = ""
redisConnections = 20
redisMaxConnections = 30

[Config]
hmacKey = "CHANGE_THIS_TO_RANDOM_STRING"
base64Media = false
enableRSS = true
enableDebug = false
proxy = ""
proxyAuth = ""
apiProxy = ""
disableTid = false
maxConcurrentReqs = 2

[Preferences]
theme = "Nitter"
replaceTwitter = "nitter.net"
replaceYouTube = "piped.video"
replaceReddit = "teddit.net"
proxyVideos = true
hlsPlayback = false
infiniteScroll = false
EOF

# Replace YOUR_PUBLIC_IP with actual IP
sed -i "s/YOUR_PUBLIC_IP/$(curl -s ifconfig.me)/" nitter.conf
```

## Step 8: Create Session Tokens

**On your LOCAL machine** (not the VM):

```bash
# Clone Nitter to get session creation tool
git clone https://github.com/zedeus/nitter.git
cd nitter/tools

# Install dependencies
pip install -r requirements.txt

# Create session tokens (replace with YOUR Twitter credentials)
python3 create_session_browser.py YOUR_USERNAME YOUR_PASSWORD --append ../sessions.jsonl
```

**Upload sessions.jsonl to VM:**

```bash
# From your local machine (in the directory with sessions.jsonl)
scp -i oracle-nitter-key.pem nitter/sessions.jsonl ubuntu@YOUR_PUBLIC_IP:~/nitter/
```

## Step 9: Start Nitter!

```bash
# On the VM
cd ~/nitter
docker compose up -d

# Check logs
docker compose logs -f nitter

# You should see:
# Config loaded from nitter.conf
# Connected to Redis
# Loaded 1 session(s)
# Starting Nitter on port 8080
```

## Step 10: Test Your Instance

```bash
# From your local machine
curl http://YOUR_PUBLIC_IP:8080/OpenAI/rss

# Or open in browser:
# http://YOUR_PUBLIC_IP:8080
```

You should see RSS XML output! 🎉

## Step 11: Update Your App

Edit `.env` in your twitter-news-summary project:

```bash
NITTER_INSTANCE=http://YOUR_PUBLIC_IP:8080
```

Then test:
```bash
make run
```

## Maintenance Commands

**View logs:**
```bash
ssh -i oracle-nitter-key.pem ubuntu@YOUR_PUBLIC_IP
cd ~/nitter
docker compose logs -f nitter
```

**Restart Nitter:**
```bash
docker compose restart
```

**Update Nitter:**
```bash
docker compose pull
docker compose up -d
```

**Update session tokens (when they expire):**
```bash
# On local machine: regenerate sessions.jsonl
python3 create_session_browser.py YOUR_USERNAME YOUR_PASSWORD --append ../sessions.jsonl

# Upload to VM
scp -i oracle-nitter-key.pem nitter/sessions.jsonl ubuntu@YOUR_PUBLIC_IP:~/nitter/

# Restart Nitter
ssh -i oracle-nitter-key.pem ubuntu@YOUR_PUBLIC_IP "cd ~/nitter && docker compose restart"
```

## Important Notes

### Keep Your Instance Active

Oracle may reclaim free instances if they're **completely idle** for extended periods. To prevent this:

1. Your news summary runs twice daily via GitHub Actions - this keeps it active! ✅
2. Optionally, set up a cron job to ping the instance:
   ```bash
   # Add to crontab (on VM)
   crontab -e

   # Add this line:
   0 */6 * * * curl -s http://localhost:8080 > /dev/null
   ```

### Security Recommendations

1. **Change hmacKey** in nitter.conf to a random string
2. **Restrict port 8080** to your IP only (in Security Lists)
3. **Keep Ubuntu updated:** `sudo apt update && sudo apt upgrade -y`
4. **Consider using a reverse proxy** with authentication for web access

### Cost Monitoring

- Check your Oracle Cloud bill dashboard - it should always show $0
- Free tier resources are clearly marked with "Always Free" tag
- You won't be charged unless you explicitly upgrade or add paid resources

## Troubleshooting

### Can't connect to instance
- Check Security List has port 22 (SSH) and 8080 allowed
- Check Ubuntu firewall: `sudo iptables -L`
- Verify instance is running in Oracle Cloud console

### Nitter not accessible
- Check Docker is running: `docker compose ps`
- Check logs: `docker compose logs nitter`
- Verify port 8080 is open: `sudo netfilter-persistent save`

### Session tokens expired
- Regenerate using the browser automation script
- Upload new sessions.jsonl to VM
- Restart containers

## Resources

- [Oracle Cloud Always Free Tier](https://www.oracle.com/cloud/free/)
- [Docker on Oracle Cloud Guide](https://sunnydsouza.hashnode.dev/setting-up-docker-and-docker-compose-on-oracle-clouds-always-free-tier-instance)
- [Oracle Cloud Documentation](https://docs.oracle.com/en-us/iaas/Content/FreeTier/freetier_topic-Always_Free_Resources.htm)

## Summary

You now have:
- ✅ Nitter running 24/7 for FREE
- ✅ Accessible at `http://YOUR_PUBLIC_IP:8080`
- ✅ No monthly costs
- ✅ Your news summary app will work reliably from GitHub Actions

Total monthly cost: **$0** 🎉
