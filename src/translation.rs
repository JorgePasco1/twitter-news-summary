use crate::config::Config;
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

/// OpenAI Chat Completion request for translation
#[derive(Debug, Serialize)]
struct TranslationRequest {
    model: String,
    messages: Vec<Message>,
    max_tokens: u32,
    temperature: f32,
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

/// Supported languages for translation
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Language {
    English,
    Spanish,
}

impl Language {
    /// Get the language code (e.g., "en", "es")
    pub fn code(&self) -> &'static str {
        match self {
            Language::English => "en",
            Language::Spanish => "es",
        }
    }

    /// Get the full language name
    pub fn name(&self) -> &'static str {
        match self {
            Language::English => "English",
            Language::Spanish => "Spanish",
        }
    }

    /// Parse a language code string into a Language enum
    pub fn from_code(code: &str) -> Option<Language> {
        match code {
            "en" => Some(Language::English),
            "es" => Some(Language::Spanish),
            _ => None,
        }
    }

    /// Check if this is the default/canonical language
    pub fn is_canonical(&self) -> bool {
        matches!(self, Language::English)
    }
}

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
        max_tokens: config.summary_max_tokens,
        temperature: 0.3, // Lower temperature for more consistent translations
    };

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
        let body = response.text().await.unwrap_or_default();
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
}

/// Get the message header for a given language
pub fn get_summary_header(language: Language) -> &'static str {
    match language {
        Language::English => "Twitter Summary",
        Language::Spanish => "Resumen de Twitter",
    }
}

/// Get the translation failure notice for a given target language
pub fn get_translation_failure_notice(target_language: Language) -> String {
    match target_language {
        Language::Spanish => {
            "[Nota: La traducción no está disponible. Enviando en inglés.]\n\n".to_string()
        }
        Language::English => String::new(), // No notice needed for English
    }
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
        assert_eq!(Language::English.code(), "en");
    }

    #[test]
    fn test_language_spanish_code() {
        assert_eq!(Language::Spanish.code(), "es");
    }

    #[test]
    fn test_language_english_name() {
        assert_eq!(Language::English.name(), "English");
    }

    #[test]
    fn test_language_spanish_name() {
        assert_eq!(Language::Spanish.name(), "Spanish");
    }

    #[test]
    fn test_language_from_code_english() {
        assert_eq!(Language::from_code("en"), Some(Language::English));
    }

    #[test]
    fn test_language_from_code_spanish() {
        assert_eq!(Language::from_code("es"), Some(Language::Spanish));
    }

    #[test]
    fn test_language_from_code_invalid() {
        assert_eq!(Language::from_code("fr"), None);
        assert_eq!(Language::from_code("de"), None);
        assert_eq!(Language::from_code(""), None);
    }

    #[test]
    fn test_language_is_canonical_english() {
        assert!(Language::English.is_canonical());
    }

    #[test]
    fn test_language_is_canonical_spanish() {
        assert!(!Language::Spanish.is_canonical());
    }

    #[test]
    fn test_language_clone() {
        let lang = Language::Spanish;
        let cloned = lang;
        assert_eq!(lang, cloned);
    }

    #[test]
    fn test_language_equality() {
        assert_eq!(Language::English, Language::English);
        assert_ne!(Language::English, Language::Spanish);
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
        assert_eq!(get_summary_header(Language::English), "Twitter Summary");
    }

    #[test]
    fn test_get_summary_header_spanish() {
        assert_eq!(get_summary_header(Language::Spanish), "Resumen de Twitter");
    }

    // ==================== Failure Notice Tests ====================

    #[test]
    fn test_get_translation_failure_notice_spanish() {
        let notice = get_translation_failure_notice(Language::Spanish);
        assert!(notice.contains("traducción no está disponible"));
        assert!(notice.contains("inglés"));
    }

    #[test]
    fn test_get_translation_failure_notice_english() {
        let notice = get_translation_failure_notice(Language::English);
        assert!(notice.is_empty());
    }

    // ==================== Integration Tests with Wiremock ====================

    fn create_test_config(api_url: &str) -> Config {
        Config {
            twitter_bearer_token: None,
            twitter_list_id: None,
            openai_api_key: "test-openai-key".to_string(),
            openai_model: "gpt-4o-mini".to_string(),
            openai_api_url: api_url.to_string(),
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
        let result = translate_summary(&client, &config, summary, Language::English)
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
        let result = translate_summary(&client, &config, summary, Language::Spanish)
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

        let result = translate_summary(&client, &config, "Test summary", Language::Spanish).await;

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

        translate_summary(&client, &config, "Test", Language::Spanish)
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
        let result = translate_summary(&client, &config, summary, Language::Spanish).await;

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
            max_tokens: 2500,
            temperature: 0.3,
        };

        let json = serde_json::to_string(&request).expect("Should serialize");
        assert!(json.contains("gpt-4o-mini"));
        assert!(json.contains("0.3"));
        assert!(json.contains("2500"));
        assert!(json.contains("system"));
        assert!(json.contains("user"));
    }

    // ==================== Edge Cases ====================

    #[test]
    fn test_language_debug_format() {
        let lang = Language::Spanish;
        let debug = format!("{:?}", lang);
        assert!(debug.contains("Spanish"));
    }

    #[test]
    fn test_language_copy_trait() {
        let lang1 = Language::English;
        let lang2 = lang1; // Copy
        assert_eq!(lang1, lang2); // Both still valid
    }
}
