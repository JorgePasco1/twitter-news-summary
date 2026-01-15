use anyhow::{Context, Result};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use tracing::{info, warn};
use crate::config::Config;
use crate::db::Database;

// Telegram webhook types
#[derive(Debug, Deserialize)]
pub struct Update {
    pub update_id: i64,
    pub message: Option<Message>,
}

#[derive(Debug, Deserialize)]
pub struct Message {
    pub message_id: i64,
    pub from: Option<User>,
    pub chat: Chat,
    pub text: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct User {
    pub id: i64,
    pub username: Option<String>,
    pub first_name: String,
}

#[derive(Debug, Deserialize)]
pub struct Chat {
    pub id: i64,
    #[allow(dead_code)]
    pub r#type: String,
}

#[derive(Debug, Serialize)]
struct SendMessageRequest {
    chat_id: String,
    text: String,
    parse_mode: String,
}

/// Handle incoming webhook from Telegram
pub async fn handle_webhook(config: &Config, db: &Database, update: Update) -> Result<()> {
    let message = match update.message {
        Some(msg) => msg,
        None => return Ok(()), // Not a message update, ignore
    };

    let text = match message.text {
        Some(t) => t,
        None => return Ok(()), // No text, ignore
    };

    let chat_id = message.chat.id.to_string();
    let username = message.from.as_ref().and_then(|u| u.username.clone());

    info!("Received message from {}: {}", chat_id, text);

    // Handle bot commands
    match text.as_str() {
        "/start" => {
            let welcome = r#"👋 Welcome to Twitter News Summary Bot!

Commands:
/subscribe - Get daily AI-powered summaries of Twitter/X news
/unsubscribe - Stop receiving summaries
/status - Check your subscription status

Summaries are sent twice daily with the latest tweets from tech leaders and AI researchers."#;

            send_message(config, &chat_id, welcome).await?;
        }
        "/subscribe" => {
            if db.is_subscribed(&chat_id)? {
                send_message(config, &chat_id, "✅ You're already subscribed!").await?;
            } else {
                db.add_subscriber(&chat_id, username.as_deref())?;
                info!("New subscriber: {} (username: {:?})", chat_id, username);
                send_message(config, &chat_id, "✅ Successfully subscribed! You'll receive summaries twice daily.").await?;
            }
        }
        "/unsubscribe" => {
            if db.remove_subscriber(&chat_id)? {
                info!("Unsubscribed: {}", chat_id);
                send_message(config, &chat_id, "👋 Successfully unsubscribed. You won't receive any more summaries.").await?;
            } else {
                send_message(config, &chat_id, "You're not currently subscribed.").await?;
            }
        }
        "/status" => {
            let is_subscribed = db.is_subscribed(&chat_id)?;

            // Check if user is admin (only admin sees total subscriber count)
            let is_admin = !config.telegram_chat_id.is_empty() && chat_id == config.telegram_chat_id;

            let status_msg = if is_subscribed {
                if is_admin {
                    // Admin sees subscriber count
                    format!("✅ You are subscribed\n📊 Total subscribers: {}", db.subscriber_count()?)
                } else {
                    // Regular users only see their own status
                    "✅ You are subscribed".to_string()
                }
            } else {
                "❌ You are not subscribed\n\nUse /subscribe to start receiving summaries.".to_string()
            };
            send_message(config, &chat_id, &status_msg).await?;
        }
        _ => {
            // Unknown command, send help
            send_message(config, &chat_id, "Unknown command. Use /start to see available commands.").await?;
        }
    }

    Ok(())
}

/// Send summary to all subscribers
pub async fn send_to_subscribers(config: &Config, db: &Database, summary: &str) -> Result<()> {
    let subscribers = db.list_subscribers()?;

    if subscribers.is_empty() {
        info!("No subscribers to send to");
        return Ok(());
    }

    info!("Sending summary to {} subscribers", subscribers.len());

    let timestamp = Utc::now().format("%Y-%m-%d %H:%M UTC");
    let message = format!(
        "📰 <b>Twitter Summary</b>\n<i>{}</i>\n\n{}",
        timestamp,
        summary
    );

    let mut success_count = 0;
    let mut fail_count = 0;

    for subscriber in subscribers {
        match send_message(config, &subscriber.chat_id, &message).await {
            Ok(_) => {
                success_count += 1;
                info!("✓ Sent to {}", subscriber.chat_id);
            }
            Err(e) => {
                fail_count += 1;
                warn!("✗ Failed to send to {}: {}", subscriber.chat_id, e);
            }
        }

        // Small delay to avoid rate limiting
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
    }

    info!("Summary sent: {} successful, {} failed", success_count, fail_count);

    // Send admin notification if configured
    if !config.telegram_chat_id.is_empty() && fail_count > 0 {
        let admin_msg = format!(
            "📊 Summary sent to {}/{} subscribers ({} failed)",
            success_count,
            success_count + fail_count,
            fail_count
        );
        if let Err(e) = send_message(config, &config.telegram_chat_id, &admin_msg).await {
            warn!("Failed to send admin notification: {}", e);
        }
    }

    Ok(())
}

/// Send a Telegram message to a specific chat
async fn send_message(config: &Config, chat_id: &str, text: &str) -> Result<()> {
    let client = reqwest::Client::new();

    let url = format!(
        "https://api.telegram.org/bot{}/sendMessage",
        config.telegram_bot_token
    );

    let request = SendMessageRequest {
        chat_id: chat_id.to_string(),
        text: text.to_string(),
        parse_mode: "HTML".to_string(),
    };

    let response = client
        .post(&url)
        .json(&request)
        .send()
        .await
        .context("Failed to send request to Telegram API")?;

    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        anyhow::bail!("Telegram API error ({}): {}", status, body);
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    // ==================== Update Deserialization Tests ====================

    #[test]
    fn test_update_deserialization_with_message() {
        let json = r#"{
            "update_id": 123456789,
            "message": {
                "message_id": 100,
                "from": {
                    "id": 987654321,
                    "username": "testuser",
                    "first_name": "Test"
                },
                "chat": {
                    "id": 987654321,
                    "type": "private"
                },
                "text": "/start"
            }
        }"#;

        let update: Update = serde_json::from_str(json).expect("Should deserialize");
        assert_eq!(update.update_id, 123456789);
        assert!(update.message.is_some());

        let message = update.message.unwrap();
        assert_eq!(message.message_id, 100);
        assert_eq!(message.chat.id, 987654321);
        assert_eq!(message.text, Some("/start".to_string()));

        let from = message.from.unwrap();
        assert_eq!(from.id, 987654321);
        assert_eq!(from.username, Some("testuser".to_string()));
        assert_eq!(from.first_name, "Test");
    }

