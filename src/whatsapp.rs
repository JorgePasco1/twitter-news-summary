use anyhow::{Context, Result};
use chrono::Utc;
use crate::config::Config;

/// Send a WhatsApp message via Twilio API
pub async fn send_message(config: &Config, summary: &str) -> Result<()> {
    let client = reqwest::Client::new();

    let timestamp = Utc::now().format("%Y-%m-%d %H:%M UTC");
    let message = format!(
        "📰 *Twitter Summary*\n_{}_\n\n{}",
        timestamp,
        summary
    );

    // Twilio API endpoint for sending messages
    let url = format!(
        "https://api.twilio.com/2010-04-01/Accounts/{}/Messages.json",
        config.twilio_account_sid
    );

    let response = client
        .post(&url)
        .basic_auth(&config.twilio_account_sid, Some(&config.twilio_auth_token))
        .form(&[
            ("From", config.twilio_whatsapp_from.as_str()),
            ("To", config.whatsapp_to.as_str()),
            ("Body", message.as_str()),
        ])
        .send()
        .await
        .context("Failed to send request to Twilio API")?;

    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        anyhow::bail!("Twilio API error ({}): {}", status, body);
    }

    Ok(())
}
