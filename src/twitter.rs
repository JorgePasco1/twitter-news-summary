use anyhow::{Context, Result};
use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use tracing::info;
use crate::config::Config;

#[derive(Debug, Deserialize)]
struct User {
    id: String,
    name: String,
    username: String,
}

#[derive(Debug, Deserialize)]
struct ListMembersResponse {
    data: Option<Vec<User>>,
    meta: Option<MembersMeta>,
}

#[derive(Debug, Deserialize)]
struct MembersMeta {
    result_count: i32,
    next_token: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Tweet {
    pub id: String,
    pub text: String,
    pub author_id: Option<String>,
    pub created_at: Option<String>,
}

/// Fetch list members from Twitter API
pub async fn fetch_list_members(config: &Config) -> Result<Vec<String>> {
    let client = reqwest::Client::new();
    let mut all_users = Vec::new();
    let mut next_token: Option<String> = None;

    // Fetch all pages of list members
    loop {
        let url = format!(
            "https://api.twitter.com/2/lists/{}/members",
            config.twitter_list_id
        );

        let mut request = client
            .get(&url)
            .bearer_auth(&config.twitter_bearer_token)
            .query(&[("max_results", "100")]);

        if let Some(token) = &next_token {
            request = request.query(&[("pagination_token", token)]);
        }

        let response = request
            .send()
            .await
            .context("Failed to send request to Twitter API")?;

        // Log rate limit information
        if let Some(remaining) = response.headers().get("x-rate-limit-remaining") {
            let limit = response.headers()
                .get("x-rate-limit-limit")
                .and_then(|v| v.to_str().ok())
                .unwrap_or("?");
            let reset = response.headers()
                .get("x-rate-limit-reset")
                .and_then(|v| v.to_str().ok())
                .and_then(|ts| ts.parse::<i64>().ok())
                .and_then(|ts| DateTime::from_timestamp(ts, 0))
                .map(|dt| dt.format("%H:%M:%S UTC").to_string())
                .unwrap_or_else(|| "?".to_string());

            info!(
                "Twitter API rate limit: {}/{} remaining (resets at {})",
                remaining.to_str().unwrap_or("?"),
                limit,
                reset
            );
        }

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            anyhow::bail!("Twitter API error ({}): {}", status, body);
        }

        let members_response: ListMembersResponse = response
            .json()
            .await
            .context("Failed to parse Twitter response")?;

        if let Some(users) = members_response.data {
            all_users.extend(users);
        }

        // Check if there are more pages
        next_token = members_response.meta.and_then(|m| m.next_token);
        if next_token.is_none() {
            break;
        }
    }

    info!("Fetched {} list members from Twitter", all_users.len());

    // Extract usernames
    let usernames: Vec<String> = all_users
        .iter()
        .map(|u| u.username.clone())
        .collect();

    Ok(usernames)
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
}

/// Fetch recent tweets from a Twitter list
pub async fn fetch_list_tweets(config: &Config) -> Result<Vec<Tweet>> {
    let client = reqwest::Client::new();

    // Twitter API v2 endpoint for list tweets
    let url = format!(
        "https://api.twitter.com/2/lists/{}/tweets",
        config.twitter_list_id
    );

    // Fetch up to 100 tweets (we'll filter by time client-side)
    let response = client
        .get(&url)
        .bearer_auth(&config.twitter_bearer_token)
        .query(&[
            ("max_results", "100"),
            ("tweet.fields", "created_at,author_id,text"),
            ("expansions", "author_id"),
            ("user.fields", "name,username"),
        ])
        .send()
        .await
        .context("Failed to send request to Twitter API")?;

    // Log rate limit information
    if let Some(remaining) = response.headers().get("x-rate-limit-remaining") {
        let limit = response.headers()
            .get("x-rate-limit-limit")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("?");
        let reset = response.headers()
            .get("x-rate-limit-reset")
            .and_then(|v| v.to_str().ok())
            .and_then(|ts| ts.parse::<i64>().ok())
            .and_then(|ts| DateTime::from_timestamp(ts, 0))
            .map(|dt| dt.format("%H:%M:%S UTC").to_string())
            .unwrap_or_else(|| "?".to_string());

        info!(
            "Twitter API rate limit: {}/{} remaining (resets at {})",
            remaining.to_str().unwrap_or("?"),
            limit,
            reset
        );
    }

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

    // Filter tweets by time window (client-side)
    let cutoff_time = Utc::now() - Duration::hours(config.hours_lookback as i64);
    let filtered_tweets: Vec<Tweet> = tweets
        .into_iter()
        .filter(|tweet| {
            tweet.created_at
                .as_ref()
                .and_then(|ts| DateTime::parse_from_rfc3339(ts).ok())
                .map(|dt| dt.with_timezone(&Utc) > cutoff_time)
                .unwrap_or(true)  // Keep tweets without timestamp
        })
        .take(config.max_tweets as usize)  // Limit to max_tweets after filtering
        .collect();

    Ok(filtered_tweets)
}
