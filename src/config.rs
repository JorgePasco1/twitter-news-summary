use anyhow::{Context, Result};

#[derive(Debug, Clone)]
pub struct Config {
    // Twitter
    pub twitter_bearer_token: String,
    pub twitter_list_id: String,

    // OpenAI
    pub openai_api_key: String,
    pub openai_model: String,

    // Twilio WhatsApp
    pub twilio_account_sid: String,
    pub twilio_auth_token: String,
    pub twilio_whatsapp_from: String,
    pub whatsapp_to: String,
}

impl Config {
    pub fn from_env() -> Result<Self> {
        Ok(Self {
            // Twitter - using Bearer Token (OAuth 2.0 App-Only)
            twitter_bearer_token: std::env::var("TWITTER_BEARER_TOKEN")
                .context("TWITTER_BEARER_TOKEN not set")?,
            twitter_list_id: std::env::var("TWITTER_LIST_ID")
                .context("TWITTER_LIST_ID not set")?,

            // OpenAI
            openai_api_key: std::env::var("OPENAI_API_KEY")
                .context("OPENAI_API_KEY not set")?,
            openai_model: std::env::var("OPENAI_MODEL")
                .unwrap_or_else(|_| "gpt-4o-mini".to_string()),

            // Twilio
            twilio_account_sid: std::env::var("TWILIO_ACCOUNT_SID")
                .context("TWILIO_ACCOUNT_SID not set")?,
            twilio_auth_token: std::env::var("TWILIO_AUTH_TOKEN")
                .context("TWILIO_AUTH_TOKEN not set")?,
            twilio_whatsapp_from: std::env::var("TWILIO_WHATSAPP_FROM")
                .context("TWILIO_WHATSAPP_FROM not set (format: whatsapp:+14155238886)")?,
            whatsapp_to: std::env::var("WHATSAPP_TO")
                .context("WHATSAPP_TO not set (format: whatsapp:+1234567890)")?,
        })
    }
}
