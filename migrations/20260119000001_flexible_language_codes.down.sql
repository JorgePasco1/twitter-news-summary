-- Rollback: Restore hardcoded language constraint

-- Remove flexible format constraint
ALTER TABLE subscribers DROP CONSTRAINT IF EXISTS valid_language_code_format;

-- Restore original hardcoded constraint (en, es only)
ALTER TABLE subscribers ADD CONSTRAINT valid_language_code
CHECK (language_code IN ('en', 'es'));

-- Remove comment
COMMENT ON COLUMN subscribers.language_code IS NULL;
