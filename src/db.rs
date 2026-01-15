use anyhow::{Context, Result};
use rusqlite::{Connection, params};
use chrono::Utc;
use std::sync::{Arc, Mutex};

#[derive(Debug, Clone)]
pub struct Subscriber {
    pub chat_id: String,
    pub username: Option<String>,
    pub subscribed_at: String,
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

        // Create subscribers table
        conn.execute(
            "CREATE TABLE IF NOT EXISTS subscribers (
                chat_id TEXT PRIMARY KEY,
                username TEXT,
                subscribed_at TEXT NOT NULL
            )",
            [],
        ).context("Failed to create subscribers table")?;

        Ok(Self {
            conn: Arc::new(Mutex::new(conn)),
        })
    }

    /// Add a new subscriber
    pub fn add_subscriber(&self, chat_id: &str, username: Option<&str>) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        let subscribed_at = Utc::now().to_rfc3339();

        conn.execute(
            "INSERT OR REPLACE INTO subscribers (chat_id, username, subscribed_at) VALUES (?1, ?2, ?3)",
            params![chat_id, username, subscribed_at],
        ).context("Failed to add subscriber")?;

        Ok(())
    }

    /// Remove a subscriber
    pub fn remove_subscriber(&self, chat_id: &str) -> Result<bool> {
        let conn = self.conn.lock().unwrap();
        let rows_affected = conn.execute(
            "DELETE FROM subscribers WHERE chat_id = ?1",
            params![chat_id],
        ).context("Failed to remove subscriber")?;

        Ok(rows_affected > 0)
    }

    /// Check if a chat_id is subscribed
    pub fn is_subscribed(&self, chat_id: &str) -> Result<bool> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare("SELECT COUNT(*) FROM subscribers WHERE chat_id = ?1")?;
        let count: i64 = stmt.query_row(params![chat_id], |row| row.get(0))?;
        Ok(count > 0)
    }

    /// Get all subscribers
    pub fn list_subscribers(&self) -> Result<Vec<Subscriber>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare("SELECT chat_id, username, subscribed_at FROM subscribers ORDER BY subscribed_at DESC")?;

        let subscribers = stmt.query_map([], |row| {
            Ok(Subscriber {
                chat_id: row.get(0)?,
                username: row.get(1)?,
                subscribed_at: row.get(2)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;

        Ok(subscribers)
    }

    /// Get count of subscribers
    pub fn subscriber_count(&self) -> Result<usize> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare("SELECT COUNT(*) FROM subscribers")?;
        let count: i64 = stmt.query_row([], |row| row.get(0))?;
        Ok(count as usize)
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
        };

        let cloned = subscriber.clone();

        assert_eq!(subscriber.chat_id, cloned.chat_id);
        assert_eq!(subscriber.username, cloned.username);
        assert_eq!(subscriber.subscribed_at, cloned.subscribed_at);
    }

    #[test]
    fn test_subscriber_debug() {
        let subscriber = Subscriber {
            chat_id: "123".to_string(),
            username: Some("test".to_string()),
            subscribed_at: "2024-01-15T10:00:00+00:00".to_string(),
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
