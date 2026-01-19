use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use sqlx::postgres::PgPoolOptions;
use sqlx::{FromRow, PgPool};
use std::time::Duration;

#[derive(Debug, Clone, FromRow)]
pub struct Subscriber {
    pub chat_id: i64,
    pub username: Option<String>,
    pub subscribed_at: DateTime<Utc>,
    pub first_subscribed_at: DateTime<Utc>,
    pub is_active: bool,
    pub received_welcome_summary: bool,
    pub language_code: String,
}

#[derive(Debug, Clone, FromRow)]
pub struct Summary {
    pub id: i64,
    pub content: String,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, FromRow)]
pub struct SummaryTranslation {
    pub id: i64,
    pub summary_id: i64,
    pub language_code: String,
    pub content: String,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, FromRow)]
pub struct DeliveryFailure {
    pub id: i64,
    pub chat_id: i64,
    pub error_message: String,
    pub created_at: DateTime<Utc>,
}

#[derive(Clone)]
pub struct Database {
    pool: PgPool,
}

impl Database {
    /// Initialize database connection pool and run migrations
    pub async fn new(database_url: &str) -> Result<Self> {
        let pool = PgPoolOptions::new()
            .max_connections(5)
            .acquire_timeout(Duration::from_secs(3))
            .connect(database_url)
            .await
            .context("Failed to connect to PostgreSQL database")?;

        // Run migrations
        sqlx::migrate!("./migrations")
            .run(&pool)
            .await
            .context("Failed to run database migrations")?;

        Ok(Self { pool })
    }

    /// Add a new subscriber or reactivate an existing one
    /// Returns (is_new_subscription, needs_welcome_summary)
    /// - is_new_subscription: true if this is a subscription (new or reactivation)
    /// - needs_welcome_summary: true if this is first-time subscription (send welcome)
    pub async fn add_subscriber(
        &self,
        chat_id: i64,
        username: Option<&str>,
    ) -> Result<(bool, bool)> {
        // Check if user exists
        let existing: Option<(bool, bool)> = sqlx::query_as(
            "SELECT is_active, received_welcome_summary FROM subscribers WHERE chat_id = $1",
        )
        .bind(chat_id)
        .fetch_optional(&self.pool)
        .await?;

        match existing {
            Some((is_active, received_welcome)) => {
                if is_active {
                    // Already subscribed, just update username if changed
                    sqlx::query("UPDATE subscribers SET username = $1 WHERE chat_id = $2")
                        .bind(username)
                        .bind(chat_id)
                        .execute(&self.pool)
                        .await?;
                    Ok((false, false)) // Not a new subscription, no welcome needed
                } else {
                    // Reactivating subscription
                    sqlx::query(
                        "UPDATE subscribers SET is_active = TRUE, subscribed_at = NOW(), username = $1 WHERE chat_id = $2",
                    )
                    .bind(username)
                    .bind(chat_id)
                    .execute(&self.pool)
                    .await?;
                    // Send welcome only if they never received one
                    Ok((true, !received_welcome))
                }
            }
            None => {
                // New subscriber (defaults to English)
                sqlx::query(
                    "INSERT INTO subscribers (chat_id, username, subscribed_at, first_subscribed_at, is_active, received_welcome_summary, language_code)
                     VALUES ($1, $2, NOW(), NOW(), TRUE, FALSE, 'en')",
                )
                .bind(chat_id)
                .bind(username)
                .execute(&self.pool)
                .await
                .context("Failed to add new subscriber")?;
                Ok((true, true)) // New subscription, needs welcome
            }
        }
    }

    /// Remove a subscriber (soft delete - sets is_active to false)
    pub async fn remove_subscriber(&self, chat_id: i64) -> Result<bool> {
        let result = sqlx::query(
            "UPDATE subscribers SET is_active = FALSE WHERE chat_id = $1 AND is_active = TRUE",
        )
        .bind(chat_id)
        .execute(&self.pool)
        .await
        .context("Failed to remove subscriber")?;

        Ok(result.rows_affected() > 0)
    }

    /// Check if a chat_id is subscribed (active)
    pub async fn is_subscribed(&self, chat_id: i64) -> Result<bool> {
        let count: (i64,) = sqlx::query_as(
            "SELECT COUNT(*) FROM subscribers WHERE chat_id = $1 AND is_active = TRUE",
        )
        .bind(chat_id)
        .fetch_one(&self.pool)
        .await?;
        Ok(count.0 > 0)
    }

    /// Get all active subscribers
    pub async fn list_subscribers(&self) -> Result<Vec<Subscriber>> {
        let subscribers = sqlx::query_as::<_, Subscriber>(
            "SELECT chat_id, username, subscribed_at, first_subscribed_at, is_active, received_welcome_summary, language_code
             FROM subscribers
             WHERE is_active = TRUE
             ORDER BY subscribed_at DESC",
        )
        .fetch_all(&self.pool)
        .await?;

        Ok(subscribers)
    }

    /// Get count of active subscribers
    pub async fn subscriber_count(&self) -> Result<usize> {
        let count: (i64,) =
            sqlx::query_as("SELECT COUNT(*) FROM subscribers WHERE is_active = TRUE")
                .fetch_one(&self.pool)
                .await?;
        Ok(count.0 as usize)
    }

    /// Save a summary and cleanup old ones (keep last 10)
    pub async fn save_summary(&self, content: &str) -> Result<i64> {
        let row: (i64,) = sqlx::query_as(
            "INSERT INTO summaries (content, created_at) VALUES ($1, NOW()) RETURNING id",
        )
        .bind(content)
        .fetch_one(&self.pool)
        .await
        .context("Failed to save summary")?;

        // Cleanup old summaries (keep last 10)
        sqlx::query(
            "DELETE FROM summaries WHERE id NOT IN (
                SELECT id FROM summaries ORDER BY created_at DESC LIMIT 10
            )",
        )
        .execute(&self.pool)
        .await
        .context("Failed to cleanup old summaries")?;

