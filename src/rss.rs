use crate::config::Config;
use crate::retry::{with_retry, RetryConfig};
use crate::twitter::Tweet;
use anyhow::{Context, Result};
use chrono::{DateTime, Duration, Utc};
use tracing::{info, warn};

/// Fetch tweets from Nitter RSS feeds for given usernames
pub async fn fetch_tweets_from_rss(config: &Config, usernames: &[String]) -> Result<Vec<Tweet>> {
    info!("Fetching RSS feeds for {} users", usernames.len());

    // Verify Nitter instance is working (with retries)
    info!("Testing Nitter instance: {}", config.nitter_instance);
    let health_check_result = with_retry(
        &RetryConfig::health_check(),
        "Nitter health check",
        || async {
            if test_nitter_instance(&config.nitter_instance, config.nitter_api_key.as_deref()).await
            {
                Ok(())
            } else {
                Err("Nitter instance not responding or returning invalid RSS")
            }
        },
    )
    .await;

    if health_check_result.is_err() {
        anyhow::bail!(
            "Nitter instance {} is not responding or returning invalid RSS feeds after multiple retries.\n\
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
    info!(
        "Fetching RSS feeds for {} users (with 3s delay between requests)",
        usernames.len()
    );

    // Fetch RSS feeds sequentially with delay to avoid rate limiting
    let mut all_tweets = Vec::new();
    let mut success_count = 0;
    let mut fail_count = 0;

    let total = usernames.len();
    let rss_retry_config = RetryConfig::rss_feed();

    for (index, username) in usernames.iter().enumerate() {
        let progress = index + 1;
        info!("[{}/{}] Fetching @{}...", progress, total, username);

        // Fetch with retries
        let result = with_retry(&rss_retry_config, &format!("RSS @{}", username), || {
            fetch_user_rss(
                &config.nitter_instance,
                username,
                config.nitter_api_key.as_deref(),
            )
        })
        .await;

        match result {
            Ok(tweets) => {
                success_count += 1;
                let tweet_count = tweets.len();
                all_tweets.extend(tweets);
                info!(
                    "[{}/{}] ✓ @{} - {} tweets fetched",
                    progress, total, username, tweet_count
                );
            }
            Err(e) => {
                fail_count += 1;
                warn!("[{}/{}] ✗ @{} - {}", progress, total, username, e);
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
        warn!(
            "All RSS fetches failed! Check your Nitter instance: {}",
            config.nitter_instance
        );
        warn!(
            "Verify it's accessible: {}/OpenAI/rss",
            config.nitter_instance
        );
        warn!("If using Fly.io, check deployment: flyctl status --app <your-app-name>");
    }

    // Sort by date (newest first)
    all_tweets.sort_by(|a, b| {
        let date_a = a
            .created_at
            .as_ref()
            .and_then(|d| DateTime::parse_from_rfc3339(d).ok());
        let date_b = b
            .created_at
            .as_ref()
            .and_then(|d| DateTime::parse_from_rfc3339(d).ok());
        date_b.cmp(&date_a)
    });

    // Filter by time window
    let cutoff_time = Utc::now() - Duration::hours(config.hours_lookback as i64);
    let filtered_tweets: Vec<Tweet> = all_tweets
        .into_iter()
        .filter(|tweet| {
            tweet
                .created_at
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
async fn test_nitter_instance(instance: &str, api_key: Option<&str>) -> bool {
    let test_url = format!("{}/OpenAI/rss", instance);

    let mut client_builder = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(5))
        .user_agent("Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36");

    // Add API key header if provided
    if let Some(key) = api_key {
        let mut headers = reqwest::header::HeaderMap::new();
        if let Ok(header_value) = key.parse() {
            headers.insert("X-API-Key", header_value);
            client_builder = client_builder.default_headers(headers);
        }
    }

    let client = match client_builder.build() {
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
                    !body.starts_with(b"<!DOCTYPE")
                        && !body.starts_with(b"<html")
                        && (body.starts_with(b"<?xml") || body.starts_with(b"<rss"))
                }
                Err(_) => false,
            }
        }
        Err(_) => false,
    }
}

/// Fetch RSS feed for a single user
async fn fetch_user_rss(
    instance: &str,
    username: &str,
    api_key: Option<&str>,
) -> Result<Vec<Tweet>> {
    let url = format!("{}/{}/rss", instance, username);

    let mut client_builder = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .user_agent("Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36");

    // Add API key header if provided
    if let Some(key) = api_key {
        let mut headers = reqwest::header::HeaderMap::new();
        headers.insert("X-API-Key", key.parse().context("Invalid API key format")?);
        client_builder = client_builder.default_headers(headers);
    }

    let client = client_builder.build()?;

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
        anyhow::bail!("Nitter instance returned HTML instead of RSS (instance may be broken/down)");
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
    let created_at = item.pub_date().map(parse_rss_date);

    // Extract tweet ID from link (https://nitter.net/username/status/123456)
    let id = item
        .link()
        .and_then(|link| link.split('/').next_back())
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

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Duration;
    use wiremock::{
        matchers::{header, method, path},
        Mock, MockServer, ResponseTemplate,
    };

    // ==================== Helper Functions ====================

    /// Create a test config with mocked Nitter instance URL
    fn create_test_config(nitter_url: &str) -> Config {
        Config {
            environment: "test".to_string(),
            twitter_bearer_token: None,
            twitter_list_id: None,
            openai_api_key: "test-key".to_string(),
            openai_model: "gpt-4o-mini".to_string(),
            openai_api_url: "https://api.openai.com/v1/chat/completions".to_string(),
            telegram_bot_token: "test-token".to_string(),
            telegram_chat_id: "".to_string(),
            telegram_webhook_secret: "test-webhook-secret".to_string(),
            max_tweets: 100,
            hours_lookback: 12,
            summary_max_tokens: 2500,
            summary_max_words: 800,
            nitter_instance: nitter_url.to_string(),
            nitter_api_key: None,
            usernames_file: "data/usernames.txt".to_string(),
            api_key: None,
            database_url: "postgres://test:test@localhost/test".to_string(),
            schedule_times: vec!["08:00".to_string(), "20:00".to_string()],
            port: 8080,
        }
    }

    /// Create a valid RSS feed XML string with the given items
    fn create_rss_feed(username: &str, items: Vec<(&str, &str, &str)>) -> String {
        let items_xml: String = items
            .iter()
            .map(|(title, link, pub_date)| {
                format!(
                    r#"<item>
                        <title>{}</title>
                        <link>{}</link>
                        <pubDate>{}</pubDate>
                    </item>"#,
                    title, link, pub_date
                )
            })
            .collect::<Vec<_>>()
            .join("\n");

        format!(
            r#"<?xml version="1.0" encoding="UTF-8"?>
            <rss version="2.0">
                <channel>
                    <title>@{} / Twitter</title>
                    <description>Twitter feed for @{}</description>
                    {}
                </channel>
            </rss>"#,
            username, username, items_xml
        )
    }

    /// Generate an RFC2822 date string for a given time offset from now
    fn rfc2822_date_offset(hours_ago: i64) -> String {
        let dt = Utc::now() - Duration::hours(hours_ago);
        dt.format("%a, %d %b %Y %H:%M:%S %z").to_string()
    }

    // ==================== parse_rss_date Tests ====================

    #[test]
    fn test_parse_rss_date_rfc2822_format() {
        let date_str = "Mon, 15 Jan 2024 10:30:00 +0000";
        let result = parse_rss_date(date_str);

        // Should parse to RFC3339 format
        assert!(result.contains("2024-01-15"));
        assert!(result.contains("10:30:00"));
    }

    #[test]
    fn test_parse_rss_date_rfc3339_format() {
        let date_str = "2024-01-15T10:30:00+00:00";
        let result = parse_rss_date(date_str);

        assert_eq!(result, "2024-01-15T10:30:00+00:00");
    }

    #[test]
    fn test_parse_rss_date_invalid_format_returns_current_time() {
        let date_str = "invalid date format";
        let before = Utc::now();
        let result = parse_rss_date(date_str);
        let after = Utc::now();

        // Parse the result back to verify it's a valid RFC3339 timestamp
        let parsed = DateTime::parse_from_rfc3339(&result).expect("Should be valid RFC3339");
        let parsed_utc = parsed.with_timezone(&Utc);

        // Should be within the test execution window
        assert!(parsed_utc >= before - Duration::seconds(1));
        assert!(parsed_utc <= after + Duration::seconds(1));
    }

    #[test]
    fn test_parse_rss_date_empty_string_returns_current_time() {
        let before = Utc::now();
        let result = parse_rss_date("");
        let after = Utc::now();

        let parsed = DateTime::parse_from_rfc3339(&result).expect("Should be valid RFC3339");
        let parsed_utc = parsed.with_timezone(&Utc);

        assert!(parsed_utc >= before - Duration::seconds(1));
        assert!(parsed_utc <= after + Duration::seconds(1));
    }

    #[test]
    fn test_parse_rss_date_various_rfc2822_timezones() {
        // Test with different timezone offsets
        let test_cases = vec![
            ("Mon, 15 Jan 2024 10:30:00 +0000", "2024-01-15"),
            ("Mon, 15 Jan 2024 10:30:00 -0500", "2024-01-15"),
            ("Mon, 15 Jan 2024 10:30:00 +0530", "2024-01-15"),
        ];

        for (input, expected_date) in test_cases {
            let result = parse_rss_date(input);
            assert!(
                result.contains(expected_date),
                "Expected {} in result {}, input: {}",
                expected_date,
                result,
                input
            );
        }
    }

    // ==================== rss_item_to_tweet Tests ====================

    #[test]
    fn test_rss_item_to_tweet_basic() {
        let mut item = rss::Item::default();
        item.set_title("This is a test tweet".to_string());
        item.set_link("https://nitter.example.com/testuser/status/123456".to_string());
        item.set_pub_date("Mon, 15 Jan 2024 10:30:00 +0000".to_string());

        let tweet = rss_item_to_tweet(&item, "testuser");
        assert!(tweet.is_some());

        let tweet = tweet.unwrap();
        assert_eq!(tweet.id, "123456");
        assert_eq!(tweet.text, "@testuser: This is a test tweet");
        assert_eq!(tweet.author_id, Some("testuser".to_string()));
        assert!(tweet.created_at.is_some());
    }

    #[test]
    fn test_rss_item_to_tweet_missing_title_returns_none() {
        let mut item = rss::Item::default();
        item.set_link("https://nitter.example.com/testuser/status/123456".to_string());
        item.set_pub_date("Mon, 15 Jan 2024 10:30:00 +0000".to_string());
        // No title set

        let tweet = rss_item_to_tweet(&item, "testuser");
        assert!(tweet.is_none(), "Should return None when title is missing");
    }

    #[test]
    fn test_rss_item_to_tweet_missing_link_uses_unknown_id() {
        let mut item = rss::Item::default();
        item.set_title("Tweet without link".to_string());
        // No link set

        let tweet = rss_item_to_tweet(&item, "testuser");
        assert!(tweet.is_some());
        assert_eq!(tweet.unwrap().id, "unknown");
    }

    #[test]
    fn test_rss_item_to_tweet_missing_pub_date() {
        let mut item = rss::Item::default();
        item.set_title("Tweet without date".to_string());
        item.set_link("https://nitter.example.com/testuser/status/123456".to_string());
        // No pub_date set

        let tweet = rss_item_to_tweet(&item, "testuser");
        assert!(tweet.is_some());

        let tweet = tweet.unwrap();
        assert!(tweet.created_at.is_none());
    }

    #[test]
    fn test_rss_item_to_tweet_extracts_id_from_various_link_formats() {
        let test_cases = vec![
            ("https://nitter.example.com/user/status/123456", "123456"),
            ("https://nitter.net/someuser/status/789012", "789012"),
            ("http://localhost:8080/test/status/555", "555"),
        ];

        for (link, expected_id) in test_cases {
            let mut item = rss::Item::default();
            item.set_title("Test".to_string());
            item.set_link(link.to_string());

            let tweet = rss_item_to_tweet(&item, "user").unwrap();
            assert_eq!(tweet.id, expected_id, "Failed for link: {}", link);
        }
    }

    #[test]
    fn test_rss_item_to_tweet_formats_text_with_username_prefix() {
        let mut item = rss::Item::default();
        item.set_title("Hello world!".to_string());
        item.set_link("https://nitter.example.com/elonmusk/status/123".to_string());

        let tweet = rss_item_to_tweet(&item, "elonmusk").unwrap();
        assert_eq!(tweet.text, "@elonmusk: Hello world!");
    }

    // ==================== test_nitter_instance Tests ====================

    #[tokio::test]
    async fn test_nitter_instance_valid_rss_response() {
        let mock_server = MockServer::start().await;

        let rss_content = create_rss_feed(
            "OpenAI",
            vec![(
                "Test tweet",
                "https://example.com/OpenAI/status/1",
                "Mon, 15 Jan 2024 10:30:00 +0000",
            )],
        );

        Mock::given(method("GET"))
            .and(path("/OpenAI/rss"))
            .respond_with(ResponseTemplate::new(200).set_body_string(rss_content))
            .mount(&mock_server)
            .await;

        let result = test_nitter_instance(&mock_server.uri(), None).await;
        assert!(result, "Should return true for valid RSS response");
    }

    #[tokio::test]
    async fn test_nitter_instance_html_response_returns_false() {
        let mock_server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/OpenAI/rss"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_string("<!DOCTYPE html><html><body>Error page</body></html>"),
            )
            .mount(&mock_server)
            .await;

        let result = test_nitter_instance(&mock_server.uri(), None).await;
        assert!(!result, "Should return false for HTML response");
    }

    #[tokio::test]
    async fn test_nitter_instance_non_success_status_returns_false() {
        let mock_server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/OpenAI/rss"))
            .respond_with(ResponseTemplate::new(500))
            .mount(&mock_server)
            .await;

        let result = test_nitter_instance(&mock_server.uri(), None).await;
        assert!(!result, "Should return false for 500 status");
    }

    #[tokio::test]
    async fn test_nitter_instance_connection_error_returns_false() {
        // Use an invalid URL that will fail to connect
        let result = test_nitter_instance("http://localhost:1", None).await;
        assert!(!result, "Should return false for connection error");
    }

    #[tokio::test]
    async fn test_nitter_instance_with_api_key() {
        let mock_server = MockServer::start().await;

        let rss_content = create_rss_feed(
            "OpenAI",
            vec![(
                "Test tweet",
                "https://example.com/OpenAI/status/1",
                "Mon, 15 Jan 2024 10:30:00 +0000",
            )],
        );

        Mock::given(method("GET"))
            .and(path("/OpenAI/rss"))
            .and(header("X-API-Key", "secret-key"))
            .respond_with(ResponseTemplate::new(200).set_body_string(rss_content))
            .mount(&mock_server)
            .await;

        let result = test_nitter_instance(&mock_server.uri(), Some("secret-key")).await;
        assert!(result, "Should work with API key header");
    }

    // ==================== fetch_user_rss Tests ====================

    #[tokio::test]
    async fn test_fetch_user_rss_success() {
        let mock_server = MockServer::start().await;

        let now = Utc::now();
        let pub_date = now.format("%a, %d %b %Y %H:%M:%S %z").to_string();

        let rss_content = create_rss_feed(
            "testuser",
            vec![
                (
                    "First tweet",
                    "https://nitter.example.com/testuser/status/1",
                    &pub_date,
                ),
                (
                    "Second tweet",
                    "https://nitter.example.com/testuser/status/2",
                    &pub_date,
                ),
            ],
        );

        Mock::given(method("GET"))
            .and(path("/testuser/rss"))
            .respond_with(ResponseTemplate::new(200).set_body_string(rss_content))
            .mount(&mock_server)
            .await;

        let tweets = fetch_user_rss(&mock_server.uri(), "testuser", None)
            .await
            .expect("Should fetch successfully");

        assert_eq!(tweets.len(), 2);
        assert!(tweets[0].text.contains("@testuser:"));
        assert!(tweets[0].text.contains("First tweet"));
    }

    #[tokio::test]
    async fn test_fetch_user_rss_404_error() {
        let mock_server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/nonexistent/rss"))
            .respond_with(ResponseTemplate::new(404).set_body_string("Not found"))
            .mount(&mock_server)
            .await;

        let result = fetch_user_rss(&mock_server.uri(), "nonexistent", None).await;
        assert!(result.is_err());

        let err = result.unwrap_err().to_string();
        assert!(
            err.contains("404"),
            "Error should mention 404 status: {}",
            err
        );
    }

    #[tokio::test]
    async fn test_fetch_user_rss_html_instead_of_rss() {
        let mock_server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/testuser/rss"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_string("<!DOCTYPE html><html><body>Error</body></html>"),
            )
            .mount(&mock_server)
            .await;

        let result = fetch_user_rss(&mock_server.uri(), "testuser", None).await;
        assert!(result.is_err());

        let err = result.unwrap_err().to_string();
        assert!(
            err.contains("HTML instead of RSS"),
            "Error should mention HTML: {}",
            err
        );
    }

    #[tokio::test]
    async fn test_fetch_user_rss_with_api_key() {
        let mock_server = MockServer::start().await;

        let rss_content = create_rss_feed(
            "testuser",
            vec![(
                "Protected tweet",
                "https://example.com/testuser/status/1",
                "Mon, 15 Jan 2024 10:30:00 +0000",
            )],
        );

        Mock::given(method("GET"))
            .and(path("/testuser/rss"))
            .and(header("X-API-Key", "my-secret-key"))
            .respond_with(ResponseTemplate::new(200).set_body_string(rss_content))
            .mount(&mock_server)
            .await;

        let tweets = fetch_user_rss(&mock_server.uri(), "testuser", Some("my-secret-key"))
            .await
            .expect("Should fetch with API key");

        assert_eq!(tweets.len(), 1);
    }

    #[tokio::test]
    async fn test_fetch_user_rss_invalid_xml() {
        let mock_server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/testuser/rss"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_string("<?xml version=\"1.0\"?><not-valid-rss>"),
            )
            .mount(&mock_server)
            .await;

        let result = fetch_user_rss(&mock_server.uri(), "testuser", None).await;
        assert!(result.is_err(), "Should fail for invalid XML");
    }

    #[tokio::test]
    async fn test_fetch_user_rss_empty_feed() {
        let mock_server = MockServer::start().await;

        let rss_content = create_rss_feed("testuser", vec![]);

        Mock::given(method("GET"))
            .and(path("/testuser/rss"))
            .respond_with(ResponseTemplate::new(200).set_body_string(rss_content))
            .mount(&mock_server)
            .await;

        let tweets = fetch_user_rss(&mock_server.uri(), "testuser", None)
            .await
            .expect("Should handle empty feed");

        assert!(tweets.is_empty());
    }

    // ==================== fetch_tweets_from_rss Tests ====================

    #[tokio::test]
    async fn test_fetch_tweets_from_rss_success() {
        let mock_server = MockServer::start().await;

        // Create RSS feed for the Nitter instance test
        let test_rss = create_rss_feed(
            "OpenAI",
            vec![(
                "Test",
                "https://example.com/OpenAI/status/1",
                "Mon, 15 Jan 2024 10:30:00 +0000",
            )],
        );

        Mock::given(method("GET"))
            .and(path("/OpenAI/rss"))
            .respond_with(ResponseTemplate::new(200).set_body_string(test_rss.clone()))
            .mount(&mock_server)
            .await;

        // Create RSS feed for user1 with recent tweets
        let recent_date = rfc2822_date_offset(1); // 1 hour ago
        let user1_rss = create_rss_feed(
            "user1",
            vec![(
                "Recent tweet from user1",
                "https://example.com/user1/status/100",
                &recent_date,
            )],
        );

        Mock::given(method("GET"))
            .and(path("/user1/rss"))
            .respond_with(ResponseTemplate::new(200).set_body_string(user1_rss))
            .mount(&mock_server)
            .await;

        let config = create_test_config(&mock_server.uri());
        let usernames = vec!["user1".to_string()];

        let tweets = fetch_tweets_from_rss(&config, &usernames)
            .await
            .expect("Should fetch tweets");

        assert!(!tweets.is_empty(), "Should have fetched tweets");
        assert!(tweets[0].text.contains("@user1:"));
    }

    #[tokio::test]
    async fn test_fetch_tweets_from_rss_filters_by_time_window() {
        let mock_server = MockServer::start().await;

        // Nitter instance test feed
        let test_rss = create_rss_feed(
            "OpenAI",
            vec![(
                "Test",
                "https://example.com/OpenAI/status/1",
                "Mon, 15 Jan 2024 10:30:00 +0000",
            )],
        );

        Mock::given(method("GET"))
            .and(path("/OpenAI/rss"))
            .respond_with(ResponseTemplate::new(200).set_body_string(test_rss))
            .mount(&mock_server)
            .await;

        // Create feed with one recent and one old tweet
        let recent_date = rfc2822_date_offset(2); // 2 hours ago (within 12h window)
        let old_date = rfc2822_date_offset(24); // 24 hours ago (outside 12h window)

        let user_rss = create_rss_feed(
            "user1",
            vec![
                (
                    "Recent tweet",
                    "https://example.com/user1/status/1",
                    &recent_date,
                ),
                ("Old tweet", "https://example.com/user1/status/2", &old_date),
            ],
        );

        Mock::given(method("GET"))
            .and(path("/user1/rss"))
            .respond_with(ResponseTemplate::new(200).set_body_string(user_rss))
            .mount(&mock_server)
            .await;

        let config = create_test_config(&mock_server.uri());
        let usernames = vec!["user1".to_string()];

        let tweets = fetch_tweets_from_rss(&config, &usernames)
            .await
            .expect("Should fetch tweets");

        // Only recent tweet should be included
        assert_eq!(tweets.len(), 1);
        assert!(tweets[0].text.contains("Recent tweet"));
    }

    #[tokio::test]
    async fn test_fetch_tweets_from_rss_respects_max_tweets() {
        let mock_server = MockServer::start().await;

        // Nitter instance test feed
        let test_rss = create_rss_feed(
            "OpenAI",
            vec![(
                "Test",
                "https://example.com/OpenAI/status/1",
                "Mon, 15 Jan 2024 10:30:00 +0000",
            )],
        );

        Mock::given(method("GET"))
            .and(path("/OpenAI/rss"))
            .respond_with(ResponseTemplate::new(200).set_body_string(test_rss))
            .mount(&mock_server)
            .await;

        // Create feed with many tweets
        let mut items = Vec::new();
        for i in 1..=10 {
            let date = rfc2822_date_offset(i);
            items.push((
                format!("Tweet {}", i),
                format!("https://example.com/user1/status/{}", i),
                date,
            ));
        }

        let items_refs: Vec<(&str, &str, &str)> = items
            .iter()
            .map(|(a, b, c)| (a.as_str(), b.as_str(), c.as_str()))
            .collect();

        let user_rss = create_rss_feed("user1", items_refs);

        Mock::given(method("GET"))
            .and(path("/user1/rss"))
            .respond_with(ResponseTemplate::new(200).set_body_string(user_rss))
            .mount(&mock_server)
            .await;

        let mut config = create_test_config(&mock_server.uri());
        config.max_tweets = 3; // Limit to 3 tweets

        let usernames = vec!["user1".to_string()];

        let tweets = fetch_tweets_from_rss(&config, &usernames)
            .await
            .expect("Should fetch tweets");

        assert_eq!(tweets.len(), 3, "Should limit to max_tweets");
    }

    #[tokio::test]
    async fn test_fetch_tweets_from_rss_sorts_by_date_newest_first() {
        let mock_server = MockServer::start().await;

        // Nitter instance test feed
        let test_rss = create_rss_feed(
            "OpenAI",
            vec![(
                "Test",
                "https://example.com/OpenAI/status/1",
                "Mon, 15 Jan 2024 10:30:00 +0000",
            )],
        );

        Mock::given(method("GET"))
            .and(path("/OpenAI/rss"))
            .respond_with(ResponseTemplate::new(200).set_body_string(test_rss))
            .mount(&mock_server)
            .await;

        // Create tweets at different times
        let date_3h_ago = rfc2822_date_offset(3);
        let date_1h_ago = rfc2822_date_offset(1);
        let date_5h_ago = rfc2822_date_offset(5);

        let user_rss = create_rss_feed(
            "user1",
            vec![
                (
                    "3 hours ago",
                    "https://example.com/user1/status/3",
                    &date_3h_ago,
                ),
                (
                    "1 hour ago",
                    "https://example.com/user1/status/1",
                    &date_1h_ago,
                ),
                (
                    "5 hours ago",
                    "https://example.com/user1/status/5",
                    &date_5h_ago,
                ),
            ],
        );

        Mock::given(method("GET"))
            .and(path("/user1/rss"))
            .respond_with(ResponseTemplate::new(200).set_body_string(user_rss))
            .mount(&mock_server)
            .await;

        let config = create_test_config(&mock_server.uri());
        let usernames = vec!["user1".to_string()];

        let tweets = fetch_tweets_from_rss(&config, &usernames)
            .await
            .expect("Should fetch tweets");

        // Verify sorted by date (newest first)
        assert!(tweets[0].text.contains("1 hour ago"));
        assert!(tweets[1].text.contains("3 hours ago"));
        assert!(tweets[2].text.contains("5 hours ago"));
    }

    #[tokio::test]
    async fn test_fetch_tweets_from_rss_handles_partial_failures() {
        let mock_server = MockServer::start().await;

        // Nitter instance test feed
        let test_rss = create_rss_feed(
            "OpenAI",
            vec![(
                "Test",
                "https://example.com/OpenAI/status/1",
                "Mon, 15 Jan 2024 10:30:00 +0000",
            )],
        );

        Mock::given(method("GET"))
            .and(path("/OpenAI/rss"))
            .respond_with(ResponseTemplate::new(200).set_body_string(test_rss))
            .mount(&mock_server)
            .await;

        // User1 succeeds
        let recent_date = rfc2822_date_offset(1);
        let user1_rss = create_rss_feed(
            "user1",
            vec![(
                "User1 tweet",
                "https://example.com/user1/status/1",
                &recent_date,
            )],
        );

        Mock::given(method("GET"))
            .and(path("/user1/rss"))
            .respond_with(ResponseTemplate::new(200).set_body_string(user1_rss))
            .mount(&mock_server)
            .await;

        // User2 fails with 404
        Mock::given(method("GET"))
            .and(path("/user2/rss"))
            .respond_with(ResponseTemplate::new(404))
            .mount(&mock_server)
            .await;

        let config = create_test_config(&mock_server.uri());
        let usernames = vec!["user1".to_string(), "user2".to_string()];

        let tweets = fetch_tweets_from_rss(&config, &usernames)
            .await
            .expect("Should handle partial failures");

        // Should still have tweets from user1
        assert_eq!(tweets.len(), 1);
        assert!(tweets[0].text.contains("User1 tweet"));
    }

    #[tokio::test]
    async fn test_fetch_tweets_from_rss_empty_usernames() {
        let mock_server = MockServer::start().await;

        // Nitter instance test feed
        let test_rss = create_rss_feed(
            "OpenAI",
            vec![(
                "Test",
                "https://example.com/OpenAI/status/1",
                "Mon, 15 Jan 2024 10:30:00 +0000",
            )],
        );

        Mock::given(method("GET"))
            .and(path("/OpenAI/rss"))
            .respond_with(ResponseTemplate::new(200).set_body_string(test_rss))
            .mount(&mock_server)
            .await;

        let config = create_test_config(&mock_server.uri());
        let usernames: Vec<String> = vec![];

        let tweets = fetch_tweets_from_rss(&config, &usernames)
            .await
            .expect("Should handle empty usernames");

        assert!(tweets.is_empty());
    }

    #[tokio::test]
    async fn test_fetch_tweets_from_rss_all_fetches_fail() {
        let mock_server = MockServer::start().await;

        // Nitter instance test feed
        let test_rss = create_rss_feed(
            "OpenAI",
            vec![(
                "Test",
                "https://example.com/OpenAI/status/1",
                "Mon, 15 Jan 2024 10:30:00 +0000",
            )],
        );

        Mock::given(method("GET"))
            .and(path("/OpenAI/rss"))
            .respond_with(ResponseTemplate::new(200).set_body_string(test_rss))
            .mount(&mock_server)
            .await;

        // All user feeds return 500
        Mock::given(method("GET"))
            .and(path("/user1/rss"))
            .respond_with(ResponseTemplate::new(500))
            .mount(&mock_server)
            .await;

        Mock::given(method("GET"))
            .and(path("/user2/rss"))
            .respond_with(ResponseTemplate::new(500))
            .mount(&mock_server)
            .await;

        let config = create_test_config(&mock_server.uri());
        let usernames = vec!["user1".to_string(), "user2".to_string()];

        let tweets = fetch_tweets_from_rss(&config, &usernames)
            .await
            .expect("Should not error even when all fetches fail");

        assert!(tweets.is_empty());
    }

    #[tokio::test]
    async fn test_fetch_tweets_from_rss_nitter_instance_down() {
        let mock_server = MockServer::start().await;

        // Nitter instance returns error for test endpoint
        Mock::given(method("GET"))
            .and(path("/OpenAI/rss"))
            .respond_with(ResponseTemplate::new(500))
            .mount(&mock_server)
            .await;

        let config = create_test_config(&mock_server.uri());
        let usernames = vec!["user1".to_string()];

        let result = fetch_tweets_from_rss(&config, &usernames).await;
        assert!(result.is_err(), "Should fail when Nitter instance is down");

        let err = result.unwrap_err().to_string();
        assert!(
            err.contains("not responding"),
            "Error should indicate Nitter is not responding: {}",
            err
        );
    }

    // ==================== Edge Case Tests ====================

    #[test]
    fn test_rss_item_with_special_characters() {
        let mut item = rss::Item::default();
        item.set_title("Tweet with <html> & special \"chars\"".to_string());
        item.set_link("https://example.com/user/status/123".to_string());

        let tweet = rss_item_to_tweet(&item, "user").unwrap();
        assert!(tweet.text.contains("<html>"));
        assert!(tweet.text.contains("&"));
    }

    #[test]
    fn test_rss_item_with_unicode() {
        let mut item = rss::Item::default();
        item.set_title("Tweet with emojis".to_string());
        item.set_link("https://example.com/user/status/123".to_string());

        let tweet = rss_item_to_tweet(&item, "user").unwrap();
        assert!(tweet.text.contains("Tweet with emojis"));
    }

    #[test]
    fn test_rss_item_with_very_long_text() {
        let long_text = "A".repeat(10000);
        let mut item = rss::Item::default();
        item.set_title(long_text.clone());
        item.set_link("https://example.com/user/status/123".to_string());

        let tweet = rss_item_to_tweet(&item, "user").unwrap();
        assert!(tweet.text.len() > 10000, "Should preserve long text");
    }

    // ==================== Retry Integration Tests ====================

    #[tokio::test]
    async fn test_health_check_retries_on_transient_failure() {
        let mock_server = MockServer::start().await;

        // Valid RSS feed for when health check succeeds
        let valid_rss = create_rss_feed(
            "OpenAI",
            vec![(
                "Test tweet",
                "https://example.com/OpenAI/status/1",
                "Mon, 15 Jan 2024 10:30:00 +0000",
            )],
        );

        // Set up mock that fails twice then succeeds
        // Using expect() to limit responses
        Mock::given(method("GET"))
            .and(path("/OpenAI/rss"))
            .respond_with(ResponseTemplate::new(500).set_body_string("Server Error"))
            .up_to_n_times(2) // Fail first 2 times
            .mount(&mock_server)
            .await;

        Mock::given(method("GET"))
            .and(path("/OpenAI/rss"))
            .respond_with(ResponseTemplate::new(200).set_body_string(valid_rss.clone()))
            .mount(&mock_server)
            .await;

        // User feed also succeeds
        let user_rss = create_rss_feed(
            "testuser",
            vec![(
                "User tweet",
                "https://example.com/testuser/status/1",
                &rfc2822_date_offset(1),
            )],
        );

        Mock::given(method("GET"))
            .and(path("/testuser/rss"))
            .respond_with(ResponseTemplate::new(200).set_body_string(user_rss))
            .mount(&mock_server)
            .await;

        let config = create_test_config(&mock_server.uri());
        let usernames = vec!["testuser".to_string()];

        // This should succeed after retries
        let result = fetch_tweets_from_rss(&config, &usernames).await;
        assert!(
            result.is_ok(),
            "Should succeed after health check retries: {:?}",
            result
        );
    }

    #[tokio::test]
    async fn test_health_check_fails_after_max_retries() {
        let mock_server = MockServer::start().await;

        // Health check always fails
        Mock::given(method("GET"))
            .and(path("/OpenAI/rss"))
            .respond_with(ResponseTemplate::new(500).set_body_string("Server Error"))
            .mount(&mock_server)
            .await;

        let config = create_test_config(&mock_server.uri());
        let usernames = vec!["testuser".to_string()];

        let start = std::time::Instant::now();
        let result = fetch_tweets_from_rss(&config, &usernames).await;
        let elapsed = start.elapsed();

        // Should fail after health check retries (4 attempts with 1s, 2s, 4s delays)
        assert!(
            result.is_err(),
            "Should fail when health check always fails"
        );

        // With health_check() preset: 4 attempts, delays of 1s, 2s, 4s = 7s total
        // Allow some tolerance
        assert!(
            elapsed >= std::time::Duration::from_secs(5),
            "Should have spent time retrying health check, got {:?}",
            elapsed
        );
    }

    #[tokio::test]
    async fn test_rss_feed_fetch_retries_on_transient_failure() {
        let mock_server = MockServer::start().await;

        // Health check succeeds
        let health_rss = create_rss_feed(
            "OpenAI",
            vec![(
                "Test",
                "https://example.com/OpenAI/status/1",
                "Mon, 15 Jan 2024 10:30:00 +0000",
            )],
        );

        Mock::given(method("GET"))
            .and(path("/OpenAI/rss"))
            .respond_with(ResponseTemplate::new(200).set_body_string(health_rss))
            .mount(&mock_server)
            .await;

        // User feed fails twice then succeeds
        Mock::given(method("GET"))
            .and(path("/testuser/rss"))
            .respond_with(ResponseTemplate::new(503).set_body_string("Service Unavailable"))
            .up_to_n_times(2)
            .mount(&mock_server)
            .await;

        let user_rss = create_rss_feed(
            "testuser",
            vec![(
                "Success tweet after retry",
                "https://example.com/testuser/status/1",
                &rfc2822_date_offset(1),
            )],
        );

        Mock::given(method("GET"))
            .and(path("/testuser/rss"))
            .respond_with(ResponseTemplate::new(200).set_body_string(user_rss))
            .mount(&mock_server)
            .await;

        let config = create_test_config(&mock_server.uri());
        let usernames = vec!["testuser".to_string()];

        let result = fetch_tweets_from_rss(&config, &usernames).await;
        assert!(
            result.is_ok(),
            "Should succeed after RSS feed retries: {:?}",
            result
        );

        let tweets = result.unwrap();
        assert_eq!(tweets.len(), 1);
        assert!(tweets[0].text.contains("Success tweet after retry"));
    }

    #[tokio::test]
    async fn test_rss_feed_fetch_fails_after_max_retries() {
        let mock_server = MockServer::start().await;

        // Health check succeeds
        let health_rss = create_rss_feed(
            "OpenAI",
            vec![(
                "Test",
                "https://example.com/OpenAI/status/1",
                "Mon, 15 Jan 2024 10:30:00 +0000",
            )],
        );

        Mock::given(method("GET"))
            .and(path("/OpenAI/rss"))
            .respond_with(ResponseTemplate::new(200).set_body_string(health_rss))
            .mount(&mock_server)
            .await;

        // User feed always fails with 500
        Mock::given(method("GET"))
            .and(path("/testuser/rss"))
            .respond_with(ResponseTemplate::new(500).set_body_string("Internal Server Error"))
            .mount(&mock_server)
            .await;

        let config = create_test_config(&mock_server.uri());
        let usernames = vec!["testuser".to_string()];

        let start = std::time::Instant::now();
        let result = fetch_tweets_from_rss(&config, &usernames).await;
        let elapsed = start.elapsed();

        // Should complete (with empty tweets) after RSS feed retries
        // rss_feed() preset: 3 attempts with 500ms, 1s delays = 1.5s total
        assert!(result.is_ok());
        let tweets = result.unwrap();
        assert!(
            tweets.is_empty(),
            "Should have no tweets when all fetches fail"
        );

        // Verify some retry delay occurred
        assert!(
            elapsed >= std::time::Duration::from_millis(1000),
            "Should have spent time retrying RSS fetch, got {:?}",
            elapsed
        );
    }

    #[tokio::test]
    async fn test_multiple_users_independent_retries() {
        let mock_server = MockServer::start().await;

        // Health check succeeds
        let health_rss = create_rss_feed(
            "OpenAI",
            vec![(
                "Test",
                "https://example.com/OpenAI/status/1",
                "Mon, 15 Jan 2024 10:30:00 +0000",
            )],
        );

        Mock::given(method("GET"))
            .and(path("/OpenAI/rss"))
            .respond_with(ResponseTemplate::new(200).set_body_string(health_rss))
            .mount(&mock_server)
            .await;

        // User1: fails on first attempt, succeeds on second
        Mock::given(method("GET"))
            .and(path("/user1/rss"))
            .respond_with(ResponseTemplate::new(503))
            .up_to_n_times(1)
            .mount(&mock_server)
            .await;

        let user1_rss = create_rss_feed(
            "user1",
            vec![(
                "User1 tweet",
                "https://example.com/user1/status/1",
                &rfc2822_date_offset(1),
            )],
        );

        Mock::given(method("GET"))
            .and(path("/user1/rss"))
            .respond_with(ResponseTemplate::new(200).set_body_string(user1_rss))
            .mount(&mock_server)
            .await;

        // User2: succeeds immediately
        let user2_rss = create_rss_feed(
            "user2",
            vec![(
                "User2 tweet",
                "https://example.com/user2/status/1",
                &rfc2822_date_offset(2),
            )],
        );

        Mock::given(method("GET"))
            .and(path("/user2/rss"))
            .respond_with(ResponseTemplate::new(200).set_body_string(user2_rss))
            .mount(&mock_server)
            .await;

        // User3: always fails
        Mock::given(method("GET"))
            .and(path("/user3/rss"))
            .respond_with(ResponseTemplate::new(500))
            .mount(&mock_server)
            .await;

        let config = create_test_config(&mock_server.uri());
        let usernames = vec![
            "user1".to_string(),
            "user2".to_string(),
            "user3".to_string(),
        ];

        let result = fetch_tweets_from_rss(&config, &usernames).await;
        assert!(result.is_ok());

        let tweets = result.unwrap();
        // Should have tweets from user1 (after retry) and user2, but not user3
        assert_eq!(tweets.len(), 2);

        let tweet_texts: Vec<&str> = tweets.iter().map(|t| t.text.as_str()).collect();
        assert!(
            tweet_texts.iter().any(|t| t.contains("User1")),
            "Should have user1's tweet"
        );
        assert!(
            tweet_texts.iter().any(|t| t.contains("User2")),
            "Should have user2's tweet"
        );
    }

    #[tokio::test]
    async fn test_health_check_succeeds_on_first_attempt() {
        let mock_server = MockServer::start().await;

        let health_rss = create_rss_feed(
            "OpenAI",
            vec![(
                "Test",
                "https://example.com/OpenAI/status/1",
                "Mon, 15 Jan 2024 10:30:00 +0000",
            )],
        );

        Mock::given(method("GET"))
            .and(path("/OpenAI/rss"))
            .respond_with(ResponseTemplate::new(200).set_body_string(health_rss))
            .expect(1) // Should only be called once
            .mount(&mock_server)
            .await;

        let user_rss = create_rss_feed(
            "testuser",
            vec![(
                "Test tweet",
                "https://example.com/testuser/status/1",
                &rfc2822_date_offset(1),
            )],
        );

        Mock::given(method("GET"))
            .and(path("/testuser/rss"))
            .respond_with(ResponseTemplate::new(200).set_body_string(user_rss))
            .mount(&mock_server)
            .await;

        let config = create_test_config(&mock_server.uri());
        let usernames = vec!["testuser".to_string()];

        let start = std::time::Instant::now();
        let result = fetch_tweets_from_rss(&config, &usernames).await;
        let elapsed = start.elapsed();

        assert!(result.is_ok());
        // Should complete quickly without retry delays for health check
        // Only delay should be the 3s between user fetches
        assert!(
            elapsed < std::time::Duration::from_secs(5),
            "Should complete quickly when no retries needed, got {:?}",
            elapsed
        );
    }

    #[tokio::test]
    async fn test_health_check_html_response_triggers_retry() {
        let mock_server = MockServer::start().await;

        // First call returns HTML (should trigger retry)
        Mock::given(method("GET"))
            .and(path("/OpenAI/rss"))
            .respond_with(
                ResponseTemplate::new(200).set_body_string("<!DOCTYPE html><html></html>"),
            )
            .up_to_n_times(1)
            .mount(&mock_server)
            .await;

        // Second call returns valid RSS
        let valid_rss = create_rss_feed(
            "OpenAI",
            vec![(
                "Test",
                "https://example.com/OpenAI/status/1",
                "Mon, 15 Jan 2024 10:30:00 +0000",
            )],
        );

        Mock::given(method("GET"))
            .and(path("/OpenAI/rss"))
            .respond_with(ResponseTemplate::new(200).set_body_string(valid_rss))
            .mount(&mock_server)
            .await;

        let user_rss = create_rss_feed(
            "testuser",
            vec![(
                "Test tweet",
                "https://example.com/testuser/status/1",
                &rfc2822_date_offset(1),
            )],
        );

        Mock::given(method("GET"))
            .and(path("/testuser/rss"))
            .respond_with(ResponseTemplate::new(200).set_body_string(user_rss))
            .mount(&mock_server)
            .await;

        let config = create_test_config(&mock_server.uri());
        let usernames = vec!["testuser".to_string()];

        let result = fetch_tweets_from_rss(&config, &usernames).await;
        assert!(
            result.is_ok(),
            "Should succeed after HTML response retry: {:?}",
            result
        );
    }
}
