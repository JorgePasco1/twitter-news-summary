/// All localized user-facing strings for a language
///
/// Strings are stored in their raw, unescaped form. When displaying in Telegram,
/// use `escape_markdownv2()` to properly escape them for MarkdownV2 format.
#[derive(Debug, Clone)]
pub struct LanguageStrings {
    // ==================== Summary Headers ====================
    /// Header shown in summary messages (e.g., "Twitter Summary")
    pub summary_header: &'static str,

    /// Notice shown when translation fails and falling back to English
    /// Empty string means no notice is needed (e.g., for English itself)
    pub translation_failure_notice: &'static str,

    // ==================== Welcome/Help Messages ====================
    /// Welcome message shown to admin users
    pub welcome_admin: &'static str,

    /// Welcome message shown to regular users
    pub welcome_user: &'static str,

    // ==================== Subscription Messages ====================
    /// Message shown when user tries to subscribe but is already subscribed
    pub subscribe_already: &'static str,

    /// Message shown when user successfully subscribes
    pub subscribe_success: &'static str,

    /// Message shown when user successfully unsubscribes
    pub unsubscribe_success: &'static str,

    /// Message shown when user tries to unsubscribe but is not subscribed
    pub unsubscribe_not_subscribed: &'static str,

    // ==================== Status Messages ====================
    /// Status message for subscribed admin users
    /// Placeholders: {language}, {count}
    pub status_subscribed_admin: &'static str,

    /// Status message for subscribed regular users
    /// Placeholders: {language}
    pub status_subscribed_user: &'static str,

    /// Status message for non-subscribed users
    pub status_not_subscribed: &'static str,

    // ==================== Language Command Messages ====================
    /// Message shown when non-subscriber tries to use /language
    pub language_not_subscribed: &'static str,

    /// Message shown when language is changed to English
    pub language_changed_english: &'static str,

    /// Message shown when language is changed to Spanish
    pub language_changed_spanish: &'static str,

    /// Message shown when invalid language code is provided
    pub language_invalid: &'static str,

    /// Message showing current language and options
    /// Placeholders: {current}
    pub language_settings: &'static str,

    // ==================== Broadcast Messages ====================
    /// Message shown when non-admin tries to use /broadcast
    pub broadcast_admin_only: &'static str,

    /// Message shown when broadcast succeeds
    /// Placeholders: {count}
    pub broadcast_success: &'static str,

    /// Message shown when broadcast partially succeeds
    /// Placeholders: {sent}, {failed}, {total}
    pub broadcast_partial: &'static str,

    /// Message shown when broadcast fails
    /// Placeholders: {error}
    pub broadcast_failed: &'static str,

    /// Usage message for /broadcast command
    pub broadcast_usage: &'static str,

    // ==================== Other Messages ====================
    /// Message shown for unknown commands
    pub unknown_command: &'static str,

    /// Header for welcome summary sent to new subscribers
    pub welcome_summary_header: &'static str,
}

// ==================== English Strings ====================

