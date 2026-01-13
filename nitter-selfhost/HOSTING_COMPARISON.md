# Nitter Hosting Options Comparison

## Quick Recommendation

**Just tell me what to use!**

- 🏆 **Oracle Cloud** - If you want truly free forever (requires more setup)
- ⚡ **Fly.io** - If you want the easiest setup (covered by free credit)
- 💻 **Local** - If you're okay running your computer 24/7

---

## Detailed Comparison

| Feature | Oracle Cloud | Fly.io | Local (Docker) |
|---------|-------------|--------|----------------|
| **Cost** | $0 forever | $0 (covered by $5 credit) | $0 (electricity) |
| **Setup Time** | 45-60 min | 10-15 min | 5 min |
| **Difficulty** | Medium | Easy | Easiest |
| **Resources** | 4 CPU + 24GB RAM | 1 CPU + 256MB RAM | Your hardware |
| **Always On** | ✅ Yes | ✅ Yes | ❌ Computer must run |
| **HTTPS** | ❌ Manual | ✅ Automatic | ❌ No |
| **Public IP** | ✅ Yes | ✅ Yes | Depends on network |
| **Auto Restart** | ✅ Yes (Docker) | ✅ Yes | ✅ Yes (Docker) |
| **Credit Card** | Required (verify) | Required ($5 credit) | Not required |
| **Limitations** | May reclaim if idle | $5/month budget | Computer must run |

---

## Option 1: Oracle Cloud Always Free ⭐

### ✅ Pros
- **Completely free forever** - not a trial, no expiration
- **Generous resources** - 4 CPUs + 24GB RAM (can host other apps too!)
- **200GB storage** included
- **No surprise bills** - free tier clearly marked
- **Global infrastructure** - enterprise-grade reliability
- **Can host multiple services** on same VM

### ❌ Cons
- **More setup required** - need to configure VM, firewall, Docker
- **Requires credit card** for identity verification (never charged)
- **Manual HTTPS** - need to set up reverse proxy for SSL
- **May reclaim if idle** - need to keep instance active (your twice-daily runs will do this)

### 📊 Resources Included
- **Compute:** 4 ARM CPUs (Ampere A1), 24GB RAM
- **Storage:** 200GB block storage
- **Network:** 10TB/month outbound data transfer
- **Always Free:** No time limits, forever

### 💰 Cost Breakdown
- **Setup:** $0
- **Monthly:** $0
- **Total:** $0 forever

### ⏱️ Setup Time
- **First time:** 45-60 minutes
- **After learning:** 20-30 minutes

### 👍 Best For
- You want truly free forever
- You don't mind a bit more setup
- You might host other apps later
- You're comfortable with Linux/SSH

### 📖 Setup Guide
See: [ORACLE_CLOUD_SETUP.md](./ORACLE_CLOUD_SETUP.md)

---

## Option 2: Fly.io ⚡

### ✅ Pros
- **Super easy setup** - 3 commands and you're done
- **Free with credit** - $5/month recurring credit covers Nitter (~$1.94/month)
- **Automatic HTTPS** - free SSL certificates, auto-renewal
- **Global CDN** - fast from anywhere
- **Great developer experience** - excellent CLI and dashboard
- **Automatic scaling** and health checks

### ❌ Cons
- **Requires credit card** for $5/month credit allocation
- **Limited by credit** - can't go crazy with resources
- **Not truly "free"** - it's credit-based (though Nitter stays under budget)

### 📊 Resources Included (with $5 credit)
- **Compute:** 1 shared CPU, 256MB RAM (enough for Nitter)
- **Storage:** 1GB volume (~$0.15/month)
- **Network:** 100GB/month outbound
- **SSL:** Free automatic HTTPS
- **Credit:** $5/month recurring

### 💰 Cost Breakdown
- **Nitter instance:** ~$1.94/month
- **Volume storage:** ~$0.15/month
- **Total usage:** ~$2.09/month
- **Monthly credit:** $5.00
- **Net cost:** **$0/month** (covered by credit)
- **Remaining credit:** ~$2.91 for other apps!

### ⏱️ Setup Time
- **First time:** 10-15 minutes
- **After learning:** 5 minutes

### 👍 Best For
- You want the easiest setup
- You like polished developer tools
- You want automatic HTTPS
- You don't mind requiring a credit card

### 📖 Setup Guide
See: [FLY_IO_SETUP.md](./FLY_IO_SETUP.md)

---

## Option 3: Local Docker 💻

### ✅ Pros
- **Easiest setup** - Docker Compose and done
- **No credit card** required
- **Full control** - it's your machine
- **No cloud complexity** - everything local

### ❌ Cons
- **Computer must run 24/7** - can't turn it off
- **Electricity costs** - minor but adds up
- **Network dependent** - needs stable internet
- **Not publicly accessible** - unless you set up port forwarding
- **No automatic HTTPS** - HTTP only (fine for personal use)

### 📊 Resources
- Whatever your computer has
- Nitter needs: ~100MB RAM minimum

### 💰 Cost Breakdown
- **Setup:** $0
- **Electricity:** ~$5-15/month (depending on computer)
- **Internet:** Already paying for it
- **Net cost:** Variable

### ⏱️ Setup Time
- **First time:** 5-10 minutes
- **After learning:** 2 minutes

