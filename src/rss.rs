use anyhow::{Context, Result};
use chrono::{DateTime, Duration, Utc};
use tracing::{info, warn};
use crate::config::Config;
use crate::twitter::Tweet;

/// Fetch tweets from Nitter RSS feeds
pub async fn fetch_tweets_from_rss(config: &Config) -> Result<Vec<Tweet>> {
    // Read usernames from file
    let usernames_content = std::fs::read_to_string(&config.usernames_file)
        .context(format!("Failed to read usernames from {}", config.usernames_file))?;

    let usernames: Vec<String> = usernames_content
        .lines()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect();

    info!("Loaded {} usernames from {}", usernames.len(), config.usernames_file);

    // Verify Nitter instance is working
    info!("Testing Nitter instance: {}", config.nitter_instance);
    if !test_nitter_instance(&config.nitter_instance).await {
        anyhow::bail!(
            "Nitter instance {} is not responding or returning invalid RSS feeds.\n\
            Please check:\n\
            1. Your Nitter instance is running (accessible in browser)\n\
            2. You can access it: {}\n\
            3. RSS feeds work: {}/OpenAI/rss",
            config.nitter_instance,
            config.nitter_instance,
            config.nitter_instance
        );
    }
    info!("✓ Nitter instance is working: {}", config.nitter_instance);
    info!("Fetching RSS feeds for {} users (with 3s delay between requests)", usernames.len());

    // Fetch RSS feeds sequentially with delay to avoid rate limiting
    let mut all_tweets = Vec::new();
    let mut success_count = 0;
    let mut fail_count = 0;

    for (index, username) in usernames.iter().enumerate() {
        info!("Fetching @{} account...", username);
        match fetch_user_rss(&config.nitter_instance, username).await {
            Ok(tweets) => {
                success_count += 1;
                let tweet_count = tweets.len();
                all_tweets.extend(tweets);
                info!("✓ @{} - {} tweets fetched", username, tweet_count);
            }
            Err(e) => {
                fail_count += 1;
                warn!("✗ Failed to fetch RSS for @{}: {}", username, e);
            }
        }

        // Add 3-second delay between requests (except after the last one)
        if index < usernames.len() - 1 {
            tokio::time::sleep(tokio::time::Duration::from_secs(3)).await;
        }
    }

    info!(
        "RSS fetch complete: {} successful, {} failed",
        success_count, fail_count
    );

    if success_count == 0 && fail_count > 0 {
        warn!("All RSS fetches failed! Check your Nitter instance: {}", config.nitter_instance);
        warn!("Verify it's accessible: {}/OpenAI/rss", config.nitter_instance);
        warn!("If using Fly.io, check deployment: flyctl status --app <your-app-name>");
    }

    // Sort by date (newest first)
    all_tweets.sort_by(|a, b| {
        let date_a = a.created_at.as_ref().and_then(|d| DateTime::parse_from_rfc3339(d).ok());
        let date_b = b.created_at.as_ref().and_then(|d| DateTime::parse_from_rfc3339(d).ok());
        date_b.cmp(&date_a)
    });

    // Filter by time window
    let cutoff_time = Utc::now() - Duration::hours(config.hours_lookback as i64);
    let filtered_tweets: Vec<Tweet> = all_tweets
        .into_iter()
        .filter(|tweet| {
            tweet.created_at
                .as_ref()
                .and_then(|ts| DateTime::parse_from_rfc3339(ts).ok())
                .map(|dt| dt.with_timezone(&Utc) > cutoff_time)
                .unwrap_or(true)
        })
        .take(config.max_tweets as usize)
        .collect();

    info!(
        "Filtered to {} tweets from last {} hours",
        filtered_tweets.len(),
        config.hours_lookback
    );

    Ok(filtered_tweets)
}

/// Test if a Nitter instance is working by fetching a sample RSS feed
async fn test_nitter_instance(instance: &str) -> bool {
    let test_url = format!("{}/OpenAI/rss", instance);

    let client = match reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(5))
        .user_agent("Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36")
        .build() {
        Ok(c) => c,
        Err(_) => return false,
    };

    match client.get(&test_url).send().await {
        Ok(response) => {
            if !response.status().is_success() {
                return false;
            }

            match response.bytes().await {
                Ok(body) => {
                    // Check if it's actually RSS/XML, not HTML
                    !body.starts_with(b"<!DOCTYPE") &&
                    !body.starts_with(b"<html") &&
                    (body.starts_with(b"<?xml") || body.starts_with(b"<rss"))
                }
                Err(_) => false,
            }
        }
        Err(_) => false,
    }
}

/// Fetch RSS feed for a single user
async fn fetch_user_rss(instance: &str, username: &str) -> Result<Vec<Tweet>> {
    let url = format!("{}/{}/rss", instance, username);

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .user_agent("Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36")
        .build()?;

    let response = client
        .get(&url)
        .send()
        .await
        .context(format!("Failed to fetch RSS for @{}", username))?;

    let status = response.status();
    if !status.is_success() {
        anyhow::bail!(
            "RSS fetch failed for @{}: HTTP {} (Instance may be down, try alternative)",
            username,
            status
        );
    }

    let body = response
        .bytes()
        .await
        .context("Failed to read RSS response")?;

    // Debug: Check if we got HTML error page instead of RSS
    if body.starts_with(b"<!DOCTYPE") || body.starts_with(b"<html") {
        anyhow::bail!(
            "Nitter instance returned HTML instead of RSS (instance may be broken/down)"
        );
    }

    let channel = rss::Channel::read_from(&body[..])
        .context(format!("Failed to parse RSS XML for @{}", username))?;

    // Convert RSS items to Tweet structs
    let tweets: Vec<Tweet> = channel
        .items()
        .iter()
        .filter_map(|item| rss_item_to_tweet(item, username))
        .collect();

    Ok(tweets)
}

/// Convert RSS item to Tweet struct
fn rss_item_to_tweet(item: &rss::Item, username: &str) -> Option<Tweet> {
    let text = item.title()?.to_string();
    let created_at = item.pub_date().map(|d| parse_rss_date(d));

    // Extract tweet ID from link (https://nitter.net/username/status/123456)
    let id = item
        .link()
        .and_then(|link| link.split('/').last())
        .unwrap_or("unknown")
        .to_string();

    // Format text with username prefix
    let formatted_text = format!("@{}: {}", username, text);

    Some(Tweet {
        id,
        text: formatted_text,
        author_id: Some(username.to_string()),
        created_at,
    })
}

/// Parse RSS date to RFC3339 format
fn parse_rss_date(date_str: &str) -> String {
    // Try to parse common RSS date formats
    if let Ok(dt) = DateTime::parse_from_rfc2822(date_str) {
        return dt.to_rfc3339();
    }

    if let Ok(dt) = DateTime::parse_from_rfc3339(date_str) {
        return dt.to_rfc3339();
    }

    // Fallback: return current time if parsing fails
    Utc::now().to_rfc3339()
}
