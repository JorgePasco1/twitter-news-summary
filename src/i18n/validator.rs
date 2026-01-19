//! Translation quality validation module.
//!
//! This module provides validation for translated content to ensure that
//! important elements are preserved during translation (e.g., @handles,
//! #hashtags, URLs, etc.).

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

impl TranslationValidator {
    /// Validate that a translation preserves important elements from the original.
    ///
    /// This function checks that:
    /// - Twitter @handles are preserved
    /// - #hashtags are preserved
    /// - $cashtags are preserved
    /// - URLs are preserved
    /// - Markdown links are preserved
    ///
    /// # Arguments
    /// * `original` - The original text (before translation)
    /// * `translated` - The translated text
    ///
    /// # Returns
    /// A `ValidationReport` containing any errors or warnings found.
    pub fn validate(original: &str, translated: &str) -> ValidationReport {
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

        report
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

        let report = TranslationValidator::validate(original, translated);
        assert!(report.is_clean());
    }

    #[test]
    fn test_validate_missing_handle() {
        let original = "Follow @elonmusk for updates";
        let translated = "Sigue a elonmusk para actualizaciones";

        let report = TranslationValidator::validate(original, translated);
        assert!(report.has_warnings());
        assert!(report.warnings[0].contains("Handle mismatch"));
    }

    #[test]
    fn test_validate_missing_hashtag() {
        let original = "This is about #AI technology";
        let translated = "Esto es sobre tecnología AI";

        let report = TranslationValidator::validate(original, translated);
        assert!(report.has_warnings());
        assert!(report.warnings[0].contains("Hashtag mismatch"));
    }

    #[test]
    fn test_validate_missing_url() {
        let original = "Read more at https://example.com";
        let translated = "Lee más aquí";

        let report = TranslationValidator::validate(original, translated);
        assert!(report.has_warnings());
        assert!(report.warnings[0].contains("URL mismatch"));
    }

    #[test]
    fn test_validate_missing_cashtag() {
        let original = "Stock price of $TSLA is up";
        let translated = "El precio de TSLA está subiendo";

        let report = TranslationValidator::validate(original, translated);
        assert!(report.has_warnings());
        assert!(report.warnings[0].contains("Cashtag mismatch"));
    }

    #[test]
    fn test_validate_complex_text() {
        let original =
            "@sama discusses #AI and $NVDA at https://example.com with [link](https://test.com)";
        let translated =
            "@sama habla de #AI y $NVDA en https://example.com con [enlace](https://test.com)";

        let report = TranslationValidator::validate(original, translated);
        assert!(report.is_clean());
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