### 👍 Best For
- Testing and development
- You already run a home server 24/7
- You have unlimited electricity/don't care about power costs
- You don't need public access

### 📖 Setup Guide
See: [README.md](./README.md) in this directory

---

## Decision Matrix

### Choose **Oracle Cloud** if:
- [ ] You want 100% free forever
- [ ] You don't mind 30-60 min setup
- [ ] You're comfortable with Linux/SSH
- [ ] You might host other apps later
- [ ] You want generous resources

### Choose **Fly.io** if:
- [ ] You want the easiest setup
- [ ] You want automatic HTTPS
- [ ] You value polished developer experience
- [ ] You're okay with credit card requirement
- [ ] 15 minutes is your max setup time

### Choose **Local Docker** if:
- [ ] You're just testing/developing
- [ ] Your computer runs 24/7 anyway
- [ ] You don't need public access
- [ ] You want 5-minute setup
- [ ] You don't want to deal with cloud

---

## What About Other Options?

### Railway
- **Pros:** Easy setup, nice UI
- **Cons:** $5 trial credit is **ONE TIME** (not recurring)
- **Verdict:** ❌ Skip - credit runs out after 2-3 months, then you pay

### Render
- **Pros:** Free tier available
- **Cons:** Apps stop after 15 min inactivity, limited hours/month
- **Verdict:** ❌ Skip - not suitable for 24/7 operation

### Google Cloud Run / AWS Lambda
- **Pros:** Pay-per-use, very cheap for low traffic
- **Cons:** More complex setup, cold starts
- **Verdict:** 🤔 Possible but overkill for Nitter

### Self-hosted at home (bare metal)
- **Pros:** Full control, no cloud
- **Cons:** Same as local Docker
- **Verdict:** ✅ Fine if you already have a home server

---

## My Recommendations

### For Most People
**Start with Fly.io** → Move to Oracle Cloud if you want to learn

**Why:**
1. Get Nitter running in 15 minutes
2. Your app works immediately with HTTPS
3. Learn cloud deployment the easy way
4. Migrate to Oracle Cloud later if you want (keep Fly.io as backup)

### For the Adventurous
**Go straight to Oracle Cloud** → Keep Fly.io as backup

**Why:**
1. Learn cloud infrastructure properly
2. Get truly free forever hosting
3. Way more resources for future projects
4. Good cloud experience to have

### For Development/Testing
**Use Local Docker** → Deploy to cloud when ready

**Why:**
1. Fastest to get started
2. Easy to debug and iterate
3. No cloud costs during development
4. Move to cloud when ready for production

---

## Cost Summary (Monthly)

| Option | Setup | Monthly | Total Year 1 |
|--------|-------|---------|-------------|
| **Oracle Cloud** | $0 | $0 | **$0** |
| **Fly.io** | $0 | $0 (credit) | **$0** |
| **Local Docker** | $0 | ~$10 (electricity) | **~$120** |
| **Railway** | $0 | $0 for 2-3 months, then ~$5 | **~$45** |

**Winner:** Oracle Cloud or Fly.io (both $0/year)

---

## Quick Start Commands

### Oracle Cloud
```bash
# See ORACLE_CLOUD_SETUP.md for full guide
# Main steps:
# 1. Create account
# 2. Launch ARM instance (2 CPU, 12GB RAM)
# 3. Install Docker
# 4. Deploy Nitter
# Time: 45-60 min
```

### Fly.io
```bash
# Install Fly CLI
brew install flyctl

# Login
flyctl auth signup

# Deploy (from nitter-fly directory)
flyctl launch
flyctl deploy

# Time: 10-15 min
```

### Local Docker
```bash
cd nitter-selfhost
./setup.sh
# Time: 5 min
```

---

## Still Can't Decide?

**Answer these questions:**

1. **Do you want to learn cloud infrastructure?**
   - Yes → **Oracle Cloud**
   - No → **Fly.io**

2. **What's your max setup time?**
   - 15 min → **Fly.io**
   - 60 min → **Oracle Cloud**
   - 5 min → **Local**

3. **Will your computer run 24/7 anyway?**
   - Yes → **Local** (easiest!)
   - No → **Oracle Cloud** or **Fly.io**

4. **Do you have a credit card you can add (won't be charged)?**
   - Yes → **Fly.io** (easiest) or **Oracle Cloud** (most free)
   - No → **Local** only

---

## Resources

- [Oracle Cloud Setup Guide](./ORACLE_CLOUD_SETUP.md)
- [Fly.io Setup Guide](./FLY_IO_SETUP.md)
- [Local Docker Setup Guide](./README.md)
- [Free Docker Hosting Options](https://blog.1byte.com/free-docker-hosting/)
- [Oracle Always Free Tier](https://www.oracle.com/cloud/free/)
- [Fly.io Pricing](https://fly.io/pricing/)

---

## Final Recommendation 🏆

For your use case (GitHub Actions running twice daily):

**Best option: Fly.io**
- Easiest setup (15 min)
- Automatic HTTPS
- Fully covered by free credit
- Just works™

**Budget option: Oracle Cloud**
- Truly free forever
- More resources
- Worth learning if you have time

**Test option: Local Docker**
- Quick to try
- Move to cloud when ready
