-- Add language preference to subscribers (default: English)
ALTER TABLE subscribers
ADD COLUMN language_code VARCHAR(5) NOT NULL DEFAULT 'en';

-- Constraint: only allow valid language codes
ALTER TABLE subscribers
ADD CONSTRAINT valid_language_code
CHECK (language_code IN ('en', 'es'));

-- Index for language-based queries (only for active subscribers)
CREATE INDEX idx_subscribers_language
ON subscribers(language_code)
WHERE is_active = TRUE;

-- Table to store translated summaries (cache translations to avoid re-translating)
CREATE TABLE summary_translations (
    id BIGSERIAL PRIMARY KEY,
    summary_id BIGINT NOT NULL REFERENCES summaries(id) ON DELETE CASCADE,
    language_code VARCHAR(5) NOT NULL,
    content TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE(summary_id, language_code)
);

-- Index for efficient translation lookups
CREATE INDEX idx_summary_translations_lookup
ON summary_translations(summary_id, language_code);