/// English language strings (canonical)
/// NOTE: These strings are pre-escaped for Telegram MarkdownV2 format.
/// Special chars escaped: - . ! ( ) but NOT * which is used for bold formatting.
pub const ENGLISH_STRINGS: LanguageStrings = LanguageStrings {
    // Summary headers
    summary_header: "Twitter Summary",
    translation_failure_notice: "", // No notice needed for English

    // Welcome messages
    welcome_admin: "👋 Welcome to Twitter News Summary Bot\\!\n\n\
Commands:\n\
/subscribe \\- Get daily AI\\-powered summaries of Twitter/X news\n\
/unsubscribe \\- Stop receiving summaries\n\
/status \\- Check your subscription status\n\
/language \\- Change summary language \\(en/es\\)\n\
/broadcast \\- Send a message to all subscribers \\(admin only\\)\n\n\
Summaries are sent twice daily with the latest tweets from tech leaders and AI researchers\\.",

    welcome_user: "👋 Welcome to Twitter News Summary Bot\\!\n\n\
Commands:\n\
/subscribe \\- Get daily AI\\-powered summaries of Twitter/X news\n\
/unsubscribe \\- Stop receiving summaries\n\
/status \\- Check your subscription status\n\
/language \\- Change summary language \\(en/es\\)\n\n\
Summaries are sent twice daily with the latest tweets from tech leaders and AI researchers\\.",

    // Subscription messages
    subscribe_already: "✅ You're already subscribed\\!",
    subscribe_success: "✅ Successfully subscribed\\! You'll receive summaries twice daily\\.\n\n\
Want summaries in Spanish? Use /language es to switch\\.",
    unsubscribe_success: "👋 Successfully unsubscribed\\. You won't receive any more summaries\\.",
    unsubscribe_not_subscribed: "You're not currently subscribed\\.",

    // Status messages
    status_subscribed_admin:
        "✅ You are subscribed\n🌐 Language: {language}\n📊 Total subscribers: {count}",
    status_subscribed_user: "✅ You are subscribed\n🌐 Language: {language}",
    status_not_subscribed:
        "❌ You are not subscribed\n\nUse /subscribe to start receiving summaries\\.",

    // Language messages
    language_not_subscribed: "You need to subscribe first\\. Use /subscribe to get started\\.",
    language_changed_english:
        "✅ Language changed to English\\. You'll receive summaries in English\\.",
    language_changed_spanish:
        "✅ Idioma cambiado a español\\. Recibirás los resúmenes en español\\.",
    language_invalid:
        "Invalid language\\. Available options:\n/language en \\- English\n/language es \\- Spanish",
    language_settings: "🌐 *Language Settings*\n\nCurrent: {current}\n\n\
To change, use:\n/language en \\- English\n/language es \\- Spanish",

    // Broadcast messages
    broadcast_admin_only: "⛔ This command is only available to the bot administrator\\.",
    broadcast_success: "✅ *Broadcast sent successfully*\\!\n\n📊 Delivered to {count} subscribers",
    broadcast_partial:
        "📡 *Broadcast completed*\n\n✅ Sent: {sent}\n❌ Failed: {failed}\n📊 Total: {total}",
    broadcast_failed: "❌ Broadcast failed: {error}",
    broadcast_usage:
        "Usage: /broadcast Your message here\n\nSends a plain text message to all subscribers\\.",

    // Other
    unknown_command: "Unknown command\\. Use /start to see available commands\\.",
    welcome_summary_header: "📰 *Hey\\! Here's what you missed* 😉",
};

// ==================== Spanish Strings ====================

