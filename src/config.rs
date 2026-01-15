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
    pub telegram_chat_id: String,  // Admin chat ID for notifications

    // Filtering
    pub max_tweets: u32,
    pub hours_lookback: u32,

    // RSS/Nitter
    pub nitter_instance: String,
    pub nitter_api_key: Option<String>,
    pub usernames_file: String,

    // Service (for web server mode)
    pub api_key: Option<String>,
    pub database_path: String,
    pub schedule_times: Vec<String>,
    pub port: u16,
}

impl Config {
    pub fn from_env() -> Result<Self> {
        // Parse schedule times
        let schedule_times_str = std::env::var("SCHEDULE_TIMES")
            .unwrap_or_else(|_| "08:00,20:00".to_string());
        let schedule_times: Vec<String> = schedule_times_str
            .split(',')
            .map(|s| s.trim().to_string())
            .collect();

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
                .unwrap_or_else(|_| "".to_string()),  // Optional in service mode

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
                .context("NITTER_INSTANCE not set - you must provide your own Nitter instance URL")?,
            nitter_api_key: std::env::var("NITTER_API_KEY").ok(),
            usernames_file: std::env::var("USERNAMES_FILE")
                .unwrap_or_else(|_| "data/usernames.txt".to_string()),

            // Service
            api_key: std::env::var("API_KEY").ok(),
            database_path: std::env::var("DATABASE_PATH")
                .unwrap_or_else(|_| "/data/subscribers.db".to_string()),
            schedule_times,
            port: std::env::var("PORT")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(8080),
        })
    }
}
