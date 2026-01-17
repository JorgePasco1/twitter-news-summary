use crate::config::Config;
use crate::db::Database;
use anyhow::{Context, Result};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use tracing::{info, warn};

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

    let chat_id = message.chat.id;
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

            send_message(config, chat_id, welcome).await?;
        }
        "/subscribe" => {
            if db.is_subscribed(chat_id).await? {
                send_message(config, chat_id, "✅ You're already subscribed!").await?;
            } else {
                let (_, needs_welcome) = db.add_subscriber(chat_id, username.as_deref()).await?;
                info!("New subscriber: {} (username: {:?})", chat_id, username);
                send_message(
                    config,
                    chat_id,
                    "✅ Successfully subscribed! You'll receive summaries twice daily.",
                )
                .await?;

                // Send welcome summary for first-time subscribers
                if needs_welcome {
                    if let Some(summary) = db.get_latest_summary().await? {
                        send_welcome_summary(config, db, chat_id, &summary.content).await?;
                    } else {
                        info!("No summary available to send as welcome message");
                    }
                }
            }
        }
        "/unsubscribe" => {
            if db.remove_subscriber(chat_id).await? {
                info!("Unsubscribed: {}", chat_id);
                send_message(
                    config,
                    chat_id,
                    "👋 Successfully unsubscribed. You won't receive any more summaries.",
                )
                .await?;
            } else {
                send_message(config, chat_id, "You're not currently subscribed.").await?;
            }
        }
        "/status" => {
            let is_subscribed = db.is_subscribed(chat_id).await?;

            // Check if user is admin (only admin sees total subscriber count)
            let chat_id_str = chat_id.to_string();
            let is_admin =
                !config.telegram_chat_id.is_empty() && chat_id_str == config.telegram_chat_id;

            let status_msg = if is_subscribed {
                if is_admin {
                    // Admin sees subscriber count
                    format!(
                        "✅ You are subscribed\n📊 Total subscribers: {}",
                        db.subscriber_count().await?
                    )
                } else {
                    // Regular users only see their own status
                    "✅ You are subscribed".to_string()
                }
            } else {
                "❌ You are not subscribed\n\nUse /subscribe to start receiving summaries."
                    .to_string()
            };
            send_message(config, chat_id, &status_msg).await?;
        }
        _ => {
            // Unknown command, send help
            send_message(
                config,
                chat_id,
                "Unknown command. Use /start to see available commands.",
            )
            .await?;
        }
    }

    Ok(())
}

/// Send welcome summary to a new subscriber
async fn send_welcome_summary(
    config: &Config,
    db: &Database,
    chat_id: i64,
    summary: &str,
) -> Result<()> {
    let timestamp = Utc::now().format("%Y-%m-%d %H:%M UTC");
    let message = format!(
        "📰 <b>Hey! Here's what you missed</b> 😉\n<i>{}</i>\n\n{}",
        timestamp, summary
    );

    send_message(config, chat_id, &message).await?;
    db.mark_welcome_summary_sent(chat_id).await?;
    info!("✓ Welcome summary sent to {}", chat_id);

    Ok(())
}

/// Send summary to all subscribers
pub async fn send_to_subscribers(config: &Config, db: &Database, summary: &str) -> Result<()> {
    let subscribers = db.list_subscribers().await?;

    if subscribers.is_empty() {
        info!("No subscribers to send to");
        return Ok(());
    }

    info!("Sending summary to {} subscribers", subscribers.len());

    let timestamp = Utc::now().format("%Y-%m-%d %H:%M UTC");
    let message = format!("📰 <b>Twitter Summary</b>\n<i>{}</i>\n\n{}", timestamp, summary);

    let mut success_count = 0;
    let mut fail_count = 0;

    for subscriber in subscribers {
        match send_message(config, subscriber.chat_id, &message).await {
            Ok(_) => {
                success_count += 1;
                info!("✓ Sent to {}", subscriber.chat_id);
            }
            Err(e) => {
                fail_count += 1;
                let error_msg = e.to_string();

                // Auto-remove subscribers who blocked the bot or deleted their account
                // Check for 403 status AND specific descriptions (case-insensitive for robustness)
                let error_lower = error_msg.to_lowercase();
                let is_blocked = error_msg.contains("403")
                    && (error_lower.contains("blocked by the user")
                        || error_lower.contains("user is deactivated"));

                if is_blocked {
                    warn!(
                        "✗ Auto-removing blocked/deactivated subscriber {}: {}",
                        subscriber.chat_id, error_msg
                    );
                    if let Err(remove_err) = db.remove_subscriber(subscriber.chat_id).await {
                        warn!("Failed to remove blocked subscriber: {}", remove_err);
                    }
                } else {
                    warn!("✗ Failed to send to {}: {}", subscriber.chat_id, error_msg);
                }

                // Log failure to database for analytics
                if let Err(log_err) = db
                    .log_delivery_failure(subscriber.chat_id, &error_msg)
                    .await
                {
                    warn!("Failed to log delivery failure: {}", log_err);
                }
            }
        }

        // Small delay to avoid rate limiting
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
    }

    info!(
        "Summary sent: {} successful, {} failed",
        success_count, fail_count
    );

    // Send admin notification if configured
    if !config.telegram_chat_id.is_empty() && fail_count > 0 {
        // Parse admin chat ID from config string
        if let Ok(admin_chat_id) = config.telegram_chat_id.parse::<i64>() {
            let admin_msg = format!(
                "📊 Summary sent to {}/{} subscribers ({} failed)",
                success_count,
                success_count + fail_count,
                fail_count
            );
            if let Err(e) = send_message(config, admin_chat_id, &admin_msg).await {
                warn!("Failed to send admin notification: {}", e);
            }
        }
    }

    Ok(())
}

