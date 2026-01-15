-- Create summaries table
CREATE TABLE summaries (
    id BIGSERIAL PRIMARY KEY,
    content TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Index for ordering by creation date
CREATE INDEX idx_summaries_created_at ON summaries(created_at DESC);