/// Spanish language strings
/// NOTE: These strings are pre-escaped for Telegram MarkdownV2 format.
/// Special chars escaped: - . ! ( ) but NOT * which is used for bold formatting.
pub const SPANISH_STRINGS: LanguageStrings = LanguageStrings {
    // Summary headers
    summary_header: "Resumen de Twitter",
    translation_failure_notice: "\\[Nota: La traducción no está disponible\\. Enviando en inglés\\.\\]\n\n",

    // Welcome messages
    welcome_admin: "👋 ¡Bienvenido al Bot de Resumen de Noticias de Twitter\\!\n\n\
Comandos:\n\
/subscribe \\- Recibe resúmenes diarios de noticias de Twitter/X con IA\n\
/unsubscribe \\- Deja de recibir resúmenes\n\
/status \\- Consulta tu estado de suscripción\n\
/language \\- Cambia el idioma de los resúmenes \\(en/es\\)\n\
/broadcast \\- Envía un mensaje a todos los suscriptores \\(solo admin\\)\n\n\
Los resúmenes se envían dos veces al día con los últimos tweets de líderes tecnológicos e investigadores de IA\\.",

    welcome_user: "👋 ¡Bienvenido al Bot de Resumen de Noticias de Twitter\\!\n\n\
Comandos:\n\
/subscribe \\- Recibe resúmenes diarios de noticias de Twitter/X con IA\n\
/unsubscribe \\- Deja de recibir resúmenes\n\
/status \\- Consulta tu estado de suscripción\n\
/language \\- Cambia el idioma de los resúmenes \\(en/es\\)\n\n\
Los resúmenes se envían dos veces al día con los últimos tweets de líderes tecnológicos e investigadores de IA\\.",

    // Subscription messages
    subscribe_already: "✅ ¡Ya estás suscrito\\!",
    subscribe_success: "✅ ¡Suscripción exitosa\\! Recibirás resúmenes dos veces al día\\.\n\n\
¿Prefieres los resúmenes en inglés? Usa /language en para cambiar\\.",
    unsubscribe_success: "👋 Suscripción cancelada exitosamente\\. No recibirás más resúmenes\\.",
    unsubscribe_not_subscribed: "No estás suscrito actualmente\\.",

    // Status messages
    status_subscribed_admin: "✅ Estás suscrito\n🌐 Idioma: {language}\n📊 Total de suscriptores: {count}",
    status_subscribed_user: "✅ Estás suscrito\n🌐 Idioma: {language}",
    status_not_subscribed: "❌ No estás suscrito\n\nUsa /subscribe para comenzar a recibir resúmenes\\.",

    // Language messages
    language_not_subscribed: "Primero necesitas suscribirte\\. Usa /subscribe para comenzar\\.",
    language_changed_english: "✅ Language changed to English\\. You'll receive summaries in English\\.",
    language_changed_spanish: "✅ Idioma cambiado a español\\. Recibirás los resúmenes en español\\.",
    language_invalid: "Idioma inválido\\. Opciones disponibles:\n/language en \\- English\n/language es \\- Español",
    language_settings: "🌐 *Configuración de Idioma*\n\nActual: {current}\n\n\
Para cambiar, usa:\n/language en \\- English\n/language es \\- Español",

    // Broadcast messages
    broadcast_admin_only: "⛔ Este comando solo está disponible para el administrador del bot\\.",
    broadcast_success: "✅ *¡Difusión enviada exitosamente*\\!\n\n📊 Entregado a {count} suscriptores",
    broadcast_partial: "📡 *Difusión completada*\n\n✅ Enviados: {sent}\n❌ Fallidos: {failed}\n📊 Total: {total}",
    broadcast_failed: "❌ Difusión fallida: {error}",
    broadcast_usage: "Uso: /broadcast Tu mensaje aquí\n\nEnvía un mensaje de texto plano a todos los suscriptores\\.",

    // Other
    unknown_command: "Comando desconocido\\. Usa /start para ver los comandos disponibles\\.",
    welcome_summary_header: "📰 *¡Hey\\! Esto es lo que te perdiste* 😉",
};

#[cfg(test)]
mod tests {
    use super::*;

    // ==================== English Strings Tests ====================

    #[test]
    fn test_english_summary_header_not_empty() {
        assert!(!ENGLISH_STRINGS.summary_header.is_empty());
    }

    #[test]
    fn test_english_welcome_admin_contains_commands() {
        assert!(ENGLISH_STRINGS.welcome_admin.contains("/subscribe"));
        assert!(ENGLISH_STRINGS.welcome_admin.contains("/broadcast"));
    }

    #[test]
    fn test_english_welcome_user_no_broadcast() {
        assert!(ENGLISH_STRINGS.welcome_user.contains("/subscribe"));
        assert!(!ENGLISH_STRINGS.welcome_user.contains("/broadcast"));
    }

    #[test]
    fn test_english_translation_failure_notice_is_empty() {
        assert_eq!(ENGLISH_STRINGS.translation_failure_notice, "");
    }

    #[test]
    fn test_english_status_messages_have_placeholders() {
        assert!(ENGLISH_STRINGS
            .status_subscribed_admin
            .contains("{language}"));
        assert!(ENGLISH_STRINGS.status_subscribed_admin.contains("{count}"));
        assert!(ENGLISH_STRINGS
            .status_subscribed_user
            .contains("{language}"));
    }

    // ==================== Spanish Strings Tests ====================

    #[test]
    fn test_spanish_summary_header_not_empty() {
        assert!(!SPANISH_STRINGS.summary_header.is_empty());
    }

    #[test]
    fn test_spanish_welcome_admin_contains_commands() {
        assert!(SPANISH_STRINGS.welcome_admin.contains("/subscribe"));
        assert!(SPANISH_STRINGS.welcome_admin.contains("/broadcast"));
    }

