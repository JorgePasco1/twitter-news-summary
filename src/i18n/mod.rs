//! Internationalization (i18n) module for multi-language support.
//!
//! This module provides a centralized, extensible architecture for managing
//! multiple languages. All language-related logic, localized strings, and
//! translation infrastructure is contained here.
//!
//! # Architecture
//!
//! - `registry`: Single source of truth for all supported languages and their metadata
//! - `language`: Type-safe Language type that replaces hardcoded enums
//! - `strings`: Centralized localized strings (Phase 3)
//! - `validator`: Translation quality validation (Phase 4)
//! - `metrics`: Translation observability and metrics (Phase 4)
//!
//! # Example
//!
//! ```rust,ignore
//! use crate::i18n::{Language, LanguageRegistry};
//!
//! // Get canonical language (English)
//! let canonical = Language::canonical();
//!
//! // Create language from code
//! let spanish = Language::from_code("es")?;
//!
//! // List all enabled languages
//! let languages = LanguageRegistry::get().list_enabled();
//! ```

mod language;
mod metrics;
mod registry;
mod strings;
mod validator;

pub use language::Language;
pub use metrics::{MetricsReport, TranslationMetrics};
pub use registry::{LanguageConfig, LanguageRegistry};
pub use strings::LanguageStrings;
pub use validator::{TranslationValidator, ValidationReport};
