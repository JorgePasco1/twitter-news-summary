use anyhow::{Context, Result};
use chrono::{DateTime, Duration, Utc};
use futures::future::join_all;
use tracing::{info, warn};
use crate::config::Config;
use crate::twitter::Tweet;

/// Fetch tweets from Nitter RSS feeds
pub async fn fetch_tweets_from_rss(config: &Config, usernames: &[String]) -> Result<Vec<Tweet>> {
    info!("Fetching RSS feeds for {} users from {}", usernames.len(), config.nitter_instance);

    // Fetch RSS feeds for all users in parallel
    let fetch_tasks: Vec<_> = usernames
        .iter()
        .map(|username| fetch_user_rss(config, username))
        .collect();

    let results = join_all(fetch_tasks).await;

    // Collect all tweets from successful fetches
    let mut all_tweets = Vec::new();
    let mut success_count = 0;
    let mut fail_count = 0;

    for (username, result) in usernames.iter().zip(results.iter()) {
        match result {
            Ok(tweets) => {
                success_count += 1;
                all_tweets.extend(tweets.clone());
            }
            Err(e) => {
                fail_count += 1;
                warn!("Failed to fetch RSS for @{}: {}", username, e);
            }
        }
    }

    info!(
        "RSS fetch complete: {} successful, {} failed",
        success_count, fail_count
    );

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

/// Fetch RSS feed for a single user
async fn fetch_user_rss(config: &Config, username: &str) -> Result<Vec<Tweet>> {
    let url = format!("{}/{}/rss", config.nitter_instance, username);

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()?;

    let response = client
        .get(&url)
        .send()
        .await
        .context(format!("Failed to fetch RSS for @{}", username))?;

    if !response.status().is_success() {
        anyhow::bail!(
            "RSS fetch failed for @{}: HTTP {}",
            username,
            response.status()
        );
    }

    let body = response
        .bytes()
        .await
        .context("Failed to read RSS response")?;

    let channel = rss::Channel::read_from(&body[..])
        .context(format!("Failed to parse RSS for @{}", username))?;

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