    #[test]
    fn test_spanish_welcome_user_no_broadcast() {
        assert!(SPANISH_STRINGS.welcome_user.contains("/subscribe"));
        assert!(!SPANISH_STRINGS.welcome_user.contains("/broadcast"));
    }

    #[test]
    fn test_spanish_translation_failure_notice_not_empty() {
        assert!(!SPANISH_STRINGS.translation_failure_notice.is_empty());
        assert!(SPANISH_STRINGS
            .translation_failure_notice
            .contains("traducción"));
    }

    #[test]
    fn test_spanish_status_messages_have_placeholders() {
        assert!(SPANISH_STRINGS
            .status_subscribed_admin
            .contains("{language}"));
        assert!(SPANISH_STRINGS.status_subscribed_admin.contains("{count}"));
        assert!(SPANISH_STRINGS
            .status_subscribed_user
            .contains("{language}"));
    }

    // ==================== Placeholder Tests ====================

    #[test]
    fn test_broadcast_success_placeholder() {
        assert!(ENGLISH_STRINGS.broadcast_success.contains("{count}"));
        assert!(SPANISH_STRINGS.broadcast_success.contains("{count}"));
    }

    #[test]
    fn test_broadcast_partial_placeholders() {
        assert!(ENGLISH_STRINGS.broadcast_partial.contains("{sent}"));
        assert!(ENGLISH_STRINGS.broadcast_partial.contains("{failed}"));
        assert!(ENGLISH_STRINGS.broadcast_partial.contains("{total}"));
    }

    #[test]
    fn test_language_settings_placeholder() {
        assert!(ENGLISH_STRINGS.language_settings.contains("{current}"));
        assert!(SPANISH_STRINGS.language_settings.contains("{current}"));
    }

    // ==================== MarkdownV2 Validation Tests ====================
    //
    // These tests ensure that all template strings are properly pre-escaped for
    // Telegram's MarkdownV2 format. This prevents the "Character 'X' is reserved
    // and must be escaped" errors that can occur in production.
    //
    // MarkdownV2 special characters that MUST be escaped: _ * [ ] ( ) ~ ` > # + - = | { } . !
    // EXCEPTION: * is used intentionally for bold formatting and should NOT be escaped in those contexts.

    /// Characters that must be escaped in MarkdownV2 (except when used for formatting)
    /// Note: This constant is kept for documentation purposes and potential future use.
    #[allow(dead_code)]
    const MARKDOWNV2_SPECIAL_CHARS: [char; 18] = [
        '_', '*', '[', ']', '(', ')', '~', '`', '>', '#', '+', '-', '=', '|', '{', '}', '.', '!',
    ];

    /// Characters that are typically problematic (the ones that caused the production bug)
    /// These are often forgotten when writing template strings.
    const COMMONLY_FORGOTTEN_CHARS: [char; 5] = ['-', '.', '!', '(', ')'];

    /// Check if a character at a given position in a string is properly escaped.
    /// A character is properly escaped if it's preceded by a backslash that is NOT itself escaped.
    fn is_escaped_at(s: &str, pos: usize) -> bool {
        if pos == 0 {
            return false;
        }
        let bytes = s.as_bytes();
        let mut backslash_count = 0;
        let mut check_pos = pos;
        while check_pos > 0 {
            check_pos -= 1;
            if bytes[check_pos] == b'\\' {
                backslash_count += 1;
            } else {
                break;
            }
        }
        // Character is escaped if preceded by an odd number of backslashes
        backslash_count % 2 == 1
    }

    /// Check if a character appears unescaped in a string.
    /// Returns the positions of all unescaped occurrences.
    fn find_unescaped_chars(s: &str, chars: &[char]) -> Vec<(usize, char)> {
        let mut unescaped = Vec::new();

        for (pos, c) in s.char_indices() {
            if chars.contains(&c) && !is_escaped_at(s, pos) {
                // Special case: * is allowed unescaped when used for bold formatting (*text*)
                // Check if this is part of a bold pattern
                if c == '*' && is_bold_formatting_asterisk(s, pos) {
                    continue;
                }
                // Special case: { and } are allowed for placeholders like {language}
                if (c == '{' || c == '}') && is_placeholder_brace(s, pos, c) {
                    continue;
                }
                unescaped.push((pos, c));
            }
        }
        unescaped
    }

