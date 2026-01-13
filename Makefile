.PHONY: help export run build check test clean

# Default target - show help
help:
	@echo "Twitter News Summary - Available Commands:"
	@echo ""
	@echo "  make export      - Export Twitter list members (one-time setup)"
	@echo "  make run         - Run the news summary job (uses RSS feeds)"
	@echo "  make build       - Build release binary"
	@echo "  make check       - Check code without building"
	@echo "  make test        - Run tests"
	@echo "  make clean       - Clean build artifacts"
	@echo ""
	@echo "Quick Start:"
	@echo "  1. Copy .env.example to .env and add credentials"
	@echo "  2. make export   # Export list members once"
	@echo "  3. make run      # Run the summary job"

# Export Twitter list members (one-time)
export:
	@echo "🔄 Exporting Twitter list members..."
	cargo run --bin export

# Run the main news summary application
run:
	@echo "🚀 Running Twitter news summary..."
	cargo run

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
