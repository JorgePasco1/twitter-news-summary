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