    /// Check if an asterisk at the given position is part of bold formatting (*text*)
    fn is_bold_formatting_asterisk(s: &str, pos: usize) -> bool {
        let bytes = s.as_bytes();
        let len = bytes.len();

        // Look for opening asterisk: followed by non-whitespace, non-asterisk
        if pos + 1 < len {
            let next_byte = bytes[pos + 1];
            if next_byte != b' ' && next_byte != b'\n' && next_byte != b'*' {
                // Could be opening asterisk of bold
                // Search for closing asterisk
                if let Some(rest) = s.get(pos + 1..) {
                    if rest.contains('*') {
                        return true;
                    }
                }
            }
        }

        // Look for closing asterisk: preceded by non-whitespace, non-asterisk
        if pos > 0 {
            let prev_byte = bytes[pos - 1];
            if prev_byte != b' ' && prev_byte != b'\n' && prev_byte != b'*' && prev_byte != b'\\' {
                return true;
            }
        }

        false
    }

    /// Check if a brace at the given position is part of a placeholder like {language}
    fn is_placeholder_brace(s: &str, pos: usize, c: char) -> bool {
        if c == '{' {
            // Opening brace: check if followed by alphanumeric and eventually }
            if let Some(rest) = s.get(pos + 1..) {
                // Valid placeholder: {word} where word is alphanumeric/underscore
                for (i, ch) in rest.char_indices() {
                    if ch == '}' && i > 0 {
                        return true; // Found closing brace
                    }
                    if !ch.is_alphanumeric() && ch != '_' {
                        return false; // Invalid character for placeholder name
                    }
                }
            }
        } else if c == '}' {
            // Closing brace: check if there's a matching { before it
            if let Some(before) = s.get(..pos) {
                if let Some(open_pos) = before.rfind('{') {
                    let between = &before[open_pos + 1..];
                    // Check if all characters between { and } are valid placeholder chars
                    if !between.is_empty()
                        && between.chars().all(|ch| ch.is_alphanumeric() || ch == '_')
                    {
                        return true;
                    }
                }
            }
        }
        false
    }

    /// Validate a template string for proper MarkdownV2 escaping.
    /// Returns a list of validation errors if any issues are found.
    fn validate_markdownv2_template(name: &str, template: &str) -> Vec<String> {
        let mut errors = Vec::new();

        // Check for commonly forgotten characters (the ones that caused the bug)
        let unescaped = find_unescaped_chars(template, &COMMONLY_FORGOTTEN_CHARS);
        for (pos, c) in unescaped {
            errors.push(format!(
                "{}: Unescaped '{}' at position {} - must be escaped as '\\{}'",
                name, c, pos, c
            ));
        }

        errors
    }

    /// Helper to get all string fields from a LanguageStrings struct
    fn get_all_string_fields(strings: &LanguageStrings) -> Vec<(&'static str, &'static str)> {
        vec![
            ("summary_header", strings.summary_header),
            (
                "translation_failure_notice",
                strings.translation_failure_notice,
            ),
            ("welcome_admin", strings.welcome_admin),
            ("welcome_user", strings.welcome_user),
            ("subscribe_already", strings.subscribe_already),
            ("subscribe_success", strings.subscribe_success),
            ("unsubscribe_success", strings.unsubscribe_success),
            (
                "unsubscribe_not_subscribed",
                strings.unsubscribe_not_subscribed,
            ),
            ("status_subscribed_admin", strings.status_subscribed_admin),
            ("status_subscribed_user", strings.status_subscribed_user),
            ("status_not_subscribed", strings.status_not_subscribed),
            ("language_not_subscribed", strings.language_not_subscribed),
            ("language_changed_english", strings.language_changed_english),
            ("language_changed_spanish", strings.language_changed_spanish),
            ("language_invalid", strings.language_invalid),
            ("language_settings", strings.language_settings),
            ("broadcast_admin_only", strings.broadcast_admin_only),
            ("broadcast_success", strings.broadcast_success),
            ("broadcast_partial", strings.broadcast_partial),
            ("broadcast_failed", strings.broadcast_failed),
            ("broadcast_usage", strings.broadcast_usage),
            ("unknown_command", strings.unknown_command),
            ("welcome_summary_header", strings.welcome_summary_header),
        ]
    }

