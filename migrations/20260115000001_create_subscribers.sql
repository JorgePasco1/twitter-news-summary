-- Create subscribers table
CREATE TABLE IF NOT EXISTS subscribers (
    chat_id BIGINT PRIMARY KEY,
    username TEXT,
    subscribed_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    first_subscribed_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    is_active BOOLEAN NOT NULL DEFAULT TRUE,
    received_welcome_summary BOOLEAN NOT NULL DEFAULT FALSE
);

-- Index for filtering active subscribers
CREATE INDEX IF NOT EXISTS idx_subscribers_active ON subscribers(is_active) WHERE is_active = TRUE;
