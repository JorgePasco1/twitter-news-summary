use anyhow::{Context, Result};
use chrono::Utc;
use rusqlite::{params, Connection, OptionalExtension};
use std::sync::{Arc, Mutex};

#[derive(Debug, Clone)]
pub struct Subscriber {
    pub chat_id: String,
    pub username: Option<String>,
    pub subscribed_at: String,
    pub first_subscribed_at: String,
    pub is_active: bool,
    pub received_welcome_summary: bool,
}

#[derive(Debug, Clone)]
pub struct Summary {
    pub id: i64,
    pub content: String,
    pub created_at: String,
}

#[derive(Clone)]
pub struct Database {
    conn: Arc<Mutex<Connection>>,
}

impl Database {
    /// Initialize database connection and create tables
    pub fn new(database_path: &str) -> Result<Self> {
        let conn = Connection::open(database_path)
            .context(format!("Failed to open database at {}", database_path))?;

        // Check if migration is needed
        let needs_migration = Self::needs_migration(&conn)?;

        if needs_migration {
            Self::run_migration(&conn)?;
        } else {
            // Create tables with new schema (for fresh databases)
            conn.execute(
                "CREATE TABLE IF NOT EXISTS subscribers (
                    chat_id TEXT PRIMARY KEY,
                    username TEXT,
                    subscribed_at TEXT NOT NULL,
                    first_subscribed_at TEXT NOT NULL,
                    is_active INTEGER NOT NULL DEFAULT 1,
                    received_welcome_summary INTEGER NOT NULL DEFAULT 0
                )",
                [],
            )
            .context("Failed to create subscribers table")?;
        }