    // ---------- English MarkdownV2 Validation Tests ----------

    #[test]
    fn test_english_all_templates_valid_markdownv2() {
        let fields = get_all_string_fields(&ENGLISH_STRINGS);
        let mut all_errors = Vec::new();

        for (name, template) in fields {
            let errors =
                validate_markdownv2_template(&format!("ENGLISH_STRINGS.{}", name), template);
            all_errors.extend(errors);
        }

        if !all_errors.is_empty() {
            panic!(
                "English templates have invalid MarkdownV2 escaping:\n{}",
                all_errors.join("\n")
            );
        }
    }

    #[test]
    fn test_english_welcome_admin_special_chars_escaped() {
        // This template contains many special characters that need escaping
        let template = ENGLISH_STRINGS.welcome_admin;

        // Should contain escaped hyphens for command descriptions
        assert!(
            template.contains("\\-"),
            "Hyphens in welcome_admin should be escaped"
        );

        // Should contain escaped parentheses
        assert!(
            template.contains("\\(") && template.contains("\\)"),
            "Parentheses in welcome_admin should be escaped"
        );

        // Should contain escaped periods
        assert!(
            template.contains("\\."),
            "Periods in welcome_admin should be escaped"
        );

        // Should contain escaped exclamation marks
        assert!(
            template.contains("\\!"),
            "Exclamation marks in welcome_admin should be escaped"
        );
    }

    #[test]
    fn test_english_status_subscribed_admin_special_chars_escaped() {
        let template = ENGLISH_STRINGS.status_subscribed_admin;
        let errors = validate_markdownv2_template("status_subscribed_admin", template);
        assert!(
            errors.is_empty(),
            "status_subscribed_admin should have all special chars escaped: {:?}",
            errors
        );
    }

    #[test]
    fn test_english_language_settings_special_chars_escaped() {
        let template = ENGLISH_STRINGS.language_settings;
        let errors = validate_markdownv2_template("language_settings", template);
        assert!(
            errors.is_empty(),
            "language_settings should have all special chars escaped: {:?}",
            errors
        );

        // Should preserve bold formatting with unescaped *
        assert!(
            template.contains("*Language Settings*"),
            "Bold formatting should be preserved"
        );
    }

    // ---------- Spanish MarkdownV2 Validation Tests ----------

    #[test]
    fn test_spanish_all_templates_valid_markdownv2() {
        let fields = get_all_string_fields(&SPANISH_STRINGS);
        let mut all_errors = Vec::new();

        for (name, template) in fields {
            let errors =
                validate_markdownv2_template(&format!("SPANISH_STRINGS.{}", name), template);
            all_errors.extend(errors);
        }

        if !all_errors.is_empty() {
            panic!(
                "Spanish templates have invalid MarkdownV2 escaping:\n{}",
                all_errors.join("\n")
            );
        }
    }

    #[test]
    fn test_spanish_welcome_admin_special_chars_escaped() {
        let template = SPANISH_STRINGS.welcome_admin;

        // Should contain escaped hyphens
        assert!(
            template.contains("\\-"),
            "Hyphens in Spanish welcome_admin should be escaped"
        );

        // Should contain escaped parentheses
        assert!(
            template.contains("\\(") && template.contains("\\)"),
            "Parentheses in Spanish welcome_admin should be escaped"
        );

        // Should contain escaped periods
        assert!(
            template.contains("\\."),
            "Periods in Spanish welcome_admin should be escaped"
        );

        // Should contain escaped exclamation marks
        assert!(
            template.contains("\\!"),
            "Exclamation marks in Spanish welcome_admin should be escaped"
        );
    }

