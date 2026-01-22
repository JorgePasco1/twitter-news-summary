use crate::config::Config;
use crate::retry::{with_retry_if, RetryConfig};
use crate::twitter::Tweet;
use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// OpenAI Chat Completion request structure
#[derive(Debug, Serialize, Clone, PartialEq)]
pub struct ChatRequest {
    pub model: String,
    pub messages: Vec<Message>,
    pub max_completion_tokens: u32,
    /// Temperature is NOT supported for reasoning models (gpt-5-nano, gpt-5-mini, gpt-5, o1 series)
    /// Only include for non-reasoning models like gpt-4o, gpt-4o-mini
    #[serde(skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f32>,
    /// Reasoning effort for reasoning models (not supported on non-reasoning models)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reasoning_effort: Option<String>,
}

/// A message in the OpenAI chat format
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub struct Message {
    pub role: String,
    pub content: String,
}

#[derive(Debug, Deserialize)]
struct ChatResponse {
    choices: Vec<Choice>,
}

#[derive(Debug, Deserialize)]
struct Choice {
    message: Message,
}

/// Build Twitter URL from author_id and tweet_id
fn build_twitter_url(author_id: &Option<String>, tweet_id: &str) -> String {
    match author_id {
        Some(username) if tweet_id != "unknown" => {
            format!("https://x.com/{}/status/{}", username, tweet_id)
        }
        _ => "Link unavailable".to_string(),
    }
}

/// Format timestamp as relative time (e.g., "2h ago") or absolute if > 24h
fn format_relative_time(created_at: &Option<String>) -> String {
    let Some(timestamp) = created_at else {
        return "time unknown".to_string();
    };

    let Ok(dt) = DateTime::parse_from_rfc3339(timestamp) else {
        return "time unknown".to_string();
    };

    let now = Utc::now();
    let tweet_time = dt.with_timezone(&Utc);
    let duration = now.signed_duration_since(tweet_time);

    let hours = duration.num_hours();
    let minutes = duration.num_minutes();

    if hours < 0 || minutes < 0 {
        // Future timestamp (shouldn't happen)
        return "just now".to_string();
    }

    match hours {
        0 => format!("{}m ago", minutes.max(1)),
        1..=23 => format!("{}h ago", hours),
        _ => tweet_time.format("%b %d, %H:%M UTC").to_string(), // "Jan 15, 10:30 UTC"
    }
}

/// Build the system prompt for tweet summarization (pure function)
pub fn build_system_prompt(max_words: u32) -> String {
    format!(
        r#"
You are an AI/ML tech news curator summarizing Twitter/X content for Telegram.

Goal: produce a high-signal digest that helps readers understand what happened, why it matters, and what to click.

## CRITICAL FORMATTING REQUIREMENT (Telegram MarkdownV2)
- Use *text* for bold (NOT **text**)
- Use _text_ for italic
- Use `code` for inline code
- Use bullet points with - (hyphen followed by space)
- Headings must be simple: emoji + words only (no punctuation at end)
- Use ONE blank line between sections
- Special characters will be escaped automatically upstream, so use natural punctuation

## HARD REQUIREMENTS (non-negotiable)
1) Start with:
🧠 Top takeaways
- 3-5 bullets, ranked by importance

2) Then include 3-5 topic sections chosen ONLY from this list (omit any that don't apply):
- 🚀 Releases
- 🔬 Research
- 🧰 Tools and Tutorials
- 🏢 Companies and Deals
- ⚖️ Policy and Safety
- 💬 Debate and Opinions

3) Each section must have 2-4 bullets max

4) Every bullet MUST end with exactly ONE markdown link: [descriptive label](url)
   - Do NOT use generic labels: Read more, Learn more, Here, Link, Thread, Watch, Details
   - Do NOT include the word "source" in link labels
   - Link label must be 3-8 words AND include a proper noun or artifact name (person/org/product/paper/release)

5) Do NOT repeat the same URL anywhere in the digest

6) Deduplicate aggressively
   - Merge tweets about the same story into one bullet
   - Prefer the most authoritative tweet link (original author, maintainer, company announcement) over reactions

7) If an item appears in 🧠 Top takeaways, it MUST NOT appear again later
   - Exception: only if you add a clearly new detail AND use a different URL

8) Do NOT invent facts
   - Only include details explicitly present in the tweet text
   - If unsure, phrase as "Claims:" or "Suggests:" and keep it minimal
   - If it’s opinion/speculation, prefix the bullet with "Opinion:"

9) Author-link consistency
   - If you name a person/org as the speaker, the linked tweet should be from them
   - If the link is from a different account, explicitly write "Via:" or "Reported by:" in the bullet

10) Use ⚖️ Policy and Safety ONLY for regulation, investigations, compliance, security vulnerabilities/incidents, or formal safety/policy updates
    - Otherwise place content in 💬 Debate and Opinions or another section

11) Naturalness rule
    - Do NOT explicitly call out these instructions or narrate your process
    - Specifically: do NOT write labels like "Why this matters:" or "Key takeaway:"
    - Instead, blend significance naturally into the sentence

## BULLET STYLE (mandatory)
Each bullet must follow this pattern:
- *What happened* — <natural, specific significance or consequence> [label](url)

Quality rules:
- Keep bullets ~1-2 lines
- The significance must be specific; avoid generic filler like "crucial", "enhances", "improves" without concrete impact
- Use numbers/versions/dates when present in tweets

## LENGTH
Keep the total summary under {} words."#,
        max_words
    )
}

/// Format tweets for the user prompt with timestamps and links (pure function)
pub fn format_tweets_for_prompt(tweets: &[Tweet]) -> String {
    tweets
        .iter()
        .enumerate()
        .map(|(i, t)| {
            format!(
                "{}. {} [{}]\n   Link: {}",
                i + 1,
                t.text,
                format_relative_time(&t.created_at),
                build_twitter_url(&t.author_id, &t.id)
            )
        })
        .collect::<Vec<_>>()
        .join("\n\n")
}

