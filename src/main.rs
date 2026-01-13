mod config;
mod openai;
mod rss;
mod telegram;
mod twitter;

use anyhow::Result;
use tracing::info;

#[tokio::main]
async fn main() -> Result<()> {
    // Load .env file (ignored in production/GitHub Actions)
    let _ = dotenvy::dotenv();

    // Initialize logging
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive("twitter_news_summary=info".parse()?)
        )
        .init();

    info!("Starting Twitter news summary job");

    // Load configuration from environment
    let config = config::Config::from_env()?;

    // Step 1: Fetch tweets from RSS feeds
    info!("Fetching tweets from RSS feeds");
    let tweets = rss::fetch_tweets_from_rss(&config).await?;
    
    if tweets.is_empty() {
        info!("No tweets found in the last period, skipping summary");
        return Ok(());
    }

    info!("Fetched {} tweets", tweets.len());

    // Step 2: Generate summary using OpenAI
    info!("Generating summary with OpenAI");
    let summary = openai::summarize_tweets(&config, &tweets).await?;

    // Step 3: Send summary via Telegram
    info!("Sending summary via Telegram");
    telegram::send_message(&config, &summary).await?;

    info!("Summary sent successfully!");
    Ok(())
}
