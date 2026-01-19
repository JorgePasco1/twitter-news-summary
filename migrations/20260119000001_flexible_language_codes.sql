-- Remove hardcoded language constraint to allow extensibility
ALTER TABLE subscribers DROP CONSTRAINT IF EXISTS valid_language_code;

-- Add flexible format constraint (2-5 lowercase letters for ISO 639-1/639-3 codes)
ALTER TABLE subscribers ADD CONSTRAINT valid_language_code_format
CHECK (language_code ~ '^[a-z]{2,5}$');

-- Add comment explaining the format
COMMENT ON COLUMN subscribers.language_code IS
'ISO 639-1/639-3 language code (e.g., en, es, fr, pt, de). Must be 2-5 lowercase letters. Supported codes are defined in the application registry.';

-- Verify existing data still valid (should only have 'en' and 'es')
DO $$
BEGIN
    IF EXISTS (
        SELECT 1 FROM subscribers
        WHERE language_code !~ '^[a-z]{2,5}$'
    ) THEN
        RAISE EXCEPTION 'Invalid language codes found in subscribers table';
    END IF;
END
$$;
