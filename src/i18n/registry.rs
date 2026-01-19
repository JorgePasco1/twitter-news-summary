//! Language registry: Single source of truth for all supported languages.
//!
//! This module provides a centralized registry of all languages supported by the
//! application. It uses a singleton pattern with `OnceLock` to ensure thread-safe
//! initialization and access.

use std::sync::OnceLock;

/// Configuration for a supported language.
///
/// Contains all metadata and settings for a specific language, including
/// its code, names, enabled status, and whether it's the canonical language.
#[derive(Debug, Clone)]
pub struct LanguageConfig {
    /// ISO 639-1 language code (e.g., "en", "es", "fr")
    pub code: &'static str,

    /// English name of the language (e.g., "English", "Spanish", "French")
    pub name: &'static str,

    /// Native name of the language (e.g., "English", "Español", "Français")
    pub native_name: &'static str,

    /// Whether this is the canonical/source language (only one should be true)
    pub is_canonical: bool,

    /// Whether this language is enabled for use
    pub enabled: bool,
}

/// Global language registry singleton.
///
/// This registry contains all supported languages and provides methods to query
/// and access them. It's initialized once on first access and remains immutable
/// thereafter.
pub struct LanguageRegistry {
    languages: Vec<LanguageConfig>,
}

/// Global registry instance (initialized lazily)
static REGISTRY: OnceLock<LanguageRegistry> = OnceLock::new();

impl LanguageRegistry {
    /// Get the global language registry instance.
    ///
    /// This method initializes the registry on first call and returns a reference
    /// to the singleton instance on subsequent calls.
    pub fn get() -> &'static LanguageRegistry {
        REGISTRY.get_or_init(|| LanguageRegistry {
            languages: default_languages(),
        })
    }

    /// Get a language configuration by its code.
    ///
    /// # Arguments
    /// * `code` - The ISO 639-1 language code (e.g., "en", "es")
    ///
    /// # Returns
    /// * `Some(&LanguageConfig)` if the language exists
    /// * `None` if the language is not found
    pub fn get_by_code(&self, code: &str) -> Option<&LanguageConfig> {
        self.languages.iter().find(|lang| lang.code == code)
    }

    /// Get all enabled languages.
    ///
    /// # Returns
    /// A vector of references to all language configurations where `enabled` is true.
    pub fn list_enabled(&self) -> Vec<&LanguageConfig> {
        self.languages.iter().filter(|lang| lang.enabled).collect()
    }

    /// Get all languages (including disabled ones).
    ///
    /// # Returns
    /// A vector of references to all language configurations.
    pub fn list_all(&self) -> Vec<&LanguageConfig> {
        self.languages.iter().collect()
    }

    /// Get the canonical language configuration.
    ///
    /// The canonical language is the source language for all translations
    /// (typically English). There should be exactly one canonical language.
    ///
    /// # Returns
    /// A reference to the canonical language configuration.
    ///
    /// # Panics
    /// Panics if no canonical language is found or if multiple canonical
    /// languages are defined (this indicates a configuration error).
    pub fn canonical(&self) -> &LanguageConfig {
        let canonical_langs: Vec<_> = self
            .languages
            .iter()
            .filter(|lang| lang.is_canonical)
            .collect();

        match canonical_langs.len() {
            0 => panic!("No canonical language found in registry"),
            1 => canonical_langs[0],
            _ => panic!("Multiple canonical languages found in registry"),
        }
    }

    /// Check if a language code is supported and enabled.
    ///
    /// # Arguments
    /// * `code` - The ISO 639-1 language code to check
    ///
    /// # Returns
    /// `true` if the language exists and is enabled, `false` otherwise.
    pub fn is_enabled(&self, code: &str) -> bool {
        self.get_by_code(code)
            .map(|lang| lang.enabled)
            .unwrap_or(false)
    }
}

/// Default language configurations.
///
/// This function returns the initial set of supported languages.
/// Currently supports English (canonical) and Spanish.
fn default_languages() -> Vec<LanguageConfig> {
    vec![
        LanguageConfig {
            code: "en",
            name: "English",
            native_name: "English",
            is_canonical: true,
            enabled: true,
        },
        LanguageConfig {
            code: "es",
            name: "Spanish",
            native_name: "Español",
            is_canonical: false,
            enabled: true,
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_registry_get_returns_singleton() {
        let registry1 = LanguageRegistry::get();
        let registry2 = LanguageRegistry::get();

        // Should return the same instance (same memory address)
        assert!(std::ptr::eq(registry1, registry2));
    }

    #[test]
    fn test_get_by_code_english() {
        let registry = LanguageRegistry::get();
        let config = registry.get_by_code("en");

        assert!(config.is_some());
        let config = config.unwrap();
        assert_eq!(config.code, "en");
        assert_eq!(config.name, "English");
        assert_eq!(config.native_name, "English");
        assert!(config.is_canonical);
        assert!(config.enabled);
    }

    #[test]
    fn test_get_by_code_spanish() {
        let registry = LanguageRegistry::get();
        let config = registry.get_by_code("es");

        assert!(config.is_some());
        let config = config.unwrap();
        assert_eq!(config.code, "es");
        assert_eq!(config.name, "Spanish");
        assert_eq!(config.native_name, "Español");
        assert!(!config.is_canonical);
        assert!(config.enabled);
    }

    #[test]
    fn test_get_by_code_nonexistent() {
        let registry = LanguageRegistry::get();
        let config = registry.get_by_code("fr");
        assert!(config.is_none());
    }

    #[test]
    fn test_list_enabled_contains_english_and_spanish() {
        let registry = LanguageRegistry::get();
        let enabled = registry.list_enabled();

        assert_eq!(enabled.len(), 2);
        assert!(enabled.iter().any(|lang| lang.code == "en"));
        assert!(enabled.iter().any(|lang| lang.code == "es"));
    }

    #[test]
    fn test_list_all_contains_english_and_spanish() {
        let registry = LanguageRegistry::get();
        let all = registry.list_all();

        assert_eq!(all.len(), 2);
        assert!(all.iter().any(|lang| lang.code == "en"));
        assert!(all.iter().any(|lang| lang.code == "es"));
    }

    #[test]
    fn test_canonical_returns_english() {
        let registry = LanguageRegistry::get();
        let canonical = registry.canonical();

        assert_eq!(canonical.code, "en");
        assert!(canonical.is_canonical);
    }

    #[test]
    fn test_is_enabled_english() {
        let registry = LanguageRegistry::get();
        assert!(registry.is_enabled("en"));
    }

    #[test]
    fn test_is_enabled_spanish() {
        let registry = LanguageRegistry::get();
        assert!(registry.is_enabled("es"));
    }

    #[test]
    fn test_is_enabled_nonexistent() {
        let registry = LanguageRegistry::get();
        assert!(!registry.is_enabled("fr"));
    }

    #[test]
    fn test_language_config_clone() {
        let config = LanguageConfig {
            code: "en",
            name: "English",
            native_name: "English",
            is_canonical: true,
            enabled: true,
        };

        let cloned = config.clone();
        assert_eq!(config.code, cloned.code);
        assert_eq!(config.name, cloned.name);
    }
}
