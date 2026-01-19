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
}
