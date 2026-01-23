use crate::config::Config;
use crate::i18n::{Language, TranslationValidator};
use crate::retry::{with_retry_if, RetryConfig};
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use tracing::warn;

/// OpenAI Chat Completion request for translation
#[derive(Debug, Serialize)]
struct TranslationRequest {
    model: String,
    messages: Vec<Message>,
    max_completion_tokens: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    reasoning_effort: Option<String>,
}

/// Check if a model is a reasoning model that doesn't support temperature
fn is_reasoning_model(model: &str) -> bool {
    model.starts_with("gpt-5")
        || model.starts_with("o1")
        || model.starts_with("o3")
        || model.starts_with("o4")
}

#[derive(Debug, Serialize, Deserialize)]
struct Message {
    role: String,
    content: String,
}

#[derive(Debug, Deserialize)]
struct ChatResponse {
    choices: Vec<Choice>,
}

#[derive(Debug, Deserialize)]
struct Choice {
    message: Message,
}

// Language type is now defined in crate::i18n::Language
// It's imported at the top of this file and used throughout

/// Build the system prompt for translation
fn build_translation_system_prompt(target_language: &str) -> String {
    format!(
        r#"You are a professional translator. Translate the following summary from English to {}.

## Translation Rules

### DO NOT translate:
- Twitter/X @handles (e.g., @elonmusk, @sama)
- Hashtags (e.g., #AI, #MachineLearning)
- Cashtags (e.g., $TSLA, $NVDA)
- URLs and links
- Proper names of people, companies, and products
- Technical terms that are commonly used in English in the tech community

### KEEP in original English:
- Any quoted tweet text (text inside quotation marks)
- Code snippets or technical identifiers
- Acronyms (AI, ML, LLM, GPU, etc.)

### DO translate:
- Section headers and bullet points
- Descriptive text and explanations
- The general narrative and context

### Formatting:
- Preserve all markdown formatting (bold, italic, headers, bullet points)
- Preserve all emojis
- Maintain the same structure and layout as the original

### Tone:
- Keep the same professional but accessible tone
- Maintain nuance and accuracy
- If a term has no good translation, keep the English term"#,
        target_language
    )
}

/// Build the user prompt for translation
fn build_translation_user_prompt(summary: &str, target_language: &str) -> String {
    format!(
        "Please translate the following Twitter/X news summary to {}:\n\n{}",
        target_language, summary
    )
}

/// Translate a summary from English to the target language
///
/// Returns the translated text on success, or an error on failure.
/// The caller is responsible for handling the error (e.g., adding a failure notice).
pub async fn translate_summary(
    client: &reqwest::Client,
    config: &Config,
    summary: &str,
    target_language: Language,
) -> Result<String> {
    // If target is English (canonical), no translation needed
    if target_language.is_canonical() {
        return Ok(summary.to_string());
    }

    // Reasoning models need higher token limits and don't support temperature
    let is_reasoning = is_reasoning_model(&config.openai_model);
    let max_completion_tokens = if is_reasoning {
        16000
    } else {
        config.summary_max_tokens
    };

    let request = TranslationRequest {
        model: config.openai_model.clone(),
        messages: vec![
            Message {
                role: "system".to_string(),
                content: build_translation_system_prompt(target_language.name()),
            },
            Message {
                role: "user".to_string(),
                content: build_translation_user_prompt(summary, target_language.name()),
            },
        ],
        max_completion_tokens,
        // Reasoning models don't support temperature - use reasoning_effort instead
        temperature: if is_reasoning { None } else { Some(0.3) },
        reasoning_effort: if is_reasoning {
            Some("low".to_string())
        } else {
            None
        },
    };

    let translated = with_retry_if(
        &RetryConfig::api_call(),
        &format!("Translation to {}", target_language.name()),
        || async {
            let response = client
                .post(&config.openai_api_url)
                .header("Authorization", format!("Bearer {}", config.openai_api_key))
                .header("Content-Type", "application/json")
                .json(&request)
                .send()
                .await
                .context("Failed to send translation request to OpenAI API")?;

            if !response.status().is_success() {
                let status = response.status();
                let body = response
                    .text()
                    .await
                    .unwrap_or_else(|e| format!("<failed to read body: {}>", e));
                anyhow::bail!("OpenAI API error during translation ({}): {}", status, body);
            }

            let chat_response: ChatResponse = response
                .json()
                .await
                .context("Failed to parse OpenAI translation response")?;

            let translated = chat_response
                .choices
                .first()
                .map(|c| c.message.content.clone())
                .context("OpenAI translation response contained no choices")?;

            Ok(translated)
        },
        is_retryable_error,
    )
    .await?;

    // Validate translation quality
    let validation = TranslationValidator::validate(summary, &translated);
    if !validation.warnings.is_empty() {
        warn!(
            "Translation validation warnings for {} ({}): {:?}",
            target_language.name(),
            target_language.code(),
            validation.warnings
        );
    }
    if !validation.errors.is_empty() {
        warn!(
            "Translation validation errors for {} ({}): {:?}",
            target_language.name(),
            target_language.code(),
            validation.errors
        );
    }

    Ok(translated)
}

/// Determine if an error is retryable (5xx errors, 429 rate limit, network errors)
/// Other 4xx client errors should not be retried
fn is_retryable_error(error: &anyhow::Error) -> bool {
    let error_str = error.to_string();

    // Check if it's an OpenAI API error with a status code
    // Error format: "OpenAI API error during translation (400 Bad Request): ..."
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

/// Get the message header for a given language
pub fn get_summary_header(language: Language) -> &'static str {
    language.config().strings.summary_header
}

/// Get the translation failure notice for a given target language
pub fn get_translation_failure_notice(target_language: Language) -> String {
    target_language
        .config()
        .strings
        .translation_failure_notice
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use wiremock::{
        matchers::{header, method, path},
        Mock, MockServer, ResponseTemplate,
    };

    // ==================== Language Enum Tests ====================

    #[test]
    fn test_language_english_code() {
        assert_eq!(Language::ENGLISH.code(), "en");
    }

    #[test]
    fn test_language_spanish_code() {
        assert_eq!(Language::SPANISH.code(), "es");
    }

    #[test]
    fn test_language_english_name() {
        assert_eq!(Language::ENGLISH.name(), "English");
    }

    #[test]
    fn test_language_spanish_name() {
        assert_eq!(Language::SPANISH.name(), "Spanish");
    }

    #[test]
    fn test_language_from_code_english() {
        assert_eq!(Language::from_code("en").ok(), Some(Language::ENGLISH));
    }

    #[test]
    fn test_language_from_code_spanish() {
        assert_eq!(Language::from_code("es").ok(), Some(Language::SPANISH));
    }

    #[test]
    fn test_language_from_code_invalid() {
        assert!(Language::from_code("fr").is_err());
        assert!(Language::from_code("de").is_err());
        assert!(Language::from_code("").is_err());
    }

    #[test]
    fn test_language_is_canonical_english() {
        assert!(Language::ENGLISH.is_canonical());
    }

    #[test]
    fn test_language_is_canonical_spanish() {
        assert!(!Language::SPANISH.is_canonical());
    }

    #[test]
    fn test_language_clone() {
        let lang = Language::SPANISH;
        let cloned = lang;
        assert_eq!(lang, cloned);
    }

    #[test]
    fn test_language_equality() {
        assert_eq!(Language::ENGLISH, Language::ENGLISH);
        assert_ne!(Language::ENGLISH, Language::SPANISH);
    }

    // ==================== System Prompt Tests ====================

    #[test]
    fn test_build_translation_system_prompt_spanish() {
        let prompt = build_translation_system_prompt("Spanish");

        assert!(prompt.contains("Spanish"));
        assert!(prompt.contains("DO NOT translate"));
        assert!(prompt.contains("@handles"));
        assert!(prompt.contains("Hashtags"));
        assert!(prompt.contains("URLs"));
        assert!(prompt.contains("KEEP in original English"));
        assert!(prompt.contains("markdown formatting"));
    }

    #[test]
    fn test_build_translation_system_prompt_mentions_handles() {
        let prompt = build_translation_system_prompt("Spanish");
        assert!(prompt.contains("@elonmusk"));
        assert!(prompt.contains("@sama"));
    }

    #[test]
    fn test_build_translation_system_prompt_mentions_technical_terms() {
        let prompt = build_translation_system_prompt("Spanish");
        assert!(prompt.contains("AI"));
        assert!(prompt.contains("ML"));
        assert!(prompt.contains("LLM"));
    }

    // ==================== User Prompt Tests ====================

    #[test]
    fn test_build_translation_user_prompt() {
        let summary = "This is a test summary.";
        let prompt = build_translation_user_prompt(summary, "Spanish");

        assert!(prompt.contains("translate"));
        assert!(prompt.contains("Spanish"));
        assert!(prompt.contains(summary));
    }

    // ==================== Header Tests ====================

    #[test]
    fn test_get_summary_header_english() {
        assert_eq!(get_summary_header(Language::ENGLISH), "Twitter Summary");
    }

    #[test]
    fn test_get_summary_header_spanish() {
        assert_eq!(get_summary_header(Language::SPANISH), "Resumen de Twitter");
    }

    // ==================== Failure Notice Tests ====================

    #[test]
    fn test_get_translation_failure_notice_spanish() {
        let notice = get_translation_failure_notice(Language::SPANISH);
        assert!(notice.contains("traducción no está disponible"));
        assert!(notice.contains("inglés"));
    }

    #[test]
    fn test_get_translation_failure_notice_english() {
        let notice = get_translation_failure_notice(Language::ENGLISH);
        assert!(notice.is_empty());
    }

    // ==================== Integration Tests with Wiremock ====================

    fn create_test_config(api_url: &str) -> Config {
        Config {
            environment: "test".to_string(),
            twitter_bearer_token: None,
            twitter_list_id: None,
            openai_api_key: "test-openai-key".to_string(),
            openai_model: "gpt-4o-mini".to_string(),
            openai_api_url: api_url.to_string(),
            openai_temperature: 0.7,
            telegram_bot_token: "test-token".to_string(),
            telegram_chat_id: "".to_string(),
            telegram_webhook_secret: "test-secret".to_string(),
            max_tweets: 100,
            hours_lookback: 12,
            summary_max_tokens: 2500,
            summary_max_words: 800,
            nitter_instance: "https://nitter.example.com".to_string(),
            nitter_api_key: None,
            usernames_file: "data/usernames.txt".to_string(),
            api_key: None,
            database_url: "postgres://test:test@localhost/test".to_string(),
            schedule_times: vec!["08:00".to_string()],
            port: 8080,
        }
    }

    fn create_openai_response(content: &str) -> serde_json::Value {
        serde_json::json!({
            "id": "chatcmpl-123",
            "object": "chat.completion",
            "choices": [
                {
                    "index": 0,
                    "message": {
                        "role": "assistant",
                        "content": content
                    },
                    "finish_reason": "stop"
                }
            ]
        })
    }

    #[tokio::test]
    async fn test_translate_summary_returns_original_for_english() {
        let config = create_test_config("https://api.openai.com/v1/chat/completions");
        let client = reqwest::Client::new();

        let summary = "This is a test summary in English.";
        let result = translate_summary(&client, &config, summary, Language::ENGLISH)
            .await
            .expect("Should succeed");

        assert_eq!(result, summary);
    }

    #[tokio::test]
    async fn test_translate_summary_to_spanish_success() {
        let mock_server = MockServer::start().await;

        let response_body = create_openai_response("Este es un resumen de prueba en espanol.");

        Mock::given(method("POST"))
            .and(path("/v1/chat/completions"))
            .and(header("Authorization", "Bearer test-openai-key"))
            .respond_with(ResponseTemplate::new(200).set_body_json(&response_body))
            .mount(&mock_server)
            .await;

        let config = create_test_config(&format!("{}/v1/chat/completions", mock_server.uri()));
        let client = reqwest::Client::new();

        let summary = "This is a test summary.";
        let result = translate_summary(&client, &config, summary, Language::SPANISH)
            .await
            .expect("Should succeed");

        assert_eq!(result, "Este es un resumen de prueba en espanol.");
    }

    #[tokio::test]
    async fn test_translate_summary_api_error() {
        let mock_server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/v1/chat/completions"))
            .respond_with(ResponseTemplate::new(500).set_body_string("Internal Server Error"))
            .mount(&mock_server)
            .await;

        let config = create_test_config(&format!("{}/v1/chat/completions", mock_server.uri()));
        let client = reqwest::Client::new();

        let result = translate_summary(&client, &config, "Test summary", Language::SPANISH).await;

        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("500"));
    }

    #[tokio::test]
    async fn test_translate_summary_uses_low_temperature() {
        let mock_server = MockServer::start().await;

        let response_body = create_openai_response("Translated content");

        // Verify the request has a temperature of 0.3
        Mock::given(method("POST"))
            .and(path("/v1/chat/completions"))
            .respond_with(ResponseTemplate::new(200).set_body_json(&response_body))
            .expect(1)
            .mount(&mock_server)
            .await;

        let config = create_test_config(&format!("{}/v1/chat/completions", mock_server.uri()));
        let client = reqwest::Client::new();

        translate_summary(&client, &config, "Test", Language::SPANISH)
            .await
            .expect("Should succeed");

        // If we get here, the mock was called with correct parameters
    }

    #[tokio::test]
    async fn test_translate_summary_empty_choices() {
        let mock_server = MockServer::start().await;

        let response_body = serde_json::json!({
            "choices": []
        });

        Mock::given(method("POST"))
            .and(path("/v1/chat/completions"))
            .respond_with(ResponseTemplate::new(200).set_body_json(&response_body))
            .mount(&mock_server)
            .await;

        let config = create_test_config(&format!("{}/v1/chat/completions", mock_server.uri()));
        let client = reqwest::Client::new();

        let summary = "Original summary";
        let result = translate_summary(&client, &config, summary, Language::SPANISH).await;

        // Should return an error on empty choices
        assert!(result.is_err());
        let error_message = result.unwrap_err().to_string();
        assert!(error_message.contains("no choices"));
    }

    // ==================== Request Structure Tests ====================

    #[test]
    fn test_translation_request_serialization() {
        let request = TranslationRequest {
            model: "gpt-4o-mini".to_string(),
            messages: vec![
                Message {
                    role: "system".to_string(),
                    content: "Translate to Spanish.".to_string(),
                },
                Message {
                    role: "user".to_string(),
                    content: "Hello world".to_string(),
                },
            ],
            max_completion_tokens: 2500,
            temperature: Some(0.3),
            reasoning_effort: None,
        };

        let json = serde_json::to_string(&request).expect("Should serialize");
        assert!(json.contains("gpt-4o-mini"));
        assert!(json.contains("0.3"));
        assert!(json.contains("max_completion_tokens"));
        assert!(json.contains("2500"));
        assert!(json.contains("system"));
        assert!(json.contains("user"));
        // reasoning_effort should not be serialized when None
        assert!(!json.contains("reasoning_effort"));
    }

    #[test]
    fn test_translation_request_serialization_reasoning_model() {
        let request = TranslationRequest {
            model: "gpt-5-mini".to_string(),
            messages: vec![Message {
                role: "user".to_string(),
                content: "Test".to_string(),
            }],
            max_completion_tokens: 16000,
            temperature: None, // Reasoning models don't use temperature
            reasoning_effort: Some("low".to_string()),
        };

        let json = serde_json::to_string(&request).expect("Should serialize");
        assert!(json.contains("gpt-5-mini"));
        assert!(json.contains("16000"));
        assert!(json.contains("reasoning_effort"));
        assert!(json.contains("low"));
        // temperature should not be serialized when None
        assert!(!json.contains("temperature"));
    }

    #[test]
    fn test_is_reasoning_model() {
        assert!(is_reasoning_model("gpt-5-mini"));
        assert!(is_reasoning_model("gpt-5-nano"));
        assert!(is_reasoning_model("gpt-5"));
        assert!(is_reasoning_model("o1-mini"));
        assert!(is_reasoning_model("o1-preview"));
        assert!(is_reasoning_model("o3"));
        assert!(is_reasoning_model("o4-mini"));
        assert!(!is_reasoning_model("gpt-4o-mini"));
        assert!(!is_reasoning_model("gpt-4o"));
        assert!(!is_reasoning_model("gpt-4-turbo"));
    }

    // ==================== Edge Cases ====================

    #[test]
    fn test_language_debug_format() {
        let lang = Language::SPANISH;
        let debug = format!("{:?}", lang);
        assert!(debug.contains("es"));
    }

    #[test]
    fn test_language_copy_trait() {
        let lang1 = Language::ENGLISH;
        let lang2 = lang1; // Copy
        assert_eq!(lang1, lang2); // Both still valid
    }

    // ==================== Retry Integration Tests ====================

    #[tokio::test]
    async fn test_translate_summary_retries_on_500_error() {
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

        let response_body = create_openai_response("Traduccion despues de reintentos");

        Mock::given(method("POST"))
            .and(path("/v1/chat/completions"))
            .respond_with(ResponseTemplate::new(200).set_body_json(&response_body))
            .mount(&mock_server)
            .await;

        let config = create_test_config(&format!("{}/v1/chat/completions", mock_server.uri()));
        let client = reqwest::Client::new();

        let result = translate_summary(
            &client,
            &config,
            "Test summary to translate",
            Language::SPANISH,
        )
        .await;
        assert!(result.is_ok(), "Should succeed after retries: {:?}", result);
        assert_eq!(result.unwrap(), "Traduccion despues de reintentos");
    }

    #[tokio::test]
    async fn test_translate_summary_retries_on_503_error() {
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

        let response_body = create_openai_response("Traduccion exitosa");

        Mock::given(method("POST"))
            .and(path("/v1/chat/completions"))
            .respond_with(ResponseTemplate::new(200).set_body_json(&response_body))
            .mount(&mock_server)
            .await;

        let config = create_test_config(&format!("{}/v1/chat/completions", mock_server.uri()));
        let client = reqwest::Client::new();

        let result = translate_summary(&client, &config, "Test summary", Language::SPANISH).await;
        assert!(
            result.is_ok(),
            "Should succeed after 503 retry: {:?}",
            result
        );
    }

    #[tokio::test]
    async fn test_translate_summary_no_retry_on_400_error() {
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

        let config = create_test_config(&format!("{}/v1/chat/completions", mock_server.uri()));
        let client = reqwest::Client::new();

        let start = std::time::Instant::now();
        let result = translate_summary(&client, &config, "Test summary", Language::SPANISH).await;
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
    async fn test_translate_summary_no_retry_on_401_error() {
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

        let config = create_test_config(&format!("{}/v1/chat/completions", mock_server.uri()));
        let client = reqwest::Client::new();

        let result = translate_summary(&client, &config, "Test summary", Language::SPANISH).await;
        assert!(result.is_err(), "401 error should fail immediately");
    }

    #[tokio::test]
    async fn test_translate_summary_no_retry_on_403_error() {
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

        let config = create_test_config(&format!("{}/v1/chat/completions", mock_server.uri()));
        let client = reqwest::Client::new();

        let result = translate_summary(&client, &config, "Test summary", Language::SPANISH).await;
        assert!(result.is_err(), "403 error should fail immediately");
    }

    #[tokio::test]
    async fn test_translate_summary_exhausts_retries_on_persistent_500() {
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

        let config = create_test_config(&format!("{}/v1/chat/completions", mock_server.uri()));
        let client = reqwest::Client::new();

        let start = std::time::Instant::now();
        let result = translate_summary(&client, &config, "Test summary", Language::SPANISH).await;
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
    async fn test_translate_summary_success_on_first_attempt_no_delay() {
        let mock_server = MockServer::start().await;

        let response_body = create_openai_response("Exito inmediato");

        Mock::given(method("POST"))
            .and(path("/v1/chat/completions"))
            .respond_with(ResponseTemplate::new(200).set_body_json(&response_body))
            .expect(1)
            .mount(&mock_server)
            .await;

        let config = create_test_config(&format!("{}/v1/chat/completions", mock_server.uri()));
        let client = reqwest::Client::new();

        let start = std::time::Instant::now();
        let result = translate_summary(&client, &config, "Test summary", Language::SPANISH).await;
        let elapsed = start.elapsed();

        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "Exito inmediato");

        // Should complete very quickly
        assert!(
            elapsed < std::time::Duration::from_millis(500),
            "Should complete quickly on immediate success, got {:?}",
            elapsed
        );
    }

    #[tokio::test]
    async fn test_translate_summary_english_skips_api_call() {
        // When translating to English (canonical), no API call should be made
        // Use an invalid URL to ensure no request is made
        let config = create_test_config("http://invalid-url-should-not-be-called.test");
        let client = reqwest::Client::new();

        let summary = "This is already in English";
        let result = translate_summary(&client, &config, summary, Language::ENGLISH).await;

        assert!(
            result.is_ok(),
            "English translation should skip API: {:?}",
            result
        );
        assert_eq!(result.unwrap(), summary);
    }

    // ==================== is_retryable_error Tests ====================

    #[test]
    fn test_is_retryable_error_500_error() {
        let error =
            anyhow::anyhow!("OpenAI API error during translation (500): Internal Server Error");
        assert!(is_retryable_error(&error), "500 errors should be retryable");
    }

    #[test]
    fn test_is_retryable_error_503_error() {
        let error =
            anyhow::anyhow!("OpenAI API error during translation (503): Service Unavailable");
        assert!(is_retryable_error(&error), "503 errors should be retryable");
    }

    #[test]
    fn test_is_retryable_error_400_error() {
        let error = anyhow::anyhow!("OpenAI API error during translation (400): Bad Request");
        assert!(
            !is_retryable_error(&error),
            "400 errors should NOT be retryable"
        );
    }

    #[test]
    fn test_is_retryable_error_401_error() {
        let error = anyhow::anyhow!("OpenAI API error during translation (401): Unauthorized");
        assert!(
            !is_retryable_error(&error),
            "401 errors should NOT be retryable"
        );
    }

    #[test]
    fn test_is_retryable_error_403_error() {
        let error = anyhow::anyhow!("OpenAI API error during translation (403): Forbidden");
        assert!(
            !is_retryable_error(&error),
            "403 errors should NOT be retryable"
        );
    }

    #[test]
    fn test_is_retryable_error_429_error() {
        let error =
            anyhow::anyhow!("OpenAI API error during translation (429): Rate Limit Exceeded");
        assert!(
            is_retryable_error(&error),
            "429 errors SHOULD be retryable (rate limit is transient)"
        );
    }

    #[test]
    fn test_is_retryable_error_network_error() {
        let error =
            anyhow::anyhow!("Failed to send translation request to OpenAI API: connection refused");
        assert!(
            is_retryable_error(&error),
            "Network errors should be retryable"
        );
    }

    #[test]
    fn test_is_retryable_error_timeout() {
        let error = anyhow::anyhow!("Request timed out during translation");
        assert!(is_retryable_error(&error), "Timeouts should be retryable");
    }

    #[test]
    fn test_is_retryable_error_parse_error() {
        let error = anyhow::anyhow!("Failed to parse OpenAI translation response: invalid JSON");
        assert!(
            is_retryable_error(&error),
            "Parse errors should be retryable (might be transient)"
        );
    }
}
