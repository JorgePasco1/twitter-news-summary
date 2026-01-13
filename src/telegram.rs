use anyhow::{Context, Result};
use chrono::Utc;
use serde::Serialize;
use crate::config::Config;

#[derive(Debug, Serialize)]
struct SendMessageRequest {
    chat_id: String,
    text: String,
    parse_mode: String,
}

/// Send a Telegram message via Telegram Bot API
pub async fn send_message(config: &Config, summary: &str) -> Result<()> {
    let client = reqwest::Client::new();

    let timestamp = Utc::now().format("%Y-%m-%d %H:%M UTC");
    let message = format!(
        "📰 *Twitter Summary*\n_{}_\n\n{}",
        timestamp,
        summary
    );

    // Telegram Bot API endpoint
    let url = format!(
        "https://api.telegram.org/bot{}/sendMessage",
        config.telegram_bot_token
    );

    let request = SendMessageRequest {
        chat_id: config.telegram_chat_id.clone(),
        text: message,
        parse_mode: "Markdown".to_string(),
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
