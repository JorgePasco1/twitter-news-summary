use anyhow::{Context, Result};
use chrono::{Duration, Utc};
use serde::{Deserialize, Serialize};
use crate::config::Config;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Tweet {
    pub id: String,
    pub text: String,
    pub author_id: Option<String>,
    pub created_at: Option<String>,
}

#[derive(Debug, Deserialize)]
struct TwitterResponse {
    data: Option<Vec<Tweet>>,
    meta: Option<Meta>,
}

#[derive(Debug, Deserialize)]
struct Meta {
    result_count: i32,
}

#[derive(Debug, Deserialize)]
struct UserData {
    id: String,
    name: String,
    username: String,
}

#[derive(Debug, Deserialize)]
struct IncludesResponse {
    users: Option<Vec<UserData>>,
}

#[derive(Debug, Deserialize)]
struct FullTwitterResponse {
    data: Option<Vec<Tweet>>,
    includes: Option<IncludesResponse>,
    meta: Option<Meta>,
}

/// Fetch recent tweets from a Twitter list
pub async fn fetch_list_tweets(config: &Config) -> Result<Vec<Tweet>> {
    let client = reqwest::Client::new();

    // Calculate cutoff time for filtering tweets
    let cutoff_time = Utc::now() - Duration::hours(config.hours_lookback as i64);
    let start_time = cutoff_time.to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
    let max_results = config.max_tweets.to_string();

    // Twitter API v2 endpoint for list tweets
    let url = format!(
        "https://api.twitter.com/2/lists/{}/tweets",
        config.twitter_list_id
    );

    let response = client
        .get(&url)
        .bearer_auth(&config.twitter_bearer_token)
        .query(&[
            ("max_results", max_results.as_str()),
            ("start_time", start_time.as_str()),
            ("tweet.fields", "created_at,author_id,text"),
            ("expansions", "author_id"),
            ("user.fields", "name,username"),
        ])
        .send()
        .await
        .context("Failed to send request to Twitter API")?;

    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        anyhow::bail!("Twitter API error ({}): {}", status, body);
    }

    let twitter_response: FullTwitterResponse = response
        .json()
        .await
        .context("Failed to parse Twitter response")?;

    // Build a map of author_id -> username for enrichment
    let user_map: std::collections::HashMap<String, String> = twitter_response
        .includes
        .and_then(|i| i.users)
        .unwrap_or_default()
        .into_iter()
        .map(|u| (u.id, format!("@{} ({})", u.username, u.name)))
        .collect();

    // Enrich tweets with author info in the text for context
    let tweets: Vec<Tweet> = twitter_response
        .data
        .unwrap_or_default()
        .into_iter()
        .map(|mut tweet| {
            if let Some(author_id) = &tweet.author_id {
                if let Some(author_info) = user_map.get(author_id) {
                    tweet.text = format!("{}: {}", author_info, tweet.text);
                }
            }
            tweet
        })
        .collect();

    Ok(tweets)
}
