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
            let status_msg = if is_subscribed {
                format!("✅ You are subscribed\n📊 Total subscribers: {}", db.subscriber_count()?)
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
        let _ = send_message(config, &config.telegram_chat_id, &admin_msg).await;
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
