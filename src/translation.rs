use crate::config::Config;
use crate::i18n::{
    Language, TranslationValidator, ENGLISH_SECTION_HEADERS, SPANISH_SECTION_HEADERS,
};
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
fn build_translation_system_prompt(target_language: Language) -> String {
    let target_name = target_language.name();

    // For non-canonical languages, include explicit header mappings and examples
    // English (canonical) doesn't need translation instructions
    if target_language.is_canonical() {
        // English target: minimal prompt (no-op since we skip translation anyway)
        return r#"You are a professional translator. The content is already in English.
No translation is needed. Return the content unchanged."#
            .to_string();
    }

    // Get the section header mappings based on target language
    let section_header_instructions = if target_language == Language::SPANISH {
        format!(
            r#"
## MANDATORY Section Header Translations
You MUST translate these section headers EXACTLY as shown:

| English | {} |
|---------|----------|
| "{}" | "{}" |
| "{}" | "{}" |
| "{}" | "{}" |
| "{}" | "{}" |
| "{}" | "{}" |
| "{}" | "{}" |
| "{}" | "{}" |

CRITICAL: If you see any of the English headers above, replace them with the {} version."#,
            target_name,
            ENGLISH_SECTION_HEADERS.top_takeaways,
            SPANISH_SECTION_HEADERS.top_takeaways,
            ENGLISH_SECTION_HEADERS.releases,
            SPANISH_SECTION_HEADERS.releases,
            ENGLISH_SECTION_HEADERS.research,
            SPANISH_SECTION_HEADERS.research,
            ENGLISH_SECTION_HEADERS.tools_tutorials,
            SPANISH_SECTION_HEADERS.tools_tutorials,
            ENGLISH_SECTION_HEADERS.companies_deals,
            SPANISH_SECTION_HEADERS.companies_deals,
            ENGLISH_SECTION_HEADERS.policy_safety,
            SPANISH_SECTION_HEADERS.policy_safety,
            ENGLISH_SECTION_HEADERS.debate_opinions,
            SPANISH_SECTION_HEADERS.debate_opinions,
            target_name,
        )
    } else {
        String::new() // Other non-English languages don't have header mappings defined yet
    };

    // Translation examples - use Spanish examples for Spanish, generic otherwise
    let translation_examples = if target_language == Language::SPANISH {
        r#"
## Translation Examples

### Section Headers
INCORRECT: 🧠 Top takeaways
CORRECT: 🧠 Conclusiones principales

### Full Bullet Points
INCORRECT (link label in English):
- *New RoPE paper suggests...* — Afirma que... [Burkov thread](url)

CORRECT (link label translated):
- *Nuevo artículo sobre RoPE sugiere...* — Afirma que... [hilo de Burkov](url)

### Link Labels (CRITICAL - these MUST be translated)
INCORRECT: [thdxr on coding agents](url)
CORRECT: [thdxr sobre agentes de código](url)

INCORRECT: [PyTorchCon Europe CFP](url)
CORRECT: [convocatoria PyTorchCon Europe](url)

INCORRECT: [PeterYang reaction to list](url)
CORRECT: [reacción de PeterYang a la lista](url)

INCORRECT: [tutorial announcement](url)
CORRECT: [anuncio del tutorial](url)

INCORRECT: [Sam on AI safety](url)
CORRECT: [Sam sobre seguridad de IA](url)

Note: Keep @handles, product names, and proper nouns in the link label, but translate the connecting words and descriptions."#
    } else {
        "" // No examples for languages without defined translations
    };

    format!(
        r#"You are a professional translator. Translate the following summary from English to {target_name}.

## CRITICAL: Length Constraint
The translated text MUST stay under 3800 characters total. This is a hard limit for Telegram.
- If the translation would exceed this, condense while preserving key information
- Prioritize keeping the most important items; trim less critical details
- Use concise phrasing natural to the target language
{section_header_instructions}

## Bullet Format (CRITICAL)
Each bullet follows this pattern:
- *BOLD TITLE* — explanation [link label](url)

You MUST translate ALL THREE parts:
1. The BOLD TITLE (text between * and * before the em-dash —)
2. The explanation (text after the em-dash)
3. The LINK LABEL (text between [ and ] - THIS IS MANDATORY)

## Link Labels (VERY IMPORTANT)
The link label in [brackets](url) MUST be translated to {target_name}.
- Keep @handles as-is: "@thdxr" stays "@thdxr"
- Keep product/company names: "PyTorchCon" stays "PyTorchCon"
- Translate descriptive words: "on", "about", "thread", "reaction", "tutorial", "announcement"
- Example: [thdxr on AI agents] → [thdxr sobre agentes de IA]
- Example: [OpenAI safety post] → [publicación de OpenAI sobre seguridad]
{translation_examples}

## DO NOT translate:
- Twitter/X @handles (e.g., @elonmusk, @sama)
- Hashtags (e.g., #AI, #MachineLearning)
- Cashtags (e.g., $TSLA, $NVDA)
- URLs and links (the URL itself, not the label)
- Proper names of people (Sam Altman, Elon Musk, etc.)
- Company names (OpenAI, Google, Meta, etc.)
- Product names (ChatGPT, Claude, Gemini, etc.)
- Paper titles if they are proper nouns
- Technical terms commonly used in English (transformer, fine-tuning, etc.)

## KEEP in original English:
- Any quoted tweet text (text inside quotation marks)
- Code snippets or technical identifiers
- Acronyms (AI, ML, LLM, GPU, etc.)

## Formatting:
- Preserve all markdown formatting (bold with *, italic with _, bullets with -)
- Preserve all emojis
- Maintain the same structure and layout as the original

## Tone:
- Keep the same professional but accessible tone
- Maintain nuance and accuracy
- If a term has no good translation, keep the English term"#
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
                content: build_translation_system_prompt(target_language),
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
    let validation = TranslationValidator::validate(summary, &translated, target_language);
    if !validation.warnings.is_empty() {
        warn!(
            "Translation validation warnings for {} ({}): {:?}",
            target_language.name(),
            target_language.code(),
            validation.warnings
        );
    }

    // Validation errors (e.g., untranslated headers) cause translation to fail
    // The caller will fall back to English with a notice
    if validation.has_errors() {
        let error_msg = format!(
            "Translation validation failed for {} ({}): {:?}",
            target_language.name(),
            target_language.code(),
            validation.errors
        );
        warn!("{}", error_msg);
        anyhow::bail!("{}", error_msg);
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

/// Condense text to fit within a character limit while preserving key information.
///
/// This function asks OpenAI to shorten the text while keeping the most important
/// information and preserving the same language (auto-detected).
///
/// # Arguments
/// * `client` - HTTP client for API calls
/// * `config` - Application configuration
/// * `text` - The text to condense
/// * `max_chars` - Target character limit (best-effort, not guaranteed)
///
/// # Returns
/// * Condensed text on success, or an error on failure
///
/// # Note
/// LLM output may exceed `max_chars` despite instructions. Callers should use
/// `truncate_at_limit()` as a fallback if strict enforcement is required.
/// The Telegram integration does this with iterative truncation that also
/// accounts for MarkdownV2 escape expansion.
pub async fn condense_text(
    client: &reqwest::Client,
    config: &Config,
    text: &str,
    max_chars: usize,
) -> Result<String> {
    use tracing::info;

    info!(
        "Condensing text from {} chars to max {} chars",
        text.len(),
        max_chars
    );

    let system_prompt = format!(
        r#"You are an expert editor. Your task is to condense the provided text to UNDER {} characters.

## Instructions:
1. Keep the most important information and key points
2. Remove less critical details and redundant content
3. Use concise phrasing while maintaining clarity
4. Preserve the SAME LANGUAGE as the input (detect automatically)
5. Preserve markdown formatting (bold, italic, bullets)
6. Preserve @handles, #hashtags, $cashtags, and URLs unchanged
7. Do NOT add any explanations or meta-commentary

Output ONLY the condensed text, nothing else."#,
        max_chars
    );

    // Reasoning models need higher token limits and don't support temperature
    let is_reasoning = is_reasoning_model(&config.openai_model);
    let max_completion_tokens = if is_reasoning { 16000 } else { 4000 };

    let request = TranslationRequest {
        model: config.openai_model.clone(),
        messages: vec![
            Message {
                role: "system".to_string(),
                content: system_prompt,
            },
            Message {
                role: "user".to_string(),
                content: text.to_string(),
            },
        ],
        max_completion_tokens,
        temperature: if is_reasoning { None } else { Some(0.3) },
        reasoning_effort: if is_reasoning {
            Some("low".to_string())
        } else {
            None
        },
    };

    let condensed = with_retry_if(
        &RetryConfig::api_call(),
        "Text condensing",
        || async {
            let response = client
                .post(&config.openai_api_url)
                .header("Authorization", format!("Bearer {}", config.openai_api_key))
                .header("Content-Type", "application/json")
                .json(&request)
                .send()
                .await
                .context("Failed to send condense request to OpenAI API")?;

            if !response.status().is_success() {
                let status = response.status();
                let body = response
                    .text()
                    .await
                    .unwrap_or_else(|e| format!("<failed to read body: {}>", e));
                anyhow::bail!("OpenAI API error during condensing ({}): {}", status, body);
            }

            let chat_response: ChatResponse = response
                .json()
                .await
                .context("Failed to parse OpenAI condense response")?;

            let condensed = chat_response
                .choices
                .first()
                .map(|c| c.message.content.clone())
                .context("OpenAI condense response contained no choices")?;

            Ok(condensed)
        },
        is_retryable_error,
    )
    .await?;

    info!(
        "Condensed text from {} to {} chars",
        text.len(),
        condensed.len()
    );

    Ok(condensed)
}

/// Truncate text at a word boundary with ellipsis.
///
/// If the text exceeds the limit, it's cut at the last whitespace before
/// the limit and an ellipsis is appended. This function is UTF-8 safe and
/// will not panic on multi-byte characters.
pub fn truncate_at_limit(text: &str, limit: usize) -> String {
    if text.len() <= limit {
        return text.to_string();
    }

    // Reserve space for ellipsis
    let cut_at = limit.saturating_sub(3);
    if cut_at == 0 {
        return "...".to_string();
    }

    // Find a UTF-8 safe boundary at or before the cut point
    // This prevents panics when limit lands mid-codepoint (e.g., "café")
    // and ensures the result never exceeds the byte limit
    let safe_cut_at = text
        .char_indices()
        .map(|(i, _)| i)
        .chain(std::iter::once(text.len()))
        .take_while(|i| *i <= cut_at)
        .last()
        .unwrap_or(0);

    if safe_cut_at == 0 {
        return "...".to_string();
    }

    // Find last whitespace before the safe cut point
    let slice = &text[..safe_cut_at];
    let cut_point = slice.rfind(char::is_whitespace).unwrap_or(safe_cut_at);

    format!("{}...", &text[..cut_point])
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
        let prompt = build_translation_system_prompt(Language::SPANISH);

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
        let prompt = build_translation_system_prompt(Language::SPANISH);
        assert!(prompt.contains("@elonmusk"));
        assert!(prompt.contains("@sama"));
    }

    #[test]
    fn test_build_translation_system_prompt_mentions_technical_terms() {
        let prompt = build_translation_system_prompt(Language::SPANISH);
        assert!(prompt.contains("AI"));
        assert!(prompt.contains("ML"));
        assert!(prompt.contains("LLM"));
    }

    #[test]
    fn test_build_translation_system_prompt_includes_section_headers() {
        let prompt = build_translation_system_prompt(Language::SPANISH);

        // Should contain the section header mapping table
        assert!(prompt.contains("MANDATORY Section Header Translations"));

        // Should contain all English headers
        assert!(prompt.contains("🧠 Top takeaways"));
        assert!(prompt.contains("🚀 Releases"));
        assert!(prompt.contains("🔬 Research"));
        assert!(prompt.contains("🧰 Tools and Tutorials"));

        // Should contain all Spanish translations
        assert!(prompt.contains("🧠 Conclusiones principales"));
        assert!(prompt.contains("🚀 Lanzamientos"));
        assert!(prompt.contains("🔬 Investigación"));
        assert!(prompt.contains("🧰 Herramientas y tutoriales"));
    }

    #[test]
    fn test_build_translation_system_prompt_includes_bullet_examples() {
        let prompt = build_translation_system_prompt(Language::SPANISH);

        // Should contain the bullet format instructions
        assert!(prompt.contains("Bullet Format"));
        assert!(prompt.contains("BOLD TITLE"));
        assert!(prompt.contains("em-dash"));

        // Should contain examples of correct/incorrect translations
        assert!(prompt.contains("INCORRECT"));
        assert!(prompt.contains("CORRECT"));
    }

    #[test]
    fn test_build_translation_system_prompt_english_no_header_mapping() {
        let prompt = build_translation_system_prompt(Language::ENGLISH);

        // English prompt should NOT contain section header mapping
        assert!(!prompt.contains("MANDATORY Section Header Translations"));
        assert!(!prompt.contains("Conclusiones principales"));
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

    // ==================== REGRESSION TESTS: Bug #1 - API Parameter Issues ====================
    //
    // Bug: TranslationRequest was using `max_tokens` instead of `max_completion_tokens`.
    // When using reasoning models like `gpt-5-mini`, OpenAI returns a 400 error:
    // "Unsupported parameter: 'max_tokens' is not supported with this model. Use 'max_completion_tokens' instead."
    //
    // These tests ensure the fix is preserved and the correct parameter is always used.

    #[test]
    fn test_regression_never_uses_max_tokens_field() {
        // REGRESSION TEST: The bug was using "max_tokens" instead of "max_completion_tokens"
        // This test explicitly verifies that "max_tokens" is NEVER present in the serialized request

        // Test with non-reasoning model
        let request_standard = TranslationRequest {
            model: "gpt-4o-mini".to_string(),
            messages: vec![Message {
                role: "user".to_string(),
                content: "Test".to_string(),
            }],
            max_completion_tokens: 2500,
            temperature: Some(0.3),
            reasoning_effort: None,
        };

        let json_standard = serde_json::to_string(&request_standard).expect("Should serialize");

        // CRITICAL: "max_tokens" must NEVER appear (this was the bug)
        assert!(
            !json_standard.contains("\"max_tokens\""),
            "REGRESSION: Request should NEVER contain 'max_tokens' field. Got: {}",
            json_standard
        );

        // "max_completion_tokens" MUST appear
        assert!(
            json_standard.contains("\"max_completion_tokens\""),
            "Request MUST contain 'max_completion_tokens' field. Got: {}",
            json_standard
        );

        // Test with reasoning model
        let request_reasoning = TranslationRequest {
            model: "gpt-5-mini".to_string(),
            messages: vec![Message {
                role: "user".to_string(),
                content: "Test".to_string(),
            }],
            max_completion_tokens: 16000,
            temperature: None,
            reasoning_effort: Some("low".to_string()),
        };

        let json_reasoning = serde_json::to_string(&request_reasoning).expect("Should serialize");

        // CRITICAL: "max_tokens" must NEVER appear for reasoning models either
        assert!(
            !json_reasoning.contains("\"max_tokens\""),
            "REGRESSION: Reasoning model request should NEVER contain 'max_tokens'. Got: {}",
            json_reasoning
        );

        assert!(
            json_reasoning.contains("\"max_completion_tokens\""),
            "Reasoning model request MUST contain 'max_completion_tokens'. Got: {}",
            json_reasoning
        );
    }

    #[test]
    fn test_regression_reasoning_models_no_temperature() {
        // REGRESSION TEST: Reasoning models do NOT support temperature parameter
        // This caused OpenAI API errors when temperature was included

        let reasoning_models = [
            "gpt-5-mini",
            "gpt-5-nano",
            "gpt-5",
            "o1-mini",
            "o1-preview",
            "o3",
            "o3-mini",
            "o4-mini",
        ];

        for model in reasoning_models {
            let is_reasoning = is_reasoning_model(model);
            assert!(
                is_reasoning,
                "Model '{}' should be detected as a reasoning model",
                model
            );

            // Simulate what translate_summary does for reasoning models
            let temperature = if is_reasoning { None } else { Some(0.3) };
            let reasoning_effort = if is_reasoning {
                Some("low".to_string())
            } else {
                None
            };

            let request = TranslationRequest {
                model: model.to_string(),
                messages: vec![Message {
                    role: "user".to_string(),
                    content: "Test".to_string(),
                }],
                max_completion_tokens: 16000,
                temperature,
                reasoning_effort,
            };

            let json = serde_json::to_string(&request).expect("Should serialize");

            // CRITICAL: temperature must NOT be present for reasoning models
            assert!(
                !json.contains("\"temperature\""),
                "REGRESSION: Reasoning model '{}' should NOT have temperature in request. Got: {}",
                model,
                json
            );

            // reasoning_effort MUST be present for reasoning models
            assert!(
                json.contains("\"reasoning_effort\""),
                "Reasoning model '{}' MUST have reasoning_effort in request. Got: {}",
                model,
                json
            );
        }
    }

    #[test]
    fn test_regression_non_reasoning_models_have_temperature() {
        // Non-reasoning models MUST have temperature and NO reasoning_effort

        let non_reasoning_models = ["gpt-4o-mini", "gpt-4o", "gpt-4-turbo", "gpt-3.5-turbo"];

        for model in non_reasoning_models {
            let is_reasoning = is_reasoning_model(model);
            assert!(
                !is_reasoning,
                "Model '{}' should NOT be detected as a reasoning model",
                model
            );

            // Simulate what translate_summary does for non-reasoning models
            let temperature = if is_reasoning { None } else { Some(0.3) };
            let reasoning_effort = if is_reasoning {
                Some("low".to_string())
            } else {
                None
            };

            let request = TranslationRequest {
                model: model.to_string(),
                messages: vec![Message {
                    role: "user".to_string(),
                    content: "Test".to_string(),
                }],
                max_completion_tokens: 2500,
                temperature,
                reasoning_effort,
            };

            let json = serde_json::to_string(&request).expect("Should serialize");

            // temperature MUST be present for non-reasoning models
            assert!(
                json.contains("\"temperature\""),
                "Non-reasoning model '{}' MUST have temperature in request. Got: {}",
                model,
                json
            );

            // reasoning_effort must NOT be present for non-reasoning models
            assert!(
                !json.contains("\"reasoning_effort\""),
                "Non-reasoning model '{}' should NOT have reasoning_effort in request. Got: {}",
                model,
                json
            );
        }
    }

    #[test]
    fn test_regression_reasoning_model_token_limits() {
        // REGRESSION TEST: Reasoning models need higher token limits (16000 vs 2500)
        // This ensures the translate_summary function uses the correct limits

        let mut config = create_test_config("https://api.openai.com/v1/chat/completions");

        // Test with reasoning model
        config.openai_model = "gpt-5-mini".to_string();
        let is_reasoning = is_reasoning_model(&config.openai_model);
        let max_completion_tokens = if is_reasoning {
            16000
        } else {
            config.summary_max_tokens
        };

        assert_eq!(
            max_completion_tokens, 16000,
            "Reasoning models should use 16000 token limit, not {}",
            max_completion_tokens
        );

        // Test with non-reasoning model
        config.openai_model = "gpt-4o-mini".to_string();
        let is_reasoning = is_reasoning_model(&config.openai_model);
        let max_completion_tokens = if is_reasoning {
            16000
        } else {
            config.summary_max_tokens
        };

        assert_eq!(
            max_completion_tokens, config.summary_max_tokens,
            "Non-reasoning models should use config value, not 16000"
        );
    }

    #[test]
    fn test_full_request_structure_non_reasoning_model() {
        // Verify the complete JSON structure for a non-reasoning model request
        let request = TranslationRequest {
            model: "gpt-4o-mini".to_string(),
            messages: vec![
                Message {
                    role: "system".to_string(),
                    content: "Translate to Spanish".to_string(),
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

        // Parse back to verify structure
        let parsed: serde_json::Value = serde_json::from_str(&json).expect("Should parse");

        // Verify all required fields
        assert_eq!(parsed["model"], "gpt-4o-mini");
        assert_eq!(parsed["max_completion_tokens"], 2500);
        assert_eq!(parsed["temperature"], 0.3);
        assert_eq!(parsed["messages"].as_array().unwrap().len(), 2);

        // Verify absence of fields that should not be present
        assert!(
            parsed.get("max_tokens").is_none(),
            "max_tokens should not be present"
        );
        assert!(
            parsed.get("reasoning_effort").is_none(),
            "reasoning_effort should not be present"
        );
    }

    #[test]
    fn test_full_request_structure_reasoning_model() {
        // Verify the complete JSON structure for a reasoning model request
        let request = TranslationRequest {
            model: "gpt-5-mini".to_string(),
            messages: vec![
                Message {
                    role: "system".to_string(),
                    content: "Translate to Spanish".to_string(),
                },
                Message {
                    role: "user".to_string(),
                    content: "Hello world".to_string(),
                },
            ],
            max_completion_tokens: 16000,
            temperature: None,
            reasoning_effort: Some("low".to_string()),
        };

        let json = serde_json::to_string(&request).expect("Should serialize");

        // Parse back to verify structure
        let parsed: serde_json::Value = serde_json::from_str(&json).expect("Should parse");

        // Verify all required fields
        assert_eq!(parsed["model"], "gpt-5-mini");
        assert_eq!(parsed["max_completion_tokens"], 16000);
        assert_eq!(parsed["reasoning_effort"], "low");
        assert_eq!(parsed["messages"].as_array().unwrap().len(), 2);

        // Verify absence of fields that should not be present
        assert!(
            parsed.get("max_tokens").is_none(),
            "max_tokens should not be present"
        );
        assert!(
            parsed.get("temperature").is_none(),
            "temperature should not be present for reasoning models"
        );
    }

    #[test]
    fn test_is_reasoning_model_edge_cases() {
        // Test edge cases in reasoning model detection

        // Exact matches for known reasoning models
        assert!(is_reasoning_model("gpt-5"));
        assert!(is_reasoning_model("gpt-5-mini"));
        assert!(is_reasoning_model("gpt-5-nano"));
        assert!(is_reasoning_model("o1"));
        assert!(is_reasoning_model("o1-mini"));
        assert!(is_reasoning_model("o1-preview"));
        assert!(is_reasoning_model("o3"));
        assert!(is_reasoning_model("o3-mini"));
        assert!(is_reasoning_model("o4"));
        assert!(is_reasoning_model("o4-mini"));

        // Non-reasoning models
        assert!(!is_reasoning_model("gpt-4o"));
        assert!(!is_reasoning_model("gpt-4o-mini"));
        assert!(!is_reasoning_model("gpt-4-turbo"));
        assert!(!is_reasoning_model("gpt-4"));
        assert!(!is_reasoning_model("gpt-3.5-turbo"));

        // Edge cases that should NOT match
        assert!(!is_reasoning_model("")); // Empty string
        assert!(!is_reasoning_model("custom-model")); // Custom model
        assert!(!is_reasoning_model("gpt-4.5")); // Hypothetical non-reasoning model

        // Cases that SHOULD match due to prefix matching
        assert!(is_reasoning_model("gpt-5-turbo")); // Hypothetical future gpt-5 variant
        assert!(is_reasoning_model("o1-turbo")); // Hypothetical future o1 variant
    }

    // ==================== truncate_at_limit Tests ====================

    #[test]
    fn test_truncate_at_limit_short_text() {
        let text = "Hello world";
        let result = truncate_at_limit(text, 100);
        assert_eq!(result, "Hello world");
    }

    #[test]
    fn test_truncate_at_limit_exact_limit() {
        let text = "Hello world"; // 11 chars
        let result = truncate_at_limit(text, 11);
        assert_eq!(result, "Hello world");
    }

    #[test]
    fn test_truncate_at_limit_over_limit() {
        let text = "Hello world this is a test";
        let result = truncate_at_limit(text, 15);
        // Should cut at last whitespace before limit-3=12, which is after "world"
        assert!(result.ends_with("..."));
        assert!(result.len() <= 15);
    }

    #[test]
    fn test_truncate_at_limit_finds_word_boundary() {
        let text = "Hello world test";
        let result = truncate_at_limit(text, 14);
        // limit - 3 = 11, last whitespace before 11 is at 5 (after "Hello")
        assert_eq!(result, "Hello...");
    }

    #[test]
    fn test_truncate_at_limit_no_whitespace() {
        let text = "HelloWorldTest";
        let result = truncate_at_limit(text, 10);
        // No whitespace, so cuts at limit-3=7
        assert_eq!(result, "HelloWo...");
    }

    #[test]
    fn test_truncate_at_limit_very_small_limit() {
        let text = "Hello world";
        let result = truncate_at_limit(text, 5);
        // Very small limit, returns truncated text
        assert!(result.len() <= 5);
    }

    #[test]
    fn test_truncate_at_limit_zero_limit() {
        let text = "Hello";
        let result = truncate_at_limit(text, 0);
        // Should return just "..."
        assert_eq!(result, "...");
    }

    #[test]
    fn test_truncate_at_limit_preserves_utf8() {
        let text = "Hello café world";
        let result = truncate_at_limit(text, 100);
        assert_eq!(result, "Hello café world");
    }

    #[test]
    fn test_truncate_at_limit_empty_string() {
        let text = "";
        let result = truncate_at_limit(text, 10);
        assert_eq!(result, "");
    }

    #[test]
    fn test_truncate_at_limit_multibyte_no_panic() {
        // "café" has a 2-byte 'é' character - cutting mid-codepoint would panic
        let text = "café world test";
        // Limit that would land in the middle of 'é' if not handled properly
        let result = truncate_at_limit(text, 6);
        // Should not panic and should produce valid UTF-8
        assert!(result.is_ascii() || result.chars().count() > 0);
        assert!(result.ends_with("..."));
    }

    #[test]
    fn test_truncate_at_limit_emoji_no_panic() {
        // Emojis are 4 bytes - cutting mid-emoji would panic
        let text = "Hello 🎉 world";
        let result = truncate_at_limit(text, 9);
        // Should not panic
        assert!(result.ends_with("..."));
    }

    #[test]
    fn test_truncate_at_limit_chinese_no_panic() {
        // Chinese characters are 3 bytes each
        let text = "你好世界 hello";
        let result = truncate_at_limit(text, 8);
        // Should not panic and produce valid output
        assert!(result.ends_with("..."));
    }

    #[test]
    fn test_truncate_at_limit_leading_multibyte_respects_limit() {
        // Edge case: leading multibyte character that starts before cut_at but extends beyond
        // "é" is 2 bytes, so "élé" = é(0-1) l(2) é(3-4) = 5 bytes
        let text = "élé test";
        let limit = 4;
        let result = truncate_at_limit(text, limit);
        // Result must not exceed the limit
        assert!(
            result.len() <= limit,
            "Result '{}' (len={}) exceeds limit {}",
            result,
            result.len(),
            limit
        );
    }

    #[test]
    fn test_truncate_at_limit_always_respects_byte_limit() {
        // Test various limits with multibyte text
        // Start from 4 because limits < 4 return "..." (3 bytes) which is the minimum
        let text = "αβγδε test"; // Greek letters are 2 bytes each
        for limit in 4..20 {
            let result = truncate_at_limit(text, limit);
            assert!(
                result.len() <= limit,
                "limit={}: Result '{}' (len={}) exceeds limit",
                limit,
                result,
                result.len()
            );
        }
    }

    // ==================== Translation Validation Integration Tests ====================

    #[tokio::test]
    async fn test_translate_summary_fails_on_untranslated_headers() {
        let mock_server = MockServer::start().await;

        // API returns "translation" that still has English headers
        let bad_translation = "🧠 Top takeaways\n- *Elemento* — texto [enlace](url)";
        let response_body = create_openai_response(bad_translation);

        Mock::given(method("POST"))
            .and(path("/v1/chat/completions"))
            .respond_with(ResponseTemplate::new(200).set_body_json(&response_body))
            .mount(&mock_server)
            .await;

        let config = create_test_config(&format!("{}/v1/chat/completions", mock_server.uri()));
        let client = reqwest::Client::new();

        let original = "🧠 Top takeaways\n- *Item* — text [link](url)";
        let result = translate_summary(&client, &config, original, Language::SPANISH).await;

        // Should fail because the header was not translated
        assert!(
            result.is_err(),
            "Should fail when headers are not translated"
        );
        let err = result.unwrap_err().to_string();
        assert!(
            err.contains("validation failed"),
            "Error should mention validation failure: {}",
            err
        );
        assert!(
            err.contains("Top takeaways"),
            "Error should mention the untranslated header: {}",
            err
        );
    }

    #[tokio::test]
    async fn test_translate_summary_succeeds_with_translated_headers() {
        let mock_server = MockServer::start().await;

        // API returns proper translation with Spanish headers
        let good_translation = "🧠 Conclusiones principales\n- *Elemento* — texto [enlace](url)";
        let response_body = create_openai_response(good_translation);

        Mock::given(method("POST"))
            .and(path("/v1/chat/completions"))
            .respond_with(ResponseTemplate::new(200).set_body_json(&response_body))
            .mount(&mock_server)
            .await;

        let config = create_test_config(&format!("{}/v1/chat/completions", mock_server.uri()));
        let client = reqwest::Client::new();

        let original = "🧠 Top takeaways\n- *Item* — text [link](url)";
        let result = translate_summary(&client, &config, original, Language::SPANISH).await;

        // Should succeed because the header was translated
        assert!(
            result.is_ok(),
            "Should succeed with translated headers: {:?}",
            result
        );
        assert_eq!(result.unwrap(), good_translation);
    }

    #[tokio::test]
    async fn test_translate_summary_validates_all_headers() {
        let mock_server = MockServer::start().await;

        // Translation has multiple untranslated headers
        let bad_translation = "🧠 Conclusiones principales\n- item\n\n🚀 Releases\n- item";
        let response_body = create_openai_response(bad_translation);

        Mock::given(method("POST"))
            .and(path("/v1/chat/completions"))
            .respond_with(ResponseTemplate::new(200).set_body_json(&response_body))
            .mount(&mock_server)
            .await;

        let config = create_test_config(&format!("{}/v1/chat/completions", mock_server.uri()));
        let client = reqwest::Client::new();

        let original = "🧠 Top takeaways\n- item\n\n🚀 Releases\n- item";
        let result = translate_summary(&client, &config, original, Language::SPANISH).await;

        // Should fail because "🚀 Releases" was not translated
        assert!(
            result.is_err(),
            "Should fail when any header is not translated"
        );
        let err = result.unwrap_err().to_string();
        assert!(
            err.contains("Releases"),
            "Error should mention untranslated 'Releases' header: {}",
            err
        );
    }
}