/// Check if a model is a reasoning model (GPT-5 family, o1 series)
/// Reasoning models do NOT support temperature parameter
fn is_reasoning_model(model: &str) -> bool {
    model.starts_with("gpt-5")
        || model.starts_with("o1")
        || model.starts_with("o3")
        || model.starts_with("o4")
}

/// Build a complete ChatRequest for summarization (pure function)
pub fn build_chat_request(config: &Config, tweets: &[Tweet]) -> ChatRequest {
    let tweets_text = format_tweets_for_prompt(tweets);
    let system_prompt = build_system_prompt(config.summary_max_words);

    let user_prompt = format!(
        r#"Please summarize these {} recent tweets from my Twitter/X list into a Telegram digest.

Context:
- Audience: AI/ML builders and tech professionals
- Goal: maximize signal; rank the most important items first
- Links: each bullet must end with exactly one markdown link using the tweet URL provided in the input

Tweets:
{}"#,
        tweets.len(),
        tweets_text
    );

    // Reasoning models (gpt-5-nano, gpt-5-mini, gpt-5, o1, o3, o4) do NOT support temperature
    // They use reasoning_effort instead (use "low" for faster responses, can increase if needed)
    let (temperature, reasoning_effort) = if is_reasoning_model(&config.openai_model) {
        (None, Some("low".to_string()))
    } else {
        (Some(config.openai_temperature), None)
    };

    ChatRequest {
        model: config.openai_model.clone(),
        messages: vec![
            Message {
                role: "system".to_string(),
                content: system_prompt,
            },
            Message {
                role: "user".to_string(),
                content: user_prompt,
            },
        ],
        max_completion_tokens: config.summary_max_tokens,
        temperature,
        reasoning_effort,
    }
}

/// Summarize tweets using OpenAI's API
///
/// # Arguments
/// * `client` - The HTTP client to use for the request
/// * `config` - Application configuration containing API key and model settings
/// * `tweets` - The tweets to summarize
pub async fn summarize_tweets(
    client: &reqwest::Client,
    config: &Config,
    tweets: &[Tweet],
) -> Result<String> {
    let request = build_chat_request(config, tweets);

    with_retry_if(
        &RetryConfig::api_call(),
        "OpenAI summarization",
        || async {
            let response = client
                .post(&config.openai_api_url)
                .header("Authorization", format!("Bearer {}", config.openai_api_key))
                .header("Content-Type", "application/json")
                .json(&request)
                .send()
                .await
                .context("Failed to send request to OpenAI API")?;

            if !response.status().is_success() {
                let status = response.status();
                let body = response
                    .text()
                    .await
                    .unwrap_or_else(|e| format!("<failed to read body: {}>", e));
                anyhow::bail!("OpenAI API error ({}): {}", status, body);
            }

            let chat_response: ChatResponse = response
                .json()
                .await
                .context("Failed to parse OpenAI response")?;

            let summary = chat_response
                .choices
                .first()
                .map(|c| c.message.content.clone())
                .unwrap_or_else(|| "No summary generated".to_string());

            Ok(summary)
        },
        is_retryable_error,
    )
    .await
}