    #[test]
    fn test_spanish_translation_failure_notice_special_chars_escaped() {
        let template = SPANISH_STRINGS.translation_failure_notice;
        let errors = validate_markdownv2_template("translation_failure_notice", template);
        assert!(
            errors.is_empty(),
            "translation_failure_notice should have all special chars escaped: {:?}",
            errors
        );

        // Should have escaped brackets for [Nota: ...]
        assert!(
            template.contains("\\[") && template.contains("\\]"),
            "Brackets in translation_failure_notice should be escaped"
        );
    }

    // ---------- Bold Formatting Preservation Tests ----------

    #[test]
    fn test_bold_formatting_preserved_in_english() {
        // Templates that use bold formatting should have unescaped * around text
        assert!(
            ENGLISH_STRINGS
                .language_settings
                .contains("*Language Settings*"),
            "Bold formatting should be preserved in language_settings"
        );
        assert!(
            ENGLISH_STRINGS
                .broadcast_success
                .contains("*Broadcast sent successfully*"),
            "Bold formatting should be preserved in broadcast_success"
        );
        assert!(
            ENGLISH_STRINGS
                .broadcast_partial
                .contains("*Broadcast completed*"),
            "Bold formatting should be preserved in broadcast_partial"
        );
        assert!(
            ENGLISH_STRINGS
                .welcome_summary_header
                .contains("*Hey\\! Here's what you missed*"),
            "Bold formatting should be preserved in welcome_summary_header"
        );
    }

    #[test]
    fn test_bold_formatting_preserved_in_spanish() {
        assert!(
            SPANISH_STRINGS
                .language_settings
                .contains("*Configuración de Idioma*"),
            "Bold formatting should be preserved in Spanish language_settings"
        );
        assert!(
            SPANISH_STRINGS
                .broadcast_success
                .contains("*¡Difusión enviada exitosamente*"),
            "Bold formatting should be preserved in Spanish broadcast_success"
        );
        assert!(
            SPANISH_STRINGS
                .broadcast_partial
                .contains("*Difusión completada*"),
            "Bold formatting should be preserved in Spanish broadcast_partial"
        );
    }

    // ---------- Placeholder Substitution Tests ----------

    #[test]
    fn test_placeholder_substitution_with_escaped_template() {
        // Test that placeholder substitution works correctly with pre-escaped templates
        let template = ENGLISH_STRINGS.status_subscribed_admin;

        // Substitute placeholders
        let result = template
            .replace("{language}", "English")
            .replace("{count}", "42");

        assert!(result.contains("English"));
        assert!(result.contains("42"));
        // Should still have the emojis and structure
        assert!(result.contains("✅"));
        assert!(result.contains("🌐"));
        assert!(result.contains("📊"));
    }

    #[test]
    fn test_placeholder_substitution_preserves_escaping() {
        let template = ENGLISH_STRINGS.language_settings;

        // Substitute placeholder
        let result = template.replace("{current}", "English");

        // The escaped characters should still be escaped
        assert!(
            result.contains("\\-"),
            "Escaping should be preserved after placeholder substitution"
        );
    }

    #[test]
    fn test_all_placeholders_have_valid_syntax() {
        // All placeholders should use the {name} format
        let placeholder_pattern = regex::Regex::new(r"\{([a-z_]+)\}").unwrap();

        let english_fields = get_all_string_fields(&ENGLISH_STRINGS);
        let spanish_fields = get_all_string_fields(&SPANISH_STRINGS);

        // Known valid placeholders
        let valid_placeholders = [
            "language", "count", "current", "sent", "failed", "total", "error",
        ];

        for (name, template) in english_fields.iter().chain(spanish_fields.iter()) {
            for cap in placeholder_pattern.captures_iter(template) {
                let placeholder_name = cap.get(1).unwrap().as_str();
                assert!(
                    valid_placeholders.contains(&placeholder_name),
                    "Unknown placeholder '{{{}}}' in {}: should be one of {:?}",
                    placeholder_name,
                    name,
                    valid_placeholders
                );
            }
        }
    }

    // ---------- Regression Test: Specific Characters That Caused the Bug ----------

