use anyhow::{Context, Result};
use rusqlite::{Connection, params, OptionalExtension};
use chrono::Utc;
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
            ).context("Failed to create subscribers table")?;
        }

        // Create summaries table (safe to run always)
        conn.execute(
            "CREATE TABLE IF NOT EXISTS summaries (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                content TEXT NOT NULL,
                created_at TEXT NOT NULL
            )",
            [],
        ).context("Failed to create summaries table")?;

        Ok(Self {
            conn: Arc::new(Mutex::new(conn)),
        })
    }

    /// Check if database migration is needed
    fn needs_migration(conn: &Connection) -> Result<bool> {
        // Check if subscribers table exists
        let table_exists: bool = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='subscribers'",
                [],
                |row| row.get::<_, i64>(0).map(|count| count > 0),
            )?;

        if !table_exists {
            return Ok(false); // New database, no migration needed
        }

        // Check if is_active column exists
        let column_exists: bool = conn
            .query_row(
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
        ).context("Failed to create new subscribers table")?;

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
            "SELECT is_active, received_welcome_summary FROM subscribers WHERE chat_id = ?1"
        )?;

        let existing: Option<(bool, bool)> = stmt.query_row(params![chat_id], |row| {
            Ok((row.get::<_, i64>(0)? != 0, row.get::<_, i64>(1)? != 0))
        }).optional()?;

        match existing {
            Some((is_active, _received_welcome)) => {
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
                    // It's a resubscription, but they already received welcome before
                    Ok((true, false))
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
        let rows_affected = conn.execute(
            "UPDATE subscribers SET is_active = 0 WHERE chat_id = ?1 AND is_active = 1",
            params![chat_id],
        ).context("Failed to remove subscriber")?;

        Ok(rows_affected > 0)
    }

    /// Check if a chat_id is subscribed (active)
    pub fn is_subscribed(&self, chat_id: &str) -> Result<bool> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare("SELECT COUNT(*) FROM subscribers WHERE chat_id = ?1 AND is_active = 1")?;
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

        let subscribers = stmt.query_map([], |row| {
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
        ).context("Failed to save summary")?;

        let id = conn.last_insert_rowid();

        // Cleanup old summaries (keep last 10)
        self.cleanup_old_summaries()?;

        Ok(id)
    }

    /// Get the latest summary
    pub fn get_latest_summary(&self) -> Result<Option<Summary>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, content, created_at FROM summaries ORDER BY created_at DESC LIMIT 1"
        )?;

        let summary = stmt.query_row([], |row| {
            Ok(Summary {
                id: row.get(0)?,
                content: row.get(1)?,
                created_at: row.get(2)?,
            })
        }).optional()?;

        Ok(summary)
    }

    /// Mark user as having received welcome summary
    pub fn mark_welcome_summary_sent(&self, chat_id: &str) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "UPDATE subscribers SET received_welcome_summary = 1 WHERE chat_id = ?1",
            params![chat_id],
        ).context("Failed to mark welcome summary as sent")?;
        Ok(())
    }

    /// Cleanup old summaries (keep last 10)
    fn cleanup_old_summaries(&self) -> Result<()> {
        let conn = self.conn.lock().unwrap();

        conn.execute(
            "DELETE FROM summaries WHERE id NOT IN (
                SELECT id FROM summaries ORDER BY created_at DESC LIMIT 10
            )",
            [],
        ).context("Failed to cleanup old summaries")?;

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
        let db = Database::new(db_path.to_str().unwrap())
            .expect("Failed to create database");
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
            db.add_subscriber("123", Some("testuser")).expect("Should add");
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

        let removed = db.remove_subscriber("nonexistent")
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
        chrono::DateTime::parse_from_rfc3339(&sub.subscribed_at)
            .expect("Should be valid RFC3339");
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
        assert_eq!(subscriber.received_welcome_summary, cloned.received_welcome_summary);
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

        db.add_subscriber("123", Some("unicode_user"))
            .expect("add");

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
        db.add_subscriber("123", Some(malicious_username)).expect("add");

        // Table should still exist and function
        let subscribers = db.list_subscribers().expect("list");
        assert_eq!(subscribers[0].username, Some(malicious_username.to_string()));
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
}
