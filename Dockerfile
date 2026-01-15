# Multi-stage build for small final image
FROM rust:1.85-alpine AS builder

# Install build dependencies
RUN apk add --no-cache musl-dev sqlite-dev

WORKDIR /app

# Copy manifests
COPY Cargo.toml Cargo.lock ./

# Copy source code
COPY src ./src
COPY data ./data

# Build release binary
RUN cargo build --release

# Final stage
FROM alpine:latest

# Install runtime dependencies
RUN apk add --no-cache sqlite-libs

WORKDIR /app

# Copy binary from builder
COPY --from=builder /app/target/release/twitter-news-summary /app/twitter-news-summary

# Copy data directory (usernames.txt)
COPY data /app/data

# Create data directory for SQLite database
RUN mkdir -p /data && chmod 777 /data

EXPOSE 8080

CMD ["/app/twitter-news-summary"]
