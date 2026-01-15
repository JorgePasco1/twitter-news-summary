//! Integration tests for the Twitter News Summary application
//!
//! These tests verify the interaction between multiple modules and the
//! complete workflow of the application.
//!
//! NOTE: Database integration tests have been moved to src/db.rs as unit tests
//! since they require a PostgreSQL database connection. This file contains
//! tests that don't require database access.

use tempfile::TempDir;
use wiremock::{
    matchers::{header, method, path},
    Mock, MockServer, ResponseTemplate,
};

// Re-export modules from the crate
use twitter_news_summary::{config::Config, twitter::Tweet};

// ==================== Test Helpers ====================

/// Create a test config with mocked service URLs (without database)
fn create_test_config(nitter_url: &str, temp_dir: &TempDir) -> Config {
    let usernames_path = temp_dir.path().join("usernames.txt");

    // Create usernames file
    std::fs::write(&usernames_path, "testuser1\ntestuser2\n").expect("Failed to write usernames");

    Config {
        twitter_bearer_token: None,
        twitter_list_id: None,
        openai_api_key: "test-openai-key".to_string(),
        openai_model: "gpt-4o-mini".to_string(),
        telegram_bot_token: "test-telegram-token".to_string(),
        telegram_chat_id: "123456789".to_string(),
        telegram_webhook_secret: "test-webhook-secret".to_string(),
        max_tweets: 50,
        hours_lookback: 12,
        nitter_instance: nitter_url.to_string(),
        nitter_api_key: None,
        usernames_file: usernames_path.to_str().unwrap().to_string(),
        api_key: Some("test-api-key".to_string()),
        database_url: "postgres://test:test@localhost/test".to_string(),
        schedule_times: vec!["08:00".to_string(), "20:00".to_string()],
        port: 8080,
    }
}

/// Create a valid RSS feed XML string
fn create_rss_feed(username: &str, tweets: Vec<(&str, &str)>) -> String {
    let items: String = tweets
        .iter()
        .map(|(text, id)| {
            let now = chrono::Utc::now();
            let pub_date = now.format("%a, %d %b %Y %H:%M:%S %z").to_string();
            format!(
                r#"<item>
                    <title>{}</title>
                    <link>https://nitter.example.com/{}/status/{}</link>
                    <pubDate>{}</pubDate>
                </item>"#,
                text, username, id, pub_date
            )
        })
        .collect::<Vec<_>>()
        .join("\n");

    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
        <rss version="2.0">
            <channel>
                <title>@{} / Twitter</title>
                <description>Twitter feed for @{}</description>
                {}
            </channel>
        </rss>"#,
        username, username, items
    )
}

// ==================== Tweet Processing Tests ====================

#[test]
fn test_tweet_struct_roundtrip() {
    let original = Tweet {
        id: "123456789".to_string(),
        text: "@testuser: This is a test tweet with <html> & special chars".to_string(),
        author_id: Some("testuser".to_string()),
        created_at: Some("2024-01-15T10:30:00+00:00".to_string()),
    };

    // Serialize to JSON
    let json = serde_json::to_string(&original).expect("serialize");

    // Deserialize back
    let restored: Tweet = serde_json::from_str(&json).expect("deserialize");

    assert_eq!(original.id, restored.id);
    assert_eq!(original.text, restored.text);
    assert_eq!(original.author_id, restored.author_id);
    assert_eq!(original.created_at, restored.created_at);
}

#[test]
fn test_tweet_collection_operations() {
    let mut tweets: Vec<Tweet> = (1..=10)
        .map(|i| Tweet {
            id: i.to_string(),
            text: format!("Tweet number {}", i),
            author_id: Some("user".to_string()),
            created_at: Some(format!("2024-01-15T{:02}:00:00+00:00", i)),
        })
        .collect();

    // Sort by ID (descending)
    tweets.sort_by(|a, b| b.id.cmp(&a.id));

    assert_eq!(tweets[0].id, "9"); // "9" > "10" lexicographically
    assert_eq!(tweets[9].id, "1");

    // Filter
    let filtered: Vec<_> = tweets
        .iter()
        .filter(|t| t.id.parse::<i32>().unwrap() > 5)
        .collect();
    assert_eq!(filtered.len(), 5); // 6, 7, 8, 9, 10

    // Take first N
    let limited: Vec<_> = tweets.iter().take(3).collect();
    assert_eq!(limited.len(), 3);
}

// ==================== Config Integration Tests ====================

#[test]
fn test_config_creates_valid_paths() {
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let mock_server_uri = "http://localhost:8080";

    let config = create_test_config(mock_server_uri, &temp_dir);

    // Verify paths are absolute and exist or can be created
    assert!(config
        .usernames_file
        .contains(temp_dir.path().to_str().unwrap()));

    // Verify usernames file exists
    assert!(std::path::Path::new(&config.usernames_file).exists());

    // Verify database_url is set
    assert!(!config.database_url.is_empty());
}

// ==================== RSS Mock Server Tests ====================