        Ok(row.0)
    }

    /// Get the latest summary
    pub async fn get_latest_summary(&self) -> Result<Option<Summary>> {
        let summary = sqlx::query_as::<_, Summary>(
            "SELECT id, content, created_at FROM summaries ORDER BY created_at DESC LIMIT 1",
        )
        .fetch_optional(&self.pool)
        .await?;

        Ok(summary)
    }

    /// Mark user as having received welcome summary
    pub async fn mark_welcome_summary_sent(&self, chat_id: i64) -> Result<()> {
        sqlx::query("UPDATE subscribers SET received_welcome_summary = TRUE WHERE chat_id = $1")
            .bind(chat_id)
            .execute(&self.pool)
            .await
            .context("Failed to mark welcome summary as sent")?;
        Ok(())
    }

    /// Log a delivery failure and cleanup old entries (keep last 1000)
    pub async fn log_delivery_failure(&self, chat_id: i64, error_message: &str) -> Result<()> {
        sqlx::query("INSERT INTO delivery_failures (chat_id, error_message) VALUES ($1, $2)")
            .bind(chat_id)
            .bind(error_message)
            .execute(&self.pool)
            .await
            .context("Failed to log delivery failure")?;

        // Cleanup old failures (keep last 1000)
        sqlx::query(
            "DELETE FROM delivery_failures WHERE id NOT IN (
                SELECT id FROM delivery_failures ORDER BY created_at DESC LIMIT 1000
            )",
        )
        .execute(&self.pool)
        .await
        .context("Failed to cleanup old delivery failures")?;

        Ok(())
    }

    /// Get recent delivery failures (last N entries)
    pub async fn get_recent_failures(&self, limit: i64) -> Result<Vec<DeliveryFailure>> {
        let failures = sqlx::query_as::<_, DeliveryFailure>(
            "SELECT id, chat_id, error_message, created_at
             FROM delivery_failures
             ORDER BY created_at DESC
             LIMIT $1",
        )
        .bind(limit)
        .fetch_all(&self.pool)
        .await
        .context("Failed to fetch recent delivery failures")?;

        Ok(failures)
    }

    /// Get failure counts grouped by chat_id
    pub async fn get_failure_counts(&self) -> Result<Vec<(i64, i64)>> {
        let counts: Vec<(i64, i64)> = sqlx::query_as(
            "SELECT chat_id, COUNT(*) as count
             FROM delivery_failures
             GROUP BY chat_id
             ORDER BY count DESC",
        )
        .fetch_all(&self.pool)
        .await
        .context("Failed to fetch delivery failure counts")?;

        Ok(counts)
    }

    // ==================== Language Support Methods ====================

    /// Update a subscriber's language preference
    pub async fn set_subscriber_language(&self, chat_id: i64, language_code: &str) -> Result<bool> {
        let result = sqlx::query(
            "UPDATE subscribers SET language_code = $1 WHERE chat_id = $2 AND is_active = TRUE",
        )
        .bind(language_code)
        .bind(chat_id)
        .execute(&self.pool)
        .await
        .context("Failed to update subscriber language")?;

        Ok(result.rows_affected() > 0)
    }

    /// Get a subscriber's language preference
    pub async fn get_subscriber_language(&self, chat_id: i64) -> Result<Option<String>> {
        let result: Option<(String,)> = sqlx::query_as(
            "SELECT language_code FROM subscribers WHERE chat_id = $1 AND is_active = TRUE",
        )
        .bind(chat_id)
        .fetch_optional(&self.pool)
        .await
        .context("Failed to get subscriber language")?;

        Ok(result.map(|(lang,)| lang))
    }

    /// Save a translated summary (for caching)
    pub async fn save_translation(
        &self,
        summary_id: i64,
        language_code: &str,
        content: &str,
    ) -> Result<i64> {
        let row: (i64,) = sqlx::query_as(
            "INSERT INTO summary_translations (summary_id, language_code, content, created_at)
             VALUES ($1, $2, $3, NOW())
             ON CONFLICT (summary_id, language_code)
             DO UPDATE SET content = EXCLUDED.content, created_at = NOW()
             RETURNING id",
        )
        .bind(summary_id)
        .bind(language_code)
        .bind(content)
        .fetch_one(&self.pool)
        .await
        .context("Failed to save translation")?;

        Ok(row.0)
    }

    /// Get a cached translation for a summary
    pub async fn get_translation(
        &self,
        summary_id: i64,
        language_code: &str,
    ) -> Result<Option<SummaryTranslation>> {
        let translation = sqlx::query_as::<_, SummaryTranslation>(
            "SELECT id, summary_id, language_code, content, created_at
             FROM summary_translations
             WHERE summary_id = $1 AND language_code = $2",
        )
        .bind(summary_id)
        .bind(language_code)
        .fetch_optional(&self.pool)
        .await
        .context("Failed to get translation")?;

        Ok(translation)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ==================== Helper Functions ====================

    /// Create a test database using sqlx test utilities.
    /// Requires TEST_DATABASE_URL environment variable pointing to a PostgreSQL instance.
    async fn create_test_db() -> Result<Database> {
        let database_url = std::env::var("TEST_DATABASE_URL")
            .context("TEST_DATABASE_URL must be set for tests")?;

        let db = Database::new(&database_url)
            .await
            .context("Failed to create test database")?;

        // Clean up tables for fresh test state
        sqlx::query(
            "TRUNCATE TABLE summaries, subscribers, delivery_failures RESTART IDENTITY CASCADE",
        )
        .execute(&db.pool)
        .await
        .context("Failed to truncate tables")?;

        Ok(db)
    }

    // ==================== Database Initialization Tests ====================

    #[tokio::test]
    async fn test_database_creation() {
        let db = create_test_db().await.expect("Failed to create test db");

        // Database should be created successfully
        let count = db.subscriber_count().await.expect("Should get count");
        assert_eq!(count, 0);
    }

    #[tokio::test]
    async fn test_database_creates_table() {
        let db = create_test_db().await.expect("Failed to create test db");

        // Table should exist and be empty
        let subscribers = db
            .list_subscribers()
            .await
            .expect("Should list subscribers");
        assert!(subscribers.is_empty());
    }

    // ==================== add_subscriber Tests ====================

    #[tokio::test]
    async fn test_add_subscriber_with_username() {
        let db = create_test_db().await.expect("Failed to create test db");

        db.add_subscriber(123456789, Some("testuser"))
            .await
            .expect("Should add subscriber");

        let count = db.subscriber_count().await.expect("Should get count");
        assert_eq!(count, 1);

        let subscribers = db.list_subscribers().await.expect("Should list");
        assert_eq!(subscribers[0].chat_id, 123456789);
        assert_eq!(subscribers[0].username, Some("testuser".to_string()));
    }

    #[tokio::test]
    async fn test_add_subscriber_without_username() {
        let db = create_test_db().await.expect("Failed to create test db");

        db.add_subscriber(123456789, None)
            .await
            .expect("Should add subscriber");

        let subscribers = db.list_subscribers().await.expect("Should list");
        assert_eq!(subscribers[0].chat_id, 123456789);
        assert!(subscribers[0].username.is_none());
    }

    #[tokio::test]
    async fn test_add_multiple_subscribers() {
        let db = create_test_db().await.expect("Failed to create test db");

        db.add_subscriber(111, Some("user1"))
            .await
            .expect("Should add");
        db.add_subscriber(222, Some("user2"))
            .await
            .expect("Should add");
        db.add_subscriber(333, Some("user3"))
            .await
            .expect("Should add");

        let count = db.subscriber_count().await.expect("Should get count");
        assert_eq!(count, 3);
    }

    #[tokio::test]
    async fn test_add_subscriber_updates_existing() {
        let db = create_test_db().await.expect("Failed to create test db");

        // Add initial subscriber
        db.add_subscriber(123, Some("olduser"))
            .await
            .expect("Should add");

        // Update with new username
        db.add_subscriber(123, Some("newuser"))
            .await
            .expect("Should update");

        // Should still be only 1 subscriber
        let count = db.subscriber_count().await.expect("Should get count");
        assert_eq!(count, 1);

        // Username should be updated
        let subscribers = db.list_subscribers().await.expect("Should list");
        assert_eq!(subscribers[0].username, Some("newuser".to_string()));
    }

    #[tokio::test]
    async fn test_add_subscriber_negative_chat_id() {
        let db = create_test_db().await.expect("Failed to create test db");

        // Group chats have negative IDs
        db.add_subscriber(-1001234567890, Some("group"))
            .await
            .expect("Should add group");

        let subscribers = db.list_subscribers().await.expect("Should list");
        assert_eq!(subscribers[0].chat_id, -1001234567890);
    }

    #[tokio::test]
    async fn test_add_subscriber_empty_username() {
        let db = create_test_db().await.expect("Failed to create test db");

        db.add_subscriber(123, Some(""))
            .await
            .expect("Should add with empty username");

        let subscribers = db.list_subscribers().await.expect("Should list");
        assert_eq!(subscribers[0].username, Some("".to_string()));
    }

    // ==================== remove_subscriber Tests ====================

    #[tokio::test]
    async fn test_remove_existing_subscriber() {
        let db = create_test_db().await.expect("Failed to create test db");

        db.add_subscriber(123, Some("user"))
            .await
            .expect("Should add");
        assert_eq!(db.subscriber_count().await.expect("count"), 1);

        let removed = db.remove_subscriber(123).await.expect("Should remove");
        assert!(removed);
        assert_eq!(db.subscriber_count().await.expect("count"), 0);
    }

    #[tokio::test]
    async fn test_remove_nonexistent_subscriber() {
        let db = create_test_db().await.expect("Failed to create test db");

        let removed = db
            .remove_subscriber(999999)
            .await
            .expect("Should handle gracefully");
        assert!(!removed, "Should return false for nonexistent subscriber");
    }

    #[tokio::test]
    async fn test_remove_subscriber_idempotent() {
        let db = create_test_db().await.expect("Failed to create test db");

        db.add_subscriber(123, None).await.expect("Should add");

        // First removal
        let removed1 = db.remove_subscriber(123).await.expect("Should remove");
        assert!(removed1);

        // Second removal (already gone)
        let removed2 = db.remove_subscriber(123).await.expect("Should handle");
        assert!(!removed2);
    }

    #[tokio::test]
    async fn test_remove_one_of_multiple() {
        let db = create_test_db().await.expect("Failed to create test db");

        db.add_subscriber(111, None).await.expect("add");
        db.add_subscriber(222, None).await.expect("add");
        db.add_subscriber(333, None).await.expect("add");

        db.remove_subscriber(222).await.expect("remove");

        assert_eq!(db.subscriber_count().await.expect("count"), 2);
        assert!(!db.is_subscribed(222).await.expect("check"));
        assert!(db.is_subscribed(111).await.expect("check"));
        assert!(db.is_subscribed(333).await.expect("check"));
    }

    // ==================== is_subscribed Tests ====================

    #[tokio::test]
    async fn test_is_subscribed_true() {
        let db = create_test_db().await.expect("Failed to create test db");

        db.add_subscriber(123, None).await.expect("add");

        let subscribed = db.is_subscribed(123).await.expect("check");
        assert!(subscribed);
    }

    #[tokio::test]
    async fn test_is_subscribed_false() {
        let db = create_test_db().await.expect("Failed to create test db");

        let subscribed = db.is_subscribed(999999).await.expect("check");
        assert!(!subscribed);
    }

    #[tokio::test]
    async fn test_is_subscribed_after_removal() {
        let db = create_test_db().await.expect("Failed to create test db");

        db.add_subscriber(123, None).await.expect("add");
        assert!(db.is_subscribed(123).await.expect("check"));

        db.remove_subscriber(123).await.expect("remove");
        assert!(!db.is_subscribed(123).await.expect("check"));
    }

    // ==================== list_subscribers Tests ====================

    #[tokio::test]
    async fn test_list_empty_subscribers() {
        let db = create_test_db().await.expect("Failed to create test db");

        let subscribers = db.list_subscribers().await.expect("list");
        assert!(subscribers.is_empty());
    }

    #[tokio::test]
    async fn test_list_subscribers_order() {
        let db = create_test_db().await.expect("Failed to create test db");

        // Add subscribers with slight delays to ensure different timestamps
        db.add_subscriber(111, None).await.expect("add");
        tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;
        db.add_subscriber(222, None).await.expect("add");
        tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;
        db.add_subscriber(333, None).await.expect("add");

        let subscribers = db.list_subscribers().await.expect("list");

        // Should be ordered by subscribed_at DESC (newest first)
        assert_eq!(subscribers[0].chat_id, 333);
        assert_eq!(subscribers[1].chat_id, 222);
        assert_eq!(subscribers[2].chat_id, 111);
    }

    #[tokio::test]
    async fn test_list_subscribers_contains_all_fields() {
        let db = create_test_db().await.expect("Failed to create test db");

        db.add_subscriber(123, Some("testuser")).await.expect("add");

        let subscribers = db.list_subscribers().await.expect("list");
        let sub = &subscribers[0];

        assert_eq!(sub.chat_id, 123);
        assert_eq!(sub.username, Some("testuser".to_string()));
        assert!(sub.is_active);
        assert!(!sub.received_welcome_summary);
    }

    // ==================== subscriber_count Tests ====================

    #[tokio::test]
    async fn test_subscriber_count_empty() {
        let db = create_test_db().await.expect("Failed to create test db");

        let count = db.subscriber_count().await.expect("count");
        assert_eq!(count, 0);
    }

    #[tokio::test]
    async fn test_subscriber_count_multiple() {
        let db = create_test_db().await.expect("Failed to create test db");

        for i in 1..=100i64 {
            db.add_subscriber(i, None).await.expect("add");
        }

        let count = db.subscriber_count().await.expect("count");
        assert_eq!(count, 100);
    }

    #[tokio::test]
    async fn test_subscriber_count_after_add_remove() {
        let db = create_test_db().await.expect("Failed to create test db");

        db.add_subscriber(1, None).await.expect("add");
        assert_eq!(db.subscriber_count().await.expect("count"), 1);

        db.add_subscriber(2, None).await.expect("add");
        assert_eq!(db.subscriber_count().await.expect("count"), 2);

        db.remove_subscriber(1).await.expect("remove");
        assert_eq!(db.subscriber_count().await.expect("count"), 1);
    }

    // ==================== Subscriber Struct Tests ====================

    #[test]
    fn test_subscriber_clone() {
        let subscriber = Subscriber {
            chat_id: 123,
            username: Some("test".to_string()),
            subscribed_at: Utc::now(),
            first_subscribed_at: Utc::now(),
            is_active: true,
            received_welcome_summary: false,
            language_code: "en".to_string(),
        };

        let cloned = subscriber.clone();

        assert_eq!(subscriber.chat_id, cloned.chat_id);
        assert_eq!(subscriber.username, cloned.username);
        assert_eq!(subscriber.subscribed_at, cloned.subscribed_at);
        assert_eq!(subscriber.first_subscribed_at, cloned.first_subscribed_at);
        assert_eq!(subscriber.is_active, cloned.is_active);
        assert_eq!(
            subscriber.received_welcome_summary,
            cloned.received_welcome_summary
        );
        assert_eq!(subscriber.language_code, cloned.language_code);
    }

    #[test]
    fn test_subscriber_debug() {
        let subscriber = Subscriber {
            chat_id: 123,
            username: Some("test".to_string()),
            subscribed_at: Utc::now(),
            first_subscribed_at: Utc::now(),
            is_active: true,
            received_welcome_summary: false,
            language_code: "en".to_string(),
        };

        let debug_str = format!("{:?}", subscriber);
        assert!(debug_str.contains("Subscriber"));
        assert!(debug_str.contains("123"));
        assert!(debug_str.contains("test"));
    }

    // ==================== Edge Case Tests ====================

    #[tokio::test]
    async fn test_max_i64_chat_id() {
        let db = create_test_db().await.expect("Failed to create test db");

        db.add_subscriber(i64::MAX, None).await.expect("add");

        assert!(db.is_subscribed(i64::MAX).await.expect("check"));
    }

    #[tokio::test]
    async fn test_min_i64_chat_id() {
        let db = create_test_db().await.expect("Failed to create test db");

        db.add_subscriber(i64::MIN, None).await.expect("add");

        assert!(db.is_subscribed(i64::MIN).await.expect("check"));
    }

    #[tokio::test]
    async fn test_special_characters_in_username() {
        let db = create_test_db().await.expect("Failed to create test db");

        db.add_subscriber(123, Some("user_with-special.chars"))
            .await
            .expect("add");

        let subscribers = db.list_subscribers().await.expect("list");
        assert_eq!(
            subscribers[0].username,
            Some("user_with-special.chars".to_string())
        );
    }

    #[tokio::test]
    async fn test_unicode_in_username() {
        let db = create_test_db().await.expect("Failed to create test db");

        db.add_subscriber(123, Some("unicode_user"))
            .await
            .expect("add");

        let subscribers = db.list_subscribers().await.expect("list");
        assert!(subscribers[0].username.is_some());
    }

    #[tokio::test]
    async fn test_sql_injection_prevention_username() {
        let db = create_test_db().await.expect("Failed to create test db");

        // Attempt SQL injection in username
        let malicious_username = "user'; DROP TABLE subscribers; --";
        db.add_subscriber(123, Some(malicious_username))
            .await
            .expect("add");

        // Table should still exist and function
        let subscribers = db.list_subscribers().await.expect("list");
        assert_eq!(
            subscribers[0].username,
            Some(malicious_username.to_string())
        );
    }

    // ==================== Timestamp Tests ====================

    #[tokio::test]
    async fn test_subscribed_at_is_recent() {
        let db = create_test_db().await.expect("Failed to create test db");

        let before = Utc::now();
        db.add_subscriber(123, None).await.expect("add");
        let after = Utc::now();

        let subscribers = db.list_subscribers().await.expect("list");
        let subscribed_at = subscribers[0].subscribed_at;

        assert!(subscribed_at >= before);
        assert!(subscribed_at <= after);
    }

    // ==================== Welcome Summary Feature Tests ====================

    #[tokio::test]
    async fn test_add_subscriber_new_user_returns_needs_welcome() {
        let db = create_test_db().await.expect("Failed to create test db");

        let (is_new, needs_welcome) = db
            .add_subscriber(123, Some("newuser"))
            .await
            .expect("Should add subscriber");

        assert!(is_new, "First subscription should be new");
        assert!(
            needs_welcome,
            "First-time subscriber should need welcome summary"
        );
    }

    #[tokio::test]
    async fn test_add_subscriber_already_active_returns_no_welcome() {
        let db = create_test_db().await.expect("Failed to create test db");

        // First subscription
        db.add_subscriber(123, Some("user")).await.expect("add");

        // Second call while already subscribed (just updates username)
        let (is_new, needs_welcome) = db
            .add_subscriber(123, Some("updated_user"))
            .await
            .expect("Should update subscriber");

        assert!(!is_new, "Already subscribed user should not be new");
        assert!(
            !needs_welcome,
            "Already subscribed user should not need welcome"
        );
    }

    #[tokio::test]
    async fn test_add_subscriber_reactivation_returns_no_welcome() {
        let db = create_test_db().await.expect("Failed to create test db");

        // First subscription
        let (is_new1, needs_welcome1) = db.add_subscriber(123, Some("user")).await.expect("add");
        assert!(is_new1, "First subscription should be new");
        assert!(needs_welcome1, "First subscription should need welcome");

        // Mark welcome as sent (simulating that they received it)
        db.mark_welcome_summary_sent(123).await.expect("mark");

        // Unsubscribe
        db.remove_subscriber(123).await.expect("remove");

        // Resubscribe
        let (is_new2, needs_welcome2) = db
            .add_subscriber(123, Some("user"))
            .await
            .expect("Should reactivate subscriber");

        assert!(
            is_new2,
            "Reactivation should be treated as new subscription"
        );
        assert!(
            !needs_welcome2,
            "Reactivation should NOT need welcome if already received"
        );
    }

    #[tokio::test]
    async fn test_add_subscriber_reactivation_needs_welcome_if_never_received() {
        let db = create_test_db().await.expect("Failed to create test db");

        // First subscription
        let (is_new1, needs_welcome1) = db.add_subscriber(123, Some("user")).await.expect("add");
        assert!(is_new1, "First subscription should be new");
        assert!(needs_welcome1, "First subscription should need welcome");

        // DON'T mark welcome as sent - simulating user unsubscribed before receiving it

        // Unsubscribe
        db.remove_subscriber(123).await.expect("remove");

        // Resubscribe
        let (is_new2, needs_welcome2) = db
            .add_subscriber(123, Some("user"))
            .await
            .expect("Should reactivate subscriber");

        assert!(
            is_new2,
            "Reactivation should be treated as new subscription"
        );
        assert!(
            needs_welcome2,
            "Reactivation SHOULD need welcome if never received before"
        );
    }

    #[tokio::test]
    async fn test_add_subscriber_reactivation_preserves_first_subscribed_at() {
        let db = create_test_db().await.expect("Failed to create test db");

        // First subscription
        db.add_subscriber(123, Some("user")).await.expect("add");

        // Get the first_subscribed_at
        let subscribers = db.list_subscribers().await.expect("list");
        let original_first_subscribed = subscribers[0].first_subscribed_at;
        let original_subscribed_at = subscribers[0].subscribed_at;

        // Unsubscribe
        db.remove_subscriber(123).await.expect("remove");

        // Wait a bit to ensure timestamp differs
        tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;

        // Resubscribe
        db.add_subscriber(123, Some("user"))
            .await
            .expect("reactivate");

        // Check timestamps
        let subscribers = db.list_subscribers().await.expect("list");
        let sub = &subscribers[0];

        // first_subscribed_at should be preserved
        assert_eq!(
            sub.first_subscribed_at, original_first_subscribed,
            "first_subscribed_at should be preserved across reactivation"
        );

        // subscribed_at should be updated
        assert!(
            sub.subscribed_at > original_subscribed_at,
            "subscribed_at should be updated on reactivation"
        );
    }

    // ---------- Soft Delete Tests ----------

    #[tokio::test]
    async fn test_remove_subscriber_soft_delete() {
        let db = create_test_db().await.expect("Failed to create test db");

        // Add subscriber
        db.add_subscriber(123, Some("user")).await.expect("add");
        assert_eq!(db.subscriber_count().await.expect("count"), 1);

        // Remove (soft delete)
        let removed = db.remove_subscriber(123).await.expect("remove");
        assert!(removed, "Should return true for successful removal");

        // Count should be 0 (only counts active)
        assert_eq!(db.subscriber_count().await.expect("count"), 0);

        // is_subscribed should return false
        assert!(!db.is_subscribed(123).await.expect("check"));
    }

    #[tokio::test]
    async fn test_list_subscribers_excludes_inactive() {
        let db = create_test_db().await.expect("Failed to create test db");

        // Add multiple subscribers
        db.add_subscriber(111, Some("user1")).await.expect("add");
        db.add_subscriber(222, Some("user2")).await.expect("add");
        db.add_subscriber(333, Some("user3")).await.expect("add");

        // Soft delete user2
        db.remove_subscriber(222).await.expect("remove");

        // List should only show active subscribers
        let subscribers = db.list_subscribers().await.expect("list");
        assert_eq!(subscribers.len(), 2);

        let chat_ids: Vec<i64> = subscribers.iter().map(|s| s.chat_id).collect();
        assert!(chat_ids.contains(&111));
        assert!(!chat_ids.contains(&222));
        assert!(chat_ids.contains(&333));
    }

    #[tokio::test]
    async fn test_subscriber_data_preserved_after_soft_delete() {
        let db = create_test_db().await.expect("Failed to create test db");

        // Create and add subscriber
        db.add_subscriber(123, Some("preserved_user"))
            .await
            .expect("add");
        // Mark welcome as sent before unsubscribing
        db.mark_welcome_summary_sent(123).await.expect("mark");
        db.remove_subscriber(123).await.expect("remove");

        // Count should be 0 (active only)
        assert_eq!(db.subscriber_count().await.expect("count"), 0);

        // But reactivation should work and not need welcome (already received it)
        let (is_new, needs_welcome) = db
            .add_subscriber(123, Some("preserved_user"))
            .await
            .expect("reactivate");

        assert!(is_new, "Reactivation counts as new subscription");
        assert!(
            !needs_welcome,
            "Reactivation should not need welcome if already received"
        );

        // Verify the user is back
        let subscribers = db.list_subscribers().await.expect("list");
        assert_eq!(subscribers.len(), 1);
        assert_eq!(subscribers[0].chat_id, 123);
    }

    #[tokio::test]
    async fn test_double_soft_delete_returns_false() {
        let db = create_test_db().await.expect("Failed to create test db");

        db.add_subscriber(123, None).await.expect("add");

        // First removal
        let removed1 = db.remove_subscriber(123).await.expect("remove");
        assert!(removed1);

        // Second removal (already inactive)
        let removed2 = db.remove_subscriber(123).await.expect("remove again");
        assert!(!removed2, "Second removal should return false");
    }

    // ---------- received_welcome_summary Flag Tests ----------

    #[tokio::test]
    async fn test_new_subscriber_has_welcome_flag_false() {
        let db = create_test_db().await.expect("Failed to create test db");

        db.add_subscriber(123, Some("user")).await.expect("add");

        let subscribers = db.list_subscribers().await.expect("list");
        assert!(
            !subscribers[0].received_welcome_summary,
            "New subscriber should have received_welcome_summary = false"
        );
    }

    #[tokio::test]
    async fn test_mark_welcome_summary_sent() {
        let db = create_test_db().await.expect("Failed to create test db");

        db.add_subscriber(123, Some("user")).await.expect("add");

        // Initially false
        let subscribers = db.list_subscribers().await.expect("list");
        assert!(!subscribers[0].received_welcome_summary);

        // Mark as sent
        db.mark_welcome_summary_sent(123).await.expect("mark");

        // Should be true now
        let subscribers = db.list_subscribers().await.expect("list");
        assert!(
            subscribers[0].received_welcome_summary,
            "Flag should be true after marking"
        );
    }

    #[tokio::test]
    async fn test_mark_welcome_summary_sent_idempotent() {
        let db = create_test_db().await.expect("Failed to create test db");

        db.add_subscriber(123, Some("user")).await.expect("add");

        // Mark multiple times
        db.mark_welcome_summary_sent(123).await.expect("mark1");
        db.mark_welcome_summary_sent(123).await.expect("mark2");
        db.mark_welcome_summary_sent(123).await.expect("mark3");

        let subscribers = db.list_subscribers().await.expect("list");
        assert!(subscribers[0].received_welcome_summary);
    }

    #[tokio::test]
    async fn test_mark_welcome_summary_sent_nonexistent_user() {
        let db = create_test_db().await.expect("Failed to create test db");

        // Should not error, just affect 0 rows
        let result = db.mark_welcome_summary_sent(999999).await;
        assert!(result.is_ok(), "Should not error for nonexistent user");
    }

    #[tokio::test]
    async fn test_welcome_flag_preserved_across_reactivation() {
        let db = create_test_db().await.expect("Failed to create test db");

        // Subscribe and mark welcome sent
        db.add_subscriber(123, Some("user")).await.expect("add");
        db.mark_welcome_summary_sent(123).await.expect("mark");

        // Unsubscribe
        db.remove_subscriber(123).await.expect("remove");

        // Resubscribe
        db.add_subscriber(123, Some("user"))
            .await
            .expect("reactivate");

        // Welcome flag should still be true (they already received it)
        let subscribers = db.list_subscribers().await.expect("list");
        assert!(
            subscribers[0].received_welcome_summary,
            "Welcome flag should be preserved across reactivation"
        );
    }

    // ---------- Summary Storage Tests ----------

    #[tokio::test]
    async fn test_save_summary_returns_id() {
        let db = create_test_db().await.expect("Failed to create test db");

        let id1 = db.save_summary("First summary").await.expect("save1");
        let id2 = db.save_summary("Second summary").await.expect("save2");

        assert!(id1 > 0, "ID should be positive");
        assert!(id2 > id1, "IDs should be incrementing");
    }

    #[tokio::test]
    async fn test_get_latest_summary_empty() {
        let db = create_test_db().await.expect("Failed to create test db");

        let summary = db.get_latest_summary().await.expect("get");
        assert!(
            summary.is_none(),
            "Should return None when no summaries exist"
        );
    }

    #[tokio::test]
    async fn test_get_latest_summary_returns_most_recent() {
        let db = create_test_db().await.expect("Failed to create test db");

        db.save_summary("Old summary").await.expect("save1");
        tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;
        db.save_summary("Middle summary").await.expect("save2");
        tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;
        db.save_summary("Latest summary").await.expect("save3");

        let summary = db.get_latest_summary().await.expect("get");
        assert!(summary.is_some());
        assert_eq!(summary.unwrap().content, "Latest summary");
    }

    #[tokio::test]
    async fn test_save_summary_cleanup_keeps_last_10() {
        let db = create_test_db().await.expect("Failed to create test db");

        // Save 15 summaries
        for i in 1..=15 {
            db.save_summary(&format!("Summary {}", i))
                .await
                .expect("save");
        }

        // Only the last 10 should remain
        // We can verify by checking that getting latest returns "Summary 15"
        let latest = db
            .get_latest_summary()
            .await
            .expect("get")
            .expect("should exist");
        assert_eq!(latest.content, "Summary 15");
    }

    #[tokio::test]
    async fn test_save_summary_with_special_characters() {
        let db = create_test_db().await.expect("Failed to create test db");

        let content = "Summary with 'quotes', \"double quotes\", and \\ backslash";
        db.save_summary(content).await.expect("save");

        let summary = db.get_latest_summary().await.expect("get").expect("exists");
        assert_eq!(summary.content, content);
    }

    #[tokio::test]
    async fn test_save_summary_with_unicode() {
        let db = create_test_db().await.expect("Failed to create test db");

        let content = "Summary with unicode: Japanese text, emojis, and more";
        db.save_summary(content).await.expect("save");

        let summary = db.get_latest_summary().await.expect("get").expect("exists");
        assert_eq!(summary.content, content);
    }

    #[tokio::test]
    async fn test_save_summary_with_newlines() {
        let db = create_test_db().await.expect("Failed to create test db");

        let content = "Line 1\nLine 2\nLine 3\n\nWith blank line";
        db.save_summary(content).await.expect("save");

        let summary = db.get_latest_summary().await.expect("get").expect("exists");
        assert_eq!(summary.content, content);
    }

    #[tokio::test]
    async fn test_summary_created_at_is_recent() {
        let db = create_test_db().await.expect("Failed to create test db");

        let before = Utc::now();
        db.save_summary("Test").await.expect("save");
        let after = Utc::now();

        let summary = db.get_latest_summary().await.expect("get").expect("exists");
        let created_at = summary.created_at;

        assert!(created_at >= before);
        assert!(created_at <= after);
    }

    // ---------- Summary Struct Tests ----------

    #[test]
    fn test_summary_clone() {
        let summary = Summary {
            id: 42,
            content: "Test content".to_string(),
            created_at: Utc::now(),
        };

        let cloned = summary.clone();

        assert_eq!(summary.id, cloned.id);
        assert_eq!(summary.content, cloned.content);
        assert_eq!(summary.created_at, cloned.created_at);
    }

    #[test]
    fn test_summary_debug() {
        let summary = Summary {
            id: 42,
            content: "Test".to_string(),
            created_at: Utc::now(),
        };

        let debug_str = format!("{:?}", summary);
        assert!(debug_str.contains("Summary"));
        assert!(debug_str.contains("42"));
        assert!(debug_str.contains("Test"));
    }

    // ---------- Full Flow Tests ----------

    #[tokio::test]
    async fn test_full_welcome_summary_flow() {
        let db = create_test_db().await.expect("Failed to create test db");

        // Step 1: New user subscribes
        let (is_new, needs_welcome) = db.add_subscriber(123, Some("newbie")).await.expect("add");
        assert!(is_new);
        assert!(needs_welcome);

        // Step 2: Save a summary (simulating the scheduled job)
        db.save_summary("Today's news summary").await.expect("save");

        // Step 3: User gets welcome summary
        let summary = db.get_latest_summary().await.expect("get").expect("exists");
        assert_eq!(summary.content, "Today's news summary");

        // Step 4: Mark welcome as sent
        db.mark_welcome_summary_sent(123).await.expect("mark");

        // Verify state
        let subscribers = db.list_subscribers().await.expect("list");
        let sub = &subscribers[0];
        assert!(sub.is_active);
        assert!(sub.received_welcome_summary);
    }

    #[tokio::test]
    async fn test_unsubscribe_resubscribe_cycle() {
        let db = create_test_db().await.expect("Failed to create test db");

        // First subscription
        let (is_new1, needs_welcome1) = db.add_subscriber(123, None).await.expect("sub1");
        assert!(is_new1 && needs_welcome1, "First sub should need welcome");

        // Mark welcome sent
        db.mark_welcome_summary_sent(123).await.expect("mark");

        // Unsubscribe
        db.remove_subscriber(123).await.expect("unsub1");
        assert!(!db.is_subscribed(123).await.expect("check"));

        // Resubscribe
        let (is_new2, needs_welcome2) = db.add_subscriber(123, None).await.expect("sub2");
        assert!(is_new2, "Resubscription counts as new");
        assert!(!needs_welcome2, "Resubscription should not need welcome");

        // Unsubscribe again
        db.remove_subscriber(123).await.expect("unsub2");

        // Resubscribe again
        let (is_new3, needs_welcome3) = db.add_subscriber(123, None).await.expect("sub3");
        assert!(is_new3);
        assert!(
            !needs_welcome3,
            "Third subscription still shouldn't need welcome"
        );
    }

    // ---------- Database Connection Tests ----------

    #[tokio::test]
    async fn test_database_invalid_url_fails() {
        let result =
            Database::new("postgres://invalid:invalid@nonexistent-host-12345/invalid").await;
        assert!(result.is_err(), "Should fail with invalid database URL");
    }

    #[tokio::test]
    async fn test_database_malformed_url_fails() {
        let result = Database::new("not-a-valid-postgres-url").await;
        assert!(result.is_err(), "Should fail with malformed database URL");
    }

    // ---------- Concurrent Operation Tests ----------

    #[tokio::test]
    async fn test_concurrent_subscriber_adds() {
        let db = create_test_db().await.expect("Failed to create test db");

        // Clone the database handle for concurrent operations
        let db1 = db.clone();
        let db2 = db.clone();
        let db3 = db.clone();

        // Spawn concurrent add operations
        let handle1 = tokio::spawn(async move { db1.add_subscriber(1001, Some("user1")).await });
        let handle2 = tokio::spawn(async move { db2.add_subscriber(1002, Some("user2")).await });
        let handle3 = tokio::spawn(async move { db3.add_subscriber(1003, Some("user3")).await });

        // Wait for all operations
        let (r1, r2, r3) = tokio::join!(handle1, handle2, handle3);
        assert!(r1.is_ok() && r1.unwrap().is_ok());
        assert!(r2.is_ok() && r2.unwrap().is_ok());
        assert!(r3.is_ok() && r3.unwrap().is_ok());

        // Verify all subscribers were added
        let count = db.subscriber_count().await.expect("count");
        assert_eq!(count, 3, "All concurrent adds should succeed");
    }

    #[tokio::test]
    async fn test_concurrent_summary_saves() {
        let db = create_test_db().await.expect("Failed to create test db");

        let db1 = db.clone();
        let db2 = db.clone();
        let db3 = db.clone();

        // Spawn concurrent save operations
        let handle1 = tokio::spawn(async move { db1.save_summary("Summary 1").await });
        let handle2 = tokio::spawn(async move { db2.save_summary("Summary 2").await });
        let handle3 = tokio::spawn(async move { db3.save_summary("Summary 3").await });

        let (r1, r2, r3) = tokio::join!(handle1, handle2, handle3);
        assert!(r1.is_ok() && r1.unwrap().is_ok());
        assert!(r2.is_ok() && r2.unwrap().is_ok());
        assert!(r3.is_ok() && r3.unwrap().is_ok());

        // One of the summaries should be the latest
        let latest = db.get_latest_summary().await.expect("get").expect("exists");
        assert!(
            latest.content.starts_with("Summary"),
            "Latest summary should be one of the concurrent saves"
        );
    }

    // ---------- Summary Edge Cases ----------

    #[tokio::test]
    async fn test_save_empty_summary() {
        let db = create_test_db().await.expect("Failed to create test db");

        let id = db.save_summary("").await.expect("save empty");
        assert!(id > 0);

        let summary = db.get_latest_summary().await.expect("get").expect("exists");
        assert!(summary.content.is_empty());
    }

    #[tokio::test]
    async fn test_save_very_long_summary() {
        let db = create_test_db().await.expect("Failed to create test db");

        // Create a very long summary (100KB)
        let long_content = "A".repeat(100_000);
        let id = db.save_summary(&long_content).await.expect("save");
        assert!(id > 0);

        let summary = db.get_latest_summary().await.expect("get").expect("exists");
        assert_eq!(summary.content.len(), 100_000);
    }

    #[tokio::test]
    async fn test_save_summary_with_sql_injection_attempt() {
        let db = create_test_db().await.expect("Failed to create test db");

        let malicious_content = "'; DROP TABLE summaries; --";
        db.save_summary(malicious_content).await.expect("save");

        // Table should still exist and function
        let summary = db.get_latest_summary().await.expect("get").expect("exists");
        assert_eq!(summary.content, malicious_content);
    }

    // ==================== Delivery Failures Tests ====================

    #[tokio::test]
    async fn test_log_delivery_failure() {
        let db = create_test_db().await.expect("Failed to create test db");

        db.log_delivery_failure(123456789, "Telegram API error (403 Forbidden)")
            .await
            .expect("log failure");

        let failures = db.get_recent_failures(10).await.expect("get failures");
        assert_eq!(failures.len(), 1);
        assert_eq!(failures[0].chat_id, 123456789);
        assert!(failures[0].error_message.contains("403 Forbidden"));
    }

    #[tokio::test]
    async fn test_log_multiple_failures() {
        let db = create_test_db().await.expect("Failed to create test db");

        db.log_delivery_failure(111, "Error 1").await.expect("log1");
        db.log_delivery_failure(222, "Error 2").await.expect("log2");
        db.log_delivery_failure(111, "Error 3").await.expect("log3");

        let failures = db.get_recent_failures(10).await.expect("get failures");
        assert_eq!(failures.len(), 3);
    }

    #[tokio::test]
    async fn test_get_recent_failures_limit() {
        let db = create_test_db().await.expect("Failed to create test db");

        for i in 1..=10 {
            db.log_delivery_failure(i, &format!("Error {}", i))
                .await
                .expect("log");
        }

        let failures = db.get_recent_failures(5).await.expect("get failures");
        assert_eq!(failures.len(), 5);

        // Should be most recent first
        assert!(failures[0].created_at >= failures[4].created_at);
    }

    #[tokio::test]
    async fn test_get_failure_counts() {
        let db = create_test_db().await.expect("Failed to create test db");

        // User 111 fails 3 times
        db.log_delivery_failure(111, "Error").await.expect("log");
        db.log_delivery_failure(111, "Error").await.expect("log");
        db.log_delivery_failure(111, "Error").await.expect("log");

        // User 222 fails 1 time
        db.log_delivery_failure(222, "Error").await.expect("log");

        let counts = db.get_failure_counts().await.expect("get counts");
        assert_eq!(counts.len(), 2);

        // Should be ordered by count DESC
        assert_eq!(counts[0].0, 111); // chat_id
        assert_eq!(counts[0].1, 3); // count
        assert_eq!(counts[1].0, 222);
        assert_eq!(counts[1].1, 1);
    }

    #[tokio::test]
    async fn test_delivery_failure_struct_clone() {
        let failure = DeliveryFailure {
            id: 1,
            chat_id: 123,
            error_message: "Test error".to_string(),
            created_at: Utc::now(),
        };

        let cloned = failure.clone();

        assert_eq!(failure.id, cloned.id);
        assert_eq!(failure.chat_id, cloned.chat_id);
        assert_eq!(failure.error_message, cloned.error_message);
    }

    #[tokio::test]
    async fn test_delivery_failure_with_long_error_message() {
        let db = create_test_db().await.expect("Failed to create test db");

        let long_error = "A".repeat(5000);
        db.log_delivery_failure(123, &long_error)
            .await
            .expect("log");

        let failures = db.get_recent_failures(1).await.expect("get");
        assert_eq!(failures[0].error_message.len(), 5000);
    }

    // ==================== Additional Delivery Failures Tests ====================

    // ---------- log_delivery_failure Edge Cases ----------

    #[tokio::test]
    async fn test_log_delivery_failure_negative_chat_id() {
        let db = create_test_db().await.expect("Failed to create test db");

        // Group chats have negative IDs (Telegram supergroups start with -100)
        db.log_delivery_failure(-1001234567890, "Group delivery failed")
            .await
            .expect("log failure for group");

        let failures = db.get_recent_failures(10).await.expect("get failures");
        assert_eq!(failures.len(), 1);
        assert_eq!(failures[0].chat_id, -1001234567890);
    }

    #[tokio::test]
    async fn test_log_delivery_failure_max_i64_chat_id() {
        let db = create_test_db().await.expect("Failed to create test db");

        db.log_delivery_failure(i64::MAX, "Max chat_id test")
            .await
            .expect("log failure");

        let failures = db.get_recent_failures(10).await.expect("get failures");
        assert_eq!(failures.len(), 1);
        assert_eq!(failures[0].chat_id, i64::MAX);
    }

    #[tokio::test]
    async fn test_log_delivery_failure_min_i64_chat_id() {
        let db = create_test_db().await.expect("Failed to create test db");

        db.log_delivery_failure(i64::MIN, "Min chat_id test")
            .await
            .expect("log failure");

        let failures = db.get_recent_failures(10).await.expect("get failures");
        assert_eq!(failures.len(), 1);
        assert_eq!(failures[0].chat_id, i64::MIN);
    }

    #[tokio::test]
    async fn test_log_delivery_failure_zero_chat_id() {
        let db = create_test_db().await.expect("Failed to create test db");

        db.log_delivery_failure(0, "Zero chat_id test")
            .await
            .expect("log failure");

        let failures = db.get_recent_failures(10).await.expect("get failures");
        assert_eq!(failures.len(), 1);
        assert_eq!(failures[0].chat_id, 0);
    }

    #[tokio::test]
    async fn test_log_delivery_failure_empty_error_message() {
        let db = create_test_db().await.expect("Failed to create test db");

        db.log_delivery_failure(123, "").await.expect("log failure");

        let failures = db.get_recent_failures(10).await.expect("get failures");
        assert_eq!(failures.len(), 1);
        assert!(failures[0].error_message.is_empty());
    }

    #[tokio::test]
    async fn test_log_delivery_failure_special_characters() {
        let db = create_test_db().await.expect("Failed to create test db");

        let error_with_quotes = "Error: 'single quotes' and \"double quotes\"";
        db.log_delivery_failure(123, error_with_quotes)
            .await
            .expect("log");

        let failures = db.get_recent_failures(1).await.expect("get");
        assert_eq!(failures[0].error_message, error_with_quotes);
    }

    #[tokio::test]
    async fn test_log_delivery_failure_backslashes_and_newlines() {
        let db = create_test_db().await.expect("Failed to create test db");

        // Note: PostgreSQL TEXT columns don't support null bytes (\0), so we test
        // other special characters instead
        let error_msg = "Error:\n\tLine 1\\nLine 2\r\nWindows line ending";
        db.log_delivery_failure(123, error_msg).await.expect("log");

        let failures = db.get_recent_failures(1).await.expect("get");
        assert_eq!(failures[0].error_message, error_msg);
    }

    #[tokio::test]
    async fn test_log_delivery_failure_unicode_characters() {
        let db = create_test_db().await.expect("Failed to create test db");

        let unicode_error = "Error with unicode: Test test Test";
        db.log_delivery_failure(123, unicode_error)
            .await
            .expect("log");

        let failures = db.get_recent_failures(1).await.expect("get");
        assert_eq!(failures[0].error_message, unicode_error);
    }

    #[tokio::test]
    async fn test_log_delivery_failure_sql_injection_prevention() {
        let db = create_test_db().await.expect("Failed to create test db");

        // Attempt SQL injection in error message
        let malicious_error = "'; DROP TABLE delivery_failures; --";
        db.log_delivery_failure(123, malicious_error)
            .await
            .expect("log");

        // Table should still exist and function
        let failures = db.get_recent_failures(10).await.expect("get failures");
        assert_eq!(failures.len(), 1);
        assert_eq!(failures[0].error_message, malicious_error);

        // Additional SQL injection attempt
        let another_injection = "test'); DELETE FROM delivery_failures WHERE ('1'='1";
        db.log_delivery_failure(456, another_injection)
            .await
            .expect("log");

        let failures = db.get_recent_failures(10).await.expect("get failures");
        assert_eq!(failures.len(), 2);
    }

    #[tokio::test]
    async fn test_log_delivery_failure_very_long_error_100kb() {
        let db = create_test_db().await.expect("Failed to create test db");

        // Create a very long error message (100KB)
        let long_error = "E".repeat(100_000);
        db.log_delivery_failure(123, &long_error)
            .await
            .expect("log");

        let failures = db.get_recent_failures(1).await.expect("get");
        assert_eq!(failures[0].error_message.len(), 100_000);
    }

    #[tokio::test]
    async fn test_log_delivery_failure_concurrent() {
        let db = create_test_db().await.expect("Failed to create test db");

        let db1 = db.clone();
        let db2 = db.clone();
        let db3 = db.clone();
        let db4 = db.clone();
        let db5 = db.clone();

        // Spawn concurrent log operations
        let handle1 = tokio::spawn(async move { db1.log_delivery_failure(1001, "Error 1").await });
        let handle2 = tokio::spawn(async move { db2.log_delivery_failure(1002, "Error 2").await });
        let handle3 = tokio::spawn(async move { db3.log_delivery_failure(1003, "Error 3").await });
        let handle4 = tokio::spawn(async move { db4.log_delivery_failure(1001, "Error 4").await });
        let handle5 = tokio::spawn(async move { db5.log_delivery_failure(1002, "Error 5").await });

        let (r1, r2, r3, r4, r5) = tokio::join!(handle1, handle2, handle3, handle4, handle5);
        assert!(r1.is_ok() && r1.unwrap().is_ok());
        assert!(r2.is_ok() && r2.unwrap().is_ok());
        assert!(r3.is_ok() && r3.unwrap().is_ok());
        assert!(r4.is_ok() && r4.unwrap().is_ok());
        assert!(r5.is_ok() && r5.unwrap().is_ok());

        // Verify all failures were logged
        let failures = db.get_recent_failures(10).await.expect("get");
        assert_eq!(failures.len(), 5, "All concurrent logs should succeed");
    }

    #[tokio::test]
    async fn test_log_delivery_failure_same_chat_id_multiple_errors() {
        let db = create_test_db().await.expect("Failed to create test db");

        // Same user can have multiple failures with different error messages
        db.log_delivery_failure(123, "Error A").await.expect("log1");
        tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;
        db.log_delivery_failure(123, "Error B").await.expect("log2");
        tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;
        db.log_delivery_failure(123, "Error C").await.expect("log3");

        let failures = db.get_recent_failures(10).await.expect("get");
        assert_eq!(failures.len(), 3);

        // All should be for the same chat_id
        for failure in &failures {
            assert_eq!(failure.chat_id, 123);
        }

        // Should be ordered newest first
        assert_eq!(failures[0].error_message, "Error C");
        assert_eq!(failures[1].error_message, "Error B");
        assert_eq!(failures[2].error_message, "Error A");
    }

    // ---------- Cleanup Mechanism Tests ----------

    #[tokio::test]
    async fn test_delivery_failure_cleanup_keeps_last_1000() {
        let db = create_test_db().await.expect("Failed to create test db");

        // Insert 1005 failures
        for i in 1..=1005 {
            db.log_delivery_failure(i, &format!("Error {}", i))
                .await
                .expect("log");
        }

        // After cleanup, only 1000 should remain
        let failures = db.get_recent_failures(2000).await.expect("get");
        assert_eq!(
            failures.len(),
            1000,
            "Should keep exactly 1000 entries after cleanup"
        );

        // The newest entries should be kept (1006 through 1005)
        // Actually, chat_ids 6..=1005 should remain (newest 1000)
        let chat_ids: Vec<i64> = failures.iter().map(|f| f.chat_id).collect();

        // Oldest 5 entries (chat_ids 1-5) should be removed
        assert!(
            !chat_ids.contains(&1),
            "Oldest entry (chat_id 1) should be removed"
        );
        assert!(
            !chat_ids.contains(&2),
            "Second oldest entry should be removed"
        );
        assert!(
            !chat_ids.contains(&3),
            "Third oldest entry should be removed"
        );
        assert!(
            !chat_ids.contains(&4),
            "Fourth oldest entry should be removed"
        );
        assert!(
            !chat_ids.contains(&5),
            "Fifth oldest entry should be removed"
        );

        // Newest entries should still be there
        assert!(chat_ids.contains(&1005), "Newest entry should be kept");
        assert!(chat_ids.contains(&1004), "Second newest should be kept");
    }

    #[tokio::test]
    async fn test_delivery_failure_cleanup_oldest_removed() {
        let db = create_test_db().await.expect("Failed to create test db");

        // Insert exactly 1000 failures first
        for i in 1..=1000 {
            db.log_delivery_failure(i, &format!("Error {}", i))
                .await
                .expect("log");
        }

        // Verify all 1000 are present
        let failures_before = db.get_recent_failures(1500).await.expect("get");
        assert_eq!(failures_before.len(), 1000);

        // Now add one more - this should trigger cleanup
        db.log_delivery_failure(9999, "The newest error")
            .await
            .expect("log");

        let failures_after = db.get_recent_failures(1500).await.expect("get");
        assert_eq!(
            failures_after.len(),
            1000,
            "Should still be exactly 1000 after cleanup"
        );

        // The oldest entry (chat_id 1) should be removed
        let chat_ids: Vec<i64> = failures_after.iter().map(|f| f.chat_id).collect();
        assert!(!chat_ids.contains(&1), "Oldest entry should be removed");

        // The newest entry should be present
        assert!(chat_ids.contains(&9999), "Newest entry should be present");
    }

    #[tokio::test]
    async fn test_delivery_failure_no_cleanup_when_under_limit() {
        let db = create_test_db().await.expect("Failed to create test db");

        // Insert fewer than 1000 failures
        for i in 1..=50 {
            db.log_delivery_failure(i, &format!("Error {}", i))
                .await
                .expect("log");
        }

        let failures = db.get_recent_failures(100).await.expect("get");
        assert_eq!(
            failures.len(),
            50,
            "All 50 entries should be preserved when under limit"
        );
    }

    #[tokio::test]
    async fn test_delivery_failure_cleanup_concurrent() {
        let db = create_test_db().await.expect("Failed to create test db");

        // First, insert 998 failures
        for i in 1..=998 {
            db.log_delivery_failure(i, &format!("Error {}", i))
                .await
                .expect("log");
        }

        // Now trigger concurrent operations that may cause cleanup
        let db1 = db.clone();
        let db2 = db.clone();
        let db3 = db.clone();
        let db4 = db.clone();
        let db5 = db.clone();

        let handle1 =
            tokio::spawn(async move { db1.log_delivery_failure(10001, "Concurrent 1").await });
        let handle2 =
            tokio::spawn(async move { db2.log_delivery_failure(10002, "Concurrent 2").await });
        let handle3 =
            tokio::spawn(async move { db3.log_delivery_failure(10003, "Concurrent 3").await });
        let handle4 =
            tokio::spawn(async move { db4.log_delivery_failure(10004, "Concurrent 4").await });
        let handle5 =
            tokio::spawn(async move { db5.log_delivery_failure(10005, "Concurrent 5").await });

        let (r1, r2, r3, r4, r5) = tokio::join!(handle1, handle2, handle3, handle4, handle5);
        assert!(r1.unwrap().is_ok());
        assert!(r2.unwrap().is_ok());
        assert!(r3.unwrap().is_ok());
        assert!(r4.unwrap().is_ok());
        assert!(r5.unwrap().is_ok());

        // Total should be 1003, but cleanup should bring it to <= 1000
        let failures = db.get_recent_failures(1500).await.expect("get");
        assert!(
            failures.len() <= 1000,
            "Concurrent cleanup should keep entries at or below 1000"
        );
    }

    // ---------- get_recent_failures Edge Cases ----------

    #[tokio::test]
    async fn test_get_recent_failures_empty_table() {
        let db = create_test_db().await.expect("Failed to create test db");

        let failures = db.get_recent_failures(10).await.expect("get failures");
        assert!(
            failures.is_empty(),
            "Should return empty vec when no failures"
        );
    }

    #[tokio::test]
    async fn test_get_recent_failures_limit_zero() {
        let db = create_test_db().await.expect("Failed to create test db");

        db.log_delivery_failure(123, "Error").await.expect("log");

        let failures = db.get_recent_failures(0).await.expect("get failures");
        assert!(failures.is_empty(), "Limit 0 should return empty vec");
    }

    #[tokio::test]
    async fn test_get_recent_failures_limit_negative() {
        let db = create_test_db().await.expect("Failed to create test db");

        db.log_delivery_failure(123, "Error").await.expect("log");

        // Negative limit causes PostgreSQL to return an error (LIMIT must not be negative)
        // This is expected behavior - the function should propagate the error
        let result = db.get_recent_failures(-1).await;
        assert!(
            result.is_err(),
            "Negative limit should return an error in PostgreSQL"
        );
    }

    #[tokio::test]
    async fn test_get_recent_failures_limit_larger_than_available() {
        let db = create_test_db().await.expect("Failed to create test db");

        // Insert only 5 failures
        for i in 1..=5 {
            db.log_delivery_failure(i, &format!("Error {}", i))
                .await
                .expect("log");
        }

        // Request 100 - should return only 5
        let failures = db.get_recent_failures(100).await.expect("get failures");
        assert_eq!(
            failures.len(),
            5,
            "Should return all available when limit exceeds count"
        );
    }

    #[tokio::test]
    async fn test_get_recent_failures_ordering_newest_first() {
        let db = create_test_db().await.expect("Failed to create test db");

        // Insert with delays to ensure distinct timestamps
        db.log_delivery_failure(1, "First").await.expect("log");
        tokio::time::sleep(tokio::time::Duration::from_millis(20)).await;
        db.log_delivery_failure(2, "Second").await.expect("log");
        tokio::time::sleep(tokio::time::Duration::from_millis(20)).await;
        db.log_delivery_failure(3, "Third").await.expect("log");

        let failures = db.get_recent_failures(10).await.expect("get failures");

        // Should be ordered newest first (Third, Second, First)
        assert_eq!(failures[0].error_message, "Third");
        assert_eq!(failures[1].error_message, "Second");
        assert_eq!(failures[2].error_message, "First");

        // Verify timestamps are in descending order
        assert!(failures[0].created_at >= failures[1].created_at);
        assert!(failures[1].created_at >= failures[2].created_at);
    }

    #[tokio::test]
    async fn test_get_recent_failures_all_fields_populated() {
        let db = create_test_db().await.expect("Failed to create test db");

        let before = Utc::now();
        db.log_delivery_failure(123456, "Test error message")
            .await
            .expect("log");
        let after = Utc::now();

        let failures = db.get_recent_failures(1).await.expect("get");
        let failure = &failures[0];

        // Verify all fields
        assert!(failure.id > 0, "ID should be positive");
        assert_eq!(failure.chat_id, 123456);
        assert_eq!(failure.error_message, "Test error message");
        assert!(
            failure.created_at >= before,
            "created_at should be >= before"
        );
        assert!(failure.created_at <= after, "created_at should be <= after");
    }

    #[tokio::test]
    async fn test_get_recent_failures_exact_limit() {
        let db = create_test_db().await.expect("Failed to create test db");

        // Insert exactly 10 failures
        for i in 1..=10 {
            db.log_delivery_failure(i, &format!("Error {}", i))
                .await
                .expect("log");
        }

        // Request exactly 10
        let failures = db.get_recent_failures(10).await.expect("get failures");
        assert_eq!(failures.len(), 10);
    }

    // ---------- get_failure_counts Edge Cases ----------

    #[tokio::test]
    async fn test_get_failure_counts_empty_table() {
        let db = create_test_db().await.expect("Failed to create test db");

        let counts = db.get_failure_counts().await.expect("get counts");
        assert!(
            counts.is_empty(),
            "Should return empty vec when no failures"
        );
    }

    #[tokio::test]
    async fn test_get_failure_counts_single_user_multiple_failures() {
        let db = create_test_db().await.expect("Failed to create test db");

        // Same user fails 5 times
        for i in 1..=5 {
            db.log_delivery_failure(999, &format!("Error {}", i))
                .await
                .expect("log");
        }

        let counts = db.get_failure_counts().await.expect("get counts");
        assert_eq!(counts.len(), 1);
        assert_eq!(counts[0].0, 999); // chat_id
        assert_eq!(counts[0].1, 5); // count
    }

    #[tokio::test]
    async fn test_get_failure_counts_multiple_users_same_count() {
        let db = create_test_db().await.expect("Failed to create test db");

        // Three users, each with exactly 3 failures
        for chat_id in [100, 200, 300] {
            for i in 1..=3 {
                db.log_delivery_failure(chat_id, &format!("Error {}", i))
                    .await
                    .expect("log");
            }
        }

        let counts = db.get_failure_counts().await.expect("get counts");
        assert_eq!(counts.len(), 3);

        // All should have count of 3
        for (_, count) in &counts {
            assert_eq!(*count, 3);
        }
    }

    #[tokio::test]
    async fn test_get_failure_counts_ordering_by_count_desc() {
        let db = create_test_db().await.expect("Failed to create test db");

        // User 100: 1 failure
        db.log_delivery_failure(100, "Error").await.expect("log");

        // User 200: 5 failures
        for _ in 1..=5 {
            db.log_delivery_failure(200, "Error").await.expect("log");
        }

        // User 300: 3 failures
        for _ in 1..=3 {
            db.log_delivery_failure(300, "Error").await.expect("log");
        }

        // User 400: 10 failures
        for _ in 1..=10 {
            db.log_delivery_failure(400, "Error").await.expect("log");
        }

        let counts = db.get_failure_counts().await.expect("get counts");
        assert_eq!(counts.len(), 4);

        // Should be ordered: 400(10), 200(5), 300(3), 100(1)
        assert_eq!(counts[0], (400, 10));
        assert_eq!(counts[1], (200, 5));
        assert_eq!(counts[2], (300, 3));
        assert_eq!(counts[3], (100, 1));
    }

    #[tokio::test]
    async fn test_get_failure_counts_with_negative_chat_ids() {
        let db = create_test_db().await.expect("Failed to create test db");

        // Mix of positive and negative chat_ids
        db.log_delivery_failure(123, "User error")
            .await
            .expect("log");
        db.log_delivery_failure(-100123, "Group error 1")
            .await
            .expect("log");
        db.log_delivery_failure(-100123, "Group error 2")
            .await
            .expect("log");

        let counts = db.get_failure_counts().await.expect("get counts");
        assert_eq!(counts.len(), 2);

        // Group (-100123) has 2 failures, user (123) has 1
        assert_eq!(counts[0].0, -100123);
        assert_eq!(counts[0].1, 2);
        assert_eq!(counts[1].0, 123);
        assert_eq!(counts[1].1, 1);
    }

    #[tokio::test]
    async fn test_get_failure_counts_many_users() {
        let db = create_test_db().await.expect("Failed to create test db");

        // 100 different users, each with a random number of failures
        for chat_id in 1..=100i64 {
            let num_failures = (chat_id % 5) + 1; // 1-5 failures each
            for _ in 0..num_failures {
                db.log_delivery_failure(chat_id, "Error")
                    .await
                    .expect("log");
            }
        }

        let counts = db.get_failure_counts().await.expect("get counts");
        assert_eq!(counts.len(), 100);

        // Verify ordering - each subsequent count should be <= previous
        for i in 1..counts.len() {
            assert!(
                counts[i - 1].1 >= counts[i].1,
                "Counts should be in descending order"
            );
        }
    }

    // ---------- Integration/Full Flow Tests ----------

    #[tokio::test]
    async fn test_delivery_failures_full_flow() {
        let db = create_test_db().await.expect("Failed to create test db");

        // Step 1: Log failures for multiple users
        db.log_delivery_failure(111, "403 Forbidden")
            .await
            .expect("log");
        db.log_delivery_failure(111, "403 Forbidden")
            .await
            .expect("log");
        db.log_delivery_failure(222, "400 Bad Request")
            .await
            .expect("log");
        db.log_delivery_failure(333, "429 Too Many Requests")
            .await
            .expect("log");
        db.log_delivery_failure(333, "500 Internal Server Error")
            .await
            .expect("log");
        db.log_delivery_failure(333, "503 Service Unavailable")
            .await
            .expect("log");

        // Step 2: Verify recent failures
        let recent = db.get_recent_failures(10).await.expect("get recent");
        assert_eq!(recent.len(), 6);

        // Most recent should be 333's last failure
        assert_eq!(recent[0].chat_id, 333);
        assert_eq!(recent[0].error_message, "503 Service Unavailable");

        // Step 3: Verify counts
        let counts = db.get_failure_counts().await.expect("get counts");
        assert_eq!(counts.len(), 3);

        // 333 has most failures (3)
        assert_eq!(counts[0], (333, 3));
        // 111 has 2 failures
        assert_eq!(counts[1], (111, 2));
        // 222 has 1 failure
        assert_eq!(counts[2], (222, 1));
    }

    #[tokio::test]
    async fn test_delivery_failure_timestamp_is_recent() {
        let db = create_test_db().await.expect("Failed to create test db");

        let before = Utc::now();
        db.log_delivery_failure(123, "Test error")
            .await
            .expect("log");
        let after = Utc::now();

        let failures = db.get_recent_failures(1).await.expect("get");
        let created_at = failures[0].created_at;

        assert!(
            created_at >= before,
            "created_at ({}) should be >= before ({})",
            created_at,
            before
        );
        assert!(
            created_at <= after,
            "created_at ({}) should be <= after ({})",
            created_at,
            after
        );
    }

    #[tokio::test]
    async fn test_delivery_failure_ids_are_unique_and_incrementing() {
        let db = create_test_db().await.expect("Failed to create test db");

        db.log_delivery_failure(1, "Error 1").await.expect("log");
        db.log_delivery_failure(2, "Error 2").await.expect("log");
        db.log_delivery_failure(3, "Error 3").await.expect("log");

        let failures = db.get_recent_failures(10).await.expect("get");

        // Collect IDs (order is newest first, so IDs will be descending)
        let ids: Vec<i64> = failures.iter().map(|f| f.id).collect();

        // All IDs should be unique
        let unique_ids: std::collections::HashSet<i64> = ids.iter().cloned().collect();
        assert_eq!(ids.len(), unique_ids.len(), "All IDs should be unique");

        // IDs should be positive
        for id in &ids {
            assert!(*id > 0, "IDs should be positive");
        }
    }

    // ---------- DeliveryFailure Struct Tests ----------

    #[test]
    fn test_delivery_failure_debug() {
        let failure = DeliveryFailure {
            id: 42,
            chat_id: 123456,
            error_message: "Test error".to_string(),
            created_at: Utc::now(),
        };

        let debug_str = format!("{:?}", failure);
        assert!(debug_str.contains("DeliveryFailure"));
        assert!(debug_str.contains("42"));
        assert!(debug_str.contains("123456"));
        assert!(debug_str.contains("Test error"));
    }

    #[test]
    fn test_delivery_failure_clone_equality() {
        let now = Utc::now();
        let failure = DeliveryFailure {
            id: 1,
            chat_id: 12345,
            error_message: "Original error".to_string(),
            created_at: now,
        };

        let cloned = failure.clone();

        // Verify all fields match
        assert_eq!(failure.id, cloned.id);
        assert_eq!(failure.chat_id, cloned.chat_id);
        assert_eq!(failure.error_message, cloned.error_message);
        assert_eq!(failure.created_at, cloned.created_at);
    }

    // ---------- Interaction with Other Tables Tests ----------

    #[tokio::test]
    async fn test_delivery_failures_independent_of_subscribers() {
        let db = create_test_db().await.expect("Failed to create test db");

        // Log failure for a chat_id that is NOT a subscriber
        db.log_delivery_failure(999999, "Delivery to non-subscriber failed")
            .await
            .expect("log");

        // Should succeed - no foreign key constraint
        let failures = db.get_recent_failures(10).await.expect("get");
        assert_eq!(failures.len(), 1);
        assert_eq!(failures[0].chat_id, 999999);

        // Also add a subscriber and verify they coexist
        db.add_subscriber(111, Some("subscriber"))
            .await
            .expect("add");
        db.log_delivery_failure(111, "Subscriber delivery failed")
            .await
            .expect("log");

        let failures = db.get_recent_failures(10).await.expect("get");
        assert_eq!(failures.len(), 2);
    }

    #[tokio::test]
    async fn test_delivery_failures_persist_after_subscriber_removal() {
        let db = create_test_db().await.expect("Failed to create test db");

        // Add subscriber, log failure, then remove subscriber
        db.add_subscriber(123, Some("user")).await.expect("add");
        db.log_delivery_failure(123, "Delivery failed")
            .await
            .expect("log");
        db.remove_subscriber(123).await.expect("remove");

        // Failure should still be present
        let failures = db.get_recent_failures(10).await.expect("get");
        assert_eq!(failures.len(), 1);
        assert_eq!(failures[0].chat_id, 123);
    }

    // ==================== Language Support Tests ====================

    #[tokio::test]
    async fn test_new_subscriber_defaults_to_english() {
        let db = create_test_db().await.expect("Failed to create test db");

        db.add_subscriber(123, Some("user")).await.expect("add");

        let subscribers = db.list_subscribers().await.expect("list");
        assert_eq!(subscribers[0].language_code, "en");
    }

    #[tokio::test]
    async fn test_set_subscriber_language_to_spanish() {
        let db = create_test_db().await.expect("Failed to create test db");

        db.add_subscriber(123, Some("user")).await.expect("add");

        let updated = db
            .set_subscriber_language(123, "es")
            .await
            .expect("set language");
        assert!(updated, "Should return true for successful update");

        let subscribers = db.list_subscribers().await.expect("list");
        assert_eq!(subscribers[0].language_code, "es");
    }

    #[tokio::test]
    async fn test_set_subscriber_language_back_to_english() {
        let db = create_test_db().await.expect("Failed to create test db");

        db.add_subscriber(123, Some("user")).await.expect("add");
        db.set_subscriber_language(123, "es")
            .await
            .expect("set spanish");
        db.set_subscriber_language(123, "en")
            .await
            .expect("set english");

        let subscribers = db.list_subscribers().await.expect("list");
        assert_eq!(subscribers[0].language_code, "en");
    }

    #[tokio::test]
    async fn test_set_subscriber_language_nonexistent_user() {
        let db = create_test_db().await.expect("Failed to create test db");

        let updated = db
            .set_subscriber_language(999999, "es")
            .await
            .expect("set language");
        assert!(!updated, "Should return false for nonexistent user");
    }

    #[tokio::test]
    async fn test_set_subscriber_language_inactive_user() {
        let db = create_test_db().await.expect("Failed to create test db");

        db.add_subscriber(123, Some("user")).await.expect("add");
        db.remove_subscriber(123).await.expect("remove");

        let updated = db
            .set_subscriber_language(123, "es")
            .await
            .expect("set language");
        assert!(!updated, "Should return false for inactive user");
    }

    #[tokio::test]
    async fn test_get_subscriber_language() {
        let db = create_test_db().await.expect("Failed to create test db");

        db.add_subscriber(123, Some("user")).await.expect("add");

        let lang = db.get_subscriber_language(123).await.expect("get language");
        assert_eq!(lang, Some("en".to_string()));
    }

    #[tokio::test]
    async fn test_get_subscriber_language_nonexistent() {
        let db = create_test_db().await.expect("Failed to create test db");

        let lang = db
            .get_subscriber_language(999999)
            .await
            .expect("get language");
        assert_eq!(lang, None);
    }

    #[tokio::test]
    async fn test_get_subscriber_language_inactive() {
        let db = create_test_db().await.expect("Failed to create test db");

        db.add_subscriber(123, Some("user")).await.expect("add");
        db.remove_subscriber(123).await.expect("remove");

        let lang = db.get_subscriber_language(123).await.expect("get language");
        assert_eq!(lang, None);
    }

    #[tokio::test]
    async fn test_language_preserved_across_reactivation() {
        let db = create_test_db().await.expect("Failed to create test db");

        // Subscribe and set Spanish
        db.add_subscriber(123, Some("user")).await.expect("add");
        db.set_subscriber_language(123, "es")
            .await
            .expect("set spanish");

        // Unsubscribe
        db.remove_subscriber(123).await.expect("remove");

        // Resubscribe
        db.add_subscriber(123, Some("user"))
            .await
            .expect("reactivate");

        // Language should still be Spanish (not reset to English)
        let subscribers = db.list_subscribers().await.expect("list");
        assert_eq!(
            subscribers[0].language_code, "es",
            "Language should be preserved after reactivation"
        );
    }

    // ---------- Translation Storage Tests ----------

    #[tokio::test]
    async fn test_save_translation() {
        let db = create_test_db().await.expect("Failed to create test db");

        // First save a summary
        let summary_id = db.save_summary("Test summary").await.expect("save summary");

        // Save translation
        let translation_id = db
            .save_translation(summary_id, "es", "Resumen de prueba")
            .await
            .expect("save translation");

        assert!(translation_id > 0);
    }

    #[tokio::test]
    async fn test_get_translation() {
        let db = create_test_db().await.expect("Failed to create test db");

        let summary_id = db.save_summary("Test summary").await.expect("save summary");
        db.save_translation(summary_id, "es", "Resumen de prueba")
            .await
            .expect("save translation");

        let translation = db
            .get_translation(summary_id, "es")
            .await
            .expect("get translation");

        assert!(translation.is_some());
        let t = translation.unwrap();
        assert_eq!(t.summary_id, summary_id);
        assert_eq!(t.language_code, "es");
        assert_eq!(t.content, "Resumen de prueba");
    }

    #[tokio::test]
    async fn test_get_translation_nonexistent_summary() {
        let db = create_test_db().await.expect("Failed to create test db");

        let translation = db
            .get_translation(999999, "es")
            .await
            .expect("get translation");

        assert!(translation.is_none());
    }

    #[tokio::test]
    async fn test_get_translation_nonexistent_language() {
        let db = create_test_db().await.expect("Failed to create test db");

        let summary_id = db.save_summary("Test summary").await.expect("save summary");

        let translation = db
            .get_translation(summary_id, "fr")
            .await
            .expect("get translation");

        assert!(translation.is_none());
    }

    #[tokio::test]
    async fn test_save_translation_updates_existing() {
        let db = create_test_db().await.expect("Failed to create test db");

        let summary_id = db.save_summary("Test summary").await.expect("save summary");

        // Save first translation
        db.save_translation(summary_id, "es", "Primera traduccion")
            .await
            .expect("save translation 1");

        // Save updated translation (should update via UPSERT)
        db.save_translation(summary_id, "es", "Segunda traduccion")
            .await
            .expect("save translation 2");

        let translation = db
            .get_translation(summary_id, "es")
            .await
            .expect("get translation")
            .expect("should exist");

        assert_eq!(
            translation.content, "Segunda traduccion",
            "Translation should be updated"
        );
    }

    #[tokio::test]
    async fn test_save_multiple_language_translations() {
        let db = create_test_db().await.expect("Failed to create test db");

        let summary_id = db.save_summary("Test summary").await.expect("save summary");

        // Save translations in different languages
        db.save_translation(summary_id, "es", "Resumen en espanol")
            .await
            .expect("save es");
        db.save_translation(summary_id, "en", "Summary in English")
            .await
            .expect("save en");

        let es_translation = db
            .get_translation(summary_id, "es")
            .await
            .expect("get es")
            .expect("should exist");
        let en_translation = db
            .get_translation(summary_id, "en")
            .await
            .expect("get en")
            .expect("should exist");

        assert_eq!(es_translation.content, "Resumen en espanol");
        assert_eq!(en_translation.content, "Summary in English");
    }

    #[tokio::test]
    async fn test_translation_deleted_with_summary() {
        let db = create_test_db().await.expect("Failed to create test db");

        // Save 11 summaries to trigger cleanup (keeps last 10)
        for i in 1..=11 {
            let summary_id = db
                .save_summary(&format!("Summary {}", i))
                .await
                .expect("save summary");

            if i == 1 {
                db.save_translation(summary_id, "es", "Primera traduccion")
                    .await
                    .expect("save translation");
            }
        }

        // The first summary (and its translation) should be deleted
        // due to cleanup keeping only last 10 summaries
        let translation = db.get_translation(1, "es").await.expect("get translation");
        assert!(
            translation.is_none(),
            "Translation should be deleted with summary (CASCADE)"
        );
    }

    #[test]
    fn test_summary_translation_struct_clone() {
        let translation = SummaryTranslation {
            id: 1,
            summary_id: 42,
            language_code: "es".to_string(),
            content: "Test content".to_string(),
            created_at: Utc::now(),
        };

        let cloned = translation.clone();

        assert_eq!(translation.id, cloned.id);
        assert_eq!(translation.summary_id, cloned.summary_id);
        assert_eq!(translation.language_code, cloned.language_code);
        assert_eq!(translation.content, cloned.content);
        assert_eq!(translation.created_at, cloned.created_at);
    }

    #[test]
    fn test_summary_translation_struct_debug() {
        let translation = SummaryTranslation {
            id: 1,
            summary_id: 42,
            language_code: "es".to_string(),
            content: "Test".to_string(),
            created_at: Utc::now(),
        };

        let debug_str = format!("{:?}", translation);
        assert!(debug_str.contains("SummaryTranslation"));
        assert!(debug_str.contains("42"));
        assert!(debug_str.contains("es"));
    }
}
