use anyhow::{Context, Result};
use chrono::DateTime;
use serde::{Deserialize, Serialize};
use tracing::info;
use crate::config::Config;

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct User {
    id: String,
    name: String,
    username: String,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct ListMembersResponse {
    data: Option<Vec<User>>,
    meta: Option<MembersMeta>,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
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
#[allow(dead_code)]
pub async fn fetch_list_members(config: &Config) -> Result<Vec<String>> {
    let bearer_token = config.twitter_bearer_token.as_ref()
        .context("TWITTER_BEARER_TOKEN not set")?;
    let list_id = config.twitter_list_id.as_ref()
        .context("TWITTER_LIST_ID not set")?;

    let client = reqwest::Client::new();
    let mut all_users = Vec::new();
    let mut next_token: Option<String> = None;

    // Fetch all pages of list members
    loop {
        let url = format!(
            "https://api.twitter.com/2/lists/{}/members",
            list_id
        );

        let mut request = client
            .get(&url)
            .bearer_auth(bearer_token)
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

