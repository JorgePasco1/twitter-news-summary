# Debug Delivery Failures

Investigate delivery failures by querying the production database for error logs, subscriber info, and related summaries.

## Usage

```
/debug-failures [chat_id] [--days N]
```

- `chat_id` (optional): Filter by specific subscriber chat ID
- `--days N` (optional): Look back N days (default: 7)

## What it does

Queries the production PostgreSQL database to investigate delivery failures:
1. Fetches recent delivery failures with error messages
2. Gets subscriber information (language preference, subscription date)
3. Retrieves related summaries to understand what content failed
4. Analyzes error patterns and suggests fixes

## Configuration

Requires `DATABASE_URL` environment variable (from `.env` file) pointing to the production Neon PostgreSQL database.

## Instructions

When the user runs this command:

### 1. Load Database URL

Read the `DATABASE_URL` from the `.env` file:
```bash
source .env && echo "Database connection ready"
```

### 2. Query Delivery Failures

Fetch recent failures (adjust days/chat_id based on user arguments):

```bash
# All recent failures
psql "$DATABASE_URL" -c "
SELECT
    df.id,
    df.chat_id,
    df.error_message,
    df.created_at,
    s.language_code as subscriber_lang,
    s.subscribed_at
FROM delivery_failures df
LEFT JOIN subscribers s ON df.chat_id = s.chat_id
ORDER BY df.created_at DESC
LIMIT 20;
"

# Or filter by specific chat_id
psql "$DATABASE_URL" -c "
SELECT * FROM delivery_failures
WHERE chat_id = {chat_id}
ORDER BY created_at DESC
LIMIT 10;
"
```

### 3. Analyze Error Patterns

Common error patterns to look for:

| Error Pattern | Meaning | Suggested Fix |
|--------------|---------|---------------|
| `can't parse entities` | MarkdownV2 escaping issue | Check `escape_markdownv2()` in `telegram.rs` |
| `chat not found` | User deleted account | Auto-removed by system |
| `bot was blocked` | User blocked bot | Auto-removed by system |
| `Connection reset` | Database connection issue | Check pool config in `db.rs` |
| `timed out` | Network/API timeout | Check retry logic |

### 4. Query Related Summaries

Find the summary that was being sent when failure occurred:

```bash
psql "$DATABASE_URL" -c "
SELECT
    id,
    created_at,
    LENGTH(content) as content_length,
    LEFT(content, 200) as content_preview
FROM summaries
WHERE created_at <= '{failure_timestamp}'
ORDER BY created_at DESC
LIMIT 1;
"
```

### 5. Check Translations (if language != 'en')

For non-English subscribers, check if translation exists:

```bash
psql "$DATABASE_URL" -c "
SELECT
    st.summary_id,
    st.language_code,
    st.created_at,
    LENGTH(st.content) as translation_length
FROM summary_translations st
WHERE st.summary_id = {summary_id}
AND st.language_code = '{language_code}';
"
```

### 6. Database Schema Reference

```sql
-- delivery_failures table
CREATE TABLE delivery_failures (
    id BIGSERIAL PRIMARY KEY,
    chat_id BIGINT NOT NULL,
    error_message TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- subscribers table
CREATE TABLE subscribers (
    chat_id BIGINT PRIMARY KEY,
    language_code VARCHAR(5) NOT NULL DEFAULT 'en',
    subscribed_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- summaries table
CREATE TABLE summaries (
    id BIGSERIAL PRIMARY KEY,
    content TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- summary_translations table
CREATE TABLE summary_translations (
    id BIGSERIAL PRIMARY KEY,
    summary_id BIGINT NOT NULL REFERENCES summaries(id) ON DELETE CASCADE,
    language_code VARCHAR(5) NOT NULL,
    content TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE(summary_id, language_code)
);
```

## Example Output

```text
## Delivery Failure Investigation

### Recent Failures (last 7 days)

| ID | Chat ID | Error | Timestamp | Language |
|----|---------|-------|-----------|----------|
| 10 | 8313640214 | MarkdownV2: Character '(' must be escaped | 2026-02-06 01:00:20 | es |
| 9 | 1234567890 | Bot was blocked by the user | 2026-02-05 13:00:15 | en |

### Analysis

**Failure #10 (chat_id: 8313640214)**
- Error type: MarkdownV2 parsing error
- Language: Spanish (es)
- Root cause: Parenthesis character in Spanish translation wasn't escaped
- Related code: `telegram.rs:615` - `escape_markdownv2()` function
- Status: Fixed in current deployment (escape_markdownv2 now applied to all content)

**Failure #9 (chat_id: 1234567890)**
- Error type: Bot blocked by user
- Status: Auto-cleaned (subscriber removed from database)

### Suggested Actions

1. No action needed - issues are either fixed or auto-handled
2. Consider adding monitoring alert for repeated MarkdownV2 failures
```

## Useful Ad-hoc Queries

### Count failures by error type (last 30 days)
```bash
psql "$DATABASE_URL" -c "
SELECT
    CASE
        WHEN error_message LIKE '%parse entities%' THEN 'MarkdownV2 Error'
        WHEN error_message LIKE '%blocked%' THEN 'Bot Blocked'
        WHEN error_message LIKE '%not found%' THEN 'Chat Not Found'
        ELSE 'Other'
    END as error_type,
    COUNT(*) as count
FROM delivery_failures
WHERE created_at > NOW() - INTERVAL '30 days'
GROUP BY 1
ORDER BY 2 DESC;
"
```

### Check subscriber health
```bash
psql "$DATABASE_URL" -c "
SELECT
    language_code,
    COUNT(*) as subscriber_count,
    MIN(subscribed_at) as oldest_subscriber,
    MAX(subscribed_at) as newest_subscriber
FROM subscribers
GROUP BY language_code
ORDER BY subscriber_count DESC;
"
```

### Recent summary delivery rate
```bash
psql "$DATABASE_URL" -c "
SELECT
    DATE(s.created_at) as date,
    COUNT(DISTINCT s.id) as summaries,
    COUNT(DISTINCT df.id) as failures,
    (SELECT COUNT(*) FROM subscribers) as total_subscribers
FROM summaries s
LEFT JOIN delivery_failures df ON DATE(df.created_at) = DATE(s.created_at)
WHERE s.created_at > NOW() - INTERVAL '7 days'
GROUP BY 1
ORDER BY 1 DESC;
"
```

## Notes

- Database uses Neon serverless PostgreSQL (may have cold start latency)
- Old summaries may be purged, but delivery_failures persist for auditing
- Connection string is in `.env` file (never commit this file)
- The `subscribers` table auto-cleans blocked/deleted accounts
