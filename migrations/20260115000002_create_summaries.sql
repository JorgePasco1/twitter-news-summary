-- Create summaries table
CREATE TABLE IF NOT EXISTS summaries (
    id BIGSERIAL PRIMARY KEY,
    content TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Index for ordering by creation date
CREATE INDEX IF NOT EXISTS idx_summaries_created_at ON summaries(created_at DESC);
