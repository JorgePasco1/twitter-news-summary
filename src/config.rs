use anyhow::{Context, Result};

#[derive(Debug, Clone)]
pub struct Config {
    // Twitter (optional - only used by export binary)
    #[allow(dead_code)]
    pub twitter_bearer_token: Option<String>,
    #[allow(dead_code)]
    pub twitter_list_id: Option<String>,

    // OpenAI
    pub openai_api_key: String,
    pub openai_model: String,

    // Telegram
    pub telegram_bot_token: String,
    pub telegram_chat_id: String,

    // Filtering
    pub max_tweets: u32,
    pub hours_lookback: u32,

    // RSS/Nitter
    pub nitter_instance: String,
    pub nitter_fallback_instances: Vec<String>,
    pub usernames_file: String,
}

impl Config {
    pub fn from_env() -> Result<Self> {
        Ok(Self {
            // Twitter - Bearer Token (OAuth 2.0 App-Only) - Optional, only for export binary
            twitter_bearer_token: std::env::var("TWITTER_BEARER_TOKEN").ok(),
            twitter_list_id: std::env::var("TWITTER_LIST_ID").ok(),

            // OpenAI
            openai_api_key: std::env::var("OPENAI_API_KEY")
                .context("OPENAI_API_KEY not set")?,
            openai_model: std::env::var("OPENAI_MODEL")
                .unwrap_or_else(|_| "gpt-4o-mini".to_string()),

            // Telegram
            telegram_bot_token: std::env::var("TELEGRAM_BOT_TOKEN")
                .context("TELEGRAM_BOT_TOKEN not set")?,
            telegram_chat_id: std::env::var("TELEGRAM_CHAT_ID")
                .context("TELEGRAM_CHAT_ID not set")?,

            // Filtering
            max_tweets: std::env::var("MAX_TWEETS")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(50),
            hours_lookback: std::env::var("HOURS_LOOKBACK")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(12),

            // RSS/Nitter
            nitter_instance: std::env::var("NITTER_INSTANCE")
                .unwrap_or_else(|_| "https://nitter.net".to_string()),
            nitter_fallback_instances: vec![
                "https://nitter.poast.org".to_string(),
                "https://nitter.privacydev.net".to_string(),
                "https://nitter.1d4.us".to_string(),
                "https://nitter.cz".to_string(),
                "https://nitter.unixfox.eu".to_string(),
            ],
            usernames_file: std::env::var("USERNAMES_FILE")
                .unwrap_or_else(|_| "data/usernames.txt".to_string()),
        })
    }
}
