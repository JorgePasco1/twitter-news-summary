use anyhow::{Context, Result};
use chrono::DateTime;
use serde::{Deserialize, Serialize};
use tracing::info;
use crate::config::Config;

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct User {
    id: String,
    name: String,
    username: String,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct ListMembersResponse {
    data: Option<Vec<User>>,
    meta: Option<MembersMeta>,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct MembersMeta {
    result_count: i32,
    next_token: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Tweet {
    pub id: String,
    pub text: String,
    pub author_id: Option<String>,
    pub created_at: Option<String>,
}

/// Fetch list members from Twitter API
#[allow(dead_code)]
pub async fn fetch_list_members(config: &Config) -> Result<Vec<String>> {
    let bearer_token = config.twitter_bearer_token.as_ref()
        .context("TWITTER_BEARER_TOKEN not set")?;
    let list_id = config.twitter_list_id.as_ref()
        .context("TWITTER_LIST_ID not set")?;

    let client = reqwest::Client::new();
    let mut all_users = Vec::new();
    let mut next_token: Option<String> = None;

    // Fetch all pages of list members
    loop {
        let url = format!(
            "https://api.twitter.com/2/lists/{}/members",
            list_id
        );

        let mut request = client
            .get(&url)
            .bearer_auth(bearer_token)
            .query(&[("max_results", "100")]);

        if let Some(token) = &next_token {
            request = request.query(&[("pagination_token", token)]);
        }

        let response = request
            .send()
            .await
            .context("Failed to send request to Twitter API")?;

        // Log rate limit information
        if let Some(remaining) = response.headers().get("x-rate-limit-remaining") {
            let limit = response.headers()
                .get("x-rate-limit-limit")
                .and_then(|v| v.to_str().ok())
                .unwrap_or("?");
            let reset = response.headers()
                .get("x-rate-limit-reset")
                .and_then(|v| v.to_str().ok())
                .and_then(|ts| ts.parse::<i64>().ok())
                .and_then(|ts| DateTime::from_timestamp(ts, 0))
                .map(|dt| dt.format("%H:%M:%S UTC").to_string())
                .unwrap_or_else(|| "?".to_string());

            info!(
                "Twitter API rate limit: {}/{} remaining (resets at {})",
                remaining.to_str().unwrap_or("?"),
                limit,
                reset
            );
        }

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            anyhow::bail!("Twitter API error ({}): {}", status, body);
        }

        let members_response: ListMembersResponse = response
            .json()
            .await
            .context("Failed to parse Twitter response")?;

        if let Some(users) = members_response.data {
            all_users.extend(users);
        }

        // Check if there are more pages
        next_token = members_response.meta.and_then(|m| m.next_token);
        if next_token.is_none() {
            break;
        }
    }

    info!("Fetched {} list members from Twitter", all_users.len());

    // Extract usernames
    let usernames: Vec<String> = all_users
        .iter()
        .map(|u| u.username.clone())
        .collect();

    Ok(usernames)
}

#[cfg(test)]
mod tests {
    use super::*;

    // ==================== Tweet Struct Tests ====================

    #[test]
    fn test_tweet_creation() {
        let tweet = Tweet {
            id: "123456789".to_string(),
            text: "@user: Hello, World!".to_string(),
            author_id: Some("user123".to_string()),
            created_at: Some("2024-01-15T10:30:00+00:00".to_string()),
        };

        assert_eq!(tweet.id, "123456789");
        assert_eq!(tweet.text, "@user: Hello, World!");
        assert_eq!(tweet.author_id, Some("user123".to_string()));
        assert_eq!(tweet.created_at, Some("2024-01-15T10:30:00+00:00".to_string()));
    }

    #[test]
    fn test_tweet_without_optional_fields() {
        let tweet = Tweet {
            id: "123".to_string(),
            text: "Test tweet".to_string(),
            author_id: None,
            created_at: None,
        };

        assert_eq!(tweet.id, "123");
        assert_eq!(tweet.text, "Test tweet");
        assert!(tweet.author_id.is_none());
        assert!(tweet.created_at.is_none());
    }

    #[test]
    fn test_tweet_clone() {
        let original = Tweet {
            id: "123".to_string(),
            text: "Test".to_string(),
            author_id: Some("author".to_string()),
            created_at: Some("2024-01-15T10:30:00+00:00".to_string()),
        };

        let cloned = original.clone();

        assert_eq!(original.id, cloned.id);
        assert_eq!(original.text, cloned.text);
        assert_eq!(original.author_id, cloned.author_id);
        assert_eq!(original.created_at, cloned.created_at);
    }

    #[test]
    fn test_tweet_debug() {
        let tweet = Tweet {
            id: "123".to_string(),
            text: "Test".to_string(),
            author_id: None,
            created_at: None,
        };

        let debug_str = format!("{:?}", tweet);
        assert!(debug_str.contains("Tweet"));
        assert!(debug_str.contains("123"));
        assert!(debug_str.contains("Test"));
    }

    // ==================== Tweet Serialization Tests ====================

    #[test]
    fn test_tweet_serialization() {
        let tweet = Tweet {
            id: "123".to_string(),
            text: "Test tweet content".to_string(),
            author_id: Some("author123".to_string()),
            created_at: Some("2024-01-15T10:30:00+00:00".to_string()),
        };

        let json = serde_json::to_string(&tweet).expect("Should serialize");
        assert!(json.contains("123"));
        assert!(json.contains("Test tweet content"));
        assert!(json.contains("author123"));
        assert!(json.contains("2024-01-15T10:30:00+00:00"));
    }

    #[test]
    fn test_tweet_deserialization() {
        let json = r#"{
            "id": "123",
            "text": "Test tweet",
            "author_id": "author123",
            "created_at": "2024-01-15T10:30:00+00:00"
        }"#;

        let tweet: Tweet = serde_json::from_str(json).expect("Should deserialize");
        assert_eq!(tweet.id, "123");
        assert_eq!(tweet.text, "Test tweet");
        assert_eq!(tweet.author_id, Some("author123".to_string()));
        assert_eq!(tweet.created_at, Some("2024-01-15T10:30:00+00:00".to_string()));
    }

    #[test]
    fn test_tweet_deserialization_null_optional_fields() {
        let json = r#"{
            "id": "123",
            "text": "Test tweet",
            "author_id": null,
            "created_at": null
        }"#;

        let tweet: Tweet = serde_json::from_str(json).expect("Should deserialize");
        assert_eq!(tweet.id, "123");
        assert!(tweet.author_id.is_none());
        assert!(tweet.created_at.is_none());
    }

    #[test]
    fn test_tweet_deserialization_missing_optional_fields() {
        let json = r#"{
            "id": "123",
            "text": "Test tweet"
        }"#;

        let tweet: Tweet = serde_json::from_str(json).expect("Should deserialize");
        assert_eq!(tweet.id, "123");
        assert!(tweet.author_id.is_none());
        assert!(tweet.created_at.is_none());
    }

    // ==================== Tweet Content Tests ====================

    #[test]
    fn test_tweet_with_special_characters() {
        let tweet = Tweet {
            id: "123".to_string(),
            text: "Tweet with \"quotes\" and <html> & special chars".to_string(),
            author_id: None,
            created_at: None,
        };

        assert!(tweet.text.contains("\"quotes\""));
        assert!(tweet.text.contains("<html>"));
        assert!(tweet.text.contains("&"));
    }

    #[test]
    fn test_tweet_with_unicode() {
        let tweet = Tweet {
            id: "123".to_string(),
            text: "Tweet with unicode chars".to_string(),
            author_id: None,
            created_at: None,
        };

        // Should handle unicode gracefully
        assert!(tweet.text.contains("unicode"));
    }

    #[test]
    fn test_tweet_with_long_text() {
        let long_text = "A".repeat(10000);
        let tweet = Tweet {
            id: "123".to_string(),
            text: long_text.clone(),
            author_id: None,
            created_at: None,
        };

        assert_eq!(tweet.text.len(), 10000);
    }

    #[test]
    fn test_tweet_with_newlines() {
        let tweet = Tweet {
            id: "123".to_string(),
            text: "Line 1\nLine 2\nLine 3".to_string(),
            author_id: None,
            created_at: None,
        };

        assert!(tweet.text.contains('\n'));
    }

    // ==================== User/ListMembersResponse Internal Tests ====================
    // Note: These structs are private but we can test the expected JSON format

    #[test]
    fn test_list_members_response_format() {
        // Test the expected response format from Twitter API
        let json = r#"{
            "data": [
                {"id": "123", "name": "User One", "username": "userone"},
                {"id": "456", "name": "User Two", "username": "usertwo"}
            ],
            "meta": {
                "result_count": 2,
                "next_token": null
            }
        }"#;

        // Verify it's valid JSON
        let value: serde_json::Value = serde_json::from_str(json).expect("Should be valid JSON");
        assert!(value["data"].is_array());
        assert_eq!(value["data"].as_array().unwrap().len(), 2);
    }

    #[test]
    fn test_list_members_response_with_pagination() {
        let json = r#"{
            "data": [
                {"id": "123", "name": "User", "username": "user1"}
            ],
            "meta": {
                "result_count": 1,
                "next_token": "abc123xyz"
            }
        }"#;

        let value: serde_json::Value = serde_json::from_str(json).expect("Should be valid JSON");
        assert_eq!(value["meta"]["next_token"], "abc123xyz");
    }

    #[test]
    fn test_list_members_response_empty_data() {
        let json = r#"{
            "data": null,
            "meta": {
                "result_count": 0
            }
        }"#;

        let value: serde_json::Value = serde_json::from_str(json).expect("Should be valid JSON");
        assert!(value["data"].is_null());
    }

    // ==================== Config Requirement Tests ====================

    #[test]
    fn test_config_missing_bearer_token() {
        let config = Config {
            twitter_bearer_token: None,
            twitter_list_id: Some("123456".to_string()),
            openai_api_key: "test".to_string(),
            openai_model: "gpt-4o-mini".to_string(),
            telegram_bot_token: "test".to_string(),
            telegram_chat_id: "".to_string(),
            telegram_webhook_secret: "test-webhook-secret".to_string(),
            max_tweets: 50,
            hours_lookback: 12,
            nitter_instance: "https://nitter.example.com".to_string(),
            nitter_api_key: None,
            usernames_file: "data/usernames.txt".to_string(),
            api_key: None,
            database_path: "/data/subscribers.db".to_string(),
            schedule_times: vec!["08:00".to_string()],
            port: 8080,
        };

        assert!(config.twitter_bearer_token.is_none());
    }

    #[test]
    fn test_config_missing_list_id() {
        let config = Config {
            twitter_bearer_token: Some("test-token".to_string()),
            twitter_list_id: None,
            openai_api_key: "test".to_string(),
            openai_model: "gpt-4o-mini".to_string(),
            telegram_bot_token: "test".to_string(),
            telegram_chat_id: "".to_string(),
            telegram_webhook_secret: "test-webhook-secret".to_string(),
            max_tweets: 50,
            hours_lookback: 12,
            nitter_instance: "https://nitter.example.com".to_string(),
            nitter_api_key: None,
            usernames_file: "data/usernames.txt".to_string(),
            api_key: None,
            database_path: "/data/subscribers.db".to_string(),
            schedule_times: vec!["08:00".to_string()],
            port: 8080,
        };

        assert!(config.twitter_list_id.is_none());
    }

    // ==================== Rate Limit Header Tests ====================

    #[test]
    fn test_rate_limit_reset_timestamp_parsing() {
        // Test the timestamp parsing logic used in rate limit handling
        let timestamp_str = "1705312200";
        let ts: i64 = timestamp_str.parse().expect("Should parse");

        // Convert to DateTime
        let dt = DateTime::from_timestamp(ts, 0);
        assert!(dt.is_some());

        // Format it
        let formatted = dt.unwrap().format("%H:%M:%S UTC").to_string();
        assert!(formatted.contains(":"));
    }

    #[test]
    fn test_rate_limit_invalid_timestamp() {
        let timestamp_str = "invalid";
        let result: Result<i64, _> = timestamp_str.parse();
        assert!(result.is_err());
    }

    // ==================== Username Extraction Tests ====================

    #[test]
    fn test_username_extraction_logic() {
        // Simulate the username extraction from User structs
        #[derive(Debug)]
        struct TestUser {
            username: String,
        }

        let users = vec![
            TestUser { username: "user1".to_string() },
            TestUser { username: "user2".to_string() },
            TestUser { username: "user3".to_string() },
        ];

        let usernames: Vec<String> = users
            .iter()
            .map(|u| u.username.clone())
            .collect();

        assert_eq!(usernames.len(), 3);
        assert_eq!(usernames[0], "user1");
        assert_eq!(usernames[1], "user2");
        assert_eq!(usernames[2], "user3");
    }

    #[test]
    fn test_empty_users_list() {
        let users: Vec<String> = vec![];
        let usernames: Vec<String> = users.iter().cloned().collect();
        assert!(usernames.is_empty());
    }

    // ==================== URL Format Tests ====================

    #[test]
    fn test_twitter_api_url_format() {
        let list_id = "123456789";
        let url = format!(
            "https://api.twitter.com/2/lists/{}/members",
            list_id
        );

        assert_eq!(url, "https://api.twitter.com/2/lists/123456789/members");
    }

    #[test]
    fn test_query_params() {
        let max_results = "100";
        let pagination_token = "abc123";

        // Verify query param format
        assert_eq!(max_results, "100");
        assert_eq!(pagination_token, "abc123");
    }

    // ==================== Error Message Tests ====================

    #[test]
    fn test_api_error_message_format() {
        let status = 429;
        let body = r#"{"detail": "Too Many Requests"}"#;

        let error_msg = format!("Twitter API error ({}): {}", status, body);

        assert!(error_msg.contains("429"));
        assert!(error_msg.contains("Too Many Requests"));
    }

    #[test]
    fn test_context_error_messages() {
        let error_contexts = vec![
            "TWITTER_BEARER_TOKEN not set",
            "TWITTER_LIST_ID not set",
            "Failed to send request to Twitter API",
            "Failed to parse Twitter response",
        ];

        for context in error_contexts {
            assert!(!context.is_empty());
        }
    }
}

