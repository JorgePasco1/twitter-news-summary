#!/bin/bash

# Nitter Self-Host Quick Setup Script

set -e

echo "🐦 Nitter Self-Host Setup"
echo "========================="
echo ""

# Check if Docker is installed
if ! command -v docker &> /dev/null; then
    echo "❌ Docker is not installed. Please install Docker first:"
    echo "   https://docs.docker.com/get-docker/"
    exit 1
fi

# Check if Docker Compose is installed
if ! command -v docker-compose &> /dev/null && ! docker compose version &> /dev/null; then
    echo "❌ Docker Compose is not installed. Please install Docker Compose first:"
    echo "   https://docs.docker.com/compose/install/"
    exit 1
fi

echo "✅ Docker and Docker Compose are installed"
echo ""

# Check if sessions.jsonl exists
if [ ! -f "sessions.jsonl" ]; then
    echo "⚠️  sessions.jsonl not found!"
    echo ""
    echo "You need to create session tokens from your Twitter account."
    echo "Follow these steps:"
    echo ""
    echo "1. Clone Nitter repo:"
    echo "   git clone https://github.com/zedeus/nitter.git"
    echo ""
    echo "2. Install Python dependencies:"
    echo "   cd nitter/tools"
    echo "   pip install -r requirements.txt"
    echo ""
    echo "3. Create session tokens:"
    echo "   python3 create_session_browser.py YOUR_USERNAME YOUR_PASSWORD --append ../sessions.jsonl"
    echo ""
    echo "4. Copy sessions.jsonl here:"
    echo "   cp nitter/sessions.jsonl $(pwd)/"
    echo ""
    echo "Then run this script again."
    exit 1
fi

echo "✅ sessions.jsonl found"
echo ""

# Check if nitter.conf exists
if [ ! -f "nitter.conf" ]; then
    echo "❌ nitter.conf not found!"
    echo "   This file should have been created. Please check the setup."
    exit 1
fi

echo "✅ nitter.conf found"
echo ""

# Check if docker-compose.yml exists
if [ ! -f "docker-compose.yml" ]; then
    echo "❌ docker-compose.yml not found!"
    echo "   This file should have been created. Please check the setup."
    exit 1
fi

echo "✅ docker-compose.yml found"
echo ""

# Start containers
echo "🚀 Starting Nitter containers..."
docker-compose up -d

echo ""
echo "⏳ Waiting for containers to be healthy..."
sleep 5

# Check if containers are running
if docker-compose ps | grep -q "Up"; then
    echo ""
    echo "✅ Nitter is running!"
    echo ""
    echo "🌐 Web interface: http://localhost:8080"
    echo "📡 RSS feed example: http://localhost:8080/OpenAI/rss"
    echo ""
    echo "Test it:"
    echo "  curl http://localhost:8080/OpenAI/rss"
    echo ""
    echo "View logs:"
    echo "  docker-compose logs -f nitter"
    echo ""
    echo "Stop Nitter:"
    echo "  docker-compose down"
    echo ""
    echo "Next step: Update your .env file:"
    echo "  NITTER_INSTANCE=http://localhost:8080"
else
    echo ""
    echo "❌ Containers failed to start. Check logs:"
    echo "   docker-compose logs"
    exit 1
fi
