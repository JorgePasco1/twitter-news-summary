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

#[cfg(test)]
mod tests {
    use super::*;
    use std::env;
    use std::sync::Mutex;

    // Global mutex to ensure test isolation for environment variable tests
    // Environment variables are process-global, so tests must not run concurrently
    static ENV_MUTEX: Mutex<()> = Mutex::new(());

    /// Helper to clear all config-related environment variables
    fn clear_env_vars() {
        let vars = [
            "TWITTER_BEARER_TOKEN",
            "TWITTER_LIST_ID",
            "OPENAI_API_KEY",
            "OPENAI_MODEL",
            "TELEGRAM_BOT_TOKEN",
            "TELEGRAM_CHAT_ID",
            "MAX_TWEETS",
            "HOURS_LOOKBACK",
            "NITTER_INSTANCE",
            "NITTER_API_KEY",
            "USERNAMES_FILE",
            "API_KEY",
            "DATABASE_PATH",
            "SCHEDULE_TIMES",
            "PORT",
        ];
        for var in vars {
            env::remove_var(var);
        }
    }

    /// Helper to set required environment variables for successful config creation
    fn set_required_env_vars() {
        env::set_var("OPENAI_API_KEY", "test-openai-key");
        env::set_var("TELEGRAM_BOT_TOKEN", "test-telegram-token");
        env::set_var("NITTER_INSTANCE", "https://nitter.example.com");
    }

    // ==================== Required Variables Tests ====================

    #[test]
    fn test_config_with_all_required_vars() {
        let _lock = ENV_MUTEX.lock().unwrap();
        clear_env_vars();
        set_required_env_vars();

        let config = Config::from_env();
        assert!(config.is_ok(), "Config should succeed with required vars");

        let config = config.unwrap();
        assert_eq!(config.openai_api_key, "test-openai-key");
        assert_eq!(config.telegram_bot_token, "test-telegram-token");
        assert_eq!(config.nitter_instance, "https://nitter.example.com");
    }

    #[test]
    fn test_config_missing_openai_api_key() {
        let _lock = ENV_MUTEX.lock().unwrap();
        clear_env_vars();
        env::set_var("TELEGRAM_BOT_TOKEN", "test-telegram-token");
        env::set_var("NITTER_INSTANCE", "https://nitter.example.com");

        let result = Config::from_env();
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(
            err.contains("OPENAI_API_KEY"),
            "Error should mention OPENAI_API_KEY: {}",
            err
        );
    }

    #[test]
    fn test_config_missing_telegram_bot_token() {
        let _lock = ENV_MUTEX.lock().unwrap();
        clear_env_vars();
        env::set_var("OPENAI_API_KEY", "test-openai-key");
        env::set_var("NITTER_INSTANCE", "https://nitter.example.com");

        let result = Config::from_env();
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(
            err.contains("TELEGRAM_BOT_TOKEN"),
            "Error should mention TELEGRAM_BOT_TOKEN: {}",
            err
        );
    }

    #[test]
    fn test_config_missing_nitter_instance() {
        let _lock = ENV_MUTEX.lock().unwrap();
        clear_env_vars();
        env::set_var("OPENAI_API_KEY", "test-openai-key");
        env::set_var("TELEGRAM_BOT_TOKEN", "test-telegram-token");

        let result = Config::from_env();
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(
            err.contains("NITTER_INSTANCE"),
            "Error should mention NITTER_INSTANCE: {}",
            err
        );
    }

    // ==================== Default Values Tests ====================

    #[test]
    fn test_config_default_values() {
        let _lock = ENV_MUTEX.lock().unwrap();
        clear_env_vars();
        set_required_env_vars();

        let config = Config::from_env().unwrap();

        // Verify default values
        assert_eq!(config.openai_model, "gpt-4o-mini");
        assert_eq!(config.max_tweets, 50);
        assert_eq!(config.hours_lookback, 12);
        assert_eq!(config.usernames_file, "data/usernames.txt");
        assert_eq!(config.database_path, "/data/subscribers.db");
        assert_eq!(config.schedule_times, vec!["08:00", "20:00"]);
        assert_eq!(config.port, 8080);
        assert_eq!(config.telegram_chat_id, "");
    }

    #[test]
    fn test_config_custom_openai_model() {
        let _lock = ENV_MUTEX.lock().unwrap();
        clear_env_vars();
        set_required_env_vars();
        env::set_var("OPENAI_MODEL", "gpt-4-turbo");

        let config = Config::from_env().unwrap();
        assert_eq!(config.openai_model, "gpt-4-turbo");
    }

    #[test]
    fn test_config_custom_max_tweets() {
        let _lock = ENV_MUTEX.lock().unwrap();
        clear_env_vars();
        set_required_env_vars();
        env::set_var("MAX_TWEETS", "100");

        let config = Config::from_env().unwrap();
        assert_eq!(config.max_tweets, 100);
    }