        // Create summaries table (safe to run always)
        conn.execute(
            "CREATE TABLE IF NOT EXISTS summaries (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                content TEXT NOT NULL,
                created_at TEXT NOT NULL
            )",
            [],
        )
        .context("Failed to create summaries table")?;

        Ok(Self {
            conn: Arc::new(Mutex::new(conn)),
        })
    }

    /// Check if database migration is needed
    fn needs_migration(conn: &Connection) -> Result<bool> {
        // Check if subscribers table exists
        let table_exists: bool = conn.query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='subscribers'",
            [],
            |row| row.get::<_, i64>(0).map(|count| count > 0),
        )?;

        if !table_exists {
            return Ok(false); // New database, no migration needed
        }

        // Check if is_active column exists
        let column_exists: bool = conn.query_row(
            "SELECT COUNT(*) FROM pragma_table_info('subscribers') WHERE name='is_active'",
            [],
            |row| row.get::<_, i64>(0).map(|count| count > 0),
        )?;

        Ok(!column_exists) // Need migration if is_active doesn't exist
    }

    /// Run database migration from old schema to new schema
    fn run_migration(conn: &Connection) -> Result<()> {
        conn.execute("BEGIN TRANSACTION", [])?;

        match Self::run_migration_inner(conn) {
            Ok(_) => {
                conn.execute("COMMIT", [])?;
                Ok(())
            }
            Err(e) => {
                conn.execute("ROLLBACK", [])?;
                Err(e).context("Migration failed and was rolled back")
            }
        }
    }

    fn run_migration_inner(conn: &Connection) -> Result<()> {
        // Create new table with all columns
        conn.execute(
            "CREATE TABLE subscribers_new (
                chat_id TEXT PRIMARY KEY,
                username TEXT,
                subscribed_at TEXT NOT NULL,
                first_subscribed_at TEXT NOT NULL,
                is_active INTEGER NOT NULL DEFAULT 1,
                received_welcome_summary INTEGER NOT NULL DEFAULT 0
            )",
            [],
        )
        .context("Failed to create new subscribers table")?;

        // Copy existing data
        // Existing users: is_active=1, received_welcome_summary=1 (don't send welcome)
        // first_subscribed_at = subscribed_at
        conn.execute(
            "INSERT INTO subscribers_new (chat_id, username, subscribed_at, first_subscribed_at, is_active, received_welcome_summary)
             SELECT chat_id, username, subscribed_at, subscribed_at, 1, 1
             FROM subscribers",
            [],
        ).context("Failed to copy data to new table")?;

        // Drop old table
        conn.execute("DROP TABLE subscribers", [])
            .context("Failed to drop old table")?;

        // Rename new table
        conn.execute("ALTER TABLE subscribers_new RENAME TO subscribers", [])
            .context("Failed to rename table")?;

        Ok(())
    }

    /// Add a new subscriber or reactivate an existing one
    /// Returns (is_new_subscription, needs_welcome_summary)
    /// - is_new_subscription: true if this is a subscription (new or reactivation)
    /// - needs_welcome_summary: true if this is first-time subscription (send welcome)
    pub fn add_subscriber(&self, chat_id: &str, username: Option<&str>) -> Result<(bool, bool)> {
        let conn = self.conn.lock().unwrap();
        let now = Utc::now().to_rfc3339();

        // Check if user exists
        let mut stmt = conn.prepare(
            "SELECT is_active, received_welcome_summary FROM subscribers WHERE chat_id = ?1",
        )?;

        let existing: Option<(bool, bool)> = stmt
            .query_row(params![chat_id], |row| {
                Ok((row.get::<_, i64>(0)? != 0, row.get::<_, i64>(1)? != 0))
            })
            .optional()?;

        match existing {
            Some((is_active, received_welcome)) => {
                if is_active {
                    // Already subscribed, just update username if changed
                    conn.execute(
                        "UPDATE subscribers SET username = ?1 WHERE chat_id = ?2",
                        params![username, chat_id],
                    )?;
                    Ok((false, false)) // Not a new subscription, no welcome needed
                } else {
                    // Reactivating subscription
                    conn.execute(
                        "UPDATE subscribers SET is_active = 1, subscribed_at = ?1, username = ?2 WHERE chat_id = ?3",
                        params![now, username, chat_id],
                    )?;
                    // Send welcome only if they never received one
                    Ok((true, !received_welcome))
                }
            }
            None => {
                // New subscriber
                conn.execute(
                    "INSERT INTO subscribers (chat_id, username, subscribed_at, first_subscribed_at, is_active, received_welcome_summary)
                     VALUES (?1, ?2, ?3, ?3, 1, 0)",
                    params![chat_id, username, now],
                ).context("Failed to add new subscriber")?;
                Ok((true, true)) // New subscription, needs welcome
            }
        }
    }

    /// Remove a subscriber (soft delete - sets is_active to 0)
    pub fn remove_subscriber(&self, chat_id: &str) -> Result<bool> {
        let conn = self.conn.lock().unwrap();
        let rows_affected = conn
            .execute(
                "UPDATE subscribers SET is_active = 0 WHERE chat_id = ?1 AND is_active = 1",
                params![chat_id],
            )
            .context("Failed to remove subscriber")?;

        Ok(rows_affected > 0)
    }

    /// Check if a chat_id is subscribed (active)
    pub fn is_subscribed(&self, chat_id: &str) -> Result<bool> {
        let conn = self.conn.lock().unwrap();
        let mut stmt =
            conn.prepare("SELECT COUNT(*) FROM subscribers WHERE chat_id = ?1 AND is_active = 1")?;
        let count: i64 = stmt.query_row(params![chat_id], |row| row.get(0))?;
        Ok(count > 0)
    }

    /// Get all active subscribers
    pub fn list_subscribers(&self) -> Result<Vec<Subscriber>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT chat_id, username, subscribed_at, first_subscribed_at, is_active, received_welcome_summary
             FROM subscribers
             WHERE is_active = 1
             ORDER BY subscribed_at DESC"
        )?;

        let subscribers = stmt
            .query_map([], |row| {
                Ok(Subscriber {
                    chat_id: row.get(0)?,
                    username: row.get(1)?,
                    subscribed_at: row.get(2)?,
                    first_subscribed_at: row.get(3)?,
                    is_active: row.get::<_, i64>(4)? != 0,
                    received_welcome_summary: row.get::<_, i64>(5)? != 0,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;

        Ok(subscribers)
    }

    /// Get count of active subscribers
    pub fn subscriber_count(&self) -> Result<usize> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare("SELECT COUNT(*) FROM subscribers WHERE is_active = 1")?;
        let count: i64 = stmt.query_row([], |row| row.get(0))?;
        Ok(count as usize)
    }

    /// Save a summary and cleanup old ones (keep last 10)
    pub fn save_summary(&self, content: &str) -> Result<i64> {
        let conn = self.conn.lock().unwrap();
        let created_at = Utc::now().to_rfc3339();

        conn.execute(
            "INSERT INTO summaries (content, created_at) VALUES (?1, ?2)",
            params![content, created_at],
        )
        .context("Failed to save summary")?;

        let id = conn.last_insert_rowid();

        // Cleanup old summaries (keep last 10)
        conn.execute(
            "DELETE FROM summaries WHERE id NOT IN (
                SELECT id FROM summaries ORDER BY created_at DESC LIMIT 10
            )",
            [],
        )
        .context("Failed to cleanup old summaries")?;

        Ok(id)
    }

    /// Get the latest summary
    pub fn get_latest_summary(&self) -> Result<Option<Summary>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, content, created_at FROM summaries ORDER BY created_at DESC LIMIT 1",
        )?;

        let summary = stmt
            .query_row([], |row| {
                Ok(Summary {
                    id: row.get(0)?,
                    content: row.get(1)?,
                    created_at: row.get(2)?,
                })
            })
            .optional()?;

        Ok(summary)
    }

    /// Mark user as having received welcome summary
    pub fn mark_welcome_summary_sent(&self, chat_id: &str) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "UPDATE subscribers SET received_welcome_summary = 1 WHERE chat_id = ?1",
            params![chat_id],
        )
        .context("Failed to mark welcome summary as sent")?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    // ==================== Helper Functions ====================

    /// Create a temporary database for testing
    fn create_test_db() -> (Database, TempDir) {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let db_path = temp_dir.path().join("test_subscribers.db");
        let db = Database::new(db_path.to_str().unwrap()).expect("Failed to create database");
        (db, temp_dir)
    }

    // ==================== Database Initialization Tests ====================

    #[test]
    fn test_database_creation() {
        let (db, _temp_dir) = create_test_db();

        // Database should be created successfully
        let count = db.subscriber_count().expect("Should get count");
        assert_eq!(count, 0);
    }

    #[test]
    fn test_database_creates_table() {
        let (db, _temp_dir) = create_test_db();

        // Table should exist and be empty
        let subscribers = db.list_subscribers().expect("Should list subscribers");
        assert!(subscribers.is_empty());
    }

    #[test]
    fn test_database_reopening() {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let db_path = temp_dir.path().join("test.db");
        let path_str = db_path.to_str().unwrap();

        // Create database and add subscriber
        {
            let db = Database::new(path_str).expect("Failed to create database");
            db.add_subscriber("123", Some("testuser"))
                .expect("Should add");
        }

        // Reopen database
        {
            let db = Database::new(path_str).expect("Failed to reopen database");
            let count = db.subscriber_count().expect("Should get count");
            assert_eq!(count, 1, "Subscriber should persist");
        }
    }

    #[test]
    fn test_invalid_database_path() {
        // Try to create database in non-existent directory
        let result = Database::new("/non/existent/path/db.db");
        assert!(result.is_err());
    }

    // ==================== add_subscriber Tests ====================

    #[test]
    fn test_add_subscriber_with_username() {
        let (db, _temp_dir) = create_test_db();

        db.add_subscriber("123456789", Some("testuser"))
            .expect("Should add subscriber");

        let count = db.subscriber_count().expect("Should get count");
        assert_eq!(count, 1);

        let subscribers = db.list_subscribers().expect("Should list");
        assert_eq!(subscribers[0].chat_id, "123456789");
        assert_eq!(subscribers[0].username, Some("testuser".to_string()));
    }

    #[test]
    fn test_add_subscriber_without_username() {
        let (db, _temp_dir) = create_test_db();

        db.add_subscriber("123456789", None)
            .expect("Should add subscriber");

        let subscribers = db.list_subscribers().expect("Should list");
        assert_eq!(subscribers[0].chat_id, "123456789");
        assert!(subscribers[0].username.is_none());
    }

    #[test]
    fn test_add_multiple_subscribers() {
        let (db, _temp_dir) = create_test_db();

        db.add_subscriber("111", Some("user1")).expect("Should add");
        db.add_subscriber("222", Some("user2")).expect("Should add");
        db.add_subscriber("333", Some("user3")).expect("Should add");

        let count = db.subscriber_count().expect("Should get count");
        assert_eq!(count, 3);
    }

    #[test]
    fn test_add_subscriber_updates_existing() {
        let (db, _temp_dir) = create_test_db();

        // Add initial subscriber
        db.add_subscriber("123", Some("olduser"))
            .expect("Should add");

        // Update with new username
        db.add_subscriber("123", Some("newuser"))
            .expect("Should update");

        // Should still be only 1 subscriber
        let count = db.subscriber_count().expect("Should get count");
        assert_eq!(count, 1);

        // Username should be updated
        let subscribers = db.list_subscribers().expect("Should list");
        assert_eq!(subscribers[0].username, Some("newuser".to_string()));
    }

    #[test]
    fn test_add_subscriber_negative_chat_id() {
        let (db, _temp_dir) = create_test_db();

        // Group chats have negative IDs
        db.add_subscriber("-1001234567890", Some("group"))
            .expect("Should add group");

        let subscribers = db.list_subscribers().expect("Should list");
        assert_eq!(subscribers[0].chat_id, "-1001234567890");
    }

    #[test]
    fn test_add_subscriber_empty_username() {
        let (db, _temp_dir) = create_test_db();

        db.add_subscriber("123", Some(""))
            .expect("Should add with empty username");

        let subscribers = db.list_subscribers().expect("Should list");
        assert_eq!(subscribers[0].username, Some("".to_string()));
    }

    // ==================== remove_subscriber Tests ====================

    #[test]
    fn test_remove_existing_subscriber() {
        let (db, _temp_dir) = create_test_db();

        db.add_subscriber("123", Some("user")).expect("Should add");
        assert_eq!(db.subscriber_count().expect("count"), 1);

        let removed = db.remove_subscriber("123").expect("Should remove");
        assert!(removed);
        assert_eq!(db.subscriber_count().expect("count"), 0);
    }

    #[test]
    fn test_remove_nonexistent_subscriber() {
        let (db, _temp_dir) = create_test_db();

        let removed = db
            .remove_subscriber("nonexistent")
            .expect("Should handle gracefully");
        assert!(!removed, "Should return false for nonexistent subscriber");
    }

    #[test]
    fn test_remove_subscriber_idempotent() {
        let (db, _temp_dir) = create_test_db();

        db.add_subscriber("123", None).expect("Should add");

        // First removal
        let removed1 = db.remove_subscriber("123").expect("Should remove");
        assert!(removed1);

        // Second removal (already gone)
        let removed2 = db.remove_subscriber("123").expect("Should handle");
        assert!(!removed2);
    }

    #[test]
    fn test_remove_one_of_multiple() {
        let (db, _temp_dir) = create_test_db();

        db.add_subscriber("111", None).expect("add");
        db.add_subscriber("222", None).expect("add");
        db.add_subscriber("333", None).expect("add");

        db.remove_subscriber("222").expect("remove");

        assert_eq!(db.subscriber_count().expect("count"), 2);
        assert!(!db.is_subscribed("222").expect("check"));
        assert!(db.is_subscribed("111").expect("check"));
        assert!(db.is_subscribed("333").expect("check"));
    }

    // ==================== is_subscribed Tests ====================

    #[test]
    fn test_is_subscribed_true() {
        let (db, _temp_dir) = create_test_db();

        db.add_subscriber("123", None).expect("add");

        let subscribed = db.is_subscribed("123").expect("check");
        assert!(subscribed);
    }

    #[test]
    fn test_is_subscribed_false() {
        let (db, _temp_dir) = create_test_db();

        let subscribed = db.is_subscribed("nonexistent").expect("check");
        assert!(!subscribed);
    }

    #[test]
    fn test_is_subscribed_after_removal() {
        let (db, _temp_dir) = create_test_db();

        db.add_subscriber("123", None).expect("add");
        assert!(db.is_subscribed("123").expect("check"));

        db.remove_subscriber("123").expect("remove");
        assert!(!db.is_subscribed("123").expect("check"));
    }

    // ==================== list_subscribers Tests ====================

    #[test]
    fn test_list_empty_subscribers() {
        let (db, _temp_dir) = create_test_db();

        let subscribers = db.list_subscribers().expect("list");
        assert!(subscribers.is_empty());
    }

    #[test]
    fn test_list_subscribers_order() {
        let (db, _temp_dir) = create_test_db();

        // Add subscribers with slight delays to ensure different timestamps
        db.add_subscriber("111", None).expect("add");
        std::thread::sleep(std::time::Duration::from_millis(10));
        db.add_subscriber("222", None).expect("add");
        std::thread::sleep(std::time::Duration::from_millis(10));
        db.add_subscriber("333", None).expect("add");

        let subscribers = db.list_subscribers().expect("list");

        // Should be ordered by subscribed_at DESC (newest first)
        assert_eq!(subscribers[0].chat_id, "333");
        assert_eq!(subscribers[1].chat_id, "222");
        assert_eq!(subscribers[2].chat_id, "111");
    }

    #[test]
    fn test_list_subscribers_contains_all_fields() {
        let (db, _temp_dir) = create_test_db();

        db.add_subscriber("123", Some("testuser")).expect("add");

        let subscribers = db.list_subscribers().expect("list");
        let sub = &subscribers[0];

        assert_eq!(sub.chat_id, "123");
        assert_eq!(sub.username, Some("testuser".to_string()));
        assert!(!sub.subscribed_at.is_empty());

        // Verify subscribed_at is valid RFC3339
        chrono::DateTime::parse_from_rfc3339(&sub.subscribed_at).expect("Should be valid RFC3339");
    }

    // ==================== subscriber_count Tests ====================

    #[test]
    fn test_subscriber_count_empty() {
        let (db, _temp_dir) = create_test_db();

        let count = db.subscriber_count().expect("count");
        assert_eq!(count, 0);
    }

    #[test]
    fn test_subscriber_count_multiple() {
        let (db, _temp_dir) = create_test_db();

        for i in 1..=100 {
            db.add_subscriber(&i.to_string(), None).expect("add");
        }

        let count = db.subscriber_count().expect("count");
        assert_eq!(count, 100);
    }

    #[test]
    fn test_subscriber_count_after_add_remove() {
        let (db, _temp_dir) = create_test_db();

        db.add_subscriber("1", None).expect("add");
        assert_eq!(db.subscriber_count().expect("count"), 1);

        db.add_subscriber("2", None).expect("add");
        assert_eq!(db.subscriber_count().expect("count"), 2);

        db.remove_subscriber("1").expect("remove");
        assert_eq!(db.subscriber_count().expect("count"), 1);
    }

    // ==================== Subscriber Struct Tests ====================

    #[test]
    fn test_subscriber_clone() {
        let subscriber = Subscriber {
            chat_id: "123".to_string(),
            username: Some("test".to_string()),
            subscribed_at: "2024-01-15T10:00:00+00:00".to_string(),
            first_subscribed_at: "2024-01-15T10:00:00+00:00".to_string(),
            is_active: true,
            received_welcome_summary: false,
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
    }

    #[test]
    fn test_subscriber_debug() {
        let subscriber = Subscriber {
            chat_id: "123".to_string(),
            username: Some("test".to_string()),
            subscribed_at: "2024-01-15T10:00:00+00:00".to_string(),
            first_subscribed_at: "2024-01-15T10:00:00+00:00".to_string(),
            is_active: true,
            received_welcome_summary: false,
        };

        let debug_str = format!("{:?}", subscriber);
        assert!(debug_str.contains("Subscriber"));
        assert!(debug_str.contains("123"));
        assert!(debug_str.contains("test"));
    }

    // ==================== Concurrency Tests ====================

    #[test]
    fn test_database_clone_shares_connection() {
        let (db, _temp_dir) = create_test_db();
        let db_clone = db.clone();

        // Add via original
        db.add_subscriber("123", None).expect("add");

        // Check via clone
        let subscribed = db_clone.is_subscribed("123").expect("check");
        assert!(subscribed);

        // Count via clone
        let count = db_clone.subscriber_count().expect("count");
        assert_eq!(count, 1);
    }

    // ==================== Edge Case Tests ====================

    #[test]
    fn test_very_long_chat_id() {
        let (db, _temp_dir) = create_test_db();

        let long_id = "1".repeat(100);
        db.add_subscriber(&long_id, None).expect("add");

        assert!(db.is_subscribed(&long_id).expect("check"));
    }

    #[test]
    fn test_special_characters_in_username() {
        let (db, _temp_dir) = create_test_db();

        db.add_subscriber("123", Some("user_with-special.chars"))
            .expect("add");

        let subscribers = db.list_subscribers().expect("list");
        assert_eq!(
            subscribers[0].username,
            Some("user_with-special.chars".to_string())
        );
    }

    #[test]
    fn test_unicode_in_username() {
        let (db, _temp_dir) = create_test_db();

        db.add_subscriber("123", Some("unicode_user")).expect("add");

        let subscribers = db.list_subscribers().expect("list");
        assert!(subscribers[0].username.is_some());
    }

    #[test]
    fn test_sql_injection_prevention_chat_id() {
        let (db, _temp_dir) = create_test_db();

        // Attempt SQL injection in chat_id
        let malicious_id = "123'; DROP TABLE subscribers; --";
        db.add_subscriber(malicious_id, None).expect("add");

        // Table should still exist and function
        let count = db.subscriber_count().expect("count");
        assert_eq!(count, 1);
    }

    #[test]
    fn test_sql_injection_prevention_username() {
        let (db, _temp_dir) = create_test_db();

        // Attempt SQL injection in username
        let malicious_username = "user'; DROP TABLE subscribers; --";
        db.add_subscriber("123", Some(malicious_username))
            .expect("add");

        // Table should still exist and function
        let subscribers = db.list_subscribers().expect("list");
        assert_eq!(
            subscribers[0].username,
            Some(malicious_username.to_string())
        );
    }

    // ==================== Timestamp Tests ====================

    #[test]
    fn test_subscribed_at_is_recent() {
        let (db, _temp_dir) = create_test_db();

        let before = Utc::now();
        db.add_subscriber("123", None).expect("add");
        let after = Utc::now();

        let subscribers = db.list_subscribers().expect("list");
        let subscribed_at = chrono::DateTime::parse_from_rfc3339(&subscribers[0].subscribed_at)
            .expect("parse")
            .with_timezone(&Utc);

        assert!(subscribed_at >= before);
        assert!(subscribed_at <= after);
    }

    // ==================== Welcome Summary Feature Tests ====================

    // ---------- add_subscriber Return Value Tests ----------

    #[test]
    fn test_add_subscriber_new_user_returns_needs_welcome() {
        let (db, _temp_dir) = create_test_db();

        let (is_new, needs_welcome) = db
            .add_subscriber("123", Some("newuser"))
            .expect("Should add subscriber");

        assert!(is_new, "First subscription should be new");
        assert!(
            needs_welcome,
            "First-time subscriber should need welcome summary"
        );
    }

    #[test]
    fn test_add_subscriber_already_active_returns_no_welcome() {
        let (db, _temp_dir) = create_test_db();

        // First subscription
        db.add_subscriber("123", Some("user")).expect("add");

        // Second call while already subscribed (just updates username)
        let (is_new, needs_welcome) = db
            .add_subscriber("123", Some("updated_user"))
            .expect("Should update subscriber");

        assert!(!is_new, "Already subscribed user should not be new");
        assert!(
            !needs_welcome,
            "Already subscribed user should not need welcome"
        );
    }

    #[test]
    fn test_add_subscriber_reactivation_returns_no_welcome() {
        let (db, _temp_dir) = create_test_db();

        // First subscription
        let (is_new1, needs_welcome1) = db.add_subscriber("123", Some("user")).expect("add");
        assert!(is_new1, "First subscription should be new");
        assert!(needs_welcome1, "First subscription should need welcome");

        // Mark welcome as sent (simulating that they received it)
        db.mark_welcome_summary_sent("123").expect("mark");

        // Unsubscribe
        db.remove_subscriber("123").expect("remove");

        // Resubscribe
        let (is_new2, needs_welcome2) = db
            .add_subscriber("123", Some("user"))
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

    #[test]
    fn test_add_subscriber_reactivation_needs_welcome_if_never_received() {
        let (db, _temp_dir) = create_test_db();

        // First subscription
        let (is_new1, needs_welcome1) = db.add_subscriber("123", Some("user")).expect("add");
        assert!(is_new1, "First subscription should be new");
        assert!(needs_welcome1, "First subscription should need welcome");

        // DON'T mark welcome as sent - simulating user unsubscribed before receiving it

        // Unsubscribe
        db.remove_subscriber("123").expect("remove");

        // Resubscribe
        let (is_new2, needs_welcome2) = db
            .add_subscriber("123", Some("user"))
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

    #[test]
    fn test_add_subscriber_reactivation_preserves_first_subscribed_at() {
        let (db, _temp_dir) = create_test_db();

        // First subscription
        db.add_subscriber("123", Some("user")).expect("add");

        // Get the first_subscribed_at
        let subscribers = db.list_subscribers().expect("list");
        let original_first_subscribed = subscribers[0].first_subscribed_at.clone();
        let original_subscribed_at = subscribers[0].subscribed_at.clone();

        // Unsubscribe
        db.remove_subscriber("123").expect("remove");

        // Wait a bit to ensure timestamp differs
        std::thread::sleep(std::time::Duration::from_millis(10));

        // Resubscribe
        db.add_subscriber("123", Some("user")).expect("reactivate");

        // Check timestamps
        let subscribers = db.list_subscribers().expect("list");
        let sub = &subscribers[0];

        // first_subscribed_at should be preserved
        assert_eq!(
            sub.first_subscribed_at, original_first_subscribed,
            "first_subscribed_at should be preserved across reactivation"
        );

        // subscribed_at should be updated
        assert_ne!(
            sub.subscribed_at, original_subscribed_at,
            "subscribed_at should be updated on reactivation"
        );
    }

    // ---------- Soft Delete Tests ----------

    #[test]
    fn test_remove_subscriber_soft_delete() {
        let (db, _temp_dir) = create_test_db();

        // Add subscriber
        db.add_subscriber("123", Some("user")).expect("add");
        assert_eq!(db.subscriber_count().expect("count"), 1);

        // Remove (soft delete)
        let removed = db.remove_subscriber("123").expect("remove");
        assert!(removed, "Should return true for successful removal");

        // Count should be 0 (only counts active)
        assert_eq!(db.subscriber_count().expect("count"), 0);

        // is_subscribed should return false
        assert!(!db.is_subscribed("123").expect("check"));
    }

    #[test]
    fn test_list_subscribers_excludes_inactive() {
        let (db, _temp_dir) = create_test_db();

        // Add multiple subscribers
        db.add_subscriber("111", Some("user1")).expect("add");
        db.add_subscriber("222", Some("user2")).expect("add");
        db.add_subscriber("333", Some("user3")).expect("add");

        // Soft delete user2
        db.remove_subscriber("222").expect("remove");

        // List should only show active subscribers
        let subscribers = db.list_subscribers().expect("list");
        assert_eq!(subscribers.len(), 2);

        let chat_ids: Vec<&str> = subscribers.iter().map(|s| s.chat_id.as_str()).collect();
        assert!(chat_ids.contains(&"111"));
        assert!(!chat_ids.contains(&"222"));
        assert!(chat_ids.contains(&"333"));
    }

    #[test]
    fn test_subscriber_data_preserved_after_soft_delete() {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let db_path = temp_dir.path().join("test.db");
        let path_str = db_path.to_str().unwrap();

        // Create and add subscriber
        {
            let db = Database::new(path_str).expect("create db");
            db.add_subscriber("123", Some("preserved_user"))
                .expect("add");
            // Mark welcome as sent before unsubscribing
            db.mark_welcome_summary_sent("123").expect("mark");
            db.remove_subscriber("123").expect("remove");
        }

        // Reopen database and verify data is still there (just inactive)
        {
            let db = Database::new(path_str).expect("reopen db");

            // Count should be 0 (active only)
            assert_eq!(db.subscriber_count().expect("count"), 0);

            // But reactivation should work and not need welcome (already received it)
            let (is_new, needs_welcome) = db
                .add_subscriber("123", Some("preserved_user"))
                .expect("reactivate");

            assert!(is_new, "Reactivation counts as new subscription");
            assert!(!needs_welcome, "Reactivation should not need welcome if already received");

            // Verify the user is back
            let subscribers = db.list_subscribers().expect("list");
            assert_eq!(subscribers.len(), 1);
            assert_eq!(subscribers[0].chat_id, "123");
        }
    }

    #[test]
    fn test_double_soft_delete_returns_false() {
        let (db, _temp_dir) = create_test_db();

        db.add_subscriber("123", None).expect("add");

        // First removal
        let removed1 = db.remove_subscriber("123").expect("remove");
        assert!(removed1);

        // Second removal (already inactive)
        let removed2 = db.remove_subscriber("123").expect("remove again");
        assert!(!removed2, "Second removal should return false");
    }

    // ---------- received_welcome_summary Flag Tests ----------

    #[test]
    fn test_new_subscriber_has_welcome_flag_false() {
        let (db, _temp_dir) = create_test_db();

        db.add_subscriber("123", Some("user")).expect("add");

        let subscribers = db.list_subscribers().expect("list");
        assert!(
            !subscribers[0].received_welcome_summary,
            "New subscriber should have received_welcome_summary = false"
        );
    }

    #[test]
    fn test_mark_welcome_summary_sent() {
        let (db, _temp_dir) = create_test_db();

        db.add_subscriber("123", Some("user")).expect("add");

        // Initially false
        let subscribers = db.list_subscribers().expect("list");
        assert!(!subscribers[0].received_welcome_summary);

        // Mark as sent
        db.mark_welcome_summary_sent("123").expect("mark");

        // Should be true now
        let subscribers = db.list_subscribers().expect("list");
        assert!(
            subscribers[0].received_welcome_summary,
            "Flag should be true after marking"
        );
    }

    #[test]
    fn test_mark_welcome_summary_sent_idempotent() {
        let (db, _temp_dir) = create_test_db();

        db.add_subscriber("123", Some("user")).expect("add");

        // Mark multiple times
        db.mark_welcome_summary_sent("123").expect("mark1");
        db.mark_welcome_summary_sent("123").expect("mark2");
        db.mark_welcome_summary_sent("123").expect("mark3");

        let subscribers = db.list_subscribers().expect("list");
        assert!(subscribers[0].received_welcome_summary);
    }

    #[test]
    fn test_mark_welcome_summary_sent_nonexistent_user() {
        let (db, _temp_dir) = create_test_db();

        // Should not error, just affect 0 rows
        let result = db.mark_welcome_summary_sent("nonexistent");
        assert!(result.is_ok(), "Should not error for nonexistent user");
    }

    #[test]
    fn test_welcome_flag_preserved_across_reactivation() {
        let (db, _temp_dir) = create_test_db();

        // Subscribe and mark welcome sent
        db.add_subscriber("123", Some("user")).expect("add");
        db.mark_welcome_summary_sent("123").expect("mark");

        // Unsubscribe
        db.remove_subscriber("123").expect("remove");

        // Resubscribe
        db.add_subscriber("123", Some("user")).expect("reactivate");

        // Welcome flag should still be true (they already received it)
        let subscribers = db.list_subscribers().expect("list");
        assert!(
            subscribers[0].received_welcome_summary,
            "Welcome flag should be preserved across reactivation"
        );
    }

    // ---------- Summary Storage Tests ----------

    #[test]
    fn test_save_summary_returns_id() {
        let (db, _temp_dir) = create_test_db();

        let id1 = db.save_summary("First summary").expect("save1");
        let id2 = db.save_summary("Second summary").expect("save2");

        assert!(id1 > 0, "ID should be positive");
        assert!(id2 > id1, "IDs should be incrementing");
    }

    #[test]
    fn test_get_latest_summary_empty() {
        let (db, _temp_dir) = create_test_db();

        let summary = db.get_latest_summary().expect("get");
        assert!(
            summary.is_none(),
            "Should return None when no summaries exist"
        );
    }

    #[test]
    fn test_get_latest_summary_returns_most_recent() {
        let (db, _temp_dir) = create_test_db();

        db.save_summary("Old summary").expect("save1");
        std::thread::sleep(std::time::Duration::from_millis(10));
        db.save_summary("Middle summary").expect("save2");
        std::thread::sleep(std::time::Duration::from_millis(10));
        db.save_summary("Latest summary").expect("save3");

        let summary = db.get_latest_summary().expect("get");
        assert!(summary.is_some());
        assert_eq!(summary.unwrap().content, "Latest summary");
    }

    #[test]
    fn test_save_summary_cleanup_keeps_last_10() {
        let (db, _temp_dir) = create_test_db();

        // Save 15 summaries
        for i in 1..=15 {
            db.save_summary(&format!("Summary {}", i)).expect("save");
        }

        // Only the last 10 should remain
        // We can verify by checking that getting latest returns "Summary 15"
        let latest = db.get_latest_summary().expect("get").expect("should exist");
        assert_eq!(latest.content, "Summary 15");

        // Note: We can't directly count summaries without exposing a method,
        // but we can verify cleanup worked by checking oldest is gone after more adds
    }

    #[test]
    fn test_save_summary_with_special_characters() {
        let (db, _temp_dir) = create_test_db();

        let content = "Summary with 'quotes', \"double quotes\", and \\ backslash";
        db.save_summary(content).expect("save");

        let summary = db.get_latest_summary().expect("get").expect("exists");
        assert_eq!(summary.content, content);
    }

    #[test]
    fn test_save_summary_with_unicode() {
        let (db, _temp_dir) = create_test_db();

        let content = "Summary with unicode: Japanese text, emojis, and more";
        db.save_summary(content).expect("save");

        let summary = db.get_latest_summary().expect("get").expect("exists");
        assert_eq!(summary.content, content);
    }

    #[test]
    fn test_save_summary_with_newlines() {
        let (db, _temp_dir) = create_test_db();

        let content = "Line 1\nLine 2\nLine 3\n\nWith blank line";
        db.save_summary(content).expect("save");

        let summary = db.get_latest_summary().expect("get").expect("exists");
        assert_eq!(summary.content, content);
    }

    #[test]
    fn test_summary_created_at_is_valid_rfc3339() {
        let (db, _temp_dir) = create_test_db();

        let before = Utc::now();
        db.save_summary("Test").expect("save");
        let after = Utc::now();

        let summary = db.get_latest_summary().expect("get").expect("exists");
        let created_at = chrono::DateTime::parse_from_rfc3339(&summary.created_at)
            .expect("Should be valid RFC3339")
            .with_timezone(&Utc);

        assert!(created_at >= before);
        assert!(created_at <= after);
    }

    // ---------- Database Migration Tests ----------

    #[test]
    fn test_migration_from_old_schema() {
        let temp_dir = TempDir::new().expect("create temp dir");
        let db_path = temp_dir.path().join("migrate.db");
        let path_str = db_path.to_str().unwrap();

        // Create database with OLD schema (without new columns)
        {
            let conn = Connection::open(path_str).expect("open");
            conn.execute(
                "CREATE TABLE subscribers (
                    chat_id TEXT PRIMARY KEY,
                    username TEXT,
                    subscribed_at TEXT NOT NULL
                )",
                [],
            )
            .expect("create old table");

            // Insert some old data
            conn.execute(
                "INSERT INTO subscribers (chat_id, username, subscribed_at) VALUES ('123', 'olduser', '2024-01-01T00:00:00+00:00')",
                [],
            ).expect("insert");
            conn.execute(
                "INSERT INTO subscribers (chat_id, username, subscribed_at) VALUES ('456', NULL, '2024-01-02T00:00:00+00:00')",
                [],
            ).expect("insert");
        }

        // Reopen with Database::new which should run migration
        {
            let db = Database::new(path_str).expect("reopen with migration");

            // Verify subscribers were migrated
            let count = db.subscriber_count().expect("count");
            assert_eq!(count, 2, "Both subscribers should be migrated");

            // Verify the data
            let subscribers = db.list_subscribers().expect("list");
            assert_eq!(subscribers.len(), 2);

            // Find user 123
            let user123 = subscribers
                .iter()
                .find(|s| s.chat_id == "123")
                .expect("find 123");
            assert_eq!(user123.username, Some("olduser".to_string()));
            assert!(user123.is_active, "Migrated user should be active");
            assert!(
                user123.received_welcome_summary,
                "Migrated user should have welcome flag = true"
            );
            assert_eq!(
                user123.first_subscribed_at, user123.subscribed_at,
                "Migrated user first_subscribed_at should equal subscribed_at"
            );
        }
    }

    #[test]
    fn test_fresh_database_uses_new_schema() {
        let (db, _temp_dir) = create_test_db();

        // Add a new subscriber
        db.add_subscriber("123", Some("newuser")).expect("add");

        let subscribers = db.list_subscribers().expect("list");
        let sub = &subscribers[0];

        // Verify all new fields exist and have correct defaults
        assert!(sub.is_active);
        assert!(
            !sub.received_welcome_summary,
            "New subscriber should have welcome flag = false"
        );
        assert_eq!(
            sub.first_subscribed_at, sub.subscribed_at,
            "New subscriber first_subscribed_at should equal subscribed_at"
        );
    }

    #[test]
    fn test_migration_does_not_run_on_fresh_db() {
        // This test ensures needs_migration returns false for fresh databases
        let temp_dir = TempDir::new().expect("create temp dir");
        let db_path = temp_dir.path().join("fresh.db");
        let path_str = db_path.to_str().unwrap();

        // Create fresh database
        let _db = Database::new(path_str).expect("create fresh db");

        // If migration ran incorrectly, there would be errors
        // The fact that Database::new succeeds is the test
    }

    #[test]
    fn test_migration_already_migrated_database() {
        let temp_dir = TempDir::new().expect("create temp dir");
        let db_path = temp_dir.path().join("already_migrated.db");
        let path_str = db_path.to_str().unwrap();

        // Create database with new schema
        {
            let _db = Database::new(path_str).expect("create");
        }

        // Reopen - should not attempt migration
        {
            let db = Database::new(path_str).expect("reopen");
            // Add subscriber to verify schema is correct
            db.add_subscriber("123", None).expect("add");
            assert_eq!(db.subscriber_count().expect("count"), 1);
        }
    }

    // ---------- Concurrency Tests ----------

    #[test]
    fn test_concurrent_save_summary_no_deadlock() {
        let (db, _temp_dir) = create_test_db();

        // Spawn multiple threads to save summaries concurrently
        let handles: Vec<_> = (0..10)
            .map(|i| {
                let db_clone = db.clone();
                std::thread::spawn(move || {
                    for j in 0..5 {
                        let content = format!("Summary from thread {} iteration {}", i, j);
                        db_clone
                            .save_summary(&content)
                            .expect("save should not deadlock");
                    }
                })
            })
            .collect();

        // Wait for all threads with timeout
        for handle in handles {
            handle
                .join()
                .expect("Thread should complete without deadlock");
        }

        // Verify we can still access the database
        let summary = db.get_latest_summary().expect("get");
        assert!(
            summary.is_some(),
            "Should have summaries after concurrent writes"
        );
    }

    #[test]
    fn test_concurrent_add_remove_subscribers() {
        let (db, _temp_dir) = create_test_db();

        // Pre-add some subscribers
        for i in 0..50 {
            db.add_subscriber(&format!("{}", i), None).expect("add");
        }

        // Concurrent add/remove operations
        let handles: Vec<_> = (0..5)
            .map(|i| {
                let db_clone = db.clone();
                std::thread::spawn(move || {
                    for j in 0..20 {
                        let id = format!("{}", i * 100 + j);
                        db_clone.add_subscriber(&id, Some("user")).expect("add");

                        // Sometimes remove
                        if j % 3 == 0 {
                            db_clone.remove_subscriber(&id).expect("remove");
                        }
                    }
                })
            })
            .collect();

        for handle in handles {
            handle.join().expect("Thread should complete");
        }

        // Database should still be functional
        let count = db.subscriber_count().expect("count");
        assert!(count > 0, "Should have some subscribers remaining");
    }

    #[test]
    fn test_concurrent_read_write_subscribers() {
        let (db, _temp_dir) = create_test_db();

        // Add initial subscriber
        db.add_subscriber("123", Some("initial")).expect("add");

        // Concurrent reads and writes
        let handles: Vec<_> = (0..10)
            .map(|i| {
                let db_clone = db.clone();
                std::thread::spawn(move || {
                    for j in 0..10 {
                        if i % 2 == 0 {
                            // Reader thread
                            let _ = db_clone.list_subscribers();
                            let _ = db_clone.subscriber_count();
                            let _ = db_clone.is_subscribed("123");
                        } else {
                            // Writer thread - use i and j for unique ID
                            let id = format!("thread{}iter{}", i, j);
                            let _ = db_clone.add_subscriber(&id, None);
                        }
                    }
                })
            })
            .collect();

        for handle in handles {
            handle.join().expect("Thread should complete");
        }

        // Verify database integrity
        assert!(db.is_subscribed("123").expect("check"));
    }

    #[test]
    fn test_save_summary_cleanup_is_atomic_with_insert() {
        let (db, _temp_dir) = create_test_db();

        // Save 15 summaries to trigger cleanup
        for i in 1..=15 {
            db.save_summary(&format!("Summary {}", i)).expect("save");
        }

        // Verify latest is accessible
        let latest = db.get_latest_summary().expect("get").expect("exists");
        assert_eq!(latest.content, "Summary 15");

        // Save more and verify cleanup continues to work
        for i in 16..=25 {
            db.save_summary(&format!("Summary {}", i)).expect("save");
        }

        let latest = db.get_latest_summary().expect("get").expect("exists");
        assert_eq!(latest.content, "Summary 25");
    }

    // ---------- Edge Cases for Welcome Summary Flow ----------

    #[test]
    fn test_full_welcome_summary_flow() {
        let (db, _temp_dir) = create_test_db();

        // Step 1: New user subscribes
        let (is_new, needs_welcome) = db.add_subscriber("123", Some("newbie")).expect("add");
        assert!(is_new);
        assert!(needs_welcome);

        // Step 2: Save a summary (simulating the scheduled job)
        db.save_summary("Today's news summary").expect("save");

        // Step 3: User gets welcome summary
        let summary = db.get_latest_summary().expect("get").expect("exists");
        assert_eq!(summary.content, "Today's news summary");

        // Step 4: Mark welcome as sent
        db.mark_welcome_summary_sent("123").expect("mark");

        // Verify state
        let subscribers = db.list_subscribers().expect("list");
        let sub = &subscribers[0];
        assert!(sub.is_active);
        assert!(sub.received_welcome_summary);
    }

    #[test]
    fn test_subscribe_before_any_summary_exists() {
        let (db, _temp_dir) = create_test_db();

        // User subscribes when no summaries exist
        let (is_new, needs_welcome) = db.add_subscriber("123", Some("early_bird")).expect("add");
        assert!(is_new);
        assert!(needs_welcome);

        // Get latest summary - should be None
        let summary = db.get_latest_summary().expect("get");
        assert!(summary.is_none(), "No summary should exist yet");

        // Application logic would skip sending welcome in this case
        // But the flag should still be marked to prevent sending later
        db.mark_welcome_summary_sent("123").expect("mark");

        let subscribers = db.list_subscribers().expect("list");
        assert!(subscribers[0].received_welcome_summary);
    }

    #[test]
    fn test_unsubscribe_resubscribe_cycle() {
        let (db, _temp_dir) = create_test_db();

        // First subscription
        let (is_new1, needs_welcome1) = db.add_subscriber("123", None).expect("sub1");
        assert!(is_new1 && needs_welcome1, "First sub should need welcome");

        // Mark welcome sent
        db.mark_welcome_summary_sent("123").expect("mark");

        // Unsubscribe
        db.remove_subscriber("123").expect("unsub1");
        assert!(!db.is_subscribed("123").expect("check"));

        // Resubscribe
        let (is_new2, needs_welcome2) = db.add_subscriber("123", None).expect("sub2");
        assert!(is_new2, "Resubscription counts as new");
        assert!(!needs_welcome2, "Resubscription should not need welcome");

        // Unsubscribe again
        db.remove_subscriber("123").expect("unsub2");

        // Resubscribe again
        let (is_new3, needs_welcome3) = db.add_subscriber("123", None).expect("sub3");
        assert!(is_new3);
        assert!(
            !needs_welcome3,
            "Third subscription still shouldn't need welcome"
        );
    }

    #[test]
    fn test_first_subscribed_at_vs_subscribed_at() {
        let (db, _temp_dir) = create_test_db();

        // First subscription
        db.add_subscriber("123", None).expect("add");
        let subs1 = db.list_subscribers().expect("list");
        let first_subscribed = subs1[0].first_subscribed_at.clone();
        let subscribed1 = subs1[0].subscribed_at.clone();

        // They should be equal on first subscription
        assert_eq!(first_subscribed, subscribed1);

        // Unsubscribe and wait
        db.remove_subscriber("123").expect("remove");
        std::thread::sleep(std::time::Duration::from_millis(50));

        // Resubscribe
        db.add_subscriber("123", None).expect("reactivate");
        let subs2 = db.list_subscribers().expect("list");

        // first_subscribed_at should be same as before
        assert_eq!(subs2[0].first_subscribed_at, first_subscribed);

        // subscribed_at should be newer
        let subscribed2_dt =
            chrono::DateTime::parse_from_rfc3339(&subs2[0].subscribed_at).expect("parse");
        let subscribed1_dt = chrono::DateTime::parse_from_rfc3339(&subscribed1).expect("parse");

        assert!(
            subscribed2_dt > subscribed1_dt,
            "subscribed_at should be updated on reactivation"
        );
    }

    // ---------- Summary Struct Tests ----------

    #[test]
    fn test_summary_clone() {
        let summary = Summary {
            id: 42,
            content: "Test content".to_string(),
            created_at: "2024-01-15T10:00:00+00:00".to_string(),
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
            created_at: "2024-01-15T10:00:00+00:00".to_string(),
        };

        let debug_str = format!("{:?}", summary);
        assert!(debug_str.contains("Summary"));
        assert!(debug_str.contains("42"));
        assert!(debug_str.contains("Test"));
    }

    // ---------- Subscriber New Fields Tests ----------

    #[test]
    fn test_subscriber_is_active_field() {
        let (db, _temp_dir) = create_test_db();

        db.add_subscriber("123", None).expect("add");

        let subscribers = db.list_subscribers().expect("list");
        assert!(
            subscribers[0].is_active,
            "Active subscriber should have is_active = true"
        );
    }

    #[test]
    fn test_subscriber_first_subscribed_at_field() {
        let (db, _temp_dir) = create_test_db();

        let before = Utc::now();
        db.add_subscriber("123", None).expect("add");
        let after = Utc::now();

        let subscribers = db.list_subscribers().expect("list");
        let first_subscribed =
            chrono::DateTime::parse_from_rfc3339(&subscribers[0].first_subscribed_at)
                .expect("parse")
                .with_timezone(&Utc);

        assert!(first_subscribed >= before);
        assert!(first_subscribed <= after);
    }

    // ---------- Error Handling Tests ----------

    #[test]
    fn test_database_operations_after_many_operations() {
        let (db, _temp_dir) = create_test_db();

        // Perform many operations
        for i in 0..100 {
            let id = format!("{}", i);
            db.add_subscriber(&id, Some(&format!("user{}", i)))
                .expect("add");
            if i % 2 == 0 {
                db.remove_subscriber(&id).expect("remove");
            }
        }

        for i in 0..50 {
            db.save_summary(&format!("Summary {}", i)).expect("save");
        }

        // Verify database is still functional
        let count = db.subscriber_count().expect("count");
        assert_eq!(count, 50, "Should have 50 active subscribers (odd numbers)");

        let summary = db.get_latest_summary().expect("get");
        assert!(summary.is_some());
    }
}