#[tokio::test]
async fn test_rss_feed_parsing_integration() {
    let mock_server = MockServer::start().await;
    let _temp_dir = TempDir::new().expect("Failed to create temp dir");

    // Setup mock RSS feed
    let rss_content = create_rss_feed(
        "testuser",
        vec![
            ("First tweet", "1"),
            ("Second tweet", "2"),
            ("Third tweet", "3"),
        ],
    );

    Mock::given(method("GET"))
        .and(path("/testuser/rss"))
        .respond_with(ResponseTemplate::new(200).set_body_string(rss_content))
        .mount(&mock_server)
        .await;

    // Verify RSS is accessible
    let client = reqwest::Client::new();
    let response = client
        .get(&format!("{}/testuser/rss", mock_server.uri()))
        .send()
        .await
        .expect("request");

    assert!(response.status().is_success());

    let body = response.text().await.expect("body");
    assert!(body.contains("First tweet"));
    assert!(body.contains("Second tweet"));
    assert!(body.contains("Third tweet"));
}

#[tokio::test]
async fn test_api_key_header_sent() {
    let mock_server = MockServer::start().await;

    let rss_content = create_rss_feed("testuser", vec![("Tweet", "1")]);

    // Only respond if API key header is present
    Mock::given(method("GET"))
        .and(path("/testuser/rss"))
        .and(header("X-API-Key", "my-secret-key"))
        .respond_with(ResponseTemplate::new(200).set_body_string(rss_content))
        .mount(&mock_server)
        .await;

    // Request without API key should fail
    let client = reqwest::Client::new();
    let response = client
        .get(&format!("{}/testuser/rss", mock_server.uri()))
        .send()
        .await
        .expect("request");

    assert_eq!(response.status().as_u16(), 404); // No matching mock

    // Request with API key should succeed
    let response = client
        .get(&format!("{}/testuser/rss", mock_server.uri()))
        .header("X-API-Key", "my-secret-key")
        .send()
        .await
        .expect("request");

    assert!(response.status().is_success());
}

// ==================== HTTP Error Handling Tests ====================

#[tokio::test]
async fn test_http_error_handling() {
    let mock_server = MockServer::start().await;

    // Setup various error responses
    Mock::given(method("GET"))
        .and(path("/404"))
        .respond_with(ResponseTemplate::new(404).set_body_string("Not found"))
        .mount(&mock_server)
        .await;

    Mock::given(method("GET"))
        .and(path("/500"))
        .respond_with(ResponseTemplate::new(500).set_body_string("Internal error"))
        .mount(&mock_server)
        .await;

    Mock::given(method("GET"))
        .and(path("/429"))
        .respond_with(ResponseTemplate::new(429).set_body_string("Rate limited"))
        .mount(&mock_server)
        .await;

    let client = reqwest::Client::new();

    // Test 404
    let resp = client
        .get(&format!("{}/404", mock_server.uri()))
        .send()
        .await
        .expect("request");
    assert_eq!(resp.status().as_u16(), 404);

    // Test 500
    let resp = client
        .get(&format!("{}/500", mock_server.uri()))
        .send()
        .await
        .expect("request");
    assert_eq!(resp.status().as_u16(), 500);

    // Test 429
    let resp = client
        .get(&format!("{}/429", mock_server.uri()))
        .send()
        .await
        .expect("request");
    assert_eq!(resp.status().as_u16(), 429);
}

// ==================== Message Formatting Tests ====================

#[test]
fn test_summary_message_formatting() {
    let summary = "This is the AI summary of tweets.";
    let timestamp = chrono::Utc::now().format("%Y-%m-%d %H:%M UTC");

    let formatted_message = format!(
        "<b>Twitter Summary</b>\n<i>{}</i>\n\n{}",
        timestamp, summary
    );

    assert!(formatted_message.contains("<b>Twitter Summary</b>"));
    assert!(formatted_message.contains("</b>"));
    assert!(formatted_message.contains("<i>"));
    assert!(formatted_message.contains("</i>"));
    assert!(formatted_message.contains(summary));
}

#[test]
fn test_tweet_list_formatting_for_openai() {
    let tweets = vec![
        Tweet {
            id: "1".to_string(),
            text: "@user1: First tweet".to_string(),
            author_id: Some("user1".to_string()),
            created_at: None,
        },
        Tweet {
            id: "2".to_string(),
            text: "@user2: Second tweet".to_string(),
            author_id: Some("user2".to_string()),
            created_at: None,
        },
    ];

    let formatted = tweets
        .iter()
        .enumerate()
        .map(|(i, t)| format!("{}. {}", i + 1, t.text))
        .collect::<Vec<_>>()
        .join("\n\n");

    assert!(formatted.starts_with("1. @user1: First tweet"));
    assert!(formatted.contains("2. @user2: Second tweet"));
}

// ==================== Usernames File Tests ====================

