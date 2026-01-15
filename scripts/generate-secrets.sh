#!/bin/bash
# Script to generate secure secrets for Telegram bot security

set -e

echo "=== Generating Secure Secrets ==="
echo ""

# Generate API key (64 hex chars = 256 bits)
TELEGRAM_WEBHOOK_SECRET=$(openssl rand -hex 32)
echo "✓ Generated TELEGRAM_WEBHOOK_SECRET (256-bit)"

echo ""
echo "=== Generated Secret ==="
echo ""
echo "TELEGRAM_WEBHOOK_SECRET=$TELEGRAM_WEBHOOK_SECRET"
echo ""

# Check if TELEGRAM_BOT_TOKEN is set
if [ -z "$TELEGRAM_BOT_TOKEN" ]; then
    echo "⚠️  WARNING: TELEGRAM_BOT_TOKEN environment variable not set"
    echo "   Please set it first: export TELEGRAM_BOT_TOKEN=your_token_here"
    echo ""
fi

echo "=== Step 1: Add to your .env file (for local development) ==="
echo ""
echo "Add this line to your .env file:"
echo ""
echo "  TELEGRAM_WEBHOOK_SECRET=$TELEGRAM_WEBHOOK_SECRET"
echo ""
echo "⚠️  WARNING: Never commit the .env file to version control!"
echo ""

echo "=== Step 2: Set in GitHub Secrets (for production) ==="
echo ""
echo "Go to your repository settings and add:"
echo "  Settings → Secrets and variables → Actions → Secrets → prod environment"
echo "  Name: TELEGRAM_WEBHOOK_SECRET"
echo "  Value: $TELEGRAM_WEBHOOK_SECRET"
echo ""

echo "=== Step 3: Configure Telegram webhook ==="
echo ""

if [ -n "$TELEGRAM_BOT_TOKEN" ]; then
    echo "Run this command to set the Telegram webhook with the secret token:"
    echo ""
    echo "  curl -X POST \"https://api.telegram.org/bot${TELEGRAM_BOT_TOKEN}/setWebhook\" \\"
    echo "    -H \"Content-Type: application/json\" \\"
    echo "    -d '{\"url\": \"https://your-webhook-url.com/webhook\", \"secret_token\": \"${TELEGRAM_WEBHOOK_SECRET}\"}'"
    echo ""
    echo "Replace 'your-webhook-url.com' with your actual webhook URL"
else
    echo "Set TELEGRAM_BOT_TOKEN and run this script again to get the webhook configuration command"
fi

echo ""
echo "=== Security Notes ==="
echo ""
echo "✓ Secret is 256-bit (64 hex characters)"
echo "✓ TELEGRAM_WEBHOOK_SECRET protects your webhook from unauthorized requests"
echo "✓ Telegram will send this token in X-Telegram-Bot-Api-Secret-Token header"
echo "✓ Your code must verify this header matches the secret"
echo ""
echo "Without this protection, anyone who discovers your webhook URL can:"
echo "  - Send fake subscription requests"
echo "  - Unsubscribe real users"
echo "  - Abuse your API quota"
echo ""