    #[test]
    fn test_config_custom_hours_lookback() {
        let _lock = ENV_MUTEX.lock().unwrap();
        clear_env_vars();
        set_required_env_vars();
        env::set_var("HOURS_LOOKBACK", "24");

        let config = Config::from_env().unwrap();
        assert_eq!(config.hours_lookback, 24);
    }

    #[test]
    fn test_config_custom_port() {
        let _lock = ENV_MUTEX.lock().unwrap();
        clear_env_vars();
        set_required_env_vars();
        env::set_var("PORT", "3000");

        let config = Config::from_env().unwrap();
        assert_eq!(config.port, 3000);
    }

    // ==================== Optional Variables Tests ====================

    #[test]
    fn test_config_optional_twitter_vars_present() {
        let _lock = ENV_MUTEX.lock().unwrap();
        clear_env_vars();
        set_required_env_vars();
        env::set_var("TWITTER_BEARER_TOKEN", "test-bearer");
        env::set_var("TWITTER_LIST_ID", "123456789");

        let config = Config::from_env().unwrap();
        assert_eq!(config.twitter_bearer_token, Some("test-bearer".to_string()));
        assert_eq!(config.twitter_list_id, Some("123456789".to_string()));
    }

    #[test]
    fn test_config_optional_twitter_vars_absent() {
        let _lock = ENV_MUTEX.lock().unwrap();
        clear_env_vars();
        set_required_env_vars();

        let config = Config::from_env().unwrap();
        assert!(config.twitter_bearer_token.is_none());
        assert!(config.twitter_list_id.is_none());
    }

    #[test]
    fn test_config_optional_nitter_api_key_present() {
        let _lock = ENV_MUTEX.lock().unwrap();
        clear_env_vars();
        set_required_env_vars();
        env::set_var("NITTER_API_KEY", "secret-nitter-key");

        let config = Config::from_env().unwrap();
        assert_eq!(config.nitter_api_key, Some("secret-nitter-key".to_string()));
    }

    #[test]
    fn test_config_optional_nitter_api_key_absent() {
        let _lock = ENV_MUTEX.lock().unwrap();
        clear_env_vars();
        set_required_env_vars();

        let config = Config::from_env().unwrap();
        assert!(config.nitter_api_key.is_none());
    }

    #[test]
    fn test_config_optional_api_key_present() {
        let _lock = ENV_MUTEX.lock().unwrap();
        clear_env_vars();
        set_required_env_vars();
        env::set_var("API_KEY", "service-api-key");

        let config = Config::from_env().unwrap();
        assert_eq!(config.api_key, Some("service-api-key".to_string()));
    }

    // ==================== Edge Cases and Invalid Input Tests ====================

    #[test]
    fn test_config_invalid_max_tweets_uses_default() {
        let _lock = ENV_MUTEX.lock().unwrap();
        clear_env_vars();
        set_required_env_vars();
        env::set_var("MAX_TWEETS", "not_a_number");

        let config = Config::from_env().unwrap();
        assert_eq!(config.max_tweets, 50, "Should use default for invalid MAX_TWEETS");
    }

    #[test]
    fn test_config_invalid_hours_lookback_uses_default() {
        let _lock = ENV_MUTEX.lock().unwrap();
        clear_env_vars();
        set_required_env_vars();
        env::set_var("HOURS_LOOKBACK", "abc");

        let config = Config::from_env().unwrap();
        assert_eq!(
            config.hours_lookback, 12,
            "Should use default for invalid HOURS_LOOKBACK"
        );
    }

    #[test]
    fn test_config_invalid_port_uses_default() {
        let _lock = ENV_MUTEX.lock().unwrap();
        clear_env_vars();
        set_required_env_vars();
        env::set_var("PORT", "invalid_port");

        let config = Config::from_env().unwrap();
        assert_eq!(config.port, 8080, "Should use default for invalid PORT");
    }

    #[test]
    fn test_config_negative_max_tweets_uses_default() {
        let _lock = ENV_MUTEX.lock().unwrap();
        clear_env_vars();
        set_required_env_vars();
        // u32 cannot be negative, so parsing "-1" will fail and use default
        env::set_var("MAX_TWEETS", "-1");

        let config = Config::from_env().unwrap();
        assert_eq!(config.max_tweets, 50, "Should use default for negative value");
    }

    #[test]
    fn test_config_empty_string_max_tweets_uses_default() {
        let _lock = ENV_MUTEX.lock().unwrap();
        clear_env_vars();
        set_required_env_vars();
        env::set_var("MAX_TWEETS", "");

        let config = Config::from_env().unwrap();
        assert_eq!(config.max_tweets, 50, "Should use default for empty string");
    }