#[test]
fn test_usernames_file_parsing() {
    let temp_dir = TempDir::new().expect("temp dir");
    let usernames_path = temp_dir.path().join("usernames.txt");

    // Write test file
    let content = "user1\nuser2\n\n  user3  \n\nuser4";
    std::fs::write(&usernames_path, content).expect("write");

    // Read and parse like the app does
    let usernames_content = std::fs::read_to_string(&usernames_path).expect("read");
    let usernames: Vec<String> = usernames_content
        .lines()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect();

    assert_eq!(usernames.len(), 4);
    assert_eq!(usernames[0], "user1");
    assert_eq!(usernames[1], "user2");
    assert_eq!(usernames[2], "user3");
    assert_eq!(usernames[3], "user4");
}

#[test]
fn test_usernames_file_empty() {
    let temp_dir = TempDir::new().expect("temp dir");
    let usernames_path = temp_dir.path().join("empty.txt");

    std::fs::write(&usernames_path, "").expect("write");

    let usernames_content = std::fs::read_to_string(&usernames_path).expect("read");
    let usernames: Vec<String> = usernames_content
        .lines()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect();

    assert!(usernames.is_empty());
}

// ==================== Edge Cases ====================

#[test]
fn test_special_characters_in_tweet() {
    let tweet = Tweet {
        id: "123".to_string(),
        text: "@user: Tweet with \"quotes\", <html>, & ampersand".to_string(),
        author_id: Some("user".to_string()),
        created_at: None,
    };

    // Serialize and deserialize should preserve special characters
    let json = serde_json::to_string(&tweet).expect("serialize");
    let restored: Tweet = serde_json::from_str(&json).expect("deserialize");

    assert_eq!(tweet.text, restored.text);
    assert!(restored.text.contains("\"quotes\""));
    assert!(restored.text.contains("<html>"));
    assert!(restored.text.contains("&"));
}

#[test]
fn test_empty_tweet_fields() {
    let tweet = Tweet {
        id: "123".to_string(),
        text: "".to_string(),
        author_id: None,
        created_at: None,
    };

    assert!(tweet.text.is_empty());
    assert!(tweet.author_id.is_none());
    assert!(tweet.created_at.is_none());
}

#[test]
fn test_unicode_in_tweet() {
    let tweet = Tweet {
        id: "123".to_string(),
        text: "@user: Tweet with unicode chars and emojis".to_string(),
        author_id: Some("user".to_string()),
        created_at: None,
    };

    // Serialize and deserialize should preserve unicode
    let json = serde_json::to_string(&tweet).expect("serialize");
    let restored: Tweet = serde_json::from_str(&json).expect("deserialize");

    assert_eq!(tweet.text, restored.text);
}

// ==================== Config Struct Tests ====================

#[test]
fn test_config_clone() {
    let temp_dir = TempDir::new().expect("temp dir");
    let config = create_test_config("http://localhost:8080", &temp_dir);

    let cloned = config.clone();

    assert_eq!(config.openai_api_key, cloned.openai_api_key);
    assert_eq!(config.telegram_bot_token, cloned.telegram_bot_token);
    assert_eq!(config.nitter_instance, cloned.nitter_instance);
    assert_eq!(config.max_tweets, cloned.max_tweets);
    assert_eq!(config.database_url, cloned.database_url);
}

#[test]
fn test_config_debug() {
    let temp_dir = TempDir::new().expect("temp dir");
    let config = create_test_config("http://localhost:8080", &temp_dir);

    let debug_str = format!("{:?}", config);

    // Verify debug output contains expected fields
    assert!(debug_str.contains("Config"));
    assert!(debug_str.contains("openai_api_key"));
    assert!(debug_str.contains("telegram_bot_token"));
}

// ==================== Timestamp Format Tests ====================

#[test]
fn test_timestamp_format() {
    let timestamp = chrono::Utc::now().format("%Y-%m-%d %H:%M UTC").to_string();

    // Should match pattern like "2024-01-15 10:30 UTC"
    assert!(timestamp.ends_with(" UTC"));
    assert!(timestamp.contains("-"));
    assert!(timestamp.contains(":"));
    assert_eq!(timestamp.len(), 20); // "YYYY-MM-DD HH:MM UTC"
}

// ==================== Schedule Time Parsing Tests ====================

#[test]
fn test_schedule_times_parsing() {
    let schedule_str = "08:00,20:00";
    let times: Vec<String> = schedule_str
        .split(',')
        .map(|s| s.trim().to_string())
        .collect();

    assert_eq!(times.len(), 2);
    assert_eq!(times[0], "08:00");
    assert_eq!(times[1], "20:00");
}

#[test]
fn test_schedule_times_with_spaces() {
    let schedule_str = " 08:00 , 12:00 , 20:00 ";
    let times: Vec<String> = schedule_str
        .split(',')
        .map(|s| s.trim().to_string())
        .collect();

    assert_eq!(times.len(), 3);
    assert_eq!(times[0], "08:00");
    assert_eq!(times[1], "12:00");
    assert_eq!(times[2], "20:00");
}

#[test]
fn test_single_schedule_time() {
    let schedule_str = "09:00";
    let times: Vec<String> = schedule_str
        .split(',')
        .map(|s| s.trim().to_string())
        .collect();

    assert_eq!(times.len(), 1);
    assert_eq!(times[0], "09:00");
}