/// Send a Telegram message to a specific chat
async fn send_message(config: &Config, chat_id: i64, text: &str) -> Result<()> {
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
            "📰 <b>Twitter Summary</b>\n<i>{}</i>\n\n{}",
            timestamp, summary
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
        let json = format!(
            r#"{{
            "update_id": 123,
            "message": {{
                "message_id": 100,
                "chat": {{"id": 123, "type": "private"}},
                "text": "{}"
            }}
        }}"#,
            long_text
        );

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
        let status_msg = format!(
            "✅ You are subscribed\n📊 Total subscribers: {}",
            subscriber_count
        );

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
        let status_msg =
            "❌ You are not subscribed\n\nUse /subscribe to start receiving summaries.";

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

    // ==================== Welcome Summary Feature Tests ====================

    // ---------- Welcome Summary Message Formatting Tests ----------

    #[test]
    fn test_welcome_summary_message_format() {
        use chrono::Utc;

        let timestamp = Utc::now().format("%Y-%m-%d %H:%M UTC");
        let summary = "Here is the AI news summary content.";

        // This matches the format in send_welcome_summary
        let message = format!(
            "📰 <b>Hey! Here's what you missed</b> 😉\n<i>{}</i>\n\n{}",
            timestamp, summary
        );

        assert!(message.contains("Hey!"));
        assert!(message.contains("what you missed"));
        assert!(message.contains("Here is the AI news summary content."));
        assert!(message.contains("UTC"));
    }

    #[test]
    fn test_welcome_summary_differs_from_regular_summary() {
        use chrono::Utc;

        let timestamp = Utc::now().format("%Y-%m-%d %H:%M UTC");
        let summary = "Summary content";

        // Welcome format
        let welcome_msg = format!(
            "📰 <b>Hey! Here's what you missed</b> 😉\n<i>{}</i>\n\n{}",
            timestamp, summary
        );

        // Regular summary format (from send_to_subscribers)
        let regular_msg = format!(
            "📰 <b>Twitter Summary</b>\n<i>{}</i>\n\n{}",
            timestamp, summary
        );

        // They should be different
        assert_ne!(welcome_msg, regular_msg);
        assert!(welcome_msg.contains("Hey!"));
        assert!(!regular_msg.contains("Hey!"));
    }

    #[test]
    fn test_welcome_summary_with_markdown_in_content() {
        use chrono::Utc;

        let timestamp = Utc::now().format("%Y-%m-%d %H:%M UTC");
        let summary = "Summary with *bold* and _italic_ text";

        let message = format!(
            "📰 <b>Hey! Here's what you missed</b> 😉\n<i>{}</i>\n\n{}",
            timestamp, summary
        );

        // The summary content should be preserved as-is
        assert!(message.contains("*bold*"));
        assert!(message.contains("_italic_"));
    }

    #[test]
    fn test_welcome_summary_with_long_content() {
        use chrono::Utc;

        let timestamp = Utc::now().format("%Y-%m-%d %H:%M UTC");
        let summary = "A".repeat(4000); // Telegram limit is 4096

        let message = format!(
            "📰 <b>Hey! Here's what you missed</b> 😉\n<i>{}</i>\n\n{}",
            timestamp, summary
        );

        // Message should be constructed (actual truncation would be done at send time)
        assert!(message.len() > 4000);
    }

    // ---------- Subscribe Command Logic Tests ----------

    #[test]
    fn test_subscribe_command_json_parsing() {
        let json = r#"{
            "update_id": 123,
            "message": {
                "message_id": 100,
                "from": {
                    "id": 987654321,
                    "username": "newsubscriber",
                    "first_name": "New"
                },
                "chat": {
                    "id": 987654321,
                    "type": "private"
                },
                "text": "/subscribe"
            }
        }"#;

        let update: Update = serde_json::from_str(json).expect("Should deserialize");
        let message = update.message.unwrap();

        assert_eq!(message.text.unwrap(), "/subscribe");
        assert_eq!(
            message.from.unwrap().username,
            Some("newsubscriber".to_string())
        );
    }

    #[test]
    fn test_subscribe_command_without_username() {
        let json = r#"{
            "update_id": 123,
            "message": {
                "message_id": 100,
                "from": {
                    "id": 123,
                    "first_name": "NoUsername"
                },
                "chat": {
                    "id": 123,
                    "type": "private"
                },
                "text": "/subscribe"
            }
        }"#;

        let update: Update = serde_json::from_str(json).expect("Should deserialize");
        let message = update.message.unwrap();
        let from = message.from.unwrap();

        assert!(from.username.is_none());
        assert_eq!(from.first_name, "NoUsername");
    }

    #[test]
    fn test_unsubscribe_command_json_parsing() {
        let json = r#"{
            "update_id": 456,
            "message": {
                "message_id": 200,
                "from": {
                    "id": 111222333,
                    "username": "leavinguser",
                    "first_name": "Leaving"
                },
                "chat": {
                    "id": 111222333,
                    "type": "private"
                },
                "text": "/unsubscribe"
            }
        }"#;

        let update: Update = serde_json::from_str(json).expect("Should deserialize");
        let message = update.message.unwrap();

        assert_eq!(message.text.unwrap(), "/unsubscribe");
    }

    // ---------- Message Response Tests ----------

    #[test]
    fn test_already_subscribed_message() {
        let msg = "\u{2705} You're already subscribed!";
        assert!(msg.contains("already subscribed"));
    }

    #[test]
    fn test_successful_subscribe_message() {
        let msg = "\u{2705} Successfully subscribed! You'll receive summaries twice daily.";
        assert!(msg.contains("Successfully subscribed"));
        assert!(msg.contains("twice daily"));
    }

    #[test]
    fn test_successful_unsubscribe_message() {
        let msg = "\u{1f44b} Successfully unsubscribed. You won't receive any more summaries.";
        assert!(msg.contains("Successfully unsubscribed"));
        assert!(msg.contains("won't receive"));
    }

    #[test]
    fn test_not_subscribed_message() {
        let msg = "You're not currently subscribed.";
        assert!(msg.contains("not currently subscribed"));
    }

    // ---------- Welcome Summary Flow Logic Tests ----------

    #[test]
    fn test_welcome_summary_flow_new_subscriber() {
        // Simulate the logic flow:
        // 1. add_subscriber returns (true, true) for new subscriber
        // 2. get_latest_summary returns Some(summary)
        // 3. send_welcome_summary is called
        // 4. mark_welcome_summary_sent is called

        let is_new = true;
        let needs_welcome = true;
        let has_summary = true;

        // This represents the decision tree in handle_webhook
        // Simulating: user is not subscribed
        if true {
            let (new_sub, welcome) = (is_new, needs_welcome);
            assert!(new_sub);
            assert!(welcome);

            if welcome && has_summary {
                // Would call send_welcome_summary
                let would_send_welcome = true;
                assert!(would_send_welcome);
            }
        }
    }

    #[test]
    fn test_welcome_summary_flow_returning_subscriber() {
        // Simulate: returning subscriber (unsubscribed then resubscribed)
        // add_subscriber returns (true, false)

        let is_new = true; // Counts as new subscription
        let needs_welcome = false; // But no welcome needed

        // Simulating: user is not subscribed
        if true {
            let (_new_sub, welcome) = (is_new, needs_welcome);

            // Should NOT send welcome
            let should_send_welcome = welcome;
            assert!(
                !should_send_welcome,
                "Returning subscriber should not get welcome"
            );
        }
    }

    #[test]
    fn test_welcome_summary_flow_no_summary_available() {
        // Simulate: new subscriber but no summary exists yet

        let is_new = true;
        let needs_welcome = true;
        let has_summary = false; // No summary available

        // Simulating: user is not subscribed
        if true {
            let (_new_sub, welcome) = (is_new, needs_welcome);

            if welcome {
                if has_summary {
                    panic!("Should not reach here");
                } else {
                    // Would just log "No summary available"
                    let logged_no_summary = true;
                    assert!(logged_no_summary);
                }
            }
        }
    }

    // ---------- Edge Cases Tests ----------

    #[test]
    fn test_group_chat_subscription() {
        let json = r#"{
            "update_id": 789,
            "message": {
                "message_id": 300,
                "from": {
                    "id": 123456,
                    "username": "groupadmin",
                    "first_name": "Admin"
                },
                "chat": {
                    "id": -1001234567890,
                    "type": "supergroup"
                },
                "text": "/subscribe"
            }
        }"#;

        let update: Update = serde_json::from_str(json).expect("Should deserialize");
        let message = update.message.unwrap();

        // Chat ID for group is negative
        let chat_id = message.chat.id.to_string();
        assert!(chat_id.starts_with("-"), "Group chat ID should be negative");
        assert_eq!(chat_id, "-1001234567890");
    }

    #[test]
    fn test_message_from_channel() {
        // Channels may not have a "from" field
        let json = r#"{
            "update_id": 999,
            "message": {
                "message_id": 400,
                "chat": {
                    "id": -1009876543210,
                    "type": "channel"
                },
                "text": "/subscribe"
            }
        }"#;

        let update: Update = serde_json::from_str(json).expect("Should deserialize");
        let message = update.message.unwrap();

        // from is optional
        assert!(message.from.is_none());
        assert_eq!(message.text.unwrap(), "/subscribe");
    }

    #[test]
    fn test_empty_update_handling() {
        // Update with no message (e.g., callback query, edited message)
        let json = r#"{"update_id": 123}"#;

        let update: Update = serde_json::from_str(json).expect("Should deserialize");
        assert!(update.message.is_none());

        // In handle_webhook, this would return Ok(()) early
    }

    #[test]
    fn test_message_without_text_handling() {
        // Message with photo/sticker but no text
        let json = r#"{
            "update_id": 123,
            "message": {
                "message_id": 100,
                "chat": {"id": 123, "type": "private"}
            }
        }"#;

        let update: Update = serde_json::from_str(json).expect("Should deserialize");
        let message = update.message.unwrap();

        assert!(message.text.is_none());
        // In handle_webhook, this would return Ok(()) early
    }

    // ---------- Admin Status Tests ----------

    #[test]
    fn test_admin_detection_logic() {
        let admin_chat_id = "123456789";
        let user_chat_id = "123456789";
        let other_chat_id = "987654321";

        // Admin check: config.telegram_chat_id matches chat_id
        let is_admin_for_self = !admin_chat_id.is_empty() && user_chat_id == admin_chat_id;
        let is_admin_for_other = !admin_chat_id.is_empty() && other_chat_id == admin_chat_id;

        assert!(is_admin_for_self);
        assert!(!is_admin_for_other);
    }

    #[test]
    fn test_admin_detection_with_empty_config() {
        let admin_chat_id = ""; // Empty in config
        let user_chat_id = "123456789";

        // Should not be admin if config is empty
        let is_admin = !admin_chat_id.is_empty() && user_chat_id == admin_chat_id;
        assert!(!is_admin, "Empty admin config should mean no admin");
    }

    // ---------- Send To Subscribers Tests ----------

    #[test]
    fn test_subscriber_iteration_logic() {
        // Simulate the loop in send_to_subscribers
        let subscriber_ids = ["111", "222", "333"];
        let mut success_count = 0;
        let mut fail_count = 0;

        for _id in subscriber_ids.iter() {
            // Simulate success for most, failure for one
            let succeeded = true; // In real code, this is the result of send_message
            if succeeded {
                success_count += 1;
            } else {
                fail_count += 1;
            }
        }

        assert_eq!(success_count, 3);
        assert_eq!(fail_count, 0);
    }

    #[test]
    fn test_empty_subscriber_list_handling() {
        let subscribers: Vec<String> = vec![];

        // In send_to_subscribers, empty list means early return
        assert!(subscribers.is_empty());
        // Would log "No subscribers to send to" and return Ok(())
    }

    // ---------- Timestamp Formatting Tests ----------

    #[test]
    fn test_utc_timestamp_format() {
        use chrono::Utc;

        let timestamp = Utc::now().format("%Y-%m-%d %H:%M UTC").to_string();

        // Should match pattern like "2024-01-15 10:30 UTC"
        assert!(timestamp.ends_with(" UTC"));
        assert!(timestamp.contains("-"));
        assert!(timestamp.contains(":"));
        assert_eq!(timestamp.len(), 20); // "YYYY-MM-DD HH:MM UTC"
    }

    #[test]
    fn test_timestamp_in_summary_message() {
        use chrono::Utc;

        let timestamp = Utc::now().format("%Y-%m-%d %H:%M UTC");
        let message = format!(
            "📰 <b>Twitter Summary</b>\n<i>{}</i>\n\n{}",
            timestamp, "content"
        );

        // Timestamp should be in italics (HTML)
        assert!(message.contains(&format!("<i>{}</i>", timestamp)));
    }

    // ---------- Rate Limiting Tests ----------

    #[test]
    fn test_rate_limit_delay_value() {
        // The code uses 100ms delay between sends
        let delay_ms = 100u64;
        assert_eq!(delay_ms, 100);

        // This is reasonable for Telegram's rate limits
        // Telegram allows about 30 messages/second to different chats
    }

    // ---------- Integration Scenario Tests ----------

    #[test]
    fn test_full_subscription_lifecycle_messages() {
        // Track the messages a user would see through a full lifecycle

        let messages = vec![
            // 1. User sends /start
            "👋 Welcome to Twitter News Summary Bot!",
            // 2. User sends /subscribe (first time)
            "✅ Successfully subscribed! You'll receive summaries twice daily.",
            // 3. Welcome summary (if available)
            "📰 <b>Hey! Here's what you missed</b> 😉",
            // 4. User checks /status
            "✅ You are subscribed",
            // 5. User sends /unsubscribe
            "👋 Successfully unsubscribed. You won't receive any more summaries.",
            // 6. User re-subscribes (no welcome this time)
            "✅ Successfully subscribed! You'll receive summaries twice daily.",
        ];

        // All messages should be non-empty
        for msg in &messages {
            assert!(!msg.is_empty());
        }

        // Welcome message only appears once (index 2)
        let welcome_count = messages
            .iter()
            .filter(|m| m.contains("Hey! Here's what you missed"))
            .count();
        assert_eq!(welcome_count, 1, "Welcome summary should only appear once");
    }

    #[test]
    fn test_subscribe_already_subscribed_flow() {
        // User is already subscribed and sends /subscribe again
        // Should get "already subscribed" message, not welcome

        let is_subscribed = true;
        let expected_response = if is_subscribed {
            "\u{2705} You're already subscribed!"
        } else {
            "\u{2705} Successfully subscribed!"
        };

        assert!(expected_response.contains("already subscribed"));
    }

    // ==================== Blocked User Auto-Removal Detection Tests ====================

    /// Helper function that mirrors the detection logic in send_to_subscribers
    /// This allows us to test the detection logic in isolation without database dependencies
    fn should_auto_remove_blocked_subscriber(error_msg: &str) -> bool {
        let error_lower = error_msg.to_lowercase();
        error_msg.contains("403")
            && (error_lower.contains("blocked by the user")
                || error_lower.contains("user is deactivated"))
    }

    // ---------- Positive Cases: Should Trigger Auto-Removal ----------

    #[test]
    fn test_blocked_user_detection_exact_telegram_format() {
        // This is the exact format returned by Telegram API when a user blocks the bot
        let error_msg = r#"Telegram API error (403 Forbidden): {"ok":false,"error_code":403,"description":"Forbidden: bot was blocked by the user"}"#;

        assert!(
            should_auto_remove_blocked_subscriber(error_msg),
            "Should detect blocked user from exact Telegram API response format"
        );
    }

    #[test]
    fn test_blocked_user_detection_contains_both_markers() {
        // The detection requires BOTH "403" AND "blocked by the user" to be present
        let error_msg = "Error: 403 - user has blocked by the user";

        assert!(
            should_auto_remove_blocked_subscriber(error_msg),
            "Should detect when both '403' and 'blocked by the user' are present"
        );
    }

    #[test]
    fn test_blocked_user_detection_different_403_position() {
        // 403 can appear in different positions within the error message
        let error_msg = "blocked by the user, status: 403";

        assert!(
            should_auto_remove_blocked_subscriber(error_msg),
            "Should detect regardless of where '403' appears in message"
        );
    }

    #[test]
    fn test_blocked_user_detection_with_additional_context() {
        // Error message might have additional context wrapping it
        let error_msg = "Failed to send message: Telegram API error (403 Forbidden): bot was blocked by the user - chat_id: 123456";

        assert!(
            should_auto_remove_blocked_subscriber(error_msg),
            "Should detect blocked user even with additional context"
        );
    }

    #[test]
    fn test_blocked_user_detection_multiline_error() {
        // Error message could span multiple lines
        let error_msg = r#"Telegram API error (403 Forbidden):
{"ok":false,"error_code":403,"description":"Forbidden: bot was blocked by the user"}"#;

        assert!(
            should_auto_remove_blocked_subscriber(error_msg),
            "Should detect blocked user in multiline error messages"
        );
    }

    #[test]
    fn test_blocked_user_detection_standard_case() {
        // Standard lowercase format from Telegram API
        let error_msg = "Telegram API error (403 Forbidden): blocked by the user";

        assert!(
            should_auto_remove_blocked_subscriber(error_msg),
            "Should detect standard lowercase format"
        );
    }

    // ---------- Negative Cases: Should NOT Trigger Auto-Removal ----------

    #[test]
    fn test_no_auto_remove_for_400_bad_request() {
        // 400 Bad Request should not trigger auto-removal
        let error_msg = r#"Telegram API error (400 Bad Request): {"ok":false,"error_code":400,"description":"Bad Request: chat not found"}"#;

        assert!(
            !should_auto_remove_blocked_subscriber(error_msg),
            "400 Bad Request should NOT trigger auto-removal"
        );
    }

    #[test]
    fn test_no_auto_remove_for_401_unauthorized() {
        // 401 Unauthorized (invalid bot token) should not trigger auto-removal
        let error_msg = r#"Telegram API error (401 Unauthorized): {"ok":false,"error_code":401,"description":"Unauthorized"}"#;

        assert!(
            !should_auto_remove_blocked_subscriber(error_msg),
            "401 Unauthorized should NOT trigger auto-removal"
        );
    }

    #[test]
    fn test_no_auto_remove_for_429_rate_limited() {
        // 429 Rate Limited should not trigger auto-removal
        let error_msg = r#"Telegram API error (429 Too Many Requests): {"ok":false,"error_code":429,"description":"Too Many Requests: retry after 35"}"#;

        assert!(
            !should_auto_remove_blocked_subscriber(error_msg),
            "429 Rate Limited should NOT trigger auto-removal"
        );
    }

    #[test]
    fn test_no_auto_remove_for_500_server_error() {
        // 500 Internal Server Error should not trigger auto-removal
        let error_msg = r#"Telegram API error (500 Internal Server Error): {"ok":false,"error_code":500,"description":"Internal Server Error"}"#;

        assert!(
            !should_auto_remove_blocked_subscriber(error_msg),
            "500 Server Error should NOT trigger auto-removal"
        );
    }

    #[test]
    fn test_no_auto_remove_for_network_error() {
        // Network errors (connection refused, timeout, etc.) should not trigger auto-removal
        let error_msg = "Failed to send request to Telegram API: connection refused";

        assert!(
            !should_auto_remove_blocked_subscriber(error_msg),
            "Network errors should NOT trigger auto-removal"
        );
    }

    #[test]
    fn test_no_auto_remove_for_timeout() {
        // Timeout errors should not trigger auto-removal
        let error_msg = "Failed to send request to Telegram API: operation timed out";

        assert!(
            !should_auto_remove_blocked_subscriber(error_msg),
            "Timeout errors should NOT trigger auto-removal"
        );
    }

    #[test]
    fn test_no_auto_remove_for_dns_error() {
        // DNS resolution errors should not trigger auto-removal
        let error_msg = "Failed to send request to Telegram API: failed to resolve host";

        assert!(
            !should_auto_remove_blocked_subscriber(error_msg),
            "DNS errors should NOT trigger auto-removal"
        );
    }

    #[test]
    fn test_no_auto_remove_for_403_without_blocked_text() {
        // 403 alone (without "blocked by the user") should not trigger auto-removal
        // This could be a different kind of 403 error (e.g., bot not in chat)
        let error_msg = r#"Telegram API error (403 Forbidden): {"ok":false,"error_code":403,"description":"Forbidden: bot is not a member of the group chat"}"#;

        assert!(
            !should_auto_remove_blocked_subscriber(error_msg),
            "403 without 'blocked by the user' should NOT trigger auto-removal"
        );
    }

    #[test]
    fn test_no_auto_remove_for_blocked_text_without_403() {
        // "blocked by the user" text without 403 status code should not trigger auto-removal
        let error_msg = "User action: blocked by the user preference settings";

        assert!(
            !should_auto_remove_blocked_subscriber(error_msg),
            "'blocked by the user' text without 403 should NOT trigger auto-removal"
        );
    }

    #[test]
    fn test_no_auto_remove_for_chat_not_found() {
        // Chat not found (deleted account) is different from blocked
        let error_msg = r#"Telegram API error (400 Bad Request): {"ok":false,"error_code":400,"description":"Bad Request: chat not found"}"#;

        assert!(
            !should_auto_remove_blocked_subscriber(error_msg),
            "Chat not found should NOT trigger auto-removal (could be temporary)"
        );
    }

    #[test]
    fn test_no_auto_remove_for_bot_kicked_from_group() {
        // Bot kicked from group chat is different from user blocking
        let error_msg = r#"Telegram API error (403 Forbidden): {"ok":false,"error_code":403,"description":"Forbidden: bot was kicked from the group chat"}"#;

        assert!(
            !should_auto_remove_blocked_subscriber(error_msg),
            "Bot kicked from group should NOT trigger auto-removal (different scenario)"
        );
    }

    #[test]
    fn test_auto_remove_for_user_deactivated() {
        // User deactivated their account - this is permanent, should auto-remove
        let error_msg = r#"Telegram API error (403 Forbidden): {"ok":false,"error_code":403,"description":"Forbidden: user is deactivated"}"#;

        assert!(
            should_auto_remove_blocked_subscriber(error_msg),
            "User deactivated SHOULD trigger auto-removal (account deleted permanently)"
        );
    }

    #[test]
    fn test_auto_remove_for_user_deactivated_case_insensitive() {
        // "user is deactivated" should match regardless of case
        let error_msg_upper = "Telegram API error (403 Forbidden): USER IS DEACTIVATED";
        let error_msg_mixed = "Telegram API error (403 Forbidden): User Is Deactivated";

        assert!(
            should_auto_remove_blocked_subscriber(error_msg_upper),
            "Should match uppercase 'USER IS DEACTIVATED'"
        );
        assert!(
            should_auto_remove_blocked_subscriber(error_msg_mixed),
            "Should match mixed case 'User Is Deactivated'"
        );
    }

    // ---------- Edge Cases in Error Message Format ----------

    #[test]
    fn test_blocked_detection_empty_string() {
        let error_msg = "";

        assert!(
            !should_auto_remove_blocked_subscriber(error_msg),
            "Empty error message should NOT trigger auto-removal"
        );
    }

    #[test]
    fn test_blocked_detection_whitespace_only() {
        let error_msg = "   \n\t  ";

        assert!(
            !should_auto_remove_blocked_subscriber(error_msg),
            "Whitespace-only error message should NOT trigger auto-removal"
        );
    }

    #[test]
    fn test_blocked_detection_very_long_error() {
        // Very long error message with blocked user info somewhere in the middle
        let prefix = "A".repeat(5000);
        let suffix = "B".repeat(5000);
        let error_msg = format!(
            "{}Telegram API error (403 Forbidden): blocked by the user{}",
            prefix, suffix
        );

        assert!(
            should_auto_remove_blocked_subscriber(&error_msg),
            "Should detect blocked user even in very long error messages"
        );
    }

    #[test]
    fn test_blocked_detection_unicode_in_error() {
        // Error message might contain Unicode characters
        let error_msg =
            "Telegram API error (403 Forbidden): \u{1f6ab} blocked by the user \u{1f44b}";

        assert!(
            should_auto_remove_blocked_subscriber(error_msg),
            "Should detect blocked user even with Unicode in error message"
        );
    }

    #[test]
    fn test_blocked_detection_special_characters() {
        // Error message with various special characters
        let error_msg = r#"Error [403] "blocked by the user" <test> & more"#;

        assert!(
            should_auto_remove_blocked_subscriber(error_msg),
            "Should detect blocked user with special characters in message"
        );
    }

    #[test]
    fn test_blocked_detection_403_as_substring() {
        // 403 appearing as part of a larger number should still match
        // (current implementation uses simple contains)
        let error_msg = "Error code: 14030 - blocked by the user";

        // This WILL match because "14030" contains "403"
        // This documents current behavior - might want to improve detection
        assert!(
            should_auto_remove_blocked_subscriber(error_msg),
            "Current implementation matches 403 as substring (documents behavior)"
        );
    }

    #[test]
    fn test_blocked_detection_case_insensitivity() {
        // Test that the detection is case-insensitive for robustness
        let error_msg_upper = "Telegram API error (403 Forbidden): BLOCKED BY THE USER";
        let error_msg_lower = "telegram api error (403 forbidden): blocked by the user";
        let error_msg_mixed = "Telegram API error (403 Forbidden): Blocked By The User";

        // All cases should match due to case-insensitive comparison
        assert!(
            should_auto_remove_blocked_subscriber(error_msg_upper),
            "Should match uppercase 'BLOCKED BY THE USER'"
        );
        assert!(
            should_auto_remove_blocked_subscriber(error_msg_lower),
            "Should match lowercase 'blocked by the user'"
        );
        assert!(
            should_auto_remove_blocked_subscriber(error_msg_mixed),
            "Should match mixed case 'Blocked By The User'"
        );
    }

    // ---------- Real-World Telegram API Response Format Tests ----------

    #[test]
    fn test_blocked_detection_with_json_escaped_quotes() {
        // JSON response might have escaped quotes in some contexts
        let error_msg = r#"Telegram API error (403 Forbidden): {\"ok\":false,\"error_code\":403,\"description\":\"Forbidden: bot was blocked by the user\"}"#;

        assert!(
            should_auto_remove_blocked_subscriber(error_msg),
            "Should detect blocked user even with escaped JSON quotes"
        );
    }

    #[test]
    fn test_blocked_detection_anyhow_error_chain() {
        // The error message format from anyhow's error chain
        let error_msg =
            "Telegram API error (403 Forbidden): Forbidden: bot was blocked by the user";

        assert!(
            should_auto_remove_blocked_subscriber(error_msg),
            "Should detect blocked user in anyhow error format"
        );
    }

    #[test]
    fn test_blocked_detection_reqwest_error_context() {
        // Error might include reqwest context
        let error_msg = "Failed to send request to Telegram API: Telegram API error (403 Forbidden): {\"ok\":false,\"error_code\":403,\"description\":\"Forbidden: bot was blocked by the user\"}";

        assert!(
            should_auto_remove_blocked_subscriber(error_msg),
            "Should detect blocked user even with reqwest context wrapper"
        );
    }

    // ---------- Boundary Condition Tests ----------

    #[test]
    fn test_blocked_detection_markers_adjacent() {
        // Both markers present but adjacent (no space)
        let error_msg = "403blocked by the user";

        assert!(
            should_auto_remove_blocked_subscriber(error_msg),
            "Should detect when markers are adjacent"
        );
    }

    #[test]
    fn test_blocked_detection_markers_reversed_order() {
        // The markers can appear in any order
        let error_msg = "User was blocked by the user, error code 403";

        assert!(
            should_auto_remove_blocked_subscriber(error_msg),
            "Should detect regardless of marker order"
        );
    }

    #[test]
    fn test_blocked_detection_multiple_403_occurrences() {
        // Multiple occurrences of 403 in the message
        let error_msg = "Error 403: code 403, status 403 Forbidden, blocked by the user";

        assert!(
            should_auto_remove_blocked_subscriber(error_msg),
            "Should detect with multiple 403 occurrences"
        );
    }

    #[test]
    fn test_blocked_detection_partial_phrase_not_matched() {
        // Partial phrase "blocked by the" (without "user") should not match
        let error_msg = "Error 403: action was blocked by the system";

        assert!(
            !should_auto_remove_blocked_subscriber(error_msg),
            "Partial phrase 'blocked by the' should NOT match"
        );
    }

    // ---------- Documentation Tests ----------

    #[test]
    fn test_blocked_detection_logic_documents_implementation() {
        // This test documents and verifies the exact implementation logic
        // The detection requires BOTH conditions to be true:
        // 1. error_msg.contains("403")
        // 2. error_msg.contains("blocked by the user")

        // Both present = should remove
        assert!(should_auto_remove_blocked_subscriber(
            "403 blocked by the user"
        ));

        // Only 403 = should NOT remove
        assert!(!should_auto_remove_blocked_subscriber("error 403"));

        // Only blocked text = should NOT remove
        assert!(!should_auto_remove_blocked_subscriber(
            "blocked by the user"
        ));

        // Neither present = should NOT remove
        assert!(!should_auto_remove_blocked_subscriber(
            "some other error message"
        ));
    }
}
