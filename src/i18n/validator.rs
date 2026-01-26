//! Translation quality validation module.
//!
//! This module provides validation for translated content to ensure that
//! important elements are preserved during translation (e.g., @handles,
//! #hashtags, URLs, etc.).

use super::strings::ENGLISH_SECTION_HEADERS;
use super::Language;
use regex::Regex;
use std::sync::OnceLock;

/// Validation report containing errors and warnings about a translation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValidationReport {
    /// Critical errors that indicate translation issues
    pub errors: Vec<String>,

    /// Non-critical warnings about potential issues
    pub warnings: Vec<String>,
}

impl ValidationReport {
    /// Create a new empty validation report
    pub fn new() -> Self {
        Self {
            errors: Vec::new(),
            warnings: Vec::new(),
        }
    }

    /// Check if the report has any errors
    pub fn has_errors(&self) -> bool {
        !self.errors.is_empty()
    }

    /// Check if the report has any warnings
    pub fn has_warnings(&self) -> bool {
        !self.warnings.is_empty()
    }

    /// Check if the report is clean (no errors or warnings)
    pub fn is_clean(&self) -> bool {
        !self.has_errors() && !self.has_warnings()
    }
}

impl Default for ValidationReport {
    fn default() -> Self {
        Self::new()
    }
}

/// Validator for translation quality.
pub struct TranslationValidator;

// Regex patterns for extraction (cached for performance)
static HANDLE_REGEX: OnceLock<Regex> = OnceLock::new();
static HASHTAG_REGEX: OnceLock<Regex> = OnceLock::new();
static CASHTAG_REGEX: OnceLock<Regex> = OnceLock::new();
static URL_REGEX: OnceLock<Regex> = OnceLock::new();
static MARKDOWN_LINK_REGEX: OnceLock<Regex> = OnceLock::new();

/// List of all English section headers used in summaries.
/// These should NOT appear in translated content for non-English targets.
pub const ENGLISH_HEADERS: [&str; 7] = [
    ENGLISH_SECTION_HEADERS.top_takeaways,
    ENGLISH_SECTION_HEADERS.releases,
    ENGLISH_SECTION_HEADERS.research,
    ENGLISH_SECTION_HEADERS.tools_tutorials,
    ENGLISH_SECTION_HEADERS.companies_deals,
    ENGLISH_SECTION_HEADERS.policy_safety,
    ENGLISH_SECTION_HEADERS.debate_opinions,
];

impl TranslationValidator {
    /// Validate that a translation preserves important elements from the original.
    ///
    /// This function checks that:
    /// - Twitter @handles are preserved
    /// - #hashtags are preserved
    /// - $cashtags are preserved
    /// - URLs are preserved
    /// - Markdown links are preserved
    /// - Section headers are translated (for non-English targets)
    ///
    /// # Arguments
    /// * `original` - The original text (before translation)
    /// * `translated` - The translated text
    /// * `target_language` - The target language of the translation
    ///
    /// # Returns
    /// A `ValidationReport` containing any errors or warnings found.
    pub fn validate(
        original: &str,
        translated: &str,
        target_language: Language,
    ) -> ValidationReport {
        let mut report = ValidationReport::new();

        // Check @handles
        let orig_handles = Self::extract_handles(original);
        let trans_handles = Self::extract_handles(translated);
        if orig_handles != trans_handles {
            report.warnings.push(format!(
                "Handle mismatch: original has {:?}, translation has {:?}",
                orig_handles, trans_handles
            ));
        }

        // Check #hashtags
        let orig_hashtags = Self::extract_hashtags(original);
        let trans_hashtags = Self::extract_hashtags(translated);
        if orig_hashtags != trans_hashtags {
            report.warnings.push(format!(
                "Hashtag mismatch: original has {:?}, translation has {:?}",
                orig_hashtags, trans_hashtags
            ));
        }

        // Check $cashtags
        let orig_cashtags = Self::extract_cashtags(original);
        let trans_cashtags = Self::extract_cashtags(translated);
        if orig_cashtags != trans_cashtags {
            report.warnings.push(format!(
                "Cashtag mismatch: original has {:?}, translation has {:?}",
                orig_cashtags, trans_cashtags
            ));
        }

        // Check URLs
        let orig_urls = Self::extract_urls(original);
        let trans_urls = Self::extract_urls(translated);
        if orig_urls != trans_urls {
            report.warnings.push(format!(
                "URL mismatch: original has {} URLs, translation has {} URLs",
                orig_urls.len(),
                trans_urls.len()
            ));
        }

        // Check markdown links count (approximate check)
        let orig_md_links = Self::extract_markdown_links(original);
        let trans_md_links = Self::extract_markdown_links(translated);
        if orig_md_links.len() != trans_md_links.len() {
            report.warnings.push(format!(
                "Markdown link count mismatch: original has {}, translation has {}",
                orig_md_links.len(),
                trans_md_links.len()
            ));
        }

        // Check for untranslated section headers (only for non-English targets)
        if !target_language.is_canonical() {
            let untranslated = Self::find_untranslated_headers(translated);
            for header in untranslated {
                report.errors.push(format!(
                    "Untranslated section header found: '{}' should be translated to {}",
                    header,
                    target_language.name()
                ));
            }
        }

        report
    }

