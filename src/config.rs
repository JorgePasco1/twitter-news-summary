use anyhow::{Context, Result};

#[derive(Debug, Clone)]
pub struct Config {
    // Environment name (for logging/debugging)
    pub environment: String,

    // Twitter (optional - only used by export binary)
    #[allow(dead_code)]
    pub twitter_bearer_token: Option<String>,
    #[allow(dead_code)]
    pub twitter_list_id: Option<String>,

    // OpenAI
    pub openai_api_key: String,
    pub openai_model: String,
    pub openai_api_url: String,
    pub openai_temperature: f32,

    // Telegram
    pub telegram_bot_token: String,
    pub telegram_chat_id: String, // Admin chat ID for notifications
    pub telegram_webhook_secret: String, // REQUIRED: Webhook secret for security

    // Filtering
    pub max_tweets: u32,
    pub hours_lookback: u32,

    // Summary generation
    pub summary_max_tokens: u32,
    pub summary_max_words: u32,

    // RSS/Nitter
    pub nitter_instance: String,
    pub nitter_api_key: Option<String>,
    pub usernames_file: String,

    // Service (for web server mode)
    pub api_key: Option<String>,
    pub database_url: String,
    pub schedule_times: Vec<String>,
    pub port: u16,
}

impl Config {
    pub fn from_env() -> Result<Self> {
        // Parse schedule times
        let schedule_times_str =
            std::env::var("SCHEDULE_TIMES").unwrap_or_else(|_| "08:00,20:00".to_string());
        let schedule_times: Vec<String> = schedule_times_str
            .split(',')
            .map(|s| s.trim().to_string())
            .collect();

        Ok(Self {
            // Environment name
            environment: std::env::var("ENVIRONMENT")
                .unwrap_or_else(|_| "development".to_string()),

            // Twitter - Bearer Token (OAuth 2.0 App-Only) - Optional, only for export binary
            twitter_bearer_token: std::env::var("TWITTER_BEARER_TOKEN").ok(),
            twitter_list_id: std::env::var("TWITTER_LIST_ID").ok(),

            // OpenAI
            openai_api_key: std::env::var("OPENAI_API_KEY")
                .context("OPENAI_API_KEY not set")?,
            openai_model: std::env::var("OPENAI_MODEL")
                .unwrap_or_else(|_| "gpt-5-mini".to_string()),
            openai_api_url: std::env::var("OPENAI_API_URL")
                .unwrap_or_else(|_| "https://api.openai.com/v1/chat/completions".to_string()),
            // Temperature must be finite and within OpenAI's accepted range (0.0-2.0)
            openai_temperature: std::env::var("OPENAI_TEMPERATURE")
                .ok()
                .and_then(|v| v.parse::<f32>().ok())
                .filter(|v| v.is_finite())
                .filter(|v| (0.0..=2.0).contains(v))
                .unwrap_or(0.7),

            // Telegram
            telegram_bot_token: std::env::var("TELEGRAM_BOT_TOKEN")
                .context("TELEGRAM_BOT_TOKEN not set")?,
            telegram_chat_id: std::env::var("TELEGRAM_CHAT_ID")
                .unwrap_or_else(|_| "".to_string()),  // Optional in service mode
            telegram_webhook_secret: std::env::var("TELEGRAM_WEBHOOK_SECRET")
                .context("TELEGRAM_WEBHOOK_SECRET not set - REQUIRED for webhook security. Generate with: openssl rand -hex 32")?,

            // Filtering
            max_tweets: std::env::var("MAX_TWEETS")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(100),
            hours_lookback: std::env::var("HOURS_LOOKBACK")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(12),

            // Summary generation (16000 default optimized for gpt-5-mini with 128k output limit)
            // NOTE: If using a different model, you MUST set SUMMARY_MAX_TOKENS appropriately:
            //   - gpt-5-mini: up to 128,000 (default 16000 is safe)
            //   - gpt-4o-mini: up to 16,384 (set SUMMARY_MAX_TOKENS=4000 or lower)
            //   - gpt-4-turbo: up to 4,096 (set SUMMARY_MAX_TOKENS=2500 or lower)
            summary_max_tokens: std::env::var("SUMMARY_MAX_TOKENS")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(16000),
            summary_max_words: std::env::var("SUMMARY_MAX_WORDS")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(800),

            // RSS/Nitter
            nitter_instance: std::env::var("NITTER_INSTANCE")
                .context("NITTER_INSTANCE not set - you must provide your own Nitter instance URL")?,
            nitter_api_key: std::env::var("NITTER_API_KEY").ok(),
            usernames_file: std::env::var("USERNAMES_FILE")
                .unwrap_or_else(|_| "data/usernames.txt".to_string()),

            // Service
            api_key: std::env::var("API_KEY").ok(),
            database_url: std::env::var("DATABASE_URL")
                .context("DATABASE_URL not set - required for PostgreSQL connection")?,
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
            "ENVIRONMENT",
            "TWITTER_BEARER_TOKEN",
            "TWITTER_LIST_ID",
            "OPENAI_API_KEY",
            "OPENAI_MODEL",
            "OPENAI_API_URL",
            "OPENAI_TEMPERATURE",
            "TELEGRAM_BOT_TOKEN",
            "TELEGRAM_CHAT_ID",
            "TELEGRAM_WEBHOOK_SECRET",
            "MAX_TWEETS",
            "HOURS_LOOKBACK",
            "SUMMARY_MAX_TOKENS",
            "SUMMARY_MAX_WORDS",
            "NITTER_INSTANCE",
            "NITTER_API_KEY",
            "USERNAMES_FILE",
            "API_KEY",
            "DATABASE_URL",
            "SCHEDULE_TIMES",
            "PORT",
            "TELEGRAM_WEBHOOK_SECRET",
        ];
        for var in vars {
            env::remove_var(var);
        }
    }

    /// Helper to set required environment variables for successful config creation
    fn set_required_env_vars() {
        env::set_var("OPENAI_API_KEY", "test-openai-key");
        env::set_var("TELEGRAM_BOT_TOKEN", "test-telegram-token");
        env::set_var("TELEGRAM_WEBHOOK_SECRET", "test-webhook-secret");
        env::set_var("NITTER_INSTANCE", "https://nitter.example.com");
        env::set_var("DATABASE_URL", "postgres://test:test@localhost/test");
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
    fn test_config_fails_without_required_vars() {
        let _lock = ENV_MUTEX.lock().unwrap();
        clear_env_vars();
        // Don't set any required vars
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
        // Note: Don't set TELEGRAM_BOT_TOKEN - that's what we're testing
        // TELEGRAM_WEBHOOK_SECRET and NITTER_INSTANCE come after, so they're not needed

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
    fn test_config_missing_telegram_webhook_secret() {
        let _lock = ENV_MUTEX.lock().unwrap();
        clear_env_vars();
        env::set_var("OPENAI_API_KEY", "test-openai-key");
        env::set_var("TELEGRAM_BOT_TOKEN", "test-telegram-token");
        // Note: Don't set TELEGRAM_WEBHOOK_SECRET - that's what we're testing

        let result = Config::from_env();
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(
            err.contains("TELEGRAM_WEBHOOK_SECRET"),
            "Error should mention TELEGRAM_WEBHOOK_SECRET: {}",
            err
        );
    }

    #[test]
    fn test_config_missing_database_url() {
        let _lock = ENV_MUTEX.lock().unwrap();
        clear_env_vars();
        env::set_var("OPENAI_API_KEY", "test-openai-key");
        env::set_var("TELEGRAM_BOT_TOKEN", "test-telegram-token");
        env::set_var("TELEGRAM_WEBHOOK_SECRET", "test-webhook-secret");
        env::set_var("NITTER_INSTANCE", "https://nitter.example.com");
        // Note: Don't set DATABASE_URL - that's what we're testing

        let result = Config::from_env();
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(
            err.contains("DATABASE_URL"),
            "Error should mention DATABASE_URL: {}",
            err
        );
    }

    #[test]
    fn test_config_missing_nitter_instance() {
        let _lock = ENV_MUTEX.lock().unwrap();
        clear_env_vars();
        env::set_var("OPENAI_API_KEY", "test-openai-key");
        env::set_var("TELEGRAM_BOT_TOKEN", "test-telegram-token");
        env::set_var("TELEGRAM_WEBHOOK_SECRET", "test-webhook-secret");
        env::set_var("DATABASE_URL", "postgres://test:test@localhost/test");

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
        assert_eq!(config.openai_model, "gpt-5-mini");
        assert_eq!(
            config.openai_api_url,
            "https://api.openai.com/v1/chat/completions"
        );
        assert!((config.openai_temperature - 0.7).abs() < f32::EPSILON);
        assert_eq!(config.max_tweets, 100);
        assert_eq!(config.hours_lookback, 12);
        assert_eq!(config.summary_max_tokens, 16000);
        assert_eq!(config.summary_max_words, 800);
        assert_eq!(config.usernames_file, "data/usernames.txt");
        assert_eq!(config.database_url, "postgres://test:test@localhost/test");
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
    fn test_config_custom_openai_temperature() {
        let _lock = ENV_MUTEX.lock().unwrap();
        clear_env_vars();
        set_required_env_vars();
        env::set_var("OPENAI_TEMPERATURE", "0.3");

        let config = Config::from_env().unwrap();
        assert!((config.openai_temperature - 0.3).abs() < f32::EPSILON);
    }

    #[test]
    fn test_config_invalid_openai_temperature_uses_default() {
        let _lock = ENV_MUTEX.lock().unwrap();
        clear_env_vars();
        set_required_env_vars();
        env::set_var("OPENAI_TEMPERATURE", "not_a_number");

        let config = Config::from_env().unwrap();
        assert!(
            (config.openai_temperature - 0.7).abs() < f32::EPSILON,
            "Should use default for invalid OPENAI_TEMPERATURE"
        );
    }

    #[test]
    fn test_config_openai_temperature_nan_uses_default() {
        let _lock = ENV_MUTEX.lock().unwrap();
        clear_env_vars();
        set_required_env_vars();
        env::set_var("OPENAI_TEMPERATURE", "NaN");

        let config = Config::from_env().unwrap();
        assert!(
            (config.openai_temperature - 0.7).abs() < f32::EPSILON,
            "Should use default for NaN OPENAI_TEMPERATURE"
        );
    }

    #[test]
    fn test_config_openai_temperature_infinity_uses_default() {
        let _lock = ENV_MUTEX.lock().unwrap();
        clear_env_vars();
        set_required_env_vars();
        env::set_var("OPENAI_TEMPERATURE", "inf");

        let config = Config::from_env().unwrap();
        assert!(
            (config.openai_temperature - 0.7).abs() < f32::EPSILON,
            "Should use default for infinite OPENAI_TEMPERATURE"
        );
    }

    #[test]
    fn test_config_openai_temperature_out_of_range_high_uses_default() {
        let _lock = ENV_MUTEX.lock().unwrap();
        clear_env_vars();
        set_required_env_vars();
        env::set_var("OPENAI_TEMPERATURE", "2.5"); // Above max of 2.0

        let config = Config::from_env().unwrap();
        assert!(
            (config.openai_temperature - 0.7).abs() < f32::EPSILON,
            "Should use default for out-of-range OPENAI_TEMPERATURE > 2.0"
        );
    }

    #[test]
    fn test_config_openai_temperature_negative_uses_default() {
        let _lock = ENV_MUTEX.lock().unwrap();
        clear_env_vars();
        set_required_env_vars();
        env::set_var("OPENAI_TEMPERATURE", "-0.5"); // Below min of 0.0

        let config = Config::from_env().unwrap();
        assert!(
            (config.openai_temperature - 0.7).abs() < f32::EPSILON,
            "Should use default for negative OPENAI_TEMPERATURE"
        );
    }

    #[test]
    fn test_config_openai_temperature_boundary_zero() {
        let _lock = ENV_MUTEX.lock().unwrap();
        clear_env_vars();
        set_required_env_vars();
        env::set_var("OPENAI_TEMPERATURE", "0.0");

        let config = Config::from_env().unwrap();
        assert!(
            config.openai_temperature.abs() < f32::EPSILON,
            "Should accept 0.0 as valid temperature"
        );
    }

    #[test]
    fn test_config_openai_temperature_boundary_two() {
        let _lock = ENV_MUTEX.lock().unwrap();
        clear_env_vars();
        set_required_env_vars();
        env::set_var("OPENAI_TEMPERATURE", "2.0");

        let config = Config::from_env().unwrap();
        assert!(
            (config.openai_temperature - 2.0).abs() < f32::EPSILON,
            "Should accept 2.0 as valid temperature"
        );
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
        assert_eq!(
            config.max_tweets, 100,
            "Should use default for invalid MAX_TWEETS"
        );
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
        assert_eq!(
            config.max_tweets, 100,
            "Should use default for negative value"
        );
    }

    #[test]
    fn test_config_empty_string_max_tweets_uses_default() {
        let _lock = ENV_MUTEX.lock().unwrap();
        clear_env_vars();
        set_required_env_vars();
        env::set_var("MAX_TWEETS", "");

        let config = Config::from_env().unwrap();
        assert_eq!(
            config.max_tweets, 100,
            "Should use default for empty string"
        );
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
    fn test_config_custom_database_url() {
        let _lock = ENV_MUTEX.lock().unwrap();
        clear_env_vars();
        set_required_env_vars();
        env::set_var("DATABASE_URL", "postgres://custom:password@localhost/mydb");

        let config = Config::from_env().unwrap();
        assert_eq!(
            config.database_url,
            "postgres://custom:password@localhost/mydb"
        );
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
