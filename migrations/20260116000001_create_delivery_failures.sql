-- Create table for tracking delivery failures
CREATE TABLE IF NOT EXISTS delivery_failures (
    id BIGSERIAL PRIMARY KEY,
    chat_id BIGINT NOT NULL,
    error_message TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Index for efficient queries by date
CREATE INDEX IF NOT EXISTS idx_delivery_failures_created_at ON delivery_failures(created_at DESC);

-- Index for lookups by chat_id
CREATE INDEX IF NOT EXISTS idx_delivery_failures_chat_id ON delivery_failures(chat_id);