    #[test]
    fn test_regression_hyphen_escaping() {
        // The hyphen (-) was one of the characters that caused the production bug.
        // All hyphens in user-facing strings must be escaped as \-

        // Test specific templates that are known to use hyphens
        let templates_with_hyphens = [
            ("welcome_admin", ENGLISH_STRINGS.welcome_admin),
            ("welcome_user", ENGLISH_STRINGS.welcome_user),
            ("language_invalid", ENGLISH_STRINGS.language_invalid),
            ("language_settings", ENGLISH_STRINGS.language_settings),
            ("Spanish welcome_admin", SPANISH_STRINGS.welcome_admin),
            ("Spanish welcome_user", SPANISH_STRINGS.welcome_user),
        ];

        for (name, template) in templates_with_hyphens {
            let unescaped = find_unescaped_chars(template, &['-']);
            assert!(
                unescaped.is_empty(),
                "Regression: {} contains unescaped hyphens at positions: {:?}",
                name,
                unescaped
            );
        }
    }

    #[test]
    fn test_regression_period_escaping() {
        // The period (.) was one of the characters that caused the production bug.
        let templates_with_periods = [
            ("subscribe_success", ENGLISH_STRINGS.subscribe_success),
            ("unsubscribe_success", ENGLISH_STRINGS.unsubscribe_success),
            ("broadcast_usage", ENGLISH_STRINGS.broadcast_usage),
            (
                "Spanish subscribe_success",
                SPANISH_STRINGS.subscribe_success,
            ),
        ];

        for (name, template) in templates_with_periods {
            let unescaped = find_unescaped_chars(template, &['.']);
            assert!(
                unescaped.is_empty(),
                "Regression: {} contains unescaped periods at positions: {:?}",
                name,
                unescaped
            );
        }
    }

    #[test]
    fn test_regression_exclamation_escaping() {
        // The exclamation mark (!) was one of the characters that caused the production bug.
        let templates_with_exclamations = [
            ("subscribe_already", ENGLISH_STRINGS.subscribe_already),
            ("subscribe_success", ENGLISH_STRINGS.subscribe_success),
            ("broadcast_success", ENGLISH_STRINGS.broadcast_success),
            (
                "Spanish subscribe_already",
                SPANISH_STRINGS.subscribe_already,
            ),
        ];

        for (name, template) in templates_with_exclamations {
            let unescaped = find_unescaped_chars(template, &['!']);
            assert!(
                unescaped.is_empty(),
                "Regression: {} contains unescaped exclamation marks at positions: {:?}",
                name,
                unescaped
            );
        }
    }

    #[test]
    fn test_regression_parentheses_escaping() {
        // Parentheses ( and ) were characters that caused the production bug.
        let templates_with_parentheses = [
            ("welcome_admin", ENGLISH_STRINGS.welcome_admin),
            ("welcome_user", ENGLISH_STRINGS.welcome_user),
            ("Spanish welcome_admin", SPANISH_STRINGS.welcome_admin),
            ("Spanish welcome_user", SPANISH_STRINGS.welcome_user),
        ];

        for (name, template) in templates_with_parentheses {
            let unescaped = find_unescaped_chars(template, &['(', ')']);
            assert!(
                unescaped.is_empty(),
                "Regression: {} contains unescaped parentheses at positions: {:?}",
                name,
                unescaped
            );
        }
    }

    // ---------- Comprehensive Field-by-Field Validation ----------

    #[test]
    fn test_every_english_field_individually() {
        // Test each field individually to make error messages more specific
        let fields = get_all_string_fields(&ENGLISH_STRINGS);

        for (name, template) in fields {
            let errors = validate_markdownv2_template(name, template);
            assert!(
                errors.is_empty(),
                "ENGLISH_STRINGS.{} has MarkdownV2 escaping errors:\n{}",
                name,
                errors.join("\n")
            );
        }
    }

    #[test]
    fn test_every_spanish_field_individually() {
        // Test each field individually to make error messages more specific
        let fields = get_all_string_fields(&SPANISH_STRINGS);

        for (name, template) in fields {
            let errors = validate_markdownv2_template(name, template);
            assert!(
                errors.is_empty(),
                "SPANISH_STRINGS.{} has MarkdownV2 escaping errors:\n{}",
                name,
                errors.join("\n")
            );
        }
    }
}
