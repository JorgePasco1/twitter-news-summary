.PHONY: help export run run-test preview preview-cached preview-send preview-cached-send build check test clean trigger trigger-test test-send test-send-test

# Default target - show help
help:
	@echo "Twitter News Summary - Available Commands"
	@echo ""
	@echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
	@echo "DEVELOPMENT (local, no sending)"
	@echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
	@echo "  make build          - Build release binary"
	@echo "  make check          - Check code without building"
	@echo "  make test           - Run tests"
	@echo "  make clean          - Clean build artifacts"
	@echo "  make export         - Export Twitter list members (one-time setup)"
	@echo ""
	@echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
	@echo "LOCAL PREVIEW (runs your code locally, no Telegram sending)"
	@echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
	@echo "  make preview        - Fetch tweets → summarize → display (saves cache)"
	@echo "  make preview-cached - Use cached tweets → summarize → display (fast iteration)"
	@echo ""
	@echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
	@echo "LOCAL TESTING (runs your code locally, sends to TEST_CHAT_ID)"
	@echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
	@echo "  make preview-send        - Fetch tweets → summarize → send to TEST_CHAT_ID"
	@echo "  make preview-cached-send - Use cached tweets → summarize → send to TEST_CHAT_ID"
	@echo "  make run                 - Run full app locally (sends to all subscribers)"
	@echo "  make run-test            - Run full app with .env.test settings"
	@echo ""
	@echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
	@echo "REMOTE: TEST BOT (triggers Fly.io test environment)"
	@echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
	@echo "  make trigger-test   - Trigger summary on TEST bot + tail logs"
	@echo "  make test-send-test - Send test message to TEST_CHAT_ID via TEST bot"
	@echo ""
	@echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
	@echo "REMOTE: PRODUCTION BOT (triggers Fly.io production - USE WITH CAUTION)"
	@echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
	@echo "  make trigger        - Trigger summary on PROD bot (sends to all subscribers!)"
	@echo "  make test-send      - Send test message to TEST_CHAT_ID via PROD bot"
	@echo ""
	@echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
	@echo "QUICK START"
	@echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
	@echo "  1. cp .env.example .env   # Add your credentials"
	@echo "  2. make export            # Export list members (one-time)"
	@echo "  3. make preview           # Preview summary locally"
	@echo "  4. make preview-send      # Test full flow with your Telegram"
	@echo ""
	@echo "For detailed documentation, see COMMANDS.md"

# Export Twitter list members (one-time)
export:
	@echo "🔄 Exporting Twitter list members..."
	cargo run --bin export

# Run the main news summary application
run:
	@echo "🚀 Running Twitter news summary..."
	cargo run --bin twitter-news-summary

# Run with test environment settings
run-test:
	@echo "🧪 Running with test environment settings..."
	@set -a; \
	  . ./.env.test; \
	  set +a; \
	  cargo run --bin twitter-news-summary

# Preview summary without sending to Telegram (for testing)
preview:
	@echo "👀 Generating summary preview..."
	cargo run --bin preview

# Preview using cached tweets (for fast iteration on formatting)
preview-cached:
	@echo "👀 Generating summary from cached tweets..."
	cargo run --bin preview -- --use-cached

# Build release binary
build:
	@echo "🔨 Building release binary..."
	cargo build --release

# Check code without building
check:
	@echo "✅ Checking code..."
	cargo check

# Run tests
test:
	@echo "🧪 Running tests..."
	cargo test

# Clean build artifacts
clean:
	@echo "🧹 Cleaning build artifacts..."
	cargo clean

# Trigger summary on Fly.io production (CAUTION: sends to all subscribers!)
trigger:
	@echo "🚀 Triggering summary on PRODUCTION Fly.io..."
	@echo "⚠️  WARNING: This will send to all production subscribers!"
	@if [ -z "$$API_KEY" ]; then \
		if [ -f .env ]; then \
			export $$(grep "^API_KEY=" .env | xargs) && \
			curl -X POST https://twitter-summary-bot.fly.dev/trigger \
				-H "X-API-Key: $$API_KEY" \
				-w "\n" || echo "❌ Failed to trigger summary"; \
		else \
			echo "❌ Error: API_KEY not found in environment or .env file"; \
			echo "   Add API_KEY to .env or export API_KEY=your_key"; \
			exit 1; \
		fi \
	else \
		curl -X POST https://twitter-summary-bot.fly.dev/trigger \
			-H "X-API-Key: $$API_KEY" \
			-w "\n" || echo "❌ Failed to trigger summary"; \
	fi

