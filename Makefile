.PHONY: help export run run-test preview preview-cached preview-send preview-cached-send test-send build check test clean trigger

# Default target - show help
help:
	@echo "Twitter News Summary - Available Commands:"
	@echo ""
	@echo "Development:"
	@echo "  make export         - Export Twitter list members (one-time setup)"
	@echo "  make run            - Run the news summary job locally (uses RSS feeds)"
	@echo "  make run-test       - Run locally with .env.test settings"
	@echo "  make preview        - Preview summary without sending (fetches tweets, saves cache)"
	@echo "  make preview-cached - Preview with cached tweets (fast iteration on formatting)"
	@echo "  make build          - Build release binary"
	@echo "  make check          - Check code without building"
	@echo "  make test           - Run tests"
	@echo "  make clean          - Clean build artifacts"
	@echo ""
	@echo "Testing:"
	@echo "  make preview-send        - Generate summary and send to test user (local)"
	@echo "  make preview-cached-send - Use cached tweets and send to test user (local)"
	@echo "  make test-send           - Send test message on Fly.io (production)"
	@echo ""
	@echo "Production:"
	@echo "  make trigger     - Trigger summary on Fly.io (requires API_KEY in .env)"
	@echo ""
	@echo "Quick Start:"
	@echo "  1. Copy .env.example to .env and add credentials"
	@echo "  2. make export   # Export list members once"
	@echo "  3. make preview  # Preview the summary first"
	@echo "  4. make run      # Run the full application"
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
	@env $$(grep -v '^#' .env.test | xargs) cargo run --bin twitter-news-summary

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

# Trigger summary on Fly.io production
trigger:
	@echo "🚀 Triggering summary on Fly.io..."
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

# Test send to specific user in production
test-send:
	@echo "🧪 Sending test message to your Telegram..."
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

# Preview and send to test user locally
preview-send:
	@echo "👀 Generating summary and sending to test user..."
	cargo run --bin preview -- --send

# Preview using cached tweets and send
preview-cached-send:
	@echo "👀 Using cached tweets and sending to test user..."
	cargo run --bin preview -- --use-cached --send
