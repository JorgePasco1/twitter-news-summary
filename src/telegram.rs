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
        "📰 <b>Twitter Summary</b>\n<i>{}</i>\n\n{}",
        timestamp,
        summary
    );

    // Save to run-history folder for debugging (only when running locally)
    if std::env::var("CI").is_err() {
        let filename = format!(
            "run-history/summary_{}.txt",
            Utc::now().format("%Y%m%d_%H%M%S")
        );
        // Ignore errors if directory doesn't exist or write fails
        let _ = std::fs::write(&filename, &message);
    }

    // Telegram Bot API endpoint
    let url = format!(
        "https://api.telegram.org/bot{}/sendMessage",
        config.telegram_bot_token
    );

    let request = SendMessageRequest {
        chat_id: config.telegram_chat_id.clone(),
        text: message,
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