    #[test]
    fn test_config_zero_max_tweets() {
        let _lock = ENV_MUTEX.lock().unwrap();
        clear_env_vars();
        set_required_env_vars();
        env::set_var("MAX_TWEETS", "0");

        let config = Config::from_env().unwrap();
        assert_eq!(config.max_tweets, 0, "Zero should be accepted");
    }

    #[test]
    fn test_config_large_max_tweets() {
        let _lock = ENV_MUTEX.lock().unwrap();
        clear_env_vars();
        set_required_env_vars();
        env::set_var("MAX_TWEETS", "4294967295"); // u32::MAX

        let config = Config::from_env().unwrap();
        assert_eq!(config.max_tweets, u32::MAX);
    }

    // ==================== Schedule Times Tests ====================

    #[test]
    fn test_config_custom_schedule_times() {
        let _lock = ENV_MUTEX.lock().unwrap();
        clear_env_vars();
        set_required_env_vars();
        env::set_var("SCHEDULE_TIMES", "06:00,12:00,18:00,22:00");

        let config = Config::from_env().unwrap();
        assert_eq!(
            config.schedule_times,
            vec!["06:00", "12:00", "18:00", "22:00"]
        );
    }

    #[test]
    fn test_config_single_schedule_time() {
        let _lock = ENV_MUTEX.lock().unwrap();
        clear_env_vars();
        set_required_env_vars();
        env::set_var("SCHEDULE_TIMES", "09:00");

        let config = Config::from_env().unwrap();
        assert_eq!(config.schedule_times, vec!["09:00"]);
    }

    #[test]
    fn test_config_schedule_times_with_spaces() {
        let _lock = ENV_MUTEX.lock().unwrap();
        clear_env_vars();
        set_required_env_vars();
        env::set_var("SCHEDULE_TIMES", " 08:00 , 20:00 ");

        let config = Config::from_env().unwrap();
        assert_eq!(config.schedule_times, vec!["08:00", "20:00"]);
    }

    // ==================== Config Clone and Debug Tests ====================

    #[test]
    fn test_config_is_clone() {
        let _lock = ENV_MUTEX.lock().unwrap();
        clear_env_vars();
        set_required_env_vars();

        let config = Config::from_env().unwrap();
        let cloned = config.clone();

        assert_eq!(config.openai_api_key, cloned.openai_api_key);
        assert_eq!(config.telegram_bot_token, cloned.telegram_bot_token);
        assert_eq!(config.nitter_instance, cloned.nitter_instance);
        assert_eq!(config.max_tweets, cloned.max_tweets);
    }

    #[test]
    fn test_config_is_debug() {
        let _lock = ENV_MUTEX.lock().unwrap();
        clear_env_vars();
        set_required_env_vars();

        let config = Config::from_env().unwrap();
        let debug_str = format!("{:?}", config);

        // Verify debug output contains expected fields
        assert!(debug_str.contains("Config"));
        assert!(debug_str.contains("openai_api_key"));
        assert!(debug_str.contains("telegram_bot_token"));
    }

    // ==================== Usernames File Path Tests ====================

    #[test]
    fn test_config_custom_usernames_file() {
        let _lock = ENV_MUTEX.lock().unwrap();
        clear_env_vars();
        set_required_env_vars();
        env::set_var("USERNAMES_FILE", "/custom/path/users.txt");

        let config = Config::from_env().unwrap();
        assert_eq!(config.usernames_file, "/custom/path/users.txt");
    }

    #[test]
    fn test_config_custom_database_path() {
        let _lock = ENV_MUTEX.lock().unwrap();
        clear_env_vars();
        set_required_env_vars();
        env::set_var("DATABASE_PATH", "/custom/db/subscribers.db");

        let config = Config::from_env().unwrap();
        assert_eq!(config.database_path, "/custom/db/subscribers.db");
    }

    // ==================== Telegram Chat ID Tests ====================

    #[test]
    fn test_config_telegram_chat_id_present() {
        let _lock = ENV_MUTEX.lock().unwrap();
        clear_env_vars();
        set_required_env_vars();
        env::set_var("TELEGRAM_CHAT_ID", "123456789");

        let config = Config::from_env().unwrap();
        assert_eq!(config.telegram_chat_id, "123456789");
    }

    #[test]
    fn test_config_telegram_chat_id_negative_for_group() {
        let _lock = ENV_MUTEX.lock().unwrap();
        clear_env_vars();
        set_required_env_vars();
        env::set_var("TELEGRAM_CHAT_ID", "-1001234567890");

        let config = Config::from_env().unwrap();
        assert_eq!(config.telegram_chat_id, "-1001234567890");
    }
}
