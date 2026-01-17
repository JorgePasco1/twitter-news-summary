//! Preview summary binary - generates and displays a summary without sending to Telegram
//!
//! Usage: cargo run --bin preview_summary
//!
//! Required environment variables:
//! - OPENAI_API_KEY
//! - NITTER_INSTANCE
//!
//! Optional:
//! - USERNAMES_FILE (defaults to data/usernames.txt)
//! - HOURS_LOOKBACK (defaults to 12)
//! - MAX_TWEETS (defaults to 100)
//! - SUMMARY_MAX_TOKENS (defaults to 2500)
//! - SUMMARY_MAX_WORDS (defaults to 800)

use anyhow::{Context, Result};
use chrono::Utc;
use std::fs;
use std::path::Path;
use tracing::info;
use twitter_news_summary::{openai, rss};

/// Minimal config for preview (no Telegram/DB required)
struct PreviewConfig {
    openai_api_key: String,
    openai_model: String,
    openai_api_url: String,
    nitter_instance: String,
    nitter_api_key: Option<String>,
    usernames_file: String,
    max_tweets: u32,
    hours_lookback: u32,
    summary_max_tokens: u32,
    summary_max_words: u32,
}

impl PreviewConfig {
    fn from_env() -> Result<Self> {
        Ok(Self {
            openai_api_key: std::env::var("OPENAI_API_KEY").context("OPENAI_API_KEY not set")?,
            openai_model: std::env::var("OPENAI_MODEL")
                .unwrap_or_else(|_| "gpt-4o-mini".to_string()),
            openai_api_url: std::env::var("OPENAI_API_URL")
                .unwrap_or_else(|_| "https://api.openai.com/v1/chat/completions".to_string()),
            nitter_instance: std::env::var("NITTER_INSTANCE").context("NITTER_INSTANCE not set")?,
            nitter_api_key: std::env::var("NITTER_API_KEY").ok(),
            usernames_file: std::env::var("USERNAMES_FILE")
                .unwrap_or_else(|_| "data/usernames.txt".to_string()),
            max_tweets: std::env::var("MAX_TWEETS")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(100),
            hours_lookback: std::env::var("HOURS_LOOKBACK")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(12),
            summary_max_tokens: std::env::var("SUMMARY_MAX_TOKENS")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(2500),
            summary_max_words: std::env::var("SUMMARY_MAX_WORDS")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(800),
        })
    }

    /// Convert to the full Config struct (with dummy values for unused fields)
    fn to_full_config(&self) -> twitter_news_summary::config::Config {
        twitter_news_summary::config::Config {
            twitter_bearer_token: None,
            twitter_list_id: None,
            openai_api_key: self.openai_api_key.clone(),
            openai_model: self.openai_model.clone(),
            openai_api_url: self.openai_api_url.clone(),
            telegram_bot_token: "unused".to_string(),
            telegram_chat_id: "unused".to_string(),
            telegram_webhook_secret: "unused".to_string(),
            max_tweets: self.max_tweets,
            hours_lookback: self.hours_lookback,
            summary_max_tokens: self.summary_max_tokens,
            summary_max_words: self.summary_max_words,
            nitter_instance: self.nitter_instance.clone(),
            nitter_api_key: self.nitter_api_key.clone(),
            usernames_file: self.usernames_file.clone(),
            api_key: None,
            database_url: "unused".to_string(),
            schedule_times: vec![],
            port: 8080,
        }
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize logging
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive("twitter_news_summary=info".parse().unwrap()),
        )
        .init();

    // Load environment from .env file
    dotenvy::dotenv().ok();

    info!("Loading configuration...");
    let preview_config = PreviewConfig::from_env()?;
    let config = preview_config.to_full_config();

    // Read usernames
    let usernames_content =
        std::fs::read_to_string(&config.usernames_file).context("Failed to read usernames file")?;
    let usernames: Vec<String> = usernames_content
        .lines()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect();

    info!("Loaded {} usernames", usernames.len());

    // Fetch tweets
    info!(
        "Fetching tweets from RSS feeds (last {} hours)...",
        config.hours_lookback
    );
    let tweets = rss::fetch_tweets_from_rss(&config, &usernames).await?;

    if tweets.is_empty() {
        println!("\n========== NO TWEETS FOUND ==========");
        println!(
            "No tweets found in the last {} hours.",
            config.hours_lookback
        );
        println!("======================================\n");
        return Ok(());
    }

    info!("Found {} tweets, generating summary...", tweets.len());

    // Generate summary
    let client = reqwest::Client::new();
    let summary = openai::summarize_tweets(&client, &config, &tweets).await?;

    // Format the message exactly as Telegram would receive it
    let timestamp = Utc::now().format("%Y-%m-%d %H:%M UTC");
    let formatted_message = format!(
        "<b>Twitter News Summary</b>\n<i>{}</i>\n\n{}",
        timestamp, summary
    );

    // Save to run-history/
    let history_dir = Path::new("run-history");
    fs::create_dir_all(history_dir).context("Failed to create run-history directory")?;

    let filename = format!("{}.md", Utc::now().format("%Y-%m-%d_%H-%M-%S"));
    let filepath = history_dir.join(&filename);

    let file_content = format!(
        "# Summary Preview - {}\n\n\
         **Tweets processed:** {}\n\
         **Time window:** last {} hours\n\n\
         ---\n\n\
         ## Telegram Message (HTML)\n\n\
         ```html\n{}\n```\n\n\
         ---\n\n\
         ## Raw Summary\n\n\
         {}\n",
        timestamp,
        tweets.len(),
        config.hours_lookback,
        formatted_message,
        summary
    );

    fs::write(&filepath, &file_content).context("Failed to write summary to run-history")?;

    // Print the preview
    println!("\n");
    println!("╔══════════════════════════════════════════════════════════════════╗");
    println!("║                    TELEGRAM MESSAGE PREVIEW                       ║");
    println!("╠══════════════════════════════════════════════════════════════════╣");
    println!(
        "║ Tweets processed: {:>4}                                           ║",
        tweets.len()
    );
    println!(
        "║ Time window: last {} hours                                        ║",
        config.hours_lookback
    );
    println!("╚══════════════════════════════════════════════════════════════════╝");
    println!();
    println!("--- HTML Message (as sent to Telegram) ---");
    println!();
    println!("{}", formatted_message);
    println!();
    println!("--- End of Message ---");
    println!();
    println!("Saved to: {}", filepath.display());
    println!();

    Ok(())
}
