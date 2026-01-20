//! Language type: Flexible, validated language representation.
//!
//! This module provides the `Language` type, which replaces the hardcoded
//! `Language` enum with a flexible struct that validates against the registry.

use crate::i18n::{LanguageConfig, LanguageRegistry};
use anyhow::{bail, Result};

/// A validated language.
///
/// This type represents a language that has been validated against the registry.
/// It ensures that only supported, enabled languages can be constructed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Language {
    /// ISO 639-1 language code (e.g., "en", "es")
    code: &'static str,
}

impl Language {
    /// Backward compatibility constant for English.
    ///
    /// This allows existing code using `Language::English` to continue working
    /// without changes by using `Language::ENGLISH` instead.
    pub const ENGLISH: Language = Language { code: "en" };

    /// Backward compatibility constant for Spanish.
    ///
    /// This allows existing code using `Language::Spanish` to continue working
    /// without changes by using `Language::SPANISH` instead.
    pub const SPANISH: Language = Language { code: "es" };

    /// Create a Language from a language code string.
    ///
    /// # Arguments
    /// * `code` - The ISO 639-1 language code (e.g., "en", "es")
    ///
    /// # Returns
    /// * `Ok(Language)` if the code is valid and the language is enabled
    /// * `Err` if the code is not found or the language is disabled
    ///
    /// # Example
    /// ```ignore
    /// let spanish = Language::from_code("es")?;
    /// ```
    pub fn from_code(code: &str) -> Result<Language> {
        let registry = LanguageRegistry::get();

        match registry.get_by_code(code) {
            Some(config) if config.enabled => Ok(Language {
                code: config.code, // Use the static str from the registry
            }),
            Some(_) => bail!("Language '{}' is not enabled", code),
            None => bail!("Unknown language code: '{}'", code),
        }
    }

    /// Get the canonical (source) language.
    ///
    /// This is the language that all summaries are originally generated in,
    /// and from which all translations are derived.
    ///
    /// # Returns
    /// The canonical Language (typically English).
    pub fn canonical() -> Language {
        let config = LanguageRegistry::get().canonical();
        Language { code: config.code }
    }

    /// Get the ISO 639-1 language code.
    ///
    /// # Returns
    /// The language code as a static string (e.g., "en", "es").
    pub fn code(&self) -> &'static str {
        self.code
    }

    /// Get the full language configuration from the registry.
    ///
    /// # Returns
    /// A reference to the `LanguageConfig` for this language.
    ///
    /// # Panics
    /// Panics if the language code is not found in the registry. This should
    /// never happen if the Language was constructed properly (via `from_code`
    /// or constants).
    pub fn config(&self) -> &'static LanguageConfig {
        LanguageRegistry::get()
            .get_by_code(self.code)
            .expect("Language code should always be valid")
    }

    /// Get the English name of the language.
    ///
    /// # Returns
    /// The language name in English (e.g., "English", "Spanish").
    pub fn name(&self) -> &'static str {
        self.config().name
    }

    /// Get the native name of the language.
    ///
    /// # Returns
    /// The language name in its native form (e.g., "English", "Español").
    pub fn native_name(&self) -> &'static str {
        self.config().native_name
    }

    /// Check if this is the canonical language.
    ///
    /// # Returns
    /// `true` if this is the source language, `false` if it's a translation target.
    pub fn is_canonical(&self) -> bool {
        self.config().is_canonical
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ==================== Constant Tests ====================

    #[test]
    fn test_english_constant() {
        let english = Language::ENGLISH;
        assert_eq!(english.code(), "en");
        assert_eq!(english.name(), "English");
        assert!(english.is_canonical());
    }

    #[test]
    fn test_spanish_constant() {
        let spanish = Language::SPANISH;
        assert_eq!(spanish.code(), "es");
        assert_eq!(spanish.name(), "Spanish");
        assert!(!spanish.is_canonical());
    }

    // ==================== from_code Tests ====================

    #[test]
    fn test_from_code_english() {
        let language = Language::from_code("en").expect("Should succeed");
        assert_eq!(language.code(), "en");
        assert_eq!(language.name(), "English");
    }

    #[test]
    fn test_from_code_spanish() {
        let language = Language::from_code("es").expect("Should succeed");
        assert_eq!(language.code(), "es");
        assert_eq!(language.name(), "Spanish");
    }

    #[test]
    fn test_from_code_invalid() {
        let result = Language::from_code("fr");
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Unknown"));
    }

    #[test]
    fn test_from_code_empty() {
        let result = Language::from_code("");
        assert!(result.is_err());
    }

    // ==================== canonical Tests ====================

    #[test]
    fn test_canonical_returns_english() {
        let canonical = Language::canonical();
        assert_eq!(canonical.code(), "en");
        assert!(canonical.is_canonical());
    }

    // ==================== Trait Tests ====================

    #[test]
    fn test_language_equality() {
        let lang1 = Language::ENGLISH;
        let lang2 = Language::from_code("en").unwrap();
        assert_eq!(lang1, lang2);
    }

    #[test]
    fn test_language_inequality() {
        let english = Language::ENGLISH;
        let spanish = Language::SPANISH;
        assert_ne!(english, spanish);
    }

    #[test]
    fn test_language_clone() {
        let lang = Language::SPANISH;
        let cloned = lang;
        assert_eq!(lang, cloned);
    }

    #[test]
    fn test_language_copy() {
        let lang1 = Language::ENGLISH;
        let lang2 = lang1; // Copy
        assert_eq!(lang1, lang2); // Both still valid
    }

    #[test]
    fn test_language_debug() {
        let lang = Language::SPANISH;
        let debug = format!("{:?}", lang);
        assert!(debug.contains("es"));
    }

    // ==================== Config Access Tests ====================

    #[test]
    fn test_config_access() {
        let lang = Language::SPANISH;
        let config = lang.config();
        assert_eq!(config.code, "es");
        assert_eq!(config.name, "Spanish");
        assert_eq!(config.native_name, "Español");
    }

    #[test]
    fn test_native_name() {
        let english = Language::ENGLISH;
        let spanish = Language::SPANISH;
        assert_eq!(english.native_name(), "English");
        assert_eq!(spanish.native_name(), "Español");
    }

    // ==================== Backward Compatibility Tests ====================

    #[test]
    fn test_backward_compat_english() {
        // Old code: Language::English.code()
        // New code: Language::ENGLISH.code()
        assert_eq!(Language::ENGLISH.code(), "en");
    }

    #[test]
    fn test_backward_compat_spanish() {
        // Old code: Language::Spanish.code()
        // New code: Language::SPANISH.code()
        assert_eq!(Language::SPANISH.code(), "es");
    }

    #[test]
    fn test_backward_compat_from_code() {
        // Old code: Language::from_code("en")
        // New code: Still works the same
        let result = Language::from_code("en");
        assert!(result.is_ok());
        assert_eq!(result.unwrap().code(), "en");
    }

    #[test]
    fn test_backward_compat_is_canonical() {
        // Old code: Language::English.is_canonical()
        // New code: Language::ENGLISH.is_canonical()
        assert!(Language::ENGLISH.is_canonical());
        assert!(!Language::SPANISH.is_canonical());
    }
}