/// Determine if an error is retryable (5xx errors, 429 rate limit, network errors)
/// Other 4xx client errors should not be retried
fn is_retryable_error(error: &anyhow::Error) -> bool {
    let error_str = error.to_string();

    // Check if it's an OpenAI API error with a status code
    // Error format: "OpenAI API error (400 Bad Request): ..."
    if error_str.contains("OpenAI API error") {
        if let Some(start) = error_str.find('(') {
            if let Some(end) = error_str[start..].find(')') {
                let status_str = &error_str[start + 1..start + end];
                // Extract just the numeric status code (e.g., "400" from "400 Bad Request")
                let status_num = status_str.split_whitespace().next().unwrap_or("");
                if let Ok(status) = status_num.parse::<u16>() {
                    // Retry 429 (rate limit) and 5xx errors
                    // Don't retry other 4xx errors (400, 401, 403, etc.)
                    return status == 429 || status >= 500;
                }
            }
        }
    }

    // Retry network errors, timeouts, and other transient failures
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    #[allow(unused_imports)]
    use wiremock::{
        matchers::{header, method, path},
        Mock, MockServer, ResponseTemplate,
    };

    // ==================== Helper Functions ====================

    /// Create a test config with custom OpenAI API endpoint
    fn create_test_config() -> Config {
        Config {
            environment: "test".to_string(),
            twitter_bearer_token: None,
            twitter_list_id: None,
            openai_api_key: "test-openai-key".to_string(),
            openai_model: "gpt-4o-mini".to_string(),
            openai_api_url: "https://api.openai.com/v1/chat/completions".to_string(),
            openai_temperature: 0.7,
            telegram_bot_token: "test-token".to_string(),
            telegram_chat_id: "".to_string(),
            telegram_webhook_secret: "test-webhook-secret".to_string(),
            max_tweets: 100,
            hours_lookback: 12,
            summary_max_tokens: 2500,
            summary_max_words: 800,
            nitter_instance: "https://nitter.example.com".to_string(),
            nitter_api_key: None,
            usernames_file: "data/usernames.txt".to_string(),
            api_key: None,
            database_url: "postgres://test:test@localhost/test".to_string(),
            schedule_times: vec!["08:00".to_string(), "20:00".to_string()],
            port: 8080,
        }
    }

    /// Create a test config with a custom API URL (for wiremock testing)
    fn create_test_config_with_url(url: &str) -> Config {
        let mut config = create_test_config();
        config.openai_api_url = url.to_string();
        config
    }

    /// Create a sample tweet for testing
    fn create_tweet(id: &str, text: &str) -> Tweet {
        Tweet {
            id: id.to_string(),
            text: text.to_string(),
            author_id: Some("testuser".to_string()),
            created_at: Some("2024-01-15T10:30:00+00:00".to_string()),
        }
    }

    /// Create a mock OpenAI success response
    fn create_openai_response(content: &str) -> serde_json::Value {
        serde_json::json!({
            "id": "chatcmpl-123",
            "object": "chat.completion",
            "created": 1705312200,
            "model": "gpt-4o-mini",
            "choices": [
                {
                    "index": 0,
                    "message": {
                        "role": "assistant",
                        "content": content
                    },
                    "finish_reason": "stop"
                }
            ],
            "usage": {
                "prompt_tokens": 100,
                "completion_tokens": 50,
                "total_tokens": 150
            }
        })
    }

    // ==================== ChatRequest Tests ====================

    #[test]
    fn test_chat_request_serialization() {
        let request = ChatRequest {
            model: "gpt-4o-mini".to_string(),
            messages: vec![
                Message {
                    role: "system".to_string(),
                    content: "You are a helpful assistant.".to_string(),
                },
                Message {
                    role: "user".to_string(),
                    content: "Hello".to_string(),
                },
            ],
            max_completion_tokens: 1000,
            temperature: Some(0.7),
            reasoning_effort: None,
        };

        let json = serde_json::to_string(&request).expect("Should serialize");
        assert!(json.contains("gpt-4o-mini"));
        assert!(json.contains("system"));
        assert!(json.contains("user"));
        assert!(json.contains("1000"));
        assert!(json.contains("0.7"));
        // reasoning_effort should not be present when None
        assert!(!json.contains("reasoning_effort"));
    }

    #[test]
    fn test_chat_request_serialization_reasoning_model() {
        let request = ChatRequest {
            model: "gpt-5-nano".to_string(),
            messages: vec![Message {
                role: "user".to_string(),
                content: "Hello".to_string(),
            }],
            max_completion_tokens: 16000,
            temperature: None, // Not supported for reasoning models
            reasoning_effort: Some("low".to_string()),
        };

        let json = serde_json::to_string(&request).expect("Should serialize");
        assert!(json.contains("gpt-5-nano"));
        assert!(json.contains("16000"));
        assert!(json.contains("reasoning_effort"));
        assert!(json.contains("low"));
        // temperature should not be present when None
        assert!(!json.contains("temperature"));
    }

    #[test]
    fn test_message_serialization() {
        let message = Message {
            role: "assistant".to_string(),
            content: "Here is the summary...".to_string(),
        };

        let json = serde_json::to_string(&message).expect("Should serialize");
        assert!(json.contains("assistant"));
        assert!(json.contains("Here is the summary..."));
    }

    // ==================== ChatResponse Deserialization Tests ====================

    #[test]
    fn test_chat_response_deserialization() {
        let json = r#"{
            "choices": [
                {
                    "message": {
                        "role": "assistant",
                        "content": "This is the summary."
                    }
                }
            ]
        }"#;

        let response: ChatResponse = serde_json::from_str(json).expect("Should deserialize");
        assert_eq!(response.choices.len(), 1);
        assert_eq!(response.choices[0].message.content, "This is the summary.");
        assert_eq!(response.choices[0].message.role, "assistant");
    }

    #[test]
    fn test_chat_response_empty_choices() {
        let json = r#"{"choices": []}"#;

        let response: ChatResponse = serde_json::from_str(json).expect("Should deserialize");
        assert!(response.choices.is_empty());
    }

    #[test]
    fn test_chat_response_multiple_choices() {
        let json = r#"{
            "choices": [
                {"message": {"role": "assistant", "content": "First choice"}},
                {"message": {"role": "assistant", "content": "Second choice"}}
            ]
        }"#;

        let response: ChatResponse = serde_json::from_str(json).expect("Should deserialize");
        assert_eq!(response.choices.len(), 2);
        assert_eq!(response.choices[0].message.content, "First choice");
        assert_eq!(response.choices[1].message.content, "Second choice");
    }

    // ==================== summarize_tweets Tests ====================

    #[tokio::test]
    async fn test_summarize_tweets_success() {
        let mock_server = MockServer::start().await;

        let response_body = create_openai_response("Here is your summary of the tweets.");

        Mock::given(method("POST"))
            .and(path("/v1/chat/completions"))
            .and(header("Authorization", "Bearer test-openai-key"))
            .and(header("Content-Type", "application/json"))
            .respond_with(ResponseTemplate::new(200).set_body_json(&response_body))
            .mount(&mock_server)
            .await;

        let config =
            create_test_config_with_url(&format!("{}/v1/chat/completions", mock_server.uri()));
        let client = reqwest::Client::new();
        let tweets = [
            create_tweet("1", "@user1: This is a test tweet"),
            create_tweet("2", "@user2: Another test tweet"),
        ];

        let result = summarize_tweets(&client, &config, &tweets).await;
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "Here is your summary of the tweets.");
    }

    #[tokio::test]
    async fn test_summarize_tweets_api_error() {
        let mock_server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/v1/chat/completions"))
            .respond_with(ResponseTemplate::new(401).set_body_string("Unauthorized"))
            .mount(&mock_server)
            .await;

        let config =
            create_test_config_with_url(&format!("{}/v1/chat/completions", mock_server.uri()));
        let client = reqwest::Client::new();
        let tweets = [create_tweet("1", "Test tweet")];

        let result = summarize_tweets(&client, &config, &tweets).await;
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("401") || err.contains("Unauthorized"));
    }

    #[tokio::test]
    async fn test_summarize_tweets_empty_choices() {
        let mock_server = MockServer::start().await;

        let response_body = serde_json::json!({
            "choices": []
        });

        Mock::given(method("POST"))
            .and(path("/v1/chat/completions"))
            .respond_with(ResponseTemplate::new(200).set_body_json(&response_body))
            .mount(&mock_server)
            .await;

        let config =
            create_test_config_with_url(&format!("{}/v1/chat/completions", mock_server.uri()));
        let client = reqwest::Client::new();
        let tweets = [create_tweet("1", "Test tweet")];

        let result = summarize_tweets(&client, &config, &tweets).await;
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "No summary generated");
    }

    // ==================== Pure Function Tests ====================

    #[test]
    fn test_build_system_prompt_is_deterministic() {
        let prompt1 = build_system_prompt(800);
        let prompt2 = build_system_prompt(800);

        assert_eq!(prompt1, prompt2, "System prompt should be deterministic");
    }

    #[test]
    fn test_build_system_prompt_is_not_empty() {
        let prompt = build_system_prompt(800);

        assert!(!prompt.is_empty(), "System prompt should not be empty");
        assert!(prompt.len() > 100, "System prompt should be substantial");
    }

    #[test]
    fn test_build_system_prompt_contains_word_limit() {
        let prompt = build_system_prompt(500);
        assert!(
            prompt.contains("500"),
            "Prompt should contain the word limit"
        );

        let prompt2 = build_system_prompt(1000);
        assert!(
            prompt2.contains("1000"),
            "Prompt should contain the word limit"
        );
    }

    #[test]
    fn test_build_system_prompt_contains_all_guidelines() {
        let prompt = build_system_prompt(800);

        // Verify all key guidelines are present
        let required_elements = [
            "Telegram",
            "AI/ML",
            "Top takeaways",
            "bullet",
            "markdown link",
            "Releases",
            "Research",
        ];

        for element in required_elements {
            assert!(
                prompt.contains(element),
                "System prompt should contain '{}', but got: {}",
                element,
                prompt
            );
        }
    }

    #[test]
    fn test_build_chat_request() {
        let config = create_test_config();
        let tweets = [create_tweet("1", "@user1: Test tweet")];

        let request = build_chat_request(&config, &tweets);

        assert_eq!(request.model, "gpt-4o-mini");
        assert_eq!(request.messages.len(), 2);
        assert_eq!(request.messages[0].role, "system");
        assert_eq!(request.messages[1].role, "user");
        assert!(request.messages[1].content.contains("1 recent tweets"));
        assert_eq!(request.max_completion_tokens, 2500);
        // Non-reasoning model should have temperature set
        assert!(request.temperature.is_some());
        assert!((request.temperature.unwrap() - 0.7).abs() < f32::EPSILON);
        // Non-reasoning model should NOT have reasoning_effort
        assert!(request.reasoning_effort.is_none());
    }

    #[test]
    fn test_build_chat_request_reasoning_model() {
        let mut config = create_test_config();
        config.openai_model = "gpt-5-nano".to_string();
        let tweets = [create_tweet("1", "Test tweet")];

        let request = build_chat_request(&config, &tweets);

        assert_eq!(request.model, "gpt-5-nano");
        // Reasoning model should NOT have temperature
        assert!(request.temperature.is_none());
        // Reasoning model should have reasoning_effort
        assert!(request.reasoning_effort.is_some());
        assert_eq!(request.reasoning_effort.as_deref(), Some("low"));
    }

    #[test]
    fn test_build_chat_request_uses_config_model() {
        let mut config = create_test_config();
        config.openai_model = "gpt-4-turbo".to_string();
        let tweets = [create_tweet("1", "Test")];

        let request = build_chat_request(&config, &tweets);

        assert_eq!(request.model, "gpt-4-turbo");
    }

    #[test]
    fn test_build_chat_request_with_empty_tweets() {
        let config = create_test_config();
        let tweets: Vec<Tweet> = vec![];

        let request = build_chat_request(&config, &tweets);

        assert_eq!(request.messages.len(), 2);
        assert!(request.messages[1].content.contains("0 recent tweets"));
    }

    #[test]
    fn test_build_chat_request_system_prompt_is_first() {
        let config = create_test_config();
        let tweets = [create_tweet("1", "Test")];

        let request = build_chat_request(&config, &tweets);

        assert_eq!(request.messages[0].role, "system");
        assert!(request.messages[0].content.contains("AI/ML"));
    }

    #[test]
    fn test_build_chat_request_with_many_tweets() {
        let config = create_test_config();
        let tweets: Vec<Tweet> = (1..=50)
            .map(|i| create_tweet(&i.to_string(), &format!("Tweet number {}", i)))
            .collect();

        let request = build_chat_request(&config, &tweets);

        assert!(request.messages[1].content.contains("50 recent tweets"));
        assert!(request.messages[1].content.contains("1. Tweet number 1"));
        assert!(request.messages[1].content.contains("50. Tweet number 50"));
    }

    // ==================== Helper Function Tests ====================

    #[test]
    fn test_build_twitter_url_with_valid_data() {
        let url = build_twitter_url(&Some("testuser".to_string()), "123456789");
        assert_eq!(url, "https://x.com/testuser/status/123456789");
    }

    #[test]
    fn test_build_twitter_url_with_unknown_id() {
        let url = build_twitter_url(&Some("testuser".to_string()), "unknown");
        assert_eq!(url, "Link unavailable");
    }

    #[test]
    fn test_build_twitter_url_without_author() {
        let url = build_twitter_url(&None, "123456789");
        assert_eq!(url, "Link unavailable");
    }

    #[test]
    fn test_build_twitter_url_without_author_and_unknown_id() {
        let url = build_twitter_url(&None, "unknown");
        assert_eq!(url, "Link unavailable");
    }

    #[test]
    fn test_format_relative_time_minutes() {
        let now = Utc::now();
        let timestamp = (now - chrono::Duration::minutes(30)).to_rfc3339();
        let result = format_relative_time(&Some(timestamp));
        assert_eq!(result, "30m ago");
    }

    #[test]
    fn test_format_relative_time_one_minute() {
        let now = Utc::now();
        let timestamp = (now - chrono::Duration::seconds(30)).to_rfc3339();
        let result = format_relative_time(&Some(timestamp));
        // Should be at least 1m, not 0m
        assert_eq!(result, "1m ago");
    }

    #[test]
    fn test_format_relative_time_hours() {
        let now = Utc::now();
        let timestamp = (now - chrono::Duration::hours(5)).to_rfc3339();
        let result = format_relative_time(&Some(timestamp));
        assert_eq!(result, "5h ago");
    }

    #[test]
    fn test_format_relative_time_23_hours() {
        let now = Utc::now();
        let timestamp = (now - chrono::Duration::hours(23)).to_rfc3339();
        let result = format_relative_time(&Some(timestamp));
        assert_eq!(result, "23h ago");
    }

    #[test]
    fn test_format_relative_time_over_24h() {
        let now = Utc::now();
        let timestamp = (now - chrono::Duration::hours(48)).to_rfc3339();
        let result = format_relative_time(&Some(timestamp));
        // Should return absolute format like "Jan 14, 10:30 UTC"
        assert!(
            result.contains(",") && result.contains("UTC"),
            "Expected absolute date format with UTC, got: {}",
            result
        );
    }

    #[test]
    fn test_format_relative_time_none() {
        let result = format_relative_time(&None);
        assert_eq!(result, "time unknown");
    }

    #[test]
    fn test_format_relative_time_invalid_timestamp() {
        let result = format_relative_time(&Some("invalid-timestamp".to_string()));
        assert_eq!(result, "time unknown");
    }

    #[test]
    fn test_format_tweets_for_prompt_includes_links() {
        let now = Utc::now();
        let timestamp = (now - chrono::Duration::hours(2)).to_rfc3339();

        let tweets = [Tweet {
            id: "123456".to_string(),
            text: "@user: Test tweet".to_string(),
            author_id: Some("user".to_string()),
            created_at: Some(timestamp),
        }];

        let tweets_text = format_tweets_for_prompt(&tweets);

        assert!(tweets_text.contains("https://x.com/user/status/123456"));
        assert!(tweets_text.contains("@user: Test tweet"));
        assert!(tweets_text.contains("2h ago"));
    }

    #[test]
    fn test_format_tweets_for_prompt_empty() {
        let tweets: Vec<Tweet> = vec![];
        let tweets_text = format_tweets_for_prompt(&tweets);
        assert!(tweets_text.is_empty());
    }

    #[test]
    fn test_format_tweets_for_prompt_multiple() {
        let tweets = [
            create_tweet("1", "First tweet"),
            create_tweet("2", "Second tweet"),
        ];

        let tweets_text = format_tweets_for_prompt(&tweets);

        assert!(tweets_text.contains("1. First tweet"));
        assert!(tweets_text.contains("2. Second tweet"));
        // Should have double newline between tweets
        assert!(tweets_text.contains("\n\n"));
    }

    // ==================== Error Handling Tests ====================

    #[test]
    fn test_empty_choices_returns_default_message() {
        let response = ChatResponse { choices: vec![] };

        let summary = response
            .choices
            .first()
            .map(|c| c.message.content.clone())
            .unwrap_or_else(|| "No summary generated".to_string());

        assert_eq!(summary, "No summary generated");
    }

    // ==================== Edge Case Tests ====================

    #[test]
    fn test_format_tweets_special_characters() {
        let tweets = [
            create_tweet("1", "@user1: Tweet with \"quotes\" and 'apostrophes'"),
            create_tweet("2", "@user2: Tweet with <html> & special chars"),
        ];

        let tweets_text = format_tweets_for_prompt(&tweets);

        // Verify special characters are preserved
        assert!(tweets_text.contains("\"quotes\""));
        assert!(tweets_text.contains("<html>"));
    }

    #[test]
    fn test_format_tweets_numbering_starts_at_one() {
        let tweets = [
            create_tweet("1", "First"),
            create_tweet("2", "Second"),
            create_tweet("3", "Third"),
        ];

        let tweets_text = format_tweets_for_prompt(&tweets);

        // Verify numbering starts at 1, not 0
        assert!(tweets_text.contains("1. First"));
        assert!(tweets_text.contains("2. Second"));
        assert!(tweets_text.contains("3. Third"));
        assert!(!tweets_text.contains("0. "));
    }

    #[test]
    fn test_format_tweets_many_tweets() {
        let tweets: Vec<Tweet> = (1..=100)
            .map(|i| create_tweet(&i.to_string(), &format!("Tweet number {}", i)))
            .collect();

        let tweets_text = format_tweets_for_prompt(&tweets);

        // Verify all tweets are included
        assert!(tweets_text.contains("1. Tweet number 1"));
        assert!(tweets_text.contains("50. Tweet number 50"));
        assert!(tweets_text.contains("100. Tweet number 100"));
    }

    // ==================== Config Tests ====================

    #[test]
    fn test_custom_model_in_config() {
        let mut config = create_test_config();
        config.openai_model = "gpt-4-turbo".to_string();

        assert_eq!(config.openai_model, "gpt-4-turbo");
    }

    #[test]
    fn test_request_parameters() {
        let config = create_test_config();
        let tweets = [create_tweet("1", "Test")];
        let request = build_chat_request(&config, &tweets);

        // Verify expected parameters (2500 is the default for summary_max_tokens)
        assert_eq!(request.max_completion_tokens, 2500);
        // Non-reasoning model uses temperature
        assert!(request.temperature.is_some());
        assert!((request.temperature.unwrap() - 0.7).abs() < f32::EPSILON);
    }

    #[test]
    fn test_config_summary_values_used() {
        let config = create_test_config();
        assert_eq!(config.summary_max_tokens, 2500);
        assert_eq!(config.summary_max_words, 800);
    }

    // ==================== Additional Wiremock Tests ====================

    #[tokio::test]
    async fn test_summarize_tweets_rate_limit_error() {
        let mock_server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/v1/chat/completions"))
            .respond_with(
                ResponseTemplate::new(429)
                    .set_body_string(r#"{"error": {"message": "Rate limit exceeded"}}"#),
            )
            .mount(&mock_server)
            .await;

        let config =
            create_test_config_with_url(&format!("{}/v1/chat/completions", mock_server.uri()));
        let client = reqwest::Client::new();
        let tweets = [create_tweet("1", "Test tweet")];

        let result = summarize_tweets(&client, &config, &tweets).await;
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("429") || err.contains("Rate limit"));
    }

    #[tokio::test]
    async fn test_summarize_tweets_malformed_json_response() {
        let mock_server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/v1/chat/completions"))
            .respond_with(ResponseTemplate::new(200).set_body_string("not valid json"))
            .mount(&mock_server)
            .await;

        let config =
            create_test_config_with_url(&format!("{}/v1/chat/completions", mock_server.uri()));
        let client = reqwest::Client::new();
        let tweets = [create_tweet("1", "Test tweet")];

        let result = summarize_tweets(&client, &config, &tweets).await;
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(
            err.contains("parse") || err.contains("Failed"),
            "Error should indicate parsing failure: {}",
            err
        );
    }

    #[tokio::test]
    async fn test_summarize_tweets_internal_server_error() {
        let mock_server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/v1/chat/completions"))
            .respond_with(
                ResponseTemplate::new(500)
                    .set_body_string(r#"{"error": {"message": "Internal server error"}}"#),
            )
            .mount(&mock_server)
            .await;

        let config =
            create_test_config_with_url(&format!("{}/v1/chat/completions", mock_server.uri()));
        let client = reqwest::Client::new();
        let tweets = [create_tweet("1", "Test tweet")];

        let result = summarize_tweets(&client, &config, &tweets).await;
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("500"));
    }

    #[tokio::test]
    async fn test_summarize_tweets_with_empty_tweet_list() {
        let mock_server = MockServer::start().await;

        let response_body = create_openai_response("No tweets to summarize.");

        Mock::given(method("POST"))
            .and(path("/v1/chat/completions"))
            .respond_with(ResponseTemplate::new(200).set_body_json(&response_body))
            .mount(&mock_server)
            .await;

        let config =
            create_test_config_with_url(&format!("{}/v1/chat/completions", mock_server.uri()));
        let client = reqwest::Client::new();
        let tweets: Vec<Tweet> = vec![];

        let result = summarize_tweets(&client, &config, &tweets).await;
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "No tweets to summarize.");
    }

    #[tokio::test]
    async fn test_summarize_tweets_verifies_request_body() {
        let mock_server = MockServer::start().await;

        let response_body = create_openai_response("Summary");

        Mock::given(method("POST"))
            .and(path("/v1/chat/completions"))
            .and(header("Authorization", "Bearer test-openai-key"))
            .and(header("Content-Type", "application/json"))
            .respond_with(ResponseTemplate::new(200).set_body_json(&response_body))
            .expect(1)
            .mount(&mock_server)
            .await;

        let config =
            create_test_config_with_url(&format!("{}/v1/chat/completions", mock_server.uri()));
        let client = reqwest::Client::new();
        let tweets = [create_tweet("1", "Test tweet")];

        let result = summarize_tweets(&client, &config, &tweets).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_summarize_tweets_uses_custom_api_url() {
        let mock_server = MockServer::start().await;

        let response_body = create_openai_response("Custom endpoint summary");

        // Mount on a custom path to verify the URL is used correctly
        Mock::given(method("POST"))
            .and(path("/custom/chat/endpoint"))
            .respond_with(ResponseTemplate::new(200).set_body_json(&response_body))
            .mount(&mock_server)
            .await;

        let config =
            create_test_config_with_url(&format!("{}/custom/chat/endpoint", mock_server.uri()));
        let client = reqwest::Client::new();
        let tweets = [create_tweet("1", "Test")];

        let result = summarize_tweets(&client, &config, &tweets).await;
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "Custom endpoint summary");
    }

    #[tokio::test]
    async fn test_summarize_tweets_handles_service_unavailable() {
        let mock_server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/v1/chat/completions"))
            .respond_with(
                ResponseTemplate::new(503).set_body_string("Service temporarily unavailable"),
            )
            .mount(&mock_server)
            .await;

        let config =
            create_test_config_with_url(&format!("{}/v1/chat/completions", mock_server.uri()));
        let client = reqwest::Client::new();
        let tweets = [create_tweet("1", "Test tweet")];

        let result = summarize_tweets(&client, &config, &tweets).await;
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("503"));
    }

    // ==================== Message and ChatRequest Equality Tests ====================

    #[test]
    fn test_message_equality() {
        let msg1 = Message {
            role: "user".to_string(),
            content: "Hello".to_string(),
        };
        let msg2 = Message {
            role: "user".to_string(),
            content: "Hello".to_string(),
        };
        let msg3 = Message {
            role: "assistant".to_string(),
            content: "Hello".to_string(),
        };

        assert_eq!(msg1, msg2);
        assert_ne!(msg1, msg3);
    }

    #[test]
    fn test_chat_request_equality() {
        let req1 = ChatRequest {
            model: "gpt-4".to_string(),
            messages: vec![],
            max_completion_tokens: 1000,
            temperature: Some(0.7),
            reasoning_effort: None,
        };
        let req2 = ChatRequest {
            model: "gpt-4".to_string(),
            messages: vec![],
            max_completion_tokens: 1000,
            temperature: Some(0.7),
            reasoning_effort: None,
        };

        assert_eq!(req1, req2);
    }

    #[test]
    fn test_chat_request_clone() {
        let original = ChatRequest {
            model: "gpt-4".to_string(),
            messages: vec![Message {
                role: "user".to_string(),
                content: "Test".to_string(),
            }],
            max_completion_tokens: 1000,
            temperature: Some(0.7),
            reasoning_effort: None,
        };

        let cloned = original.clone();

        assert_eq!(original, cloned);
    }

    #[test]
    fn test_message_clone() {
        let original = Message {
            role: "system".to_string(),
            content: "You are a helpful assistant.".to_string(),
        };

        let cloned = original.clone();

        assert_eq!(original, cloned);
    }

    // ==================== Retry Integration Tests ====================

    #[tokio::test]
    async fn test_summarize_tweets_retries_on_500_error() {
        let mock_server = MockServer::start().await;

        // First two requests fail with 500, third succeeds
        Mock::given(method("POST"))
            .and(path("/v1/chat/completions"))
            .respond_with(
                ResponseTemplate::new(500)
                    .set_body_string(r#"{"error": {"message": "Internal Server Error"}}"#),
            )
            .up_to_n_times(2)
            .mount(&mock_server)
            .await;

        let response_body = create_openai_response("Summary after retry");

        Mock::given(method("POST"))
            .and(path("/v1/chat/completions"))
            .respond_with(ResponseTemplate::new(200).set_body_json(&response_body))
            .mount(&mock_server)
            .await;

        let config =
            create_test_config_with_url(&format!("{}/v1/chat/completions", mock_server.uri()));
        let client = reqwest::Client::new();
        let tweets = [create_tweet("1", "Test tweet")];

        let result = summarize_tweets(&client, &config, &tweets).await;
        assert!(result.is_ok(), "Should succeed after retries: {:?}", result);
        assert_eq!(result.unwrap(), "Summary after retry");
    }

    #[tokio::test]
    async fn test_summarize_tweets_retries_on_503_error() {
        let mock_server = MockServer::start().await;

        // First request fails with 503 Service Unavailable
        Mock::given(method("POST"))
            .and(path("/v1/chat/completions"))
            .respond_with(
                ResponseTemplate::new(503).set_body_string("Service temporarily unavailable"),
            )
            .up_to_n_times(1)
            .mount(&mock_server)
            .await;

        let response_body = create_openai_response("Summary after 503 retry");

        Mock::given(method("POST"))
            .and(path("/v1/chat/completions"))
            .respond_with(ResponseTemplate::new(200).set_body_json(&response_body))
            .mount(&mock_server)
            .await;

        let config =
            create_test_config_with_url(&format!("{}/v1/chat/completions", mock_server.uri()));
        let client = reqwest::Client::new();
        let tweets = [create_tweet("1", "Test tweet")];

        let result = summarize_tweets(&client, &config, &tweets).await;
        assert!(
            result.is_ok(),
            "Should succeed after 503 retry: {:?}",
            result
        );
    }

    #[tokio::test]
    async fn test_summarize_tweets_no_retry_on_400_error() {
        let mock_server = MockServer::start().await;

        // 400 Bad Request should NOT be retried
        Mock::given(method("POST"))
            .and(path("/v1/chat/completions"))
            .respond_with(
                ResponseTemplate::new(400)
                    .set_body_string(r#"{"error": {"message": "Bad request"}}"#),
            )
            .expect(1) // Should only be called once - no retries
            .mount(&mock_server)
            .await;

        let config =
            create_test_config_with_url(&format!("{}/v1/chat/completions", mock_server.uri()));
        let client = reqwest::Client::new();
        let tweets = [create_tweet("1", "Test tweet")];

        let start = std::time::Instant::now();
        let result = summarize_tweets(&client, &config, &tweets).await;
        let elapsed = start.elapsed();

        assert!(result.is_err(), "400 error should fail");
        let err = result.unwrap_err().to_string();
        assert!(
            err.contains("400"),
            "Error should mention 400 status: {}",
            err
        );

        // Should fail quickly without retry delays
        assert!(
            elapsed < std::time::Duration::from_secs(1),
            "400 error should fail immediately without retries, took {:?}",
            elapsed
        );
    }

    #[tokio::test]
    async fn test_summarize_tweets_no_retry_on_401_error() {
        let mock_server = MockServer::start().await;

        // 401 Unauthorized should NOT be retried
        Mock::given(method("POST"))
            .and(path("/v1/chat/completions"))
            .respond_with(
                ResponseTemplate::new(401)
                    .set_body_string(r#"{"error": {"message": "Invalid API key"}}"#),
            )
            .expect(1)
            .mount(&mock_server)
            .await;

        let config =
            create_test_config_with_url(&format!("{}/v1/chat/completions", mock_server.uri()));
        let client = reqwest::Client::new();
        let tweets = [create_tweet("1", "Test tweet")];

        let result = summarize_tweets(&client, &config, &tweets).await;
        assert!(result.is_err(), "401 error should fail immediately");
    }

    #[tokio::test]
    async fn test_summarize_tweets_no_retry_on_403_error() {
        let mock_server = MockServer::start().await;

        // 403 Forbidden should NOT be retried
        Mock::given(method("POST"))
            .and(path("/v1/chat/completions"))
            .respond_with(
                ResponseTemplate::new(403)
                    .set_body_string(r#"{"error": {"message": "Access denied"}}"#),
            )
            .expect(1)
            .mount(&mock_server)
            .await;

        let config =
            create_test_config_with_url(&format!("{}/v1/chat/completions", mock_server.uri()));
        let client = reqwest::Client::new();
        let tweets = [create_tweet("1", "Test tweet")];

        let result = summarize_tweets(&client, &config, &tweets).await;
        assert!(result.is_err(), "403 error should fail immediately");
    }

    #[tokio::test]
    async fn test_summarize_tweets_no_retry_on_422_error() {
        let mock_server = MockServer::start().await;

        // 422 Unprocessable Entity should NOT be retried
        Mock::given(method("POST"))
            .and(path("/v1/chat/completions"))
            .respond_with(
                ResponseTemplate::new(422)
                    .set_body_string(r#"{"error": {"message": "Invalid parameters"}}"#),
            )
            .expect(1)
            .mount(&mock_server)
            .await;

        let config =
            create_test_config_with_url(&format!("{}/v1/chat/completions", mock_server.uri()));
        let client = reqwest::Client::new();
        let tweets = [create_tweet("1", "Test tweet")];

        let result = summarize_tweets(&client, &config, &tweets).await;
        assert!(result.is_err(), "422 error should fail immediately");
    }

    #[tokio::test]
    async fn test_summarize_tweets_exhausts_retries_on_persistent_500() {
        let mock_server = MockServer::start().await;

        // All requests fail with 500
        Mock::given(method("POST"))
            .and(path("/v1/chat/completions"))
            .respond_with(
                ResponseTemplate::new(500)
                    .set_body_string(r#"{"error": {"message": "Persistent failure"}}"#),
            )
            .expect(3) // api_call() preset has 3 attempts
            .mount(&mock_server)
            .await;

        let config =
            create_test_config_with_url(&format!("{}/v1/chat/completions", mock_server.uri()));
        let client = reqwest::Client::new();
        let tweets = [create_tweet("1", "Test tweet")];

        let start = std::time::Instant::now();
        let result = summarize_tweets(&client, &config, &tweets).await;
        let elapsed = start.elapsed();

        assert!(result.is_err(), "Should fail after exhausting retries");
        let err = result.unwrap_err().to_string();
        assert!(err.contains("500"), "Error should mention 500: {}", err);

        // api_call() preset: 3 attempts with 1s, 2s delays = 3s minimum
        assert!(
            elapsed >= std::time::Duration::from_secs(2),
            "Should have spent time retrying, got {:?}",
            elapsed
        );
    }

    #[tokio::test]
    async fn test_summarize_tweets_retries_on_rate_limit_429() {
        let mock_server = MockServer::start().await;

        // 429 Rate Limit IS retryable (transient error)
        // First two requests fail with 429, third succeeds
        Mock::given(method("POST"))
            .and(path("/v1/chat/completions"))
            .respond_with(
                ResponseTemplate::new(429)
                    .set_body_string(r#"{"error": {"message": "Rate limit exceeded"}}"#),
            )
            .up_to_n_times(2)
            .mount(&mock_server)
            .await;

        let response_body = create_openai_response("Success after rate limit");

        Mock::given(method("POST"))
            .and(path("/v1/chat/completions"))
            .respond_with(ResponseTemplate::new(200).set_body_json(&response_body))
            .mount(&mock_server)
            .await;

        let config =
            create_test_config_with_url(&format!("{}/v1/chat/completions", mock_server.uri()));
        let client = reqwest::Client::new();
        let tweets = [create_tweet("1", "Test tweet")];

        let result = summarize_tweets(&client, &config, &tweets).await;
        assert!(
            result.is_ok(),
            "429 errors should be retried and succeed: {:?}",
            result
        );
        assert_eq!(result.unwrap(), "Success after rate limit");
    }

    #[tokio::test]
    async fn test_summarize_tweets_success_on_first_attempt_no_delay() {
        let mock_server = MockServer::start().await;

        let response_body = create_openai_response("Immediate success");

        Mock::given(method("POST"))
            .and(path("/v1/chat/completions"))
            .respond_with(ResponseTemplate::new(200).set_body_json(&response_body))
            .expect(1)
            .mount(&mock_server)
            .await;

        let config =
            create_test_config_with_url(&format!("{}/v1/chat/completions", mock_server.uri()));
        let client = reqwest::Client::new();
        let tweets = [create_tweet("1", "Test tweet")];

        let start = std::time::Instant::now();
        let result = summarize_tweets(&client, &config, &tweets).await;
        let elapsed = start.elapsed();

        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "Immediate success");

        // Should complete very quickly
        assert!(
            elapsed < std::time::Duration::from_millis(500),
            "Should complete quickly on immediate success, got {:?}",
            elapsed
        );
    }

    // ==================== is_retryable_error Tests ====================

    #[test]
    fn test_is_retryable_error_500_error() {
        let error = anyhow::anyhow!("OpenAI API error (500): Internal Server Error");
        assert!(is_retryable_error(&error), "500 errors should be retryable");
    }

    #[test]
    fn test_is_retryable_error_503_error() {
        let error = anyhow::anyhow!("OpenAI API error (503): Service Unavailable");
        assert!(is_retryable_error(&error), "503 errors should be retryable");
    }

    #[test]
    fn test_is_retryable_error_400_error() {
        let error = anyhow::anyhow!("OpenAI API error (400): Bad Request");
        assert!(
            !is_retryable_error(&error),
            "400 errors should NOT be retryable"
        );
    }

    #[test]
    fn test_is_retryable_error_401_error() {
        let error = anyhow::anyhow!("OpenAI API error (401): Unauthorized");
        assert!(
            !is_retryable_error(&error),
            "401 errors should NOT be retryable"
        );
    }

    #[test]
    fn test_is_retryable_error_403_error() {
        let error = anyhow::anyhow!("OpenAI API error (403): Forbidden");
        assert!(
            !is_retryable_error(&error),
            "403 errors should NOT be retryable"
        );
    }

    #[test]
    fn test_is_retryable_error_429_error() {
        let error = anyhow::anyhow!("OpenAI API error (429): Rate Limit Exceeded");
        assert!(
            is_retryable_error(&error),
            "429 errors SHOULD be retryable (rate limit is transient)"
        );
    }

    #[test]
    fn test_is_retryable_error_network_error() {
        let error = anyhow::anyhow!("Failed to send request to OpenAI API: connection refused");
        assert!(
            is_retryable_error(&error),
            "Network errors should be retryable"
        );
    }

    #[test]
    fn test_is_retryable_error_timeout() {
        let error = anyhow::anyhow!("Request timed out");
        assert!(is_retryable_error(&error), "Timeouts should be retryable");
    }

    #[test]
    fn test_is_retryable_error_parse_error() {
        let error = anyhow::anyhow!("Failed to parse OpenAI response: invalid JSON");
        assert!(
            is_retryable_error(&error),
            "Parse errors should be retryable (might be transient)"
        );
    }

    #[test]
    fn test_is_retryable_error_non_api_error() {
        let error = anyhow::anyhow!("Some random error without status code");
        assert!(
            is_retryable_error(&error),
            "Unknown errors should be retryable by default"
        );
    }
}
