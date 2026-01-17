//! Preview summary binary - generates and displays a summary without sending to Telegram
//!
//! Usage:
//!   cargo run --bin preview                  # Fetch tweets and generate summary
//!   cargo run --bin preview -- --use-cached  # Use cached tweets from last run
//!   make preview                              # Same as first command
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
use twitter_news_summary::{openai, rss, twitter::Tweet};

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

/// Escape special characters for Telegram's MarkdownV2 parse mode
fn escape_markdownv2(text: &str) -> String {
    let special_chars = [
        '_', '*', '[', ']', '(', ')', '~', '`', '>', '#', '+', '-', '=', '|', '{', '}', '.', '!',
    ];
    let mut result = String::with_capacity(text.len() * 2);
    for c in text.chars() {
        if special_chars.contains(&c) {
            result.push('\\');
        }
        result.push(c);
    }
    result
}

/// Save tweets to cache file
fn save_tweets_cache(tweets: &[Tweet]) -> Result<()> {
    let cache_dir = Path::new("run-history");
    fs::create_dir_all(cache_dir).context("Failed to create run-history directory")?;

    let cache_path = cache_dir.join("tweets_cache.json");
    let json = serde_json::to_string_pretty(tweets)?;
    fs::write(&cache_path, json).context("Failed to write tweets cache")?;

    info!(
        "Saved {} tweets to cache at {}",
        tweets.len(),
        cache_path.display()
    );
    Ok(())
}

/// Load tweets from cache file
fn load_tweets_cache() -> Result<Vec<Tweet>> {
    let cache_path = Path::new("run-history/tweets_cache.json");

    if !cache_path.exists() {
        anyhow::bail!(
            "No tweets cache found at {}. Run without --use-cached first.",
            cache_path.display()
        );
    }

    let contents = fs::read_to_string(cache_path).context("Failed to read tweets cache")?;
    let tweets: Vec<Tweet> =
        serde_json::from_str(&contents).context("Failed to parse tweets cache")?;

    info!(
        "Loaded {} tweets from cache at {}",
        tweets.len(),
        cache_path.display()
    );
    Ok(tweets)
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

    // Parse CLI arguments
    let args: Vec<String> = std::env::args().collect();
    let use_cached = args
        .iter()
        .any(|arg| arg == "--use-cached" || arg == "--use-cached-tweets");

    info!("Loading configuration...");
    let preview_config = PreviewConfig::from_env()?;
    let config = preview_config.to_full_config();

    // Fetch or load tweets
    let tweets = if use_cached {
        info!("Using cached tweets from previous run...");
        load_tweets_cache()?
    } else {
        // Read usernames
        let usernames_content = std::fs::read_to_string(&config.usernames_file)
            .context("Failed to read usernames file")?;
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
        let fetched_tweets = rss::fetch_tweets_from_rss(&config, &usernames).await?;

        // Save to cache for future use
        if !fetched_tweets.is_empty() {
            save_tweets_cache(&fetched_tweets)?;
        }

        fetched_tweets
    };

    if tweets.is_empty() {
        println!("\n========== NO TWEETS FOUND ==========");
        if use_cached {
            println!("No tweets in cache.");
        } else {
            println!(
                "No tweets found in the last {} hours.",
                config.hours_lookback
            );
        }
        println!("======================================\n");
        return Ok(());
    }

    info!("Found {} tweets, generating summary...", tweets.len());

    // Generate summary
    let client = reqwest::Client::new();
    let summary = openai::summarize_tweets(&client, &config, &tweets).await?;

    // Format the message exactly as Telegram would receive it (MarkdownV2)
    let timestamp = Utc::now().format("%Y-%m-%d %H:%M UTC");
    let formatted_message = format!(
        "📰 *Twitter Summary*\n_{}_\n\n{}",
        timestamp,
        escape_markdownv2(&summary)
    );

    // Save to run-history/
    let history_dir = Path::new("run-history");
    fs::create_dir_all(history_dir).context("Failed to create run-history directory")?;

    let filename = format!("{}.md", Utc::now().format("%Y-%m-%d_%H-%M-%S"));
    let filepath = history_dir.join(&filename);

    let file_content = format!(
        "# Summary Preview - {}\n\n\
         **Tweets processed:** {}\n\
         **Time window:** last {} hours\n\
         **Source:** {}\n\n\
         ---\n\n\
         ## Telegram Message (MarkdownV2)\n\n\
         ```\n{}\n```\n\n\
         ---\n\n\
         ## Raw Summary (from OpenAI)\n\n\
         {}\n\n\
         ---\n\n\
         ## Sample Tweets\n\n\
         {}\n",
        timestamp,
        tweets.len(),
        config.hours_lookback,
        if use_cached {
            "cached tweets"
        } else {
            "fresh fetch"
        },
        formatted_message,
        summary,
        tweets
            .iter()
            .take(5)
            .enumerate()
            .map(|(i, t)| format!(
                "{}. {} ({})",
                i + 1,
                t.text,
                t.author_id.as_deref().unwrap_or("unknown")
            ))
            .collect::<Vec<_>>()
            .join("\n")
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
    println!(
        "║ Source: {:53} ║",
        if use_cached {
            "cached tweets"
        } else {
            "fresh fetch"
        }
    );
    println!("╚══════════════════════════════════════════════════════════════════╝");
    println!();
    println!("--- MarkdownV2 Message (as sent to Telegram) ---");
    println!();
    println!("{}", formatted_message);
    println!();
    println!("--- End of Message ---");
    println!();
    println!("💾 Saved to: {}", filepath.display());
    if !use_cached {
        println!("💾 Tweets cached to: run-history/tweets_cache.json");
        println!("   (Use --use-cached flag to iterate on formatting without re-fetching)");
    }
    println!();

    Ok(())
}