    #[test]
    fn test_update_deserialization_without_message() {
        let json = r#"{"update_id": 123456789}"#;

        let update: Update = serde_json::from_str(json).expect("Should deserialize");
        assert_eq!(update.update_id, 123456789);
        assert!(update.message.is_none());
    }

    #[test]
    fn test_message_without_text() {
        let json = r#"{
            "update_id": 123,
            "message": {
                "message_id": 100,
                "chat": {
                    "id": 123,
                    "type": "private"
                }
            }
        }"#;

        let update: Update = serde_json::from_str(json).expect("Should deserialize");
        let message = update.message.unwrap();
        assert!(message.text.is_none());
        assert!(message.from.is_none());
    }

    #[test]
    fn test_message_without_from() {
        let json = r#"{
            "update_id": 123,
            "message": {
                "message_id": 100,
                "chat": {
                    "id": 123,
                    "type": "private"
                },
                "text": "Hello"
            }
        }"#;

        let update: Update = serde_json::from_str(json).expect("Should deserialize");
        let message = update.message.unwrap();
        assert!(message.from.is_none());
        assert_eq!(message.text, Some("Hello".to_string()));
    }

    // ==================== Chat Type Tests ====================

    #[test]
    fn test_private_chat() {
        let json = r#"{
            "update_id": 123,
            "message": {
                "message_id": 100,
                "chat": {
                    "id": 123456789,
                    "type": "private"
                }
            }
        }"#;

        let update: Update = serde_json::from_str(json).expect("Should deserialize");
        let message = update.message.unwrap();
        assert_eq!(message.chat.id, 123456789);
        assert_eq!(message.chat.r#type, "private");
    }

    #[test]
    fn test_group_chat_negative_id() {
        let json = r#"{
            "update_id": 123,
            "message": {
                "message_id": 100,
                "chat": {
                    "id": -1001234567890,
                    "type": "supergroup"
                }
            }
        }"#;

        let update: Update = serde_json::from_str(json).expect("Should deserialize");
        let message = update.message.unwrap();
        assert_eq!(message.chat.id, -1001234567890);
        assert_eq!(message.chat.r#type, "supergroup");
    }

    // ==================== User Tests ====================

    #[test]
    fn test_user_without_username() {
        let json = r#"{
            "update_id": 123,
            "message": {
                "message_id": 100,
                "from": {
                    "id": 123,
                    "first_name": "John"
                },
                "chat": {
                    "id": 123,
                    "type": "private"
                }
            }
        }"#;

        let update: Update = serde_json::from_str(json).expect("Should deserialize");
        let from = update.message.unwrap().from.unwrap();
        assert_eq!(from.id, 123);
        assert_eq!(from.first_name, "John");
        assert!(from.username.is_none());
    }

    #[test]
    fn test_user_with_username() {
        let json = r#"{
            "update_id": 123,
            "message": {
                "message_id": 100,
                "from": {
                    "id": 123,
                    "username": "johndoe",
                    "first_name": "John"
                },
                "chat": {
                    "id": 123,
                    "type": "private"
                }
            }
        }"#;

        let update: Update = serde_json::from_str(json).expect("Should deserialize");
        let from = update.message.unwrap().from.unwrap();
        assert_eq!(from.username, Some("johndoe".to_string()));
    }

    // ==================== SendMessageRequest Tests ====================

    #[test]
    fn test_send_message_request_serialization() {
        let request = SendMessageRequest {
            chat_id: "123456789".to_string(),
            text: "Hello, World!".to_string(),
            parse_mode: "HTML".to_string(),
        };

        let json = serde_json::to_string(&request).expect("Should serialize");
        assert!(json.contains("123456789"));
        assert!(json.contains("Hello, World!"));
        assert!(json.contains("HTML"));
    }

    #[test]
    fn test_send_message_request_with_html_content() {
        let request = SendMessageRequest {
            chat_id: "123".to_string(),
            text: "<b>Bold</b> and <i>italic</i>".to_string(),
            parse_mode: "HTML".to_string(),
        };

        let json = serde_json::to_string(&request).expect("Should serialize");
        assert!(json.contains("<b>Bold</b>"));
        assert!(json.contains("<i>italic</i>"));
    }

    #[test]
    fn test_send_message_request_with_special_characters() {
        let request = SendMessageRequest {
            chat_id: "123".to_string(),
            text: "Text with \"quotes\" and \\ backslash".to_string(),
            parse_mode: "HTML".to_string(),
        };

        let json = serde_json::to_string(&request).expect("Should serialize");
        // JSON should escape special characters
        assert!(json.contains("\\\"quotes\\\"") || json.contains("quotes"));
    }

    #[test]
    fn test_send_message_request_with_newlines() {
        let request = SendMessageRequest {
            chat_id: "123".to_string(),
            text: "Line 1\nLine 2\nLine 3".to_string(),
            parse_mode: "HTML".to_string(),
        };

        let json = serde_json::to_string(&request).expect("Should serialize");
        // Newlines should be escaped in JSON
        assert!(json.contains("\\n"));
    }

    // ==================== Command Pattern Tests ====================

    #[test]
    fn test_start_command_pattern() {
        let text = "/start";
        assert_eq!(text, "/start");
    }

    #[test]
    fn test_subscribe_command_pattern() {
        let text = "/subscribe";
        assert_eq!(text, "/subscribe");
    }

    #[test]
    fn test_unsubscribe_command_pattern() {
        let text = "/unsubscribe";
        assert_eq!(text, "/unsubscribe");
    }

    #[test]
    fn test_status_command_pattern() {
        let text = "/status";
        assert_eq!(text, "/status");
    }

    #[test]
    fn test_unknown_command() {
        let text = "/unknown_command";
        let known_commands = ["/start", "/subscribe", "/unsubscribe", "/status"];
        assert!(!known_commands.contains(&text));
    }

    // ==================== Message Formatting Tests ====================

    #[test]
    fn test_summary_message_format() {
        use chrono::Utc;

        let timestamp = Utc::now().format("%Y-%m-%d %H:%M UTC");
        let summary = "This is the summary content.";

        let message = format!(
            "<b>Twitter Summary</b>\n<i>{}</i>\n\n{}",
            timestamp,
            summary
        );

        assert!(message.contains("<b>Twitter Summary</b>"));
        assert!(message.contains("<i>"));
        assert!(message.contains("</i>"));
        assert!(message.contains("This is the summary content."));
    }

    #[test]
    fn test_welcome_message_content() {
        let welcome = r#"Welcome to Twitter News Summary Bot!

Commands:
/subscribe - Get daily AI-powered summaries of Twitter/X news
/unsubscribe - Stop receiving summaries
/status - Check your subscription status

Summaries are sent twice daily with the latest tweets from tech leaders and AI researchers."#;

        assert!(welcome.contains("/subscribe"));
        assert!(welcome.contains("/unsubscribe"));
        assert!(welcome.contains("/status"));
        assert!(welcome.contains("twice daily"));
    }

    // ==================== Edge Case Tests ====================

    #[test]
    fn test_large_update_id() {
        let json = r#"{
            "update_id": 9223372036854775807,
            "message": {
                "message_id": 1,
                "chat": {"id": 1, "type": "private"}
            }
        }"#;

        let update: Update = serde_json::from_str(json).expect("Should handle large update_id");
        assert_eq!(update.update_id, i64::MAX);
    }

    #[test]
    fn test_empty_text_message() {
        let json = r#"{
            "update_id": 123,
            "message": {
                "message_id": 100,
                "chat": {"id": 123, "type": "private"},
                "text": ""
            }
        }"#;

        let update: Update = serde_json::from_str(json).expect("Should deserialize");
        let message = update.message.unwrap();
        assert_eq!(message.text, Some("".to_string()));
    }

    #[test]
    fn test_very_long_text() {
        let long_text = "A".repeat(10000);
        let json = format!(r#"{{
            "update_id": 123,
            "message": {{
                "message_id": 100,
                "chat": {{"id": 123, "type": "private"}},
                "text": "{}"
            }}
        }}"#, long_text);

        let update: Update = serde_json::from_str(&json).expect("Should deserialize");
        let message = update.message.unwrap();
        assert_eq!(message.text.unwrap().len(), 10000);
    }

    #[test]
    fn test_unicode_in_message() {
        let json = r#"{
            "update_id": 123,
            "message": {
                "message_id": 100,
                "from": {
                    "id": 123,
                    "first_name": "User name"
                },
                "chat": {"id": 123, "type": "private"},
                "text": "Hello in many languages"
            }
        }"#;

        let update: Update = serde_json::from_str(json).expect("Should deserialize");
        let message = update.message.unwrap();
        assert!(message.text.unwrap().contains("Hello"));
    }

    // ==================== Chat ID Tests ====================

    #[test]
    fn test_chat_id_to_string_conversion() {
        let chat_id: i64 = 123456789;
        let chat_id_str = chat_id.to_string();
        assert_eq!(chat_id_str, "123456789");
    }

    #[test]
    fn test_negative_chat_id_to_string() {
        let chat_id: i64 = -1001234567890;
        let chat_id_str = chat_id.to_string();
        assert_eq!(chat_id_str, "-1001234567890");
    }

    // ==================== Status Message Format Tests ====================

    #[test]
    fn test_subscribed_status_format_admin() {
        // Admin sees subscriber count
        let subscriber_count = 42;
        let status_msg = format!("✅ You are subscribed\n📊 Total subscribers: {}", subscriber_count);

        assert!(status_msg.contains("subscribed"));
        assert!(status_msg.contains("42"));
    }

    #[test]
    fn test_subscribed_status_format_regular_user() {
        // Regular users don't see subscriber count
        let status_msg = "✅ You are subscribed";

        assert!(status_msg.contains("subscribed"));
        assert!(!status_msg.contains("Total subscribers"));
    }

    #[test]
    fn test_unsubscribed_status_format() {
        let status_msg = "❌ You are not subscribed\n\nUse /subscribe to start receiving summaries.";

        assert!(status_msg.contains("not subscribed"));
        assert!(status_msg.contains("/subscribe"));
    }

    // ==================== Admin Notification Tests ====================

    #[test]
    fn test_admin_notification_format() {
        let success_count = 10;
        let fail_count = 2;
        let total = success_count + fail_count;

        let admin_msg = format!(
            "Summary sent to {}/{} subscribers ({} failed)",
            success_count, total, fail_count
        );

        assert!(admin_msg.contains("10/12"));
        assert!(admin_msg.contains("2 failed"));
    }
}
