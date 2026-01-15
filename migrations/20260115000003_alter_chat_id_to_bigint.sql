-- Migrate chat_id from TEXT to BIGINT if needed
-- This handles the case where the table was created with TEXT before the BIGINT change

DO $$
BEGIN
    -- Check if chat_id column is TEXT type and alter it to BIGINT
    IF EXISTS (
        SELECT 1
        FROM information_schema.columns
        WHERE table_name = 'subscribers'
        AND column_name = 'chat_id'
        AND data_type = 'text'
    ) THEN
        -- Alter the column type from TEXT to BIGINT
        ALTER TABLE subscribers
        ALTER COLUMN chat_id TYPE BIGINT USING chat_id::BIGINT;

        RAISE NOTICE 'Converted chat_id from TEXT to BIGINT';
    END IF;
END $$;
