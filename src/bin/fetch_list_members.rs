use anyhow::{Context, Result};
use std::fs;
use tracing::info;
use twitter_news_summary::{config, twitter};

#[tokio::main]
async fn main() -> Result<()> {
    // Load .env file
    let _ = dotenvy::dotenv();

    // Initialize logging
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive("fetch_list_members=info".parse()?)
        )
        .init();

    info!("Starting Twitter list members export");

    // Load config from environment
    let config = config::Config::from_env()?;

    // Ensure Twitter credentials are set (required for export)
    if config.twitter_bearer_token.is_none() || config.twitter_list_id.is_none() {
        anyhow::bail!(
            "Twitter credentials required for export.\n\
            Set TWITTER_BEARER_TOKEN and TWITTER_LIST_ID in .env file."
        );
    }

    // Fetch list members using shared function
    let usernames = twitter::fetch_list_members(&config).await?;

    // Save to file
    let output_path = "data/usernames.txt";
    fs::write(output_path, usernames.join("\n"))
        .context("Failed to write usernames to file")?;

    info!("✓ Exported {} usernames to {}", usernames.len(), output_path);
    info!("The main app will now read usernames from this file.");

    Ok(())
}