# Trigger summary on Fly.io TEST environment (uses .env.test)
# After triggering, automatically tails logs and exits when the job completes
trigger-test:
	@echo "🧪 Triggering summary on TEST Fly.io..."
	@if [ -f .env.test ]; then \
		export $$(grep "^API_KEY=" .env.test | xargs) && \
		if [ -z "$$API_KEY" ]; then \
			echo "❌ Error: API_KEY not found in .env.test file"; \
			exit 1; \
		fi && \
		(curl -s -X POST https://twitter-summary-bot-test.fly.dev/trigger \
			-H "X-API-Key: $$API_KEY" > /dev/null 2>&1 &) && \
		echo "✅ Trigger request sent" && \
		echo "" && \
		echo "📋 Tailing logs (will auto-exit on completion, or Ctrl+C to stop)..." && \
		echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━" && \
		(fly logs -a twitter-summary-bot-test 2>&1 || echo "❌ Failed to tail logs") | \
			awk '/Summary job completed|Manual trigger completed|No tweets found|ERROR|FATAL|panic/{print; exit} {print}' && \
		echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━" && \
		echo "✅ Done"; \
	else \
		echo "❌ Error: .env.test file not found"; \
		echo "   Create .env.test with API_KEY for the test bot"; \
		exit 1; \
	fi

# Test send to specific user via PRODUCTION bot
test-send:
	@echo "🧪 Sending test message via PRODUCTION bot..."
	@if [ -z "$$API_KEY" ]; then \
		if [ -f .env ]; then \
			export $$(grep "^API_KEY=" .env | xargs) && \
			export $$(grep "^TEST_CHAT_ID=" .env | xargs) && \
			if [ -z "$$TEST_CHAT_ID" ]; then \
				echo "❌ Error: TEST_CHAT_ID not found in environment or .env file"; \
				exit 1; \
			fi && \
			curl -X POST "https://twitter-summary-bot.fly.dev/test?chat_id=$$TEST_CHAT_ID" \
				-H "X-API-Key: $$API_KEY" \
				-w "\n" || echo "❌ Failed to send test message"; \
		else \
			echo "❌ Error: API_KEY not found in environment or .env file"; \
			exit 1; \
		fi \
	else \
		if [ -z "$$TEST_CHAT_ID" ] && [ -f .env ]; then \
			export $$(grep "^TEST_CHAT_ID=" .env | xargs); \
		fi; \
		if [ -z "$$TEST_CHAT_ID" ]; then \
			echo "❌ Error: TEST_CHAT_ID not found in environment or .env file"; \
			exit 1; \
		fi; \
		curl -X POST "https://twitter-summary-bot.fly.dev/test?chat_id=$$TEST_CHAT_ID" \
			-H "X-API-Key: $$API_KEY" \
			-w "\n" || echo "❌ Failed to send test message"; \
	fi

# Test send to specific user via TEST bot (uses .env.test)
test-send-test:
	@echo "🧪 Sending test message via TEST bot..."
	@if [ -f .env.test ]; then \
		export $$(grep "^API_KEY=" .env.test | xargs) && \
		export $$(grep "^TEST_CHAT_ID=" .env.test | xargs) && \
		if [ -z "$$API_KEY" ]; then \
			echo "❌ Error: API_KEY not found in .env.test file"; \
			exit 1; \
		fi && \
		if [ -z "$$TEST_CHAT_ID" ]; then \
			echo "❌ Error: TEST_CHAT_ID not found in .env.test file"; \
			exit 1; \
		fi && \
		curl -X POST "https://twitter-summary-bot-test.fly.dev/test?chat_id=$$TEST_CHAT_ID" \
			-H "X-API-Key: $$API_KEY" \
			-w "\n" || echo "❌ Failed to send test message"; \
	else \
		echo "❌ Error: .env.test file not found"; \
		echo "   Create .env.test with API_KEY and TEST_CHAT_ID for the test bot"; \
		exit 1; \
	fi

# Preview and send to test user locally
preview-send:
	@echo "👀 Generating summary and sending to test user..."
	cargo run --bin preview -- --send

# Preview using cached tweets and send
preview-cached-send:
	@echo "👀 Using cached tweets and sending to test user..."
	cargo run --bin preview -- --use-cached --send