    /// Find any English section headers that appear in the translated text.
    ///
    /// Returns a list of headers that were NOT translated.
    pub fn find_untranslated_headers(translated: &str) -> Vec<&'static str> {
        ENGLISH_HEADERS
            .iter()
            .filter(|header| translated.contains(*header))
            .copied()
            .collect()
    }

    /// Extract all @handles from text
    fn extract_handles(text: &str) -> Vec<String> {
        let regex = HANDLE_REGEX.get_or_init(|| Regex::new(r"@([a-zA-Z0-9_]+)").unwrap());

        regex
            .captures_iter(text)
            .filter_map(|cap| cap.get(0).map(|m| m.as_str().to_string()))
            .collect()
    }

    /// Extract all #hashtags from text
    fn extract_hashtags(text: &str) -> Vec<String> {
        let regex = HASHTAG_REGEX.get_or_init(|| Regex::new(r"#([a-zA-Z0-9_]+)").unwrap());

        regex
            .captures_iter(text)
            .filter_map(|cap| cap.get(0).map(|m| m.as_str().to_string()))
            .collect()
    }

    /// Extract all $cashtags from text
    fn extract_cashtags(text: &str) -> Vec<String> {
        let regex = CASHTAG_REGEX.get_or_init(|| Regex::new(r"\$([A-Z]{1,5})(?:\b|$)").unwrap());

        regex
            .captures_iter(text)
            .filter_map(|cap| cap.get(0).map(|m| m.as_str().to_string()))
            .collect()
    }

    /// Extract all URLs from text
    fn extract_urls(text: &str) -> Vec<String> {
        let regex = URL_REGEX.get_or_init(|| Regex::new(r"https?://[^\s)\]]+").unwrap());

        regex
            .find_iter(text)
            .map(|m| m.as_str().to_string())
            .collect()
    }

    /// Extract markdown links from text (approximate)
    fn extract_markdown_links(text: &str) -> Vec<String> {
        let regex =
            MARKDOWN_LINK_REGEX.get_or_init(|| Regex::new(r"\[([^\]]+)\]\(([^)]+)\)").unwrap());

        regex
            .find_iter(text)
            .map(|m| m.as_str().to_string())
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ==================== Handle Extraction Tests ====================

    #[test]
    fn test_extract_handles_single() {
        let text = "Check out @elonmusk's latest tweet";
        let handles = TranslationValidator::extract_handles(text);
        assert_eq!(handles, vec!["@elonmusk"]);
    }

    #[test]
    fn test_extract_handles_multiple() {
        let text = "Both @elonmusk and @sama are talking about AI";
        let handles = TranslationValidator::extract_handles(text);
        assert_eq!(handles, vec!["@elonmusk", "@sama"]);
    }

    #[test]
    fn test_extract_handles_none() {
        let text = "No handles in this text";
        let handles = TranslationValidator::extract_handles(text);
        assert!(handles.is_empty());
    }

    #[test]
    fn test_extract_handles_with_underscores() {
        let text = "Follow @user_name_123 for updates";
        let handles = TranslationValidator::extract_handles(text);
        assert_eq!(handles, vec!["@user_name_123"]);
    }

    // ==================== Hashtag Extraction Tests ====================

    #[test]
    fn test_extract_hashtags_single() {
        let text = "This is about #AI and machine learning";
        let hashtags = TranslationValidator::extract_hashtags(text);
        assert_eq!(hashtags, vec!["#AI"]);
    }

    #[test]
    fn test_extract_hashtags_multiple() {
        let text = "#MachineLearning and #AI are transforming #Tech";
        let hashtags = TranslationValidator::extract_hashtags(text);
        assert_eq!(hashtags, vec!["#MachineLearning", "#AI", "#Tech"]);
    }

    #[test]
    fn test_extract_hashtags_none() {
        let text = "No hashtags here";
        let hashtags = TranslationValidator::extract_hashtags(text);
        assert!(hashtags.is_empty());
    }

    // ==================== Cashtag Extraction Tests ====================

    #[test]
    fn test_extract_cashtags_single() {
        let text = "Stock price of $TSLA is rising";
        let cashtags = TranslationValidator::extract_cashtags(text);
        assert_eq!(cashtags, vec!["$TSLA"]);
    }

    #[test]
    fn test_extract_cashtags_multiple() {
        let text = "Both $TSLA and $NVDA are up today";
        let cashtags = TranslationValidator::extract_cashtags(text);
        assert_eq!(cashtags, vec!["$TSLA", "$NVDA"]);
    }

    #[test]
    fn test_extract_cashtags_none() {
        let text = "No stock symbols here";
        let cashtags = TranslationValidator::extract_cashtags(text);
        assert!(cashtags.is_empty());
    }

    // ==================== URL Extraction Tests ====================

    #[test]
    fn test_extract_urls_single() {
        let text = "Read more at https://example.com for details";
        let urls = TranslationValidator::extract_urls(text);
        assert_eq!(urls, vec!["https://example.com"]);
    }

    #[test]
    fn test_extract_urls_multiple() {
        let text = "Check https://example.com and http://test.org";
        let urls = TranslationValidator::extract_urls(text);
        assert_eq!(urls, vec!["https://example.com", "http://test.org"]);
    }

    #[test]
    fn test_extract_urls_none() {
        let text = "No URLs in this text";
        let urls = TranslationValidator::extract_urls(text);
        assert!(urls.is_empty());
    }

    #[test]
    fn test_extract_urls_in_markdown() {
        let text = "[Click here](https://example.com)";
        let urls = TranslationValidator::extract_urls(text);
        assert_eq!(urls, vec!["https://example.com"]);
    }

    // ==================== Markdown Link Tests ====================

    #[test]
    fn test_extract_markdown_links_single() {
        let text = "Check [this link](https://example.com) for more";
        let links = TranslationValidator::extract_markdown_links(text);
        assert_eq!(links, vec!["[this link](https://example.com)"]);
    }

    #[test]
    fn test_extract_markdown_links_multiple() {
        let text = "[Link 1](https://a.com) and [Link 2](https://b.com)";
        let links = TranslationValidator::extract_markdown_links(text);
        assert_eq!(links.len(), 2);
    }

    #[test]
    fn test_extract_markdown_links_none() {
        let text = "No markdown links here";
        let links = TranslationValidator::extract_markdown_links(text);
        assert!(links.is_empty());
    }

    // ==================== Validation Tests ====================

    #[test]
    fn test_validate_perfect_translation() {
        let original = "Check out @elonmusk talking about #AI at https://example.com";
        let translated = "Mira a @elonmusk hablando de #AI en https://example.com";

        let report = TranslationValidator::validate(original, translated, Language::SPANISH);
        assert!(report.is_clean());
    }

    #[test]
    fn test_validate_missing_handle() {
        let original = "Follow @elonmusk for updates";
        let translated = "Sigue a elonmusk para actualizaciones";

        let report = TranslationValidator::validate(original, translated, Language::SPANISH);
        assert!(report.has_warnings());
        assert!(report.warnings[0].contains("Handle mismatch"));
    }

    #[test]
    fn test_validate_missing_hashtag() {
        let original = "This is about #AI technology";
        let translated = "Esto es sobre tecnología AI";

        let report = TranslationValidator::validate(original, translated, Language::SPANISH);
        assert!(report.has_warnings());
        assert!(report.warnings[0].contains("Hashtag mismatch"));
    }

    #[test]
    fn test_validate_missing_url() {
        let original = "Read more at https://example.com";
        let translated = "Lee más aquí";

        let report = TranslationValidator::validate(original, translated, Language::SPANISH);
        assert!(report.has_warnings());
        assert!(report.warnings[0].contains("URL mismatch"));
    }

    #[test]
    fn test_validate_missing_cashtag() {
        let original = "Stock price of $TSLA is up";
        let translated = "El precio de TSLA está subiendo";

        let report = TranslationValidator::validate(original, translated, Language::SPANISH);
        assert!(report.has_warnings());
        assert!(report.warnings[0].contains("Cashtag mismatch"));
    }

    #[test]
    fn test_validate_complex_text() {
        let original =
            "@sama discusses #AI and $NVDA at https://example.com with [link](https://test.com)";
        let translated =
            "@sama habla de #AI y $NVDA en https://example.com con [enlace](https://test.com)";

        let report = TranslationValidator::validate(original, translated, Language::SPANISH);
        assert!(report.is_clean());
    }

    // ==================== Untranslated Header Detection Tests ====================

    #[test]
    fn test_validate_detects_untranslated_headers() {
        let original = "🧠 Top takeaways\n- *Item* — text [link](url)";
        // Translation leaves the header in English - this is a validation ERROR
        let translated = "🧠 Top takeaways\n- *Elemento* — texto [enlace](url)";

        let report = TranslationValidator::validate(original, translated, Language::SPANISH);
        assert!(report.has_errors());
        assert!(report.errors[0].contains("Untranslated section header"));
        assert!(report.errors[0].contains("Top takeaways"));
    }

    #[test]
    fn test_validate_accepts_translated_headers() {
        let original = "🧠 Top takeaways\n- *Item* — text [link](url)";
        // Properly translated - header is in Spanish
        let translated = "🧠 Conclusiones principales\n- *Elemento* — texto [enlace](url)";

        let report = TranslationValidator::validate(original, translated, Language::SPANISH);
        // Should not have errors about untranslated headers
        assert!(
            !report.has_errors(),
            "Should not have errors for properly translated headers"
        );
    }

    #[test]
    fn test_validate_english_target_ignores_english_headers() {
        let original = "🧠 Top takeaways\n- *Item* — text";
        let translated = "🧠 Top takeaways\n- *Item* — text"; // Same content

        // For English target, English headers are expected - no error
        let report = TranslationValidator::validate(original, translated, Language::ENGLISH);
        assert!(!report.has_errors());
    }

    #[test]
    fn test_find_untranslated_headers_single() {
        let text = "🧠 Top takeaways\n- *Item* — text";
        let untranslated = TranslationValidator::find_untranslated_headers(text);
        assert_eq!(untranslated.len(), 1);
        assert!(untranslated.contains(&"🧠 Top takeaways"));
    }

    #[test]
    fn test_find_untranslated_headers_multiple() {
        let text = "🧠 Top takeaways\n- item\n\n🚀 Releases\n- item\n\n🔬 Research\n- item";
        let untranslated = TranslationValidator::find_untranslated_headers(text);
        assert_eq!(untranslated.len(), 3);
        assert!(untranslated.contains(&"🧠 Top takeaways"));
        assert!(untranslated.contains(&"🚀 Releases"));
        assert!(untranslated.contains(&"🔬 Research"));
    }

    #[test]
    fn test_find_untranslated_headers_none_when_translated() {
        // All headers are in Spanish - should find none
        let text = "🧠 Conclusiones principales\n- elemento\n\n🚀 Lanzamientos\n- elemento";
        let untranslated = TranslationValidator::find_untranslated_headers(text);
        assert!(untranslated.is_empty());
    }

    #[test]
    fn test_find_untranslated_headers_partial_translation() {
        // Some headers translated, some not
        let text = "🧠 Conclusiones principales\n- item\n\n🚀 Releases\n- item"; // Releases NOT translated
        let untranslated = TranslationValidator::find_untranslated_headers(text);
        assert_eq!(untranslated.len(), 1);
        assert!(untranslated.contains(&"🚀 Releases"));
        assert!(!untranslated.contains(&"🧠 Top takeaways")); // This was translated
    }

    #[test]
    fn test_all_english_headers_are_covered() {
        // Verify ENGLISH_HEADERS contains all the expected headers
        assert_eq!(ENGLISH_HEADERS.len(), 7);
        assert!(ENGLISH_HEADERS.contains(&"🧠 Top takeaways"));
        assert!(ENGLISH_HEADERS.contains(&"🚀 Releases"));
        assert!(ENGLISH_HEADERS.contains(&"🔬 Research"));
        assert!(ENGLISH_HEADERS.contains(&"🧰 Tools and Tutorials"));
        assert!(ENGLISH_HEADERS.contains(&"🏢 Companies and Deals"));
        assert!(ENGLISH_HEADERS.contains(&"⚖️ Policy and Safety"));
        assert!(ENGLISH_HEADERS.contains(&"💬 Debate and Opinions"));
    }

    #[test]
    fn test_validation_report_new() {
        let report = ValidationReport::new();
        assert!(report.is_clean());
        assert!(!report.has_errors());
        assert!(!report.has_warnings());
    }

    #[test]
    fn test_validation_report_with_warning() {
        let mut report = ValidationReport::new();
        report.warnings.push("Test warning".to_string());

        assert!(!report.is_clean());
        assert!(!report.has_errors());
        assert!(report.has_warnings());
    }

    #[test]
    fn test_validation_report_with_error() {
        let mut report = ValidationReport::new();
        report.errors.push("Test error".to_string());

        assert!(!report.is_clean());
        assert!(report.has_errors());
        assert!(!report.has_warnings());
    }
}
