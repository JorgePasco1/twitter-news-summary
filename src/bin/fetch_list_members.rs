use anyhow::{Context, Result};
use serde::Deserialize;
use std::fs;
use tracing::info;

#[derive(Debug, Deserialize)]
struct User {
    id: String,
    name: String,
    username: String,
}

#[derive(Debug, Deserialize)]
struct ListMembersResponse {
    data: Option<Vec<User>>,
    meta: Option<Meta>,
}

#[derive(Debug, Deserialize)]
struct Meta {
    result_count: i32,
    next_token: Option<String>,
}

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
    let bearer_token = std::env::var("TWITTER_BEARER_TOKEN")
        .context("TWITTER_BEARER_TOKEN not set")?;
    let list_id = std::env::var("TWITTER_LIST_ID")
        .context("TWITTER_LIST_ID not set")?;

    let client = reqwest::Client::new();
    let mut all_users = Vec::new();
    let mut next_token: Option<String> = None;

    // Fetch all pages of list members
    loop {
        info!("Fetching list members (page {})", all_users.len() / 100 + 1);

        let url = format!("https://api.twitter.com/2/lists/{}/members", list_id);

        let mut request = client
            .get(&url)
            .bearer_auth(&bearer_token)
            .query(&[("max_results", "100")]);

        if let Some(token) = &next_token {
            request = request.query(&[("pagination_token", token)]);
        }

        let response = request
            .send()
            .await
            .context("Failed to send request to Twitter API")?;

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

    info!("Fetched {} total users from list", all_users.len());

    // Extract usernames and save to file
    let usernames: Vec<String> = all_users
        .iter()
        .map(|u| u.username.clone())
        .collect();

    let output_path = "data/usernames.txt";
    fs::write(output_path, usernames.join("\n"))
        .context("Failed to write usernames to file")?;

    info!("✓ Exported {} usernames to {}", usernames.len(), output_path);
    info!("You can now remove TWITTER_BEARER_TOKEN and TWITTER_LIST_ID from .env");

    Ok(())
}
