mod config;
mod openai;
mod twitter;
mod whatsapp;

use anyhow::Result;
use tracing::{info, error};

#[tokio::main]
async fn main() -> Result<()> {
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

    // Step 1: Fetch tweets from the Twitter list
    info!("Fetching tweets from list: {}", config.twitter_list_id);
    let tweets = twitter::fetch_list_tweets(&config).await?;
    
    if tweets.is_empty() {
        info!("No tweets found in the last period, skipping summary");
        return Ok(());
    }

    info!("Fetched {} tweets", tweets.len());

    // Step 2: Generate summary using OpenAI
    info!("Generating summary with OpenAI");
    let summary = openai::summarize_tweets(&config, &tweets).await?;

    // Step 3: Send summary via WhatsApp (Twilio)
    info!("Sending summary via WhatsApp");
    whatsapp::send_message(&config, &summary).await?;

    info!("Summary sent successfully!");
    Ok(())
}
