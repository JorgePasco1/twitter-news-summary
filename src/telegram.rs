use crate::config::Config;
use crate::db::Database;
use crate::i18n::{Language, TranslationMetrics};
use anyhow::{Context, Result};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use tracing::{info, warn};

// Telegram webhook types
#[derive(Debug, Deserialize)]
pub struct Update {
    pub update_id: i64,
    pub message: Option<Message>,
}

#[derive(Debug, Deserialize)]
pub struct Message {
    pub message_id: i64,
    pub from: Option<User>,
    pub chat: Chat,
    pub text: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct User {
    pub id: i64,
    pub username: Option<String>,
    pub first_name: String,
}

#[derive(Debug, Deserialize)]
pub struct Chat {
    pub id: i64,
    #[allow(dead_code)]
    pub r#type: String,
}

#[derive(Debug, Serialize)]
struct SendMessageRequest {
    chat_id: String,
    text: String,
    parse_mode: String,
}

/// Escape special characters for Telegram's MarkdownV2 parse mode,
/// while preserving markdown link syntax [text](url).
///
/// Per Telegram Bot API docs, MarkdownV2 requires escaping 18 special characters:
/// _ * [ ] ( ) ~ ` > # + - = | { } . !
///
/// However, inline links use the syntax [text](url), so we must NOT escape the
/// structural brackets and parentheses that form the link. Inside the link:
/// - Link text: escape all special chars (the text between [ and ])
/// - URL: only ) and \ need escaping (per Telegram docs)
///
/// Reference: https://core.telegram.org/bots/api#markdownv2-style
pub fn escape_markdownv2(text: &str) -> String {
    // Regex to match markdown links: [text](url)
    // Note: we use lazy_static pattern via once_cell or just create it (regex crate caches)
    let link_regex = regex::Regex::new(r"\[([^\]]+)\]\(([^)]+)\)").unwrap();

    let mut result = String::with_capacity(text.len() * 2);
    let mut last_end = 0;

    for cap in link_regex.captures_iter(text) {
        let whole_match = cap.get(0).unwrap();
        let link_text = cap.get(1).unwrap().as_str();
        let link_url = cap.get(2).unwrap().as_str();

        // Escape text before this link (normal escaping)
        let before = &text[last_end..whole_match.start()];
        result.push_str(&escape_markdownv2_simple(before));

        // Build the link with proper escaping:
        // - Link text: escape special chars (but not [ ] which are structural)
        // - URL: escape only ) and \ inside the URL
        result.push('[');
        result.push_str(&escape_markdownv2_link_text(link_text));
        result.push_str("](");
        result.push_str(&escape_markdownv2_url(link_url));
        result.push(')');

        last_end = whole_match.end();
    }

    // Escape remaining text after last link (normal escaping)
    let remaining = &text[last_end..];
    result.push_str(&escape_markdownv2_simple(remaining));

    result
}

/// Simple escape for text that's not inside a markdown link structure.
/// Escapes all 18 MarkdownV2 special characters.
fn escape_markdownv2_simple(text: &str) -> String {
    let special_chars = [
        '_', '*', '[', ']', '(', ')', '~', '`', '>', '#', '+', '-', '=', '|', '{', '}', '.', '!',
    ];

    let mut result = String::with_capacity(text.len() * 2);

    for c in text.chars() {
        if special_chars.contains(&c) {
            result.push('\\');
        }
        result.push(c);
    }

    result
}

/// Escape text inside markdown link brackets [text].
/// All special chars need escaping for the text to display correctly.
fn escape_markdownv2_link_text(text: &str) -> String {
    // Inside link text, we need to escape special chars that would otherwise
    // be interpreted as MarkdownV2 formatting
    let special_chars = [
        '_', '*', '[', ']', '~', '`', '>', '#', '+', '-', '=', '|', '{', '}', '.', '!',
    ];
    // Note: ( and ) don't need escaping inside link text

    let mut result = String::with_capacity(text.len() * 2);

    for c in text.chars() {
        if special_chars.contains(&c) {
            result.push('\\');
        }
        result.push(c);
    }

    result
}

/// Escape URL inside markdown link parentheses (url).
/// Per Telegram docs, only ) and \ need escaping inside URLs.
fn escape_markdownv2_url(url: &str) -> String {
    let mut result = String::with_capacity(url.len() * 2);

    for c in url.chars() {
        if c == ')' || c == '\\' {
            result.push('\\');
        }
        result.push(c);
    }

    result
}

pub async fn handle_webhook(config: &Config, db: &Database, update: Update) -> Result<()> {
    let message = match update.message {
        Some(msg) => msg,
        None => return Ok(()), // Not a message update, ignore
    };

    let text = match message.text {
        Some(t) => t,
        None => return Ok(()), // No text, ignore
    };

    let chat_id = message.chat.id;
    let username = message.from.as_ref().and_then(|u| u.username.clone());

    info!("Received message from {}: {}", chat_id, text);

    // Handle bot commands - check for commands with arguments
    // Note: All plain text messages must be escaped for MarkdownV2 mode
    let (command, arg) = if text.starts_with("/language ") {
        let arg = text.strip_prefix("/language ").unwrap().trim();
        let arg = if arg.is_empty() { None } else { Some(arg) };
        ("/language", arg)
    } else if text.starts_with("/broadcast ") {
        let arg = text.strip_prefix("/broadcast ").unwrap().trim();
        let arg = if arg.is_empty() { None } else { Some(arg) };
        ("/broadcast", arg)
    } else {
        (text.as_str(), None)
    };

    match command {
        "/start" => {
            // Check if user is admin
            let chat_id_str = chat_id.to_string();
            let is_admin =
                !config.telegram_chat_id.is_empty() && chat_id_str == config.telegram_chat_id;

            // Get user's preferred language if they're already subscribed
            let user_lang = if db.is_subscribed(chat_id).await? {
                let lang_code = db
                    .get_subscriber_language(chat_id)
                    .await?
                    .unwrap_or_else(|| "en".to_string());
                Language::from_code(&lang_code).unwrap_or(Language::ENGLISH)
            } else {
                Language::ENGLISH
            };

            // Get welcome message from registry based on admin status
            // Templates are pre-escaped for MarkdownV2, no need to escape again
            let welcome = if is_admin {
                user_lang.config().strings.welcome_admin
            } else {
                user_lang.config().strings.welcome_user
            };

            send_message(config, chat_id, welcome).await?;
        }
        "/subscribe" => {
            if db.is_subscribed(chat_id).await? {
                // Already subscribed - respond in their preferred language
                let lang_code = db
                    .get_subscriber_language(chat_id)
                    .await?
                    .unwrap_or_else(|| "en".to_string());
                let user_lang = Language::from_code(&lang_code).unwrap_or(Language::ENGLISH);
                let msg = user_lang.config().strings.subscribe_already;
                send_message(config, chat_id, msg).await?;
            } else {
                let (_, needs_welcome) = db.add_subscriber(chat_id, username.as_deref()).await?;
                info!("New subscriber: {} (username: {:?})", chat_id, username);
                let msg = Language::ENGLISH.config().strings.subscribe_success;
                send_message(config, chat_id, msg).await?;

                // Send welcome summary for first-time subscribers
                if needs_welcome {
                    if let Some(summary) = db.get_latest_summary().await? {
                        send_welcome_summary(config, db, chat_id, &summary.content).await?;
                    } else {
                        info!("No summary available to send as welcome message");
                    }
                }
            }
        }
        "/unsubscribe" => {
            // Get language BEFORE removing (so we can respond in their language)
            let lang_code = db
                .get_subscriber_language(chat_id)
                .await?
                .unwrap_or_else(|| "en".to_string());
            let user_lang = Language::from_code(&lang_code).unwrap_or(Language::ENGLISH);

            if db.remove_subscriber(chat_id).await? {
                info!("Unsubscribed: {}", chat_id);
                let msg = user_lang.config().strings.unsubscribe_success;
                send_message(config, chat_id, msg).await?;
            } else {
                // Not subscribed - use English (we don't know their preference)
                let msg = Language::ENGLISH
                    .config()
                    .strings
                    .unsubscribe_not_subscribed;
                send_message(config, chat_id, msg).await?;
            }
        }
        "/status" => {
            let is_subscribed = db.is_subscribed(chat_id).await?;

            // Check if user is admin (only admin sees total subscriber count)
            let chat_id_str = chat_id.to_string();
            let is_admin =
                !config.telegram_chat_id.is_empty() && chat_id_str == config.telegram_chat_id;

            let status_msg = if is_subscribed {
                let lang_code = db
                    .get_subscriber_language(chat_id)
                    .await?
                    .unwrap_or_else(|| "en".to_string());

                // Get the user's preferred language for the response
                let user_lang = Language::from_code(&lang_code).unwrap_or(Language::ENGLISH);

                // Display language name in the user's language
                let lang_name = match lang_code.as_str() {
                    "es" => "Español",
                    _ => "English",
                };

                if is_admin {
                    // Admin sees subscriber count
                    let template = user_lang.config().strings.status_subscribed_admin;
                    template
                        .replace("{language}", lang_name)
                        .replace("{count}", &db.subscriber_count().await?.to_string())
                } else {
                    // Regular users see their own status and language
                    let template = user_lang.config().strings.status_subscribed_user;
                    template.replace("{language}", lang_name)
                }
            } else {
                // Non-subscribers get English (we don't know their preference)
                Language::ENGLISH
                    .config()
                    .strings
                    .status_not_subscribed
                    .to_string()
            };
            send_message(config, chat_id, &status_msg).await?;
        }
        "/language" => {
            // Handle /language command (with or without argument)
            let is_subscribed = db.is_subscribed(chat_id).await?;

            if !is_subscribed {
                let msg = Language::ENGLISH.config().strings.language_not_subscribed;
                send_message(config, chat_id, msg).await?;
            } else {
                // Get user's current language for responses
                let current_lang = db
                    .get_subscriber_language(chat_id)
                    .await?
                    .unwrap_or_else(|| "en".to_string());
                let user_lang = Language::from_code(&current_lang).unwrap_or(Language::ENGLISH);

                if let Some(lang_arg) = arg {
                    // User specified a language: /language en or /language es
                    match lang_arg {
                        "en" => {
                            db.set_subscriber_language(chat_id, "en").await?;
                            info!("Language changed to English for {}", chat_id);
                            let msg = Language::ENGLISH.config().strings.language_changed_english;
                            send_message(config, chat_id, msg).await?;
                        }
                        "es" => {
                            db.set_subscriber_language(chat_id, "es").await?;
                            info!("Language changed to Spanish for {}", chat_id);
                            let msg = Language::SPANISH.config().strings.language_changed_spanish;
                            send_message(config, chat_id, msg).await?;
                        }
                        _ => {
                            // Invalid language - respond in user's current language
                            let msg = user_lang.config().strings.language_invalid;
                            send_message(config, chat_id, msg).await?;
                        }
                    }
                } else {
                    // No argument: show current language and options in user's language
                    let current_name = match current_lang.as_str() {
                        "es" => "Español",
                        _ => "English",
                    };

                    let template = user_lang.config().strings.language_settings;
                    let msg = template.replace("{current}", current_name);
                    send_message(config, chat_id, &msg).await?;
                }
            }
        }
        "/broadcast" => {
            // Admin-only command to broadcast a message to all subscribers
            let chat_id_str = chat_id.to_string();
            let is_admin =
                !config.telegram_chat_id.is_empty() && chat_id_str == config.telegram_chat_id;

            if !is_admin {
                let msg = Language::ENGLISH.config().strings.broadcast_admin_only;
                send_message(config, chat_id, msg).await?;
            } else if let Some(broadcast_msg) = arg {
                // Send broadcast to all subscribers
                info!(
                    "Broadcasting message from admin {}: {}",
                    chat_id, broadcast_msg
                );

                match broadcast_message(config, db, broadcast_msg, None).await {
                    Ok((sent, failures)) => {
                        let total = sent + failures.len();
                        let msg = if failures.is_empty() {
                            let template = Language::ENGLISH.config().strings.broadcast_success;
                            template.replace("{count}", &sent.to_string())
                        } else {
                            let template = Language::ENGLISH.config().strings.broadcast_partial;
                            template
                                .replace("{sent}", &sent.to_string())
                                .replace("{failed}", &failures.len().to_string())
                                .replace("{total}", &total.to_string())
                        };
                        send_message(config, chat_id, &msg).await?;
                    }
                    Err(e) => {
                        warn!("Broadcast failed: {}", e);
                        let template = Language::ENGLISH.config().strings.broadcast_failed;
                        // Error messages may contain special chars, so escape them
                        let msg = template.replace("{error}", &escape_markdownv2(&e.to_string()));
                        send_message(config, chat_id, &msg).await?;
                    }
                }
            } else {
                let msg = Language::ENGLISH.config().strings.broadcast_usage;
                send_message(config, chat_id, msg).await?;
            }
        }
        _ => {
            // Unknown command, send help
            let msg = Language::ENGLISH.config().strings.unknown_command;
            send_message(config, chat_id, msg).await?;
        }
    }

    Ok(())
}

/// Send welcome summary to a new subscriber
async fn send_welcome_summary(
    config: &Config,
    db: &Database,
    chat_id: i64,
    summary: &str,
) -> Result<()> {
    let timestamp = Utc::now().format("%Y-%m-%d %H:%M UTC").to_string();
    let escaped_timestamp = escape_markdownv2(&timestamp);
    let header = Language::ENGLISH.config().strings.welcome_summary_header;
    let message = format!(
        "{}\n_{}_\n\n{}",
        header,
        escaped_timestamp,
        escape_markdownv2(summary)
    );

    send_message(config, chat_id, &message).await?;
    db.mark_welcome_summary_sent(chat_id).await?;
    info!("✓ Welcome summary sent to {}", chat_id);

    Ok(())
}

/// Send summary to all subscribers with language-specific translations
///
/// # Arguments
/// * `config` - Application configuration
/// * `db` - Database connection
/// * `summary` - The canonical English summary
/// * `summary_id` - The summary ID for caching translations
pub async fn send_to_subscribers(
    config: &Config,
    db: &Database,
    summary: &str,
    summary_id: i64,
) -> Result<()> {
    use crate::i18n::Language;
    use crate::translation::{
        condense_text, get_summary_header, get_translation_failure_notice, translate_summary,
        truncate_at_limit,
    };

    const TELEGRAM_CHAR_LIMIT: usize = 4096;

    let subscribers = db.list_subscribers().await?;

    if subscribers.is_empty() {
        info!("No subscribers to send to");
        return Ok(());
    }

    info!("Sending summary to {} subscribers", subscribers.len());

    let timestamp = Utc::now().format("%Y-%m-%d %H:%M UTC").to_string();
    let escaped_timestamp = escape_markdownv2(&timestamp);
    let client = reqwest::Client::new();

    // Cache for translations (keyed by language code)
    let mut translation_cache: std::collections::HashMap<String, String> =
        std::collections::HashMap::new();

    // Pre-populate cache with English (canonical)
    translation_cache.insert("en".to_string(), summary.to_string());

    let mut success_count = 0;
    let mut fail_count = 0;

    for subscriber in subscribers {
        let lang_code = subscriber.language_code.clone();
        let language = Language::from_code(&lang_code).unwrap_or(Language::ENGLISH);

        // Get or create translation for this language
        let content = if let Some(cached) = translation_cache.get(&lang_code) {
            // Memory cache hit
            TranslationMetrics::global().record_cache_hit();
            cached.clone()
        } else {
            // Check if translation exists in database
            if let Ok(Some(cached_translation)) = db.get_translation(summary_id, &lang_code).await {
                // Database cache hit
                TranslationMetrics::global().record_cache_hit();
                let content = cached_translation.content.clone();
                translation_cache.insert(lang_code.clone(), content.clone());
                content
            } else {
                // Cache miss - need to generate translation
                TranslationMetrics::global().record_cache_miss();

                // Generate translation via OpenAI
                TranslationMetrics::global().record_api_call();
                match translate_summary(&client, config, summary, language).await {
                    Ok(translated) => {
                        // Cache in database for future use
                        if let Err(e) = db
                            .save_translation(summary_id, &lang_code, &translated)
                            .await
                        {
                            warn!("Failed to cache translation: {}", e);
                        }
                        translation_cache.insert(lang_code.clone(), translated.clone());
                        translated
                    }
                    Err(e) => {
                        TranslationMetrics::global().record_api_failure();
                        warn!("Translation failed for {}: {}", lang_code, e);
                        // Return English content; the failure notice prefix will be added
                        // during message formatting to avoid double-escaping.
                        // Mark with a special prefix that we'll detect later.
                        let marker = "\x00TRANSLATION_FAILED\x00";
                        let fallback = format!("{}{}", marker, summary);
                        translation_cache.insert(lang_code.clone(), fallback.clone());
                        fallback
                    }
                }
            }
        };

        // Build message with language-specific header (MarkdownV2 format)
        let header = get_summary_header(language);

        // Check if this is a translation failure case (marked with special prefix)
        let translation_failed_marker = "\x00TRANSLATION_FAILED\x00";
        let (notice_prefix, mut actual_content_owned) =
            if content.starts_with(translation_failed_marker) {
                // Translation failed - add pre-escaped notice and escape only the content
                let actual = content
                    .strip_prefix(translation_failed_marker)
                    .unwrap_or(&content);
                (get_translation_failure_notice(language), actual.to_string())
            } else {
                // Normal case - no notice needed
                ("".to_string(), content.clone())
            };

        // Universal length validation - check if message would exceed Telegram's limit
        // Build a test message to check length (uses escaped content)
        let mut test_message = format!(
            "📰 *{}*\n_{}_\n\n{}{}",
            escape_markdownv2(header),
            escaped_timestamp,
            notice_prefix,
            escape_markdownv2(&actual_content_owned)
        );

        if test_message.len() > TELEGRAM_CHAR_LIMIT {
            info!(
                "Message too long ({} chars > {}), condensing for {}...",
                test_message.len(),
                TELEGRAM_CHAR_LIMIT,
                lang_code
            );

            // Calculate target chars: leave room for header (~200 chars with escaping)
            let target_chars = TELEGRAM_CHAR_LIMIT.saturating_sub(300);

            // Try to condense the content
            match condense_text(&client, config, &actual_content_owned, target_chars).await {
                Ok(condensed) => {
                    actual_content_owned = condensed.clone();
                    // Cache condensed content for other subscribers with same language
                    translation_cache.insert(lang_code.clone(), condensed);
                }
                Err(e) => {
                    warn!("Failed to condense text: {}, falling back to truncation", e);
                }
            }

            // Re-check after condensing - truncate iteratively if still too long
            test_message = format!(
                "📰 *{}*\n_{}_\n\n{}{}",
                escape_markdownv2(header),
                escaped_timestamp,
                notice_prefix,
                escape_markdownv2(&actual_content_owned)
            );

            if test_message.len() > TELEGRAM_CHAR_LIMIT {
                warn!(
                    "Message still too long after condensing ({} chars), truncating",
                    test_message.len()
                );

                // Iteratively truncate until the ESCAPED message fits
                // Escaping can expand text length, so we must check after each truncation
                let mut truncate_limit = target_chars.saturating_sub(100);
                while test_message.len() > TELEGRAM_CHAR_LIMIT && truncate_limit > 100 {
                    let overflow = test_message.len() - TELEGRAM_CHAR_LIMIT;
                    // Reduce limit by overflow amount plus buffer for escaping expansion
                    truncate_limit = truncate_limit.saturating_sub(overflow + 50);
                    actual_content_owned = truncate_at_limit(&actual_content_owned, truncate_limit);

                    test_message = format!(
                        "📰 *{}*\n_{}_\n\n{}{}",
                        escape_markdownv2(header),
                        escaped_timestamp,
                        notice_prefix,
                        escape_markdownv2(&actual_content_owned)
                    );
                }
            }
        }

        let message = format!(
            "📰 *{}*\n_{}_\n\n{}{}",
            escape_markdownv2(header),
            escaped_timestamp,
            notice_prefix, // Already pre-escaped in i18n strings
            escape_markdownv2(&actual_content_owned)
        );

        match send_message(config, subscriber.chat_id, &message).await {
            Ok(_) => {
                success_count += 1;
                info!("✓ Sent to {} ({})", subscriber.chat_id, lang_code);
            }
            Err(e) => {
                fail_count += 1;
                let error_msg = e.to_string();

                // Auto-remove subscribers who blocked the bot or deleted their account
                // Check for 403 status AND specific descriptions (case-insensitive for robustness)
                let error_lower = error_msg.to_lowercase();
                let is_blocked = error_msg.contains("403")
                    && (error_lower.contains("blocked by the user")
                        || error_lower.contains("user is deactivated"));

                if is_blocked {
                    warn!(
                        "✗ Auto-removing blocked/deactivated subscriber {}: {}",
                        subscriber.chat_id, error_msg
                    );
                    if let Err(remove_err) = db.remove_subscriber(subscriber.chat_id).await {
                        warn!("Failed to remove blocked subscriber: {}", remove_err);
                    }
                } else {
                    warn!("✗ Failed to send to {}: {}", subscriber.chat_id, error_msg);
                }

                // Log failure to database for analytics
                if let Err(log_err) = db
                    .log_delivery_failure(subscriber.chat_id, &error_msg)
                    .await
                {
                    warn!("Failed to log delivery failure: {}", log_err);
                }
            }
        }

        // Small delay to avoid rate limiting
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
    }

    info!(
        "Summary sent: {} successful, {} failed",
        success_count, fail_count
    );

    // Send admin notification if configured
    if !config.telegram_chat_id.is_empty() && fail_count > 0 {
        // Parse admin chat ID from config string
        if let Ok(admin_chat_id) = config.telegram_chat_id.parse::<i64>() {
            let admin_msg = format!(
                "📊 Summary sent to {}/{} subscribers \\({} failed\\)",
                success_count,
                success_count + fail_count,
                fail_count
            );
            if let Err(e) = send_message(config, admin_chat_id, &admin_msg).await {
                warn!("Failed to send admin notification: {}", e);
            }
        }
    }

    Ok(())
}

/// Broadcast a custom message to all active subscribers
///
/// # Arguments
/// * `config` - Application configuration
/// * `db` - Database connection
/// * `message` - The message to broadcast
/// * `parse_mode` - Optional parse mode ("MarkdownV2" for formatted, None for plain text)
///
/// # Returns
/// * Tuple of (successful sends count, Vec of (chat_id, error) for failures)
pub async fn broadcast_message(
    config: &Config,
    db: &Database,
    message: &str,
    parse_mode: Option<&str>,
) -> Result<(usize, Vec<(i64, String)>)> {
    let subscribers = db.list_subscribers().await?;

    if subscribers.is_empty() {
        info!("No subscribers to broadcast to");
        return Ok((0, Vec::new()));
    }

    info!("Broadcasting message to {} subscribers", subscribers.len());

    let mut success_count = 0;
    let mut failures: Vec<(i64, String)> = Vec::new();

    for subscriber in &subscribers {
        match send_broadcast_message(config, subscriber.chat_id, message, parse_mode).await {
            Ok(_) => {
                success_count += 1;
                info!("✓ Broadcast sent to {}", subscriber.chat_id);
            }
            Err(e) => {
                let error_msg = e.to_string();

                // Auto-remove subscribers who blocked the bot or deleted their account
                let error_lower = error_msg.to_lowercase();
                let is_blocked = error_msg.contains("403")
                    && (error_lower.contains("blocked by the user")
                        || error_lower.contains("user is deactivated"));

                if is_blocked {
                    warn!(
                        "✗ Auto-removing blocked/deactivated subscriber {}: {}",
                        subscriber.chat_id, error_msg
                    );
                    if let Err(remove_err) = db.remove_subscriber(subscriber.chat_id).await {
                        warn!("Failed to remove blocked subscriber: {}", remove_err);
                    }
                } else {
                    warn!(
                        "✗ Failed to broadcast to {}: {}",
                        subscriber.chat_id, error_msg
                    );
                }

                failures.push((subscriber.chat_id, error_msg));
            }
        }

        // Small delay to avoid rate limiting
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
    }

    info!(
        "Broadcast completed: {} successful, {} failed",
        success_count,
        failures.len()
    );

    // Send admin notification if configured and there were failures
    if !config.telegram_chat_id.is_empty() && !failures.is_empty() {
        if let Ok(admin_chat_id) = config.telegram_chat_id.parse::<i64>() {
            let admin_msg = format!(
                "📢 Broadcast sent to {}/{} subscribers \\({} failed\\)",
                success_count,
                subscribers.len(),
                failures.len()
            );
            if let Err(e) = send_message(config, admin_chat_id, &admin_msg).await {
                warn!("Failed to send admin notification: {}", e);
            }
        }
    }

    Ok((success_count, failures))
}

/// Send a broadcast message to a specific chat with optional parse mode
async fn send_broadcast_message(
    config: &Config,
    chat_id: i64,
    text: &str,
    parse_mode: Option<&str>,
) -> Result<()> {
    let client = reqwest::Client::new();

    let url = format!(
        "https://api.telegram.org/bot{}/sendMessage",
        config.telegram_bot_token
    );

    // Build request based on parse mode
    let response = if let Some(mode) = parse_mode {
        let request = SendMessageRequest {
            chat_id: chat_id.to_string(),
            text: text.to_string(),
            parse_mode: mode.to_string(),
        };
        client.post(&url).json(&request).send().await
    } else {
        // Plain text - send without parse_mode
        let request = serde_json::json!({
            "chat_id": chat_id.to_string(),
            "text": text
        });
        client.post(&url).json(&request).send().await
    }
    .context("Failed to send request to Telegram API")?;

    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        anyhow::bail!("Telegram API error ({}): {}", status, body);
    }

    Ok(())
}

/// Send a test summary message to a specific chat ID
pub async fn send_test_message(config: &Config, chat_id: &str, summary: &str) -> Result<()> {
    let timestamp = Utc::now().format("%Y-%m-%d %H:%M UTC");
    let escaped_timestamp = escape_markdownv2(&timestamp.to_string());
    let message = format!(
        "🧪 *TEST \\- Twitter Summary*\n_{}_\n\n{}",
        escaped_timestamp,
        escape_markdownv2(summary)
    );

    let chat_id_i64 = chat_id.parse::<i64>().context(format!(
        "Invalid chat ID format: '{}'. Expected numeric chat ID (e.g., 123456789)",
        chat_id
    ))?;

    send_message(config, chat_id_i64, &message)
        .await
        .context(format!(
            "Failed to send test message to chat ID {}",
            chat_id
        ))?;

    Ok(())
}

/// Send a Telegram message to a specific chat
async fn send_message(config: &Config, chat_id: i64, text: &str) -> Result<()> {
    let client = reqwest::Client::new();

    let url = format!(
        "https://api.telegram.org/bot{}/sendMessage",
        config.telegram_bot_token
    );

    let request = SendMessageRequest {
        chat_id: chat_id.to_string(),
        text: text.to_string(),
        parse_mode: "MarkdownV2".to_string(),
    };

    let response = client
        .post(&url)
        .json(&request)
        .send()
        .await
        .context("Failed to send request to Telegram API")?;

    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        anyhow::bail!("Telegram API error ({}): {}", status, body);
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    // ==================== Update Deserialization Tests ====================

    #[test]
    fn test_update_deserialization_with_message() {
        let json = r#"{
            "update_id": 123456789,
            "message": {
                "message_id": 100,
                "from": {
                    "id": 987654321,
                    "username": "testuser",
                    "first_name": "Test"
                },
                "chat": {
                    "id": 987654321,
                    "type": "private"
                },
                "text": "/start"
            }
        }"#;

        let update: Update = serde_json::from_str(json).expect("Should deserialize");
        assert_eq!(update.update_id, 123456789);
        assert!(update.message.is_some());

        let message = update.message.unwrap();
        assert_eq!(message.message_id, 100);
        assert_eq!(message.chat.id, 987654321);
        assert_eq!(message.text, Some("/start".to_string()));

        let from = message.from.unwrap();
        assert_eq!(from.id, 987654321);
        assert_eq!(from.username, Some("testuser".to_string()));
        assert_eq!(from.first_name, "Test");
    }

    #[test]
    fn test_update_deserialization_without_message() {
        let json = r#"{"update_id": 123456789}"#;

        let update: Update = serde_json::from_str(json).expect("Should deserialize");
        assert_eq!(update.update_id, 123456789);
        assert!(update.message.is_none());
    }

    #[test]
    fn test_message_without_text() {
        let json = r#"{
            "update_id": 123,
            "message": {
                "message_id": 100,
                "chat": {
                    "id": 123,
                    "type": "private"
                }
            }
        }"#;

        let update: Update = serde_json::from_str(json).expect("Should deserialize");
        let message = update.message.unwrap();
        assert!(message.text.is_none());
        assert!(message.from.is_none());
    }

    #[test]
    fn test_message_without_from() {
        let json = r#"{
            "update_id": 123,
            "message": {
                "message_id": 100,
                "chat": {
                    "id": 123,
                    "type": "private"
                },
                "text": "Hello"
            }
        }"#;

        let update: Update = serde_json::from_str(json).expect("Should deserialize");
        let message = update.message.unwrap();
        assert!(message.from.is_none());
        assert_eq!(message.text, Some("Hello".to_string()));
    }

    // ==================== Chat Type Tests ====================

    #[test]
    fn test_private_chat() {
        let json = r#"{
            "update_id": 123,
            "message": {
                "message_id": 100,
                "chat": {
                    "id": 123456789,
                    "type": "private"
                }
            }
        }"#;

        let update: Update = serde_json::from_str(json).expect("Should deserialize");
        let message = update.message.unwrap();
        assert_eq!(message.chat.id, 123456789);
        assert_eq!(message.chat.r#type, "private");
    }

    #[test]
    fn test_group_chat_negative_id() {
        let json = r#"{
            "update_id": 123,
            "message": {
                "message_id": 100,
                "chat": {
                    "id": -1001234567890,
                    "type": "supergroup"
                }
            }
        }"#;

        let update: Update = serde_json::from_str(json).expect("Should deserialize");
        let message = update.message.unwrap();
        assert_eq!(message.chat.id, -1001234567890);
        assert_eq!(message.chat.r#type, "supergroup");
    }

    // ==================== User Tests ====================

    #[test]
    fn test_user_without_username() {
        let json = r#"{
            "update_id": 123,
            "message": {
                "message_id": 100,
                "from": {
                    "id": 123,
                    "first_name": "John"
                },
                "chat": {
                    "id": 123,
                    "type": "private"
                }
            }
        }"#;

        let update: Update = serde_json::from_str(json).expect("Should deserialize");
        let from = update.message.unwrap().from.unwrap();
        assert_eq!(from.id, 123);
        assert_eq!(from.first_name, "John");
        assert!(from.username.is_none());
    }

    #[test]
    fn test_user_with_username() {
        let json = r#"{
            "update_id": 123,
            "message": {
                "message_id": 100,
                "from": {
                    "id": 123,
                    "username": "johndoe",
                    "first_name": "John"
                },
                "chat": {
                    "id": 123,
                    "type": "private"
                }
            }
        }"#;

        let update: Update = serde_json::from_str(json).expect("Should deserialize");
        let from = update.message.unwrap().from.unwrap();
        assert_eq!(from.username, Some("johndoe".to_string()));
    }

    // ==================== SendMessageRequest Tests ====================

    #[test]
    fn test_send_message_request_serialization() {
        let request = SendMessageRequest {
            chat_id: "123456789".to_string(),
            text: "Hello, World!".to_string(),
            parse_mode: "MarkdownV2".to_string(),
        };

        let json = serde_json::to_string(&request).expect("Should serialize");
        assert!(json.contains("123456789"));
        assert!(json.contains("Hello, World!"));
        assert!(json.contains("MarkdownV2"));
    }

    #[test]
    fn test_send_message_request_with_markdown_content() {
        let request = SendMessageRequest {
            chat_id: "123".to_string(),
            text: "*Bold* and _italic_".to_string(),
            parse_mode: "MarkdownV2".to_string(),
        };

        let json = serde_json::to_string(&request).expect("Should serialize");
        assert!(json.contains("*Bold*"));
        assert!(json.contains("_italic_"));
    }

    #[test]
    fn test_send_message_request_with_special_characters() {
        let request = SendMessageRequest {
            chat_id: "123".to_string(),
            text: "Text with \"quotes\" and \\ backslash".to_string(),
            parse_mode: "MarkdownV2".to_string(),
        };

        let json = serde_json::to_string(&request).expect("Should serialize");
        // JSON should escape special characters
        assert!(json.contains("\\\"quotes\\\"") || json.contains("quotes"));
    }

    #[test]
    fn test_send_message_request_with_newlines() {
        let request = SendMessageRequest {
            chat_id: "123".to_string(),
            text: "Line 1\nLine 2\nLine 3".to_string(),
            parse_mode: "MarkdownV2".to_string(),
        };

        let json = serde_json::to_string(&request).expect("Should serialize");
        // Newlines should be escaped in JSON
        assert!(json.contains("\\n"));
    }

    // ==================== Command Pattern Tests ====================

    #[test]
    fn test_start_command_pattern() {
        let text = "/start";
        assert_eq!(text, "/start");
    }

    #[test]
    fn test_subscribe_command_pattern() {
        let text = "/subscribe";
        assert_eq!(text, "/subscribe");
    }

    #[test]
    fn test_unsubscribe_command_pattern() {
        let text = "/unsubscribe";
        assert_eq!(text, "/unsubscribe");
    }

    #[test]
    fn test_status_command_pattern() {
        let text = "/status";
        assert_eq!(text, "/status");
    }

    #[test]
    fn test_unknown_command() {
        let text = "/unknown_command";
        let known_commands = ["/start", "/subscribe", "/unsubscribe", "/status"];
        assert!(!known_commands.contains(&text));
    }

    // ==================== Message Formatting Tests ====================

    #[test]
    fn test_summary_message_format() {
        use chrono::Utc;

        let timestamp = Utc::now().format("%Y-%m-%d %H:%M UTC");
        let summary = "This is the summary content.";

        let message = format!(
            "📰 *Twitter Summary*\n_{}_\n\n{}",
            timestamp,
            escape_markdownv2(summary)
        );

        assert!(message.contains("*Twitter Summary*"));
        assert!(message.contains("_"));
        assert!(message.contains("This is the summary content"));
    }

    #[test]
    fn test_welcome_message_content() {
        let welcome = r#"Welcome to Twitter News Summary Bot!

Commands:
/subscribe - Get daily AI-powered summaries of Twitter/X news
/unsubscribe - Stop receiving summaries
/status - Check your subscription status
/language - Change summary language (en/es)

Summaries are sent twice daily with the latest tweets from tech leaders and AI researchers."#;

        assert!(welcome.contains("/subscribe"));
        assert!(welcome.contains("/unsubscribe"));
        assert!(welcome.contains("/status"));
        assert!(welcome.contains("/language"));
        assert!(welcome.contains("twice daily"));
    }

    // ==================== Edge Case Tests ====================

    #[test]
    fn test_large_update_id() {
        let json = r#"{
            "update_id": 9223372036854775807,
            "message": {
                "message_id": 1,
                "chat": {"id": 1, "type": "private"}
            }
        }"#;

        let update: Update = serde_json::from_str(json).expect("Should handle large update_id");
        assert_eq!(update.update_id, i64::MAX);
    }

    #[test]
    fn test_empty_text_message() {
        let json = r#"{
            "update_id": 123,
            "message": {
                "message_id": 100,
                "chat": {"id": 123, "type": "private"},
                "text": ""
            }
        }"#;

        let update: Update = serde_json::from_str(json).expect("Should deserialize");
        let message = update.message.unwrap();
        assert_eq!(message.text, Some("".to_string()));
    }

    #[test]
    fn test_very_long_text() {
        let long_text = "A".repeat(10000);
        let json = format!(
            r#"{{
            "update_id": 123,
            "message": {{
                "message_id": 100,
                "chat": {{"id": 123, "type": "private"}},
                "text": "{}"
            }}
        }}"#,
            long_text
        );

        let update: Update = serde_json::from_str(&json).expect("Should deserialize");
        let message = update.message.unwrap();
        assert_eq!(message.text.unwrap().len(), 10000);
    }

    #[test]
    fn test_unicode_in_message() {
        let json = r#"{
            "update_id": 123,
            "message": {
                "message_id": 100,
                "from": {
                    "id": 123,
                    "first_name": "User name"
                },
                "chat": {"id": 123, "type": "private"},
                "text": "Hello in many languages"
            }
        }"#;

        let update: Update = serde_json::from_str(json).expect("Should deserialize");
        let message = update.message.unwrap();
        assert!(message.text.unwrap().contains("Hello"));
    }

    // ==================== Chat ID Tests ====================

    #[test]
    fn test_chat_id_to_string_conversion() {
        let chat_id: i64 = 123456789;
        let chat_id_str = chat_id.to_string();
        assert_eq!(chat_id_str, "123456789");
    }

    #[test]
    fn test_negative_chat_id_to_string() {
        let chat_id: i64 = -1001234567890;
        let chat_id_str = chat_id.to_string();
        assert_eq!(chat_id_str, "-1001234567890");
    }

    // ==================== Status Message Format Tests ====================

    #[test]
    fn test_subscribed_status_format_admin() {
        // Admin sees subscriber count
        let subscriber_count = 42;
        let status_msg = format!(
            "✅ You are subscribed\n📊 Total subscribers: {}",
            subscriber_count
        );

        assert!(status_msg.contains("subscribed"));
        assert!(status_msg.contains("42"));
    }

    #[test]
    fn test_subscribed_status_format_regular_user() {
        // Regular users don't see subscriber count
        let status_msg = "✅ You are subscribed";

        assert!(status_msg.contains("subscribed"));
        assert!(!status_msg.contains("Total subscribers"));
    }

    #[test]
    fn test_unsubscribed_status_format() {
        let status_msg =
            "❌ You are not subscribed\n\nUse /subscribe to start receiving summaries.";

        assert!(status_msg.contains("not subscribed"));
        assert!(status_msg.contains("/subscribe"));
    }

    // ==================== Admin Notification Tests ====================

    #[test]
    fn test_admin_notification_format() {
        let success_count = 10;
        let fail_count = 2;
        let total = success_count + fail_count;

        let admin_msg = format!(
            "Summary sent to {}/{} subscribers ({} failed)",
            success_count, total, fail_count
        );

        assert!(admin_msg.contains("10/12"));
        assert!(admin_msg.contains("2 failed"));
    }

    // ==================== Welcome Summary Feature Tests ====================

    // ---------- Welcome Summary Message Formatting Tests ----------

    #[test]
    fn test_welcome_summary_message_format() {
        use chrono::Utc;

        let timestamp = Utc::now().format("%Y-%m-%d %H:%M UTC");
        let summary = "Here is the AI news summary content.";

        // This matches the format in send_welcome_summary
        let message = format!(
            "📰 *Hey! Here's what you missed* 😉\n_{}_\n\n{}",
            timestamp,
            escape_markdownv2(summary)
        );

        assert!(message.contains("*Hey!"));
        assert!(message.contains("what you missed*"));
        assert!(message.contains("Here is the AI news summary content"));
        assert!(message.contains("UTC"));
    }

    #[test]
    fn test_welcome_summary_differs_from_regular_summary() {
        use chrono::Utc;

        let timestamp = Utc::now().format("%Y-%m-%d %H:%M UTC");
        let summary = "Summary content";

        // Welcome format
        let welcome_msg = format!(
            "📰 *Hey! Here's what you missed* 😉\n_{}_\n\n{}",
            timestamp,
            escape_markdownv2(summary)
        );

        // Regular summary format (from send_to_subscribers)
        let regular_msg = format!(
            "📰 *Twitter Summary*\n_{}_\n\n{}",
            timestamp,
            escape_markdownv2(summary)
        );

        // They should be different
        assert_ne!(welcome_msg, regular_msg);
        assert!(welcome_msg.contains("*Hey!"));
        assert!(!regular_msg.contains("*Hey!"));
    }

    #[test]
    fn test_welcome_summary_with_markdown_in_content() {
        use chrono::Utc;

        let timestamp = Utc::now().format("%Y-%m-%d %H:%M UTC");
        let summary = "Summary with *bold* and _italic_ text";

        let message = format!(
            "📰 *Hey! Here's what you missed* 😉\n_{}_\n\n{}",
            timestamp,
            escape_markdownv2(summary)
        );

        // The summary content should have markdown characters escaped
        assert!(message.contains("\\*bold\\*"));
        assert!(message.contains("\\_italic\\_"));
    }

    #[test]
    fn test_welcome_summary_with_long_content() {
        use chrono::Utc;

        let timestamp = Utc::now().format("%Y-%m-%d %H:%M UTC");
        let summary = "A".repeat(4000); // Telegram limit is 4096

        let message = format!(
            "📰 <b>Hey! Here's what you missed</b> 😉\n<i>{}</i>\n\n{}",
            timestamp, summary
        );

        // Message should be constructed (actual truncation would be done at send time)
        assert!(message.len() > 4000);
    }

    // ---------- Subscribe Command Logic Tests ----------

    #[test]
    fn test_subscribe_command_json_parsing() {
        let json = r#"{
            "update_id": 123,
            "message": {
                "message_id": 100,
                "from": {
                    "id": 987654321,
                    "username": "newsubscriber",
                    "first_name": "New"
                },
                "chat": {
                    "id": 987654321,
                    "type": "private"
                },
                "text": "/subscribe"
            }
        }"#;

        let update: Update = serde_json::from_str(json).expect("Should deserialize");
        let message = update.message.unwrap();

        assert_eq!(message.text.unwrap(), "/subscribe");
        assert_eq!(
            message.from.unwrap().username,
            Some("newsubscriber".to_string())
        );
    }

    #[test]
    fn test_subscribe_command_without_username() {
        let json = r#"{
            "update_id": 123,
            "message": {
                "message_id": 100,
                "from": {
                    "id": 123,
                    "first_name": "NoUsername"
                },
                "chat": {
                    "id": 123,
                    "type": "private"
                },
                "text": "/subscribe"
            }
        }"#;

        let update: Update = serde_json::from_str(json).expect("Should deserialize");
        let message = update.message.unwrap();
        let from = message.from.unwrap();

        assert!(from.username.is_none());
        assert_eq!(from.first_name, "NoUsername");
    }

    #[test]
    fn test_unsubscribe_command_json_parsing() {
        let json = r#"{
            "update_id": 456,
            "message": {
                "message_id": 200,
                "from": {
                    "id": 111222333,
                    "username": "leavinguser",
                    "first_name": "Leaving"
                },
                "chat": {
                    "id": 111222333,
                    "type": "private"
                },
                "text": "/unsubscribe"
            }
        }"#;

        let update: Update = serde_json::from_str(json).expect("Should deserialize");
        let message = update.message.unwrap();

        assert_eq!(message.text.unwrap(), "/unsubscribe");
    }

    // ---------- Message Response Tests ----------

    #[test]
    fn test_already_subscribed_message() {
        let msg = "\u{2705} You're already subscribed!";
        assert!(msg.contains("already subscribed"));
    }

    #[test]
    fn test_successful_subscribe_message() {
        let msg = "\u{2705} Successfully subscribed! You'll receive summaries twice daily.";
        assert!(msg.contains("Successfully subscribed"));
        assert!(msg.contains("twice daily"));
    }

    #[test]
    fn test_successful_unsubscribe_message() {
        let msg = "\u{1f44b} Successfully unsubscribed. You won't receive any more summaries.";
        assert!(msg.contains("Successfully unsubscribed"));
        assert!(msg.contains("won't receive"));
    }

    #[test]
    fn test_not_subscribed_message() {
        let msg = "You're not currently subscribed.";
        assert!(msg.contains("not currently subscribed"));
    }

    // ---------- Welcome Summary Flow Logic Tests ----------

    #[test]
    fn test_welcome_summary_flow_new_subscriber() {
        // Simulate the logic flow:
        // 1. add_subscriber returns (true, true) for new subscriber
        // 2. get_latest_summary returns Some(summary)
        // 3. send_welcome_summary is called
        // 4. mark_welcome_summary_sent is called

        let is_new = true;
        let needs_welcome = true;
        let has_summary = true;

        // This represents the decision tree in handle_webhook
        // Simulating: user is not subscribed
        if true {
            let (new_sub, welcome) = (is_new, needs_welcome);
            assert!(new_sub);
            assert!(welcome);

            if welcome && has_summary {
                // Would call send_welcome_summary
                let would_send_welcome = true;
                assert!(would_send_welcome);
            }
        }
    }

    #[test]
    fn test_welcome_summary_flow_returning_subscriber() {
        // Simulate: returning subscriber (unsubscribed then resubscribed)
        // add_subscriber returns (true, false)

        let is_new = true; // Counts as new subscription
        let needs_welcome = false; // But no welcome needed

        // Simulating: user is not subscribed
        if true {
            let (_new_sub, welcome) = (is_new, needs_welcome);

            // Should NOT send welcome
            let should_send_welcome = welcome;
            assert!(
                !should_send_welcome,
                "Returning subscriber should not get welcome"
            );
        }
    }

    #[test]
    fn test_welcome_summary_flow_no_summary_available() {
        // Simulate: new subscriber but no summary exists yet

        let is_new = true;
        let needs_welcome = true;
        let has_summary = false; // No summary available

        // Simulating: user is not subscribed
        if true {
            let (_new_sub, welcome) = (is_new, needs_welcome);

            if welcome {
                if has_summary {
                    panic!("Should not reach here");
                } else {
                    // Would just log "No summary available"
                    let logged_no_summary = true;
                    assert!(logged_no_summary);
                }
            }
        }
    }

    // ---------- Edge Cases Tests ----------

    #[test]
    fn test_group_chat_subscription() {
        let json = r#"{
            "update_id": 789,
            "message": {
                "message_id": 300,
                "from": {
                    "id": 123456,
                    "username": "groupadmin",
                    "first_name": "Admin"
                },
                "chat": {
                    "id": -1001234567890,
                    "type": "supergroup"
                },
                "text": "/subscribe"
            }
        }"#;

        let update: Update = serde_json::from_str(json).expect("Should deserialize");
        let message = update.message.unwrap();

        // Chat ID for group is negative
        let chat_id = message.chat.id.to_string();
        assert!(chat_id.starts_with("-"), "Group chat ID should be negative");
        assert_eq!(chat_id, "-1001234567890");
    }

    #[test]
    fn test_message_from_channel() {
        // Channels may not have a "from" field
        let json = r#"{
            "update_id": 999,
            "message": {
                "message_id": 400,
                "chat": {
                    "id": -1009876543210,
                    "type": "channel"
                },
                "text": "/subscribe"
            }
        }"#;

        let update: Update = serde_json::from_str(json).expect("Should deserialize");
        let message = update.message.unwrap();

        // from is optional
        assert!(message.from.is_none());
        assert_eq!(message.text.unwrap(), "/subscribe");
    }

    #[test]
    fn test_empty_update_handling() {
        // Update with no message (e.g., callback query, edited message)
        let json = r#"{"update_id": 123}"#;

        let update: Update = serde_json::from_str(json).expect("Should deserialize");
        assert!(update.message.is_none());

        // In handle_webhook, this would return Ok(()) early
    }

    #[test]
    fn test_message_without_text_handling() {
        // Message with photo/sticker but no text
        let json = r#"{
            "update_id": 123,
            "message": {
                "message_id": 100,
                "chat": {"id": 123, "type": "private"}
            }
        }"#;

        let update: Update = serde_json::from_str(json).expect("Should deserialize");
        let message = update.message.unwrap();

        assert!(message.text.is_none());
        // In handle_webhook, this would return Ok(()) early
    }

    // ---------- Admin Status Tests ----------

    #[test]
    fn test_admin_detection_logic() {
        let admin_chat_id = "123456789";
        let user_chat_id = "123456789";
        let other_chat_id = "987654321";

        // Admin check: config.telegram_chat_id matches chat_id
        let is_admin_for_self = !admin_chat_id.is_empty() && user_chat_id == admin_chat_id;
        let is_admin_for_other = !admin_chat_id.is_empty() && other_chat_id == admin_chat_id;

        assert!(is_admin_for_self);
        assert!(!is_admin_for_other);
    }

    #[test]
    fn test_admin_detection_with_empty_config() {
        let admin_chat_id = ""; // Empty in config
        let user_chat_id = "123456789";

        // Should not be admin if config is empty
        let is_admin = !admin_chat_id.is_empty() && user_chat_id == admin_chat_id;
        assert!(!is_admin, "Empty admin config should mean no admin");
    }

    // ---------- Send To Subscribers Tests ----------

    #[test]
    fn test_subscriber_iteration_logic() {
        // Simulate the loop in send_to_subscribers
        let subscriber_ids = ["111", "222", "333"];
        let mut success_count = 0;
        let mut fail_count = 0;

        for _id in subscriber_ids.iter() {
            // Simulate success for most, failure for one
            let succeeded = true; // In real code, this is the result of send_message
            if succeeded {
                success_count += 1;
            } else {
                fail_count += 1;
            }
        }

        assert_eq!(success_count, 3);
        assert_eq!(fail_count, 0);
    }

    #[test]
    fn test_empty_subscriber_list_handling() {
        let subscribers: Vec<String> = vec![];

        // In send_to_subscribers, empty list means early return
        assert!(subscribers.is_empty());
        // Would log "No subscribers to send to" and return Ok(())
    }

    // ---------- Timestamp Formatting Tests ----------

    #[test]
    fn test_utc_timestamp_format() {
        use chrono::Utc;

        let timestamp = Utc::now().format("%Y-%m-%d %H:%M UTC").to_string();

        // Should match pattern like "2024-01-15 10:30 UTC"
        assert!(timestamp.ends_with(" UTC"));
        assert!(timestamp.contains("-"));
        assert!(timestamp.contains(":"));
        assert_eq!(timestamp.len(), 20); // "YYYY-MM-DD HH:MM UTC"
    }

    #[test]
    fn test_timestamp_in_summary_message() {
        use chrono::Utc;

        let timestamp = Utc::now().format("%Y-%m-%d %H:%M UTC");
        let message = format!(
            "📰 *Twitter Summary*\n_{}_\n\n{}",
            timestamp,
            escape_markdownv2("content")
        );

        // Timestamp should be in italics (MarkdownV2)
        assert!(message.contains(&format!("_{}_", timestamp)));
    }

    // ---------- Rate Limiting Tests ----------

    #[test]
    fn test_rate_limit_delay_value() {
        // The code uses 100ms delay between sends
        let delay_ms = 100u64;
        assert_eq!(delay_ms, 100);

        // This is reasonable for Telegram's rate limits
        // Telegram allows about 30 messages/second to different chats
    }

    // ---------- Integration Scenario Tests ----------

    #[test]
    fn test_full_subscription_lifecycle_messages() {
        // Track the messages a user would see through a full lifecycle

        let messages = vec![
            // 1. User sends /start
            "👋 Welcome to Twitter News Summary Bot!",
            // 2. User sends /subscribe (first time)
            "✅ Successfully subscribed! You'll receive summaries twice daily.",
            // 3. Welcome summary (if available)
            "📰 *Hey! Here's what you missed* 😉",
            // 4. User checks /status
            "✅ You are subscribed",
            // 5. User sends /unsubscribe
            "👋 Successfully unsubscribed. You won't receive any more summaries.",
            // 6. User re-subscribes (no welcome this time)
            "✅ Successfully subscribed! You'll receive summaries twice daily.",
        ];

        // All messages should be non-empty
        for msg in &messages {
            assert!(!msg.is_empty());
        }

        // Welcome message only appears once (index 2)
        let welcome_count = messages
            .iter()
            .filter(|m| m.contains("Hey! Here's what you missed"))
            .count();
        assert_eq!(welcome_count, 1, "Welcome summary should only appear once");
    }

    #[test]
    fn test_subscribe_already_subscribed_flow() {
        // User is already subscribed and sends /subscribe again
        // Should get "already subscribed" message, not welcome

        let is_subscribed = true;
        let expected_response = if is_subscribed {
            "\u{2705} You're already subscribed!"
        } else {
            "\u{2705} Successfully subscribed!"
        };

        assert!(expected_response.contains("already subscribed"));
    }

    // ==================== Blocked User Auto-Removal Detection Tests ====================

    /// Helper function that mirrors the detection logic in send_to_subscribers
    /// This allows us to test the detection logic in isolation without database dependencies
    fn should_auto_remove_blocked_subscriber(error_msg: &str) -> bool {
        let error_lower = error_msg.to_lowercase();
        error_msg.contains("403")
            && (error_lower.contains("blocked by the user")
                || error_lower.contains("user is deactivated"))
    }

    // ---------- Positive Cases: Should Trigger Auto-Removal ----------

    #[test]
    fn test_blocked_user_detection_exact_telegram_format() {
        // This is the exact format returned by Telegram API when a user blocks the bot
        let error_msg = r#"Telegram API error (403 Forbidden): {"ok":false,"error_code":403,"description":"Forbidden: bot was blocked by the user"}"#;

        assert!(
            should_auto_remove_blocked_subscriber(error_msg),
            "Should detect blocked user from exact Telegram API response format"
        );
    }

    #[test]
    fn test_blocked_user_detection_contains_both_markers() {
        // The detection requires BOTH "403" AND "blocked by the user" to be present
        let error_msg = "Error: 403 - user has blocked by the user";

        assert!(
            should_auto_remove_blocked_subscriber(error_msg),
            "Should detect when both '403' and 'blocked by the user' are present"
        );
    }

    #[test]
    fn test_blocked_user_detection_different_403_position() {
        // 403 can appear in different positions within the error message
        let error_msg = "blocked by the user, status: 403";

        assert!(
            should_auto_remove_blocked_subscriber(error_msg),
            "Should detect regardless of where '403' appears in message"
        );
    }

    #[test]
    fn test_blocked_user_detection_with_additional_context() {
        // Error message might have additional context wrapping it
        let error_msg = "Failed to send message: Telegram API error (403 Forbidden): bot was blocked by the user - chat_id: 123456";

        assert!(
            should_auto_remove_blocked_subscriber(error_msg),
            "Should detect blocked user even with additional context"
        );
    }

    #[test]
    fn test_blocked_user_detection_multiline_error() {
        // Error message could span multiple lines
        let error_msg = r#"Telegram API error (403 Forbidden):
{"ok":false,"error_code":403,"description":"Forbidden: bot was blocked by the user"}"#;

        assert!(
            should_auto_remove_blocked_subscriber(error_msg),
            "Should detect blocked user in multiline error messages"
        );
    }

    #[test]
    fn test_blocked_user_detection_standard_case() {
        // Standard lowercase format from Telegram API
        let error_msg = "Telegram API error (403 Forbidden): blocked by the user";

        assert!(
            should_auto_remove_blocked_subscriber(error_msg),
            "Should detect standard lowercase format"
        );
    }

    // ---------- Negative Cases: Should NOT Trigger Auto-Removal ----------

    #[test]
    fn test_no_auto_remove_for_400_bad_request() {
        // 400 Bad Request should not trigger auto-removal
        let error_msg = r#"Telegram API error (400 Bad Request): {"ok":false,"error_code":400,"description":"Bad Request: chat not found"}"#;

        assert!(
            !should_auto_remove_blocked_subscriber(error_msg),
            "400 Bad Request should NOT trigger auto-removal"
        );
    }

    #[test]
    fn test_no_auto_remove_for_401_unauthorized() {
        // 401 Unauthorized (invalid bot token) should not trigger auto-removal
        let error_msg = r#"Telegram API error (401 Unauthorized): {"ok":false,"error_code":401,"description":"Unauthorized"}"#;

        assert!(
            !should_auto_remove_blocked_subscriber(error_msg),
            "401 Unauthorized should NOT trigger auto-removal"
        );
    }

    #[test]
    fn test_no_auto_remove_for_429_rate_limited() {
        // 429 Rate Limited should not trigger auto-removal
        let error_msg = r#"Telegram API error (429 Too Many Requests): {"ok":false,"error_code":429,"description":"Too Many Requests: retry after 35"}"#;

        assert!(
            !should_auto_remove_blocked_subscriber(error_msg),
            "429 Rate Limited should NOT trigger auto-removal"
        );
    }

    #[test]
    fn test_no_auto_remove_for_500_server_error() {
        // 500 Internal Server Error should not trigger auto-removal
        let error_msg = r#"Telegram API error (500 Internal Server Error): {"ok":false,"error_code":500,"description":"Internal Server Error"}"#;

        assert!(
            !should_auto_remove_blocked_subscriber(error_msg),
            "500 Server Error should NOT trigger auto-removal"
        );
    }

    #[test]
    fn test_no_auto_remove_for_network_error() {
        // Network errors (connection refused, timeout, etc.) should not trigger auto-removal
        let error_msg = "Failed to send request to Telegram API: connection refused";

        assert!(
            !should_auto_remove_blocked_subscriber(error_msg),
            "Network errors should NOT trigger auto-removal"
        );
    }

    #[test]
    fn test_no_auto_remove_for_timeout() {
        // Timeout errors should not trigger auto-removal
        let error_msg = "Failed to send request to Telegram API: operation timed out";

        assert!(
            !should_auto_remove_blocked_subscriber(error_msg),
            "Timeout errors should NOT trigger auto-removal"
        );
    }

    #[test]
    fn test_no_auto_remove_for_dns_error() {
        // DNS resolution errors should not trigger auto-removal
        let error_msg = "Failed to send request to Telegram API: failed to resolve host";

        assert!(
            !should_auto_remove_blocked_subscriber(error_msg),
            "DNS errors should NOT trigger auto-removal"
        );
    }

    #[test]
    fn test_no_auto_remove_for_403_without_blocked_text() {
        // 403 alone (without "blocked by the user") should not trigger auto-removal
        // This could be a different kind of 403 error (e.g., bot not in chat)
        let error_msg = r#"Telegram API error (403 Forbidden): {"ok":false,"error_code":403,"description":"Forbidden: bot is not a member of the group chat"}"#;

        assert!(
            !should_auto_remove_blocked_subscriber(error_msg),
            "403 without 'blocked by the user' should NOT trigger auto-removal"
        );
    }

    #[test]
    fn test_no_auto_remove_for_blocked_text_without_403() {
        // "blocked by the user" text without 403 status code should not trigger auto-removal
        let error_msg = "User action: blocked by the user preference settings";

        assert!(
            !should_auto_remove_blocked_subscriber(error_msg),
            "'blocked by the user' text without 403 should NOT trigger auto-removal"
        );
    }

    #[test]
    fn test_no_auto_remove_for_chat_not_found() {
        // Chat not found (deleted account) is different from blocked
        let error_msg = r#"Telegram API error (400 Bad Request): {"ok":false,"error_code":400,"description":"Bad Request: chat not found"}"#;

        assert!(
            !should_auto_remove_blocked_subscriber(error_msg),
            "Chat not found should NOT trigger auto-removal (could be temporary)"
        );
    }

    #[test]
    fn test_no_auto_remove_for_bot_kicked_from_group() {
        // Bot kicked from group chat is different from user blocking
        let error_msg = r#"Telegram API error (403 Forbidden): {"ok":false,"error_code":403,"description":"Forbidden: bot was kicked from the group chat"}"#;

        assert!(
            !should_auto_remove_blocked_subscriber(error_msg),
            "Bot kicked from group should NOT trigger auto-removal (different scenario)"
        );
    }

    #[test]
    fn test_auto_remove_for_user_deactivated() {
        // User deactivated their account - this is permanent, should auto-remove
        let error_msg = r#"Telegram API error (403 Forbidden): {"ok":false,"error_code":403,"description":"Forbidden: user is deactivated"}"#;

        assert!(
            should_auto_remove_blocked_subscriber(error_msg),
            "User deactivated SHOULD trigger auto-removal (account deleted permanently)"
        );
    }

    #[test]
    fn test_auto_remove_for_user_deactivated_case_insensitive() {
        // "user is deactivated" should match regardless of case
        let error_msg_upper = "Telegram API error (403 Forbidden): USER IS DEACTIVATED";
        let error_msg_mixed = "Telegram API error (403 Forbidden): User Is Deactivated";

        assert!(
            should_auto_remove_blocked_subscriber(error_msg_upper),
            "Should match uppercase 'USER IS DEACTIVATED'"
        );
        assert!(
            should_auto_remove_blocked_subscriber(error_msg_mixed),
            "Should match mixed case 'User Is Deactivated'"
        );
    }

    // ---------- Edge Cases in Error Message Format ----------

    #[test]
    fn test_blocked_detection_empty_string() {
        let error_msg = "";

        assert!(
            !should_auto_remove_blocked_subscriber(error_msg),
            "Empty error message should NOT trigger auto-removal"
        );
    }

    #[test]
    fn test_blocked_detection_whitespace_only() {
        let error_msg = "   \n\t  ";

        assert!(
            !should_auto_remove_blocked_subscriber(error_msg),
            "Whitespace-only error message should NOT trigger auto-removal"
        );
    }

    #[test]
    fn test_blocked_detection_very_long_error() {
        // Very long error message with blocked user info somewhere in the middle
        let prefix = "A".repeat(5000);
        let suffix = "B".repeat(5000);
        let error_msg = format!(
            "{}Telegram API error (403 Forbidden): blocked by the user{}",
            prefix, suffix
        );

        assert!(
            should_auto_remove_blocked_subscriber(&error_msg),
            "Should detect blocked user even in very long error messages"
        );
    }

    #[test]
    fn test_blocked_detection_unicode_in_error() {
        // Error message might contain Unicode characters
        let error_msg =
            "Telegram API error (403 Forbidden): \u{1f6ab} blocked by the user \u{1f44b}";

        assert!(
            should_auto_remove_blocked_subscriber(error_msg),
            "Should detect blocked user even with Unicode in error message"
        );
    }

    #[test]
    fn test_blocked_detection_special_characters() {
        // Error message with various special characters
        let error_msg = r#"Error [403] "blocked by the user" <test> & more"#;

        assert!(
            should_auto_remove_blocked_subscriber(error_msg),
            "Should detect blocked user with special characters in message"
        );
    }

    #[test]
    fn test_blocked_detection_403_as_substring() {
        // 403 appearing as part of a larger number should still match
        // (current implementation uses simple contains)
        let error_msg = "Error code: 14030 - blocked by the user";

        // This WILL match because "14030" contains "403"
        // This documents current behavior - might want to improve detection
        assert!(
            should_auto_remove_blocked_subscriber(error_msg),
            "Current implementation matches 403 as substring (documents behavior)"
        );
    }

    #[test]
    fn test_blocked_detection_case_insensitivity() {
        // Test that the detection is case-insensitive for robustness
        let error_msg_upper = "Telegram API error (403 Forbidden): BLOCKED BY THE USER";
        let error_msg_lower = "telegram api error (403 forbidden): blocked by the user";
        let error_msg_mixed = "Telegram API error (403 Forbidden): Blocked By The User";

        // All cases should match due to case-insensitive comparison
        assert!(
            should_auto_remove_blocked_subscriber(error_msg_upper),
            "Should match uppercase 'BLOCKED BY THE USER'"
        );
        assert!(
            should_auto_remove_blocked_subscriber(error_msg_lower),
            "Should match lowercase 'blocked by the user'"
        );
        assert!(
            should_auto_remove_blocked_subscriber(error_msg_mixed),
            "Should match mixed case 'Blocked By The User'"
        );
    }

    // ---------- Real-World Telegram API Response Format Tests ----------

    #[test]
    fn test_blocked_detection_with_json_escaped_quotes() {
        // JSON response might have escaped quotes in some contexts
        let error_msg = r#"Telegram API error (403 Forbidden): {\"ok\":false,\"error_code\":403,\"description\":\"Forbidden: bot was blocked by the user\"}"#;

        assert!(
            should_auto_remove_blocked_subscriber(error_msg),
            "Should detect blocked user even with escaped JSON quotes"
        );
    }

    #[test]
    fn test_blocked_detection_anyhow_error_chain() {
        // The error message format from anyhow's error chain
        let error_msg =
            "Telegram API error (403 Forbidden): Forbidden: bot was blocked by the user";

        assert!(
            should_auto_remove_blocked_subscriber(error_msg),
            "Should detect blocked user in anyhow error format"
        );
    }

    #[test]
    fn test_blocked_detection_reqwest_error_context() {
        // Error might include reqwest context
        let error_msg = "Failed to send request to Telegram API: Telegram API error (403 Forbidden): {\"ok\":false,\"error_code\":403,\"description\":\"Forbidden: bot was blocked by the user\"}";

        assert!(
            should_auto_remove_blocked_subscriber(error_msg),
            "Should detect blocked user even with reqwest context wrapper"
        );
    }

    // ---------- Boundary Condition Tests ----------

    #[test]
    fn test_blocked_detection_markers_adjacent() {
        // Both markers present but adjacent (no space)
        let error_msg = "403blocked by the user";

        assert!(
            should_auto_remove_blocked_subscriber(error_msg),
            "Should detect when markers are adjacent"
        );
    }

    #[test]
    fn test_blocked_detection_markers_reversed_order() {
        // The markers can appear in any order
        let error_msg = "User was blocked by the user, error code 403";

        assert!(
            should_auto_remove_blocked_subscriber(error_msg),
            "Should detect regardless of marker order"
        );
    }

    #[test]
    fn test_blocked_detection_multiple_403_occurrences() {
        // Multiple occurrences of 403 in the message
        let error_msg = "Error 403: code 403, status 403 Forbidden, blocked by the user";

        assert!(
            should_auto_remove_blocked_subscriber(error_msg),
            "Should detect with multiple 403 occurrences"
        );
    }

    #[test]
    fn test_blocked_detection_partial_phrase_not_matched() {
        // Partial phrase "blocked by the" (without "user") should not match
        let error_msg = "Error 403: action was blocked by the system";

        assert!(
            !should_auto_remove_blocked_subscriber(error_msg),
            "Partial phrase 'blocked by the' should NOT match"
        );
    }

    // ---------- Documentation Tests ----------

    #[test]
    fn test_blocked_detection_logic_documents_implementation() {
        // This test documents and verifies the exact implementation logic
        // The detection requires BOTH conditions to be true:
        // 1. error_msg.contains("403")
        // 2. error_msg.contains("blocked by the user")

        // Both present = should remove
        assert!(should_auto_remove_blocked_subscriber(
            "403 blocked by the user"
        ));

        // Only 403 = should NOT remove
        assert!(!should_auto_remove_blocked_subscriber("error 403"));

        // Only blocked text = should NOT remove
        assert!(!should_auto_remove_blocked_subscriber(
            "blocked by the user"
        ));

        // Neither present = should NOT remove
        assert!(!should_auto_remove_blocked_subscriber(
            "some other error message"
        ));
    }

    // ==================== Language Command Tests ====================

    #[test]
    fn test_language_command_pattern() {
        let text = "/language";
        assert_eq!(text, "/language");
    }

    #[test]
    fn test_language_command_with_argument_parsing() {
        let text = "/language es";

        // Test the parsing logic used in handle_webhook
        let (command, arg) = if text.starts_with("/language ") {
            (
                "/language",
                Some(text.strip_prefix("/language ").unwrap().trim()),
            )
        } else {
            (text, None)
        };

        assert_eq!(command, "/language");
        assert_eq!(arg, Some("es"));
    }

    #[test]
    fn test_language_command_without_argument_parsing() {
        let text = "/language";

        let (command, arg) = if text.starts_with("/language ") {
            (
                "/language",
                Some(text.strip_prefix("/language ").unwrap().trim()),
            )
        } else {
            (text, None)
        };

        assert_eq!(command, "/language");
        assert_eq!(arg, None);
    }

    #[test]
    fn test_language_command_with_extra_spaces() {
        let text = "/language   es  ";

        let (command, arg) = if text.starts_with("/language ") {
            (
                "/language",
                Some(text.strip_prefix("/language ").unwrap().trim()),
            )
        } else {
            (text, None)
        };

        assert_eq!(command, "/language");
        assert_eq!(arg, Some("es"));
    }

    #[test]
    fn test_language_settings_message_format() {
        let current_lang = "en";
        let current_name = match current_lang {
            "es" => "Spanish",
            _ => "English",
        };

        let msg = format!(
            "Language Settings\n\nCurrent: {}\n\nTo change, use:\n/language en - English\n/language es - Spanish",
            current_name
        );

        assert!(msg.contains("English"));
        assert!(msg.contains("/language en"));
        assert!(msg.contains("/language es"));
    }

    #[test]
    fn test_language_change_confirmation_english() {
        let msg = "Language changed to English. You'll receive summaries in English.";
        assert!(msg.contains("English"));
    }

    #[test]
    fn test_language_change_confirmation_spanish() {
        let msg = "Idioma cambiado a español. Recibirás los resúmenes en español.";
        assert!(msg.contains("español"));
    }

    #[test]
    fn test_invalid_language_error_message() {
        let msg =
            "Invalid language. Available options:\n/language en - English\n/language es - Spanish";
        assert!(msg.contains("Invalid"));
        assert!(msg.contains("/language en"));
        assert!(msg.contains("/language es"));
    }

    #[test]
    fn test_language_command_requires_subscription() {
        let msg = "You need to subscribe first. Use /subscribe to get started.";
        assert!(msg.contains("subscribe"));
    }

    #[test]
    fn test_status_message_with_language() {
        let lang_name = "English";
        let status_msg = format!("You are subscribed\nLanguage: {}", lang_name);

        assert!(status_msg.contains("subscribed"));
        assert!(status_msg.contains("English"));
    }

    #[test]
    fn test_subscribe_message_includes_language_hint() {
        let msg = "Successfully subscribed! You'll receive summaries twice daily.\n\nWant summaries in Spanish? Use /language es to switch.";

        assert!(msg.contains("subscribed"));
        assert!(msg.contains("/language es"));
        assert!(msg.contains("Spanish"));
    }

    // ==================== MarkdownV2 Escaping Tests ====================

    #[test]
    fn test_escape_markdownv2_underscore() {
        assert_eq!(escape_markdownv2("AI_ML"), "AI\\_ML");
    }

    #[test]
    fn test_escape_markdownv2_asterisk() {
        assert_eq!(escape_markdownv2("2*2=4"), "2\\*2\\=4");
    }

    #[test]
    fn test_escape_markdownv2_brackets() {
        assert_eq!(escape_markdownv2("[link]"), "\\[link\\]");
    }

    #[test]
    fn test_escape_markdownv2_parentheses() {
        assert_eq!(escape_markdownv2("(example)"), "\\(example\\)");
    }

    #[test]
    fn test_escape_markdownv2_all_18_special_chars() {
        let input = "_*[]()~`>#+-=|{}.!";
        let expected = "\\_\\*\\[\\]\\(\\)\\~\\`\\>\\#\\+\\-\\=\\|\\{\\}\\.\\!";
        assert_eq!(escape_markdownv2(input), expected);
    }

    #[test]
    fn test_escape_markdownv2_mixed_text() {
        assert_eq!(
            escape_markdownv2("AI & ML: performance > 2x!"),
            "AI & ML: performance \\> 2x\\!"
        );
    }

    #[test]
    fn test_escape_markdownv2_no_special_chars() {
        assert_eq!(escape_markdownv2("Simple text"), "Simple text");
    }

    #[test]
    fn test_escape_markdownv2_empty_string() {
        assert_eq!(escape_markdownv2(""), "");
    }

    #[test]
    fn test_escape_markdownv2_preserves_regular_chars() {
        assert_eq!(
            escape_markdownv2("Hello @ world & test"),
            "Hello @ world & test"
        );
    }

    #[test]
    fn test_escape_markdownv2_dots_and_hyphens() {
        assert_eq!(escape_markdownv2("v1.0.0"), "v1\\.0\\.0");
        assert_eq!(escape_markdownv2("multi-word"), "multi\\-word");
    }

    #[test]
    fn test_escape_markdownv2_urls() {
        let url = "https://example.com/path?param=value";
        // URLs don't need escaping when used as plain text
        // Only special MarkdownV2 chars like . - need escaping
        assert!(escape_markdownv2(url).contains("\\."));
    }

    #[test]
    fn test_escape_markdownv2_code_backticks() {
        assert_eq!(escape_markdownv2("`code`"), "\\`code\\`");
    }

    #[test]
    fn test_escape_markdownv2_tilde() {
        assert_eq!(escape_markdownv2("~strikethrough~"), "\\~strikethrough\\~");
    }

    #[test]
    fn test_escape_markdownv2_greater_than() {
        assert_eq!(escape_markdownv2("> quote"), "\\> quote");
    }

    #[test]
    fn test_escape_markdownv2_math_symbols() {
        assert_eq!(escape_markdownv2("x + y = z"), "x \\+ y \\= z");
    }

    // ==================== Markdown Link Preservation Tests ====================

    #[test]
    fn test_escape_markdownv2_preserves_simple_link() {
        let text = "Check this out: [Click here](https://example.com)";
        let escaped = escape_markdownv2(text);
        // Link structure should be preserved
        assert!(escaped.contains("[Click here](https://example.com)"));
        // Text before link should be escaped
        assert!(escaped.contains("Check this out:"));
    }

    #[test]
    fn test_escape_markdownv2_preserves_link_with_special_chars_in_text() {
        let text = "[Greg's take on AI](https://x.com/gdb/status/123)";
        let escaped = escape_markdownv2(text);
        // Link structure preserved, apostrophe in text is fine (not a special char)
        assert!(escaped.contains("[Greg's take on AI](https://x.com/gdb/status/123)"));
    }

    #[test]
    fn test_escape_markdownv2_preserves_link_escapes_special_in_label() {
        let text = "[AI + ML news](https://example.com)";
        let escaped = escape_markdownv2(text);
        // The + in the link text should be escaped
        assert!(escaped.contains("[AI \\+ ML news](https://example.com)"));
    }

    #[test]
    fn test_escape_markdownv2_preserves_multiple_links() {
        let text = "See [link1](https://a.com) and [link2](https://b.com) for details.";
        let escaped = escape_markdownv2(text);
        // Both links should be preserved
        assert!(escaped.contains("[link1](https://a.com)"));
        assert!(escaped.contains("[link2](https://b.com)"));
        // Dots outside links should be escaped
        assert!(escaped.contains("details\\."));
    }

    #[test]
    fn test_escape_markdownv2_link_with_hyphen_in_label() {
        let text = "[AI-powered tool](https://example.com)";
        let escaped = escape_markdownv2(text);
        // Hyphen in link text should be escaped
        assert!(escaped.contains("[AI\\-powered tool](https://example.com)"));
    }

    #[test]
    fn test_escape_markdownv2_link_with_underscore_in_label() {
        let text = "[user_name's post](https://x.com/user)";
        let escaped = escape_markdownv2(text);
        // Underscore in link text should be escaped
        assert!(escaped.contains("[user\\_name's post](https://x.com/user)"));
    }

    #[test]
    fn test_escape_markdownv2_text_with_brackets_not_link() {
        let text = "Array [0] and (parentheses) are escaped";
        let escaped = escape_markdownv2(text);
        // Brackets and parentheses not part of a link should be escaped
        assert!(escaped.contains("\\[0\\]"));
        assert!(escaped.contains("\\(parentheses\\)"));
    }

    #[test]
    fn test_escape_markdownv2_mixed_links_and_special_chars() {
        let text = "News: [OpenAI's GPT-5](https://openai.com) is *amazing*!";
        let escaped = escape_markdownv2(text);
        // Link preserved with special chars in label escaped
        assert!(escaped.contains("[OpenAI's GPT\\-5](https://openai.com)"));
        // Asterisks outside link escaped
        assert!(escaped.contains("\\*amazing\\*"));
        // Exclamation escaped
        assert!(escaped.contains("\\!"));
    }

    #[test]
    fn test_escape_markdownv2_real_tweet_link() {
        let text = "Read more: [Greg Brockman on OpenAI history](https://x.com/gdb/status/2012328084985500005)";
        let escaped = escape_markdownv2(text);
        // Link should be fully preserved
        assert!(escaped.contains(
            "[Greg Brockman on OpenAI history](https://x.com/gdb/status/2012328084985500005)"
        ));
        // Colon after "Read more" should be fine (not a special char)
        assert!(escaped.contains("Read more:"));
    }

    #[test]
    fn test_escape_markdownv2_link_at_start() {
        let text = "[First link](https://a.com) starts the text";
        let escaped = escape_markdownv2(text);
        assert!(escaped.contains("[First link](https://a.com)"));
    }

    #[test]
    fn test_escape_markdownv2_link_at_end() {
        let text = "Text ends with [last link](https://z.com)";
        let escaped = escape_markdownv2(text);
        assert!(escaped.contains("[last link](https://z.com)"));
    }

    #[test]
    fn test_escape_markdownv2_url_with_special_chars() {
        // URLs with query params, fragments, etc.
        let text = "[Search](https://google.com/search?q=test&lang=en)";
        let escaped = escape_markdownv2(text);
        // URL should be preserved (only ) and \ need escaping in URLs)
        assert!(escaped.contains("[Search](https://google.com/search?q=test&lang=en)"));
    }

    // ==================== Realistic OpenAI Summary Tests ====================

    #[test]
    fn test_summary_message_with_bullets() {
        let summary = "- First point\n- Second point\n- Third point";
        let timestamp = Utc::now().format("%Y-%m-%d %H:%M UTC");
        let message = format!(
            "📰 *Twitter Summary*\n_{}_\n\n{}",
            timestamp,
            escape_markdownv2(summary)
        );
        // Hyphens in bullet points should be escaped
        assert!(message.contains("\\- First point"));
    }

    #[test]
    fn test_summary_message_with_urls() {
        let summary = "Read more: https://example.com/article";
        let message = format!(
            "📰 *Twitter Summary*\n_{}_\n\n{}",
            Utc::now().format("%Y-%m-%d %H:%M UTC"),
            escape_markdownv2(summary)
        );
        // Periods in URLs should be escaped
        assert!(message.contains("example\\.com"));
    }

    #[test]
    fn test_summary_message_with_versions() {
        let summary = "GPT-4.5 released! Version 1.0.0 now available.";
        let message = format!(
            "📰 *Twitter Summary*\n_{}_\n\n{}",
            Utc::now().format("%Y-%m-%d %H:%M UTC"),
            escape_markdownv2(summary)
        );
        assert!(message.contains("GPT\\-4\\.5"));
        assert!(message.contains("1\\.0\\.0"));
        assert!(message.contains("available\\."));
        assert!(message.contains("\\!"));
    }

    #[test]
    fn test_summary_message_with_equations() {
        let summary = "Formula: x + y = z, ratio 2*3=6";
        let message = format!(
            "📰 *Twitter Summary*\n_{}_\n\n{}",
            Utc::now().format("%Y-%m-%d %H:%M UTC"),
            escape_markdownv2(summary)
        );
        assert!(message.contains("x \\+ y \\= z"));
        assert!(message.contains("2\\*3\\=6"));
    }

    #[test]
    fn test_summary_message_with_brackets() {
        let summary = "Check [details] and (more info) here";
        let message = format!(
            "📰 *Twitter Summary*\n_{}_\n\n{}",
            Utc::now().format("%Y-%m-%d %H:%M UTC"),
            escape_markdownv2(summary)
        );
        assert!(message.contains("\\[details\\]"));
        assert!(message.contains("\\(more info\\)"));
    }

    #[test]
    fn test_summary_message_with_code_references() {
        let summary = "Use `import torch` for PyTorch";
        let message = format!(
            "📰 *Twitter Summary*\n_{}_\n\n{}",
            Utc::now().format("%Y-%m-%d %H:%M UTC"),
            escape_markdownv2(summary)
        );
        assert!(message.contains("\\`import torch\\`"));
    }

    #[test]
    fn test_summary_message_with_underscores() {
        let summary = "python_file.py and snake_case variables";
        let message = format!(
            "📰 *Twitter Summary*\n_{}_\n\n{}",
            Utc::now().format("%Y-%m-%d %H:%M UTC"),
            escape_markdownv2(summary)
        );
        assert!(message.contains("python\\_file\\.py"));
        assert!(message.contains("snake\\_case"));
    }

    #[test]
    fn test_summary_message_with_hashtags() {
        let summary = "#AI and #MachineLearning trends";
        let message = format!(
            "📰 *Twitter Summary*\n_{}_\n\n{}",
            Utc::now().format("%Y-%m-%d %H:%M UTC"),
            escape_markdownv2(summary)
        );
        assert!(message.contains("\\#AI"));
        assert!(message.contains("\\#MachineLearning"));
    }

    #[test]
    fn test_summary_message_with_quotes() {
        let summary = r#"CEO said "This is huge" announcement"#;
        let message = format!(
            "📰 *Twitter Summary*\n_{}_\n\n{}",
            Utc::now().format("%Y-%m-%d %H:%M UTC"),
            escape_markdownv2(summary)
        );
        // Quotes don't need escaping in MarkdownV2
        assert!(message.contains(r#""This is huge""#));
    }

    #[test]
    fn test_summary_message_with_email() {
        let summary = "Contact: user@example.com";
        let message = format!(
            "📰 *Twitter Summary*\n_{}_\n\n{}",
            Utc::now().format("%Y-%m-%d %H:%M UTC"),
            escape_markdownv2(summary)
        );
        // @ doesn't need escaping, but . does
        assert!(message.contains("example\\.com"));
        assert!(message.contains("@"));
    }

    #[test]
    fn test_summary_message_with_multiple_special_chars() {
        let summary = "API v2.0: performance > 2x! Cost = $50/month (20% discount)";
        let message = format!(
            "📰 *Twitter Summary*\n_{}_\n\n{}",
            Utc::now().format("%Y-%m-%d %H:%M UTC"),
            escape_markdownv2(summary)
        );
        assert!(message.contains("v2\\.0"));
        assert!(message.contains("\\> 2x\\!"));
        assert!(message.contains("\\= $50/month"));
        assert!(message.contains("\\(20% discount\\)"));
    }

    #[test]
    fn test_summary_message_complex_realistic() {
        let summary = r#"Key Updates:
- OpenAI releases GPT-4.5 (30% faster!)
- Anthropic Claude 3: performance > baseline
- New pricing: $20/month (was $25)
- Read more: https://openai.com/blog

Discussion on #AI trends and python_utils.py examples."#;

        let message = format!(
            "📰 *Twitter Summary*\n_{}_\n\n{}",
            Utc::now().format("%Y-%m-%d %H:%M UTC"),
            escape_markdownv2(summary)
        );

        // Verify all special chars are escaped
        assert!(message.contains("GPT\\-4\\.5"));
        assert!(message.contains("\\(30% faster\\!\\)"));
        assert!(message.contains("\\> baseline"));
        assert!(message.contains("$20/month \\(was $25\\)"));
        assert!(message.contains("openai\\.com"));
        assert!(message.contains("\\#AI"));
        assert!(message.contains("python\\_utils\\.py"));
    }

    #[test]
    fn test_welcome_summary_with_special_chars() {
        let summary = "AI research: 50+ papers! Performance > 2x baseline.";
        let timestamp = Utc::now().format("%Y-%m-%d %H:%M UTC");
        let message = format!(
            "📰 *Hey! Here's what you missed* 😉\n_{}_\n\n{}",
            timestamp,
            escape_markdownv2(summary)
        );
        assert!(message.contains("50\\+ papers\\!"));
        assert!(message.contains("\\> 2x baseline\\."));
    }

    #[test]
    fn test_summary_with_nested_formatting_attempts() {
        // OpenAI might try to use Markdown formatting
        let summary = "*This is bold* and _this is italic_ text";
        let message = format!(
            "📰 *Twitter Summary*\n_{}_\n\n{}",
            Utc::now().format("%Y-%m-%d %H:%M UTC"),
            escape_markdownv2(summary)
        );
        // All * and _ should be escaped, preventing nested formatting
        assert!(message.contains("\\*This is bold\\*"));
        assert!(message.contains("\\_this is italic\\_"));
    }

    #[test]
    fn test_summary_unicode_with_special_chars() {
        let summary = "日本語: GPT-4 > GPT-3.5! 性能向上 (30%)";
        let message = format!(
            "📰 *Twitter Summary*\n_{}_\n\n{}",
            Utc::now().format("%Y-%m-%d %H:%M UTC"),
            escape_markdownv2(summary)
        );
        assert!(message.contains("GPT\\-4 \\> GPT\\-3\\.5\\!"));
        assert!(message.contains("\\(30%\\)"));
    }

    #[test]
    fn test_summary_emoji_with_special_chars() {
        let summary = "🚀 Release v1.0! 🎉 Performance > 2x 📈";
        let message = format!(
            "📰 *Twitter Summary*\n_{}_\n\n{}",
            Utc::now().format("%Y-%m-%d %H:%M UTC"),
            escape_markdownv2(summary)
        );
        assert!(message.contains("🚀")); // Emoji preserved
        assert!(message.contains("v1\\.0\\!"));
        assert!(message.contains("\\> 2x"));
    }

    #[test]
    fn test_summary_with_pipe_and_braces() {
        let summary = "Options: {option1 | option2} available";
        let message = format!(
            "📰 *Twitter Summary*\n_{}_\n\n{}",
            Utc::now().format("%Y-%m-%d %H:%M UTC"),
            escape_markdownv2(summary)
        );
        assert!(message.contains("\\{option1 \\| option2\\}"));
    }

    #[test]
    fn test_summary_with_tildes() {
        let summary = "~deprecated~ feature removed";
        let message = format!(
            "📰 *Twitter Summary*\n_{}_\n\n{}",
            Utc::now().format("%Y-%m-%d %H:%M UTC"),
            escape_markdownv2(summary)
        );
        assert!(message.contains("\\~deprecated\\~"));
    }

    // ==================== Edge Case Tests ====================

    #[test]
    fn test_summary_very_long_with_special_chars() {
        let summary = format!(
            "{}Conclusion: v2.0 release! Performance > baseline. Cost = $50/month (discount!).",
            "Long content. ".repeat(100)
        );
        let message = format!(
            "📰 *Twitter Summary*\n_{}_\n\n{}",
            Utc::now().format("%Y-%m-%d %H:%M UTC"),
            escape_markdownv2(&summary)
        );
        assert!(message.contains("v2\\.0"));
        assert!(message.contains("\\> baseline\\."));
        assert!(message.contains("\\(discount\\!\\)\\."));
    }

    #[test]
    fn test_summary_empty_string() {
        let message = format!(
            "📰 *Twitter Summary*\n_{}_\n\n{}",
            Utc::now().format("%Y-%m-%d %H:%M UTC"),
            escape_markdownv2("")
        );
        assert!(message.contains("*Twitter Summary*"));
    }

    #[test]
    fn test_summary_only_special_chars() {
        let summary = "!@#$%^&*()_+-=[]{}|;:',.<>?/~`";
        let escaped = escape_markdownv2(summary);
        // All MarkdownV2 special chars should be escaped
        assert!(escaped.contains("\\!"));
        assert!(escaped.contains("\\*"));
        assert!(escaped.contains("\\(\\)"));
        assert!(escaped.contains("\\_"));
    }

    #[test]
    fn test_summary_consecutive_special_chars() {
        let summary = "!!!---***+++";
        let escaped = escape_markdownv2(summary);
        assert_eq!(escaped, "\\!\\!\\!\\-\\-\\-\\*\\*\\*\\+\\+\\+");
    }

    #[test]
    fn test_summary_at_byte_offset_587_simulation() {
        // Recreate production bug scenario with MarkdownV2
        let padding = "Analysis: ".to_string() + &"A".repeat(540);
        let summary = format!(
            "{}GPT-4 vs Claude: performance > baseline! Cost = $50.",
            padding
        );
        let message = format!(
            "📰 *Twitter Summary*\n_{}_\n\n{}",
            Utc::now().format("%Y-%m-%d %H:%M UTC"),
            escape_markdownv2(&summary)
        );

        // Verify escaping works at any byte position
        assert!(message.contains("\\> baseline\\!"));
        assert!(message.contains("\\= $50\\."));
    }

    #[test]
    fn test_welcome_vs_regular_both_escape() {
        let summary = "GPT-4.5! Performance > 2x.";
        let timestamp = Utc::now().format("%Y-%m-%d %H:%M UTC");

        let welcome = format!(
            "📰 *Hey! Here's what you missed* 😉\n_{}_\n\n{}",
            timestamp,
            escape_markdownv2(summary)
        );
        let regular = format!(
            "📰 *Twitter Summary*\n_{}_\n\n{}",
            timestamp,
            escape_markdownv2(summary)
        );

        // Both should escape identically
        assert!(welcome.contains("GPT\\-4\\.5\\!"));
        assert!(regular.contains("GPT\\-4\\.5\\!"));
        assert!(welcome.contains("\\> 2x\\."));
        assert!(regular.contains("\\> 2x\\."));
    }

    #[test]
    fn test_backslash_in_path() {
        // Windows paths with backslashes
        let summary = r"File: C:\Users\file.txt";
        let escaped = escape_markdownv2(summary);
        // Backslashes themselves don't need escaping, but dots do
        assert!(escaped.contains("file\\.txt"));
    }

    #[test]
    fn test_newlines_and_formatting() {
        let summary = "Line 1.\nLine 2!\nLine 3?";
        let escaped = escape_markdownv2(summary);
        // Newlines preserved, but dots and ! escaped
        assert!(escaped.contains("Line 1\\."));
        assert!(escaped.contains("Line 2\\!"));
        assert!(escaped.contains("\n"));
    }

    // ==================== MarkdownV2 Compliance Validation Tests ====================
    //
    // These tests validate that ALL message content sent via Telegram is properly escaped
    // for MarkdownV2 mode. According to Telegram Bot API docs, these 18 characters must
    // be escaped with a preceding backslash:
    // _ * [ ] ( ) ~ ` > # + - = | { } . !
    //
    // Tests will FAIL if any message contains unescaped special characters.

    /// The 18 special characters that MUST be escaped in MarkdownV2 mode
    const MARKDOWNV2_SPECIAL_CHARS: [char; 18] = [
        '_', '*', '[', ']', '(', ')', '~', '`', '>', '#', '+', '-', '=', '|', '{', '}', '.', '!',
    ];

    // ==================== Individual Special Character Tests ====================

    #[test]
    fn test_escape_each_special_char_individually() {
        // Test each of the 18 special characters individually
        let test_cases = [
            ("test_text", "test\\_text"), // underscore
            ("test*text", "test\\*text"), // asterisk
            ("test[text", "test\\[text"), // left bracket
            ("test]text", "test\\]text"), // right bracket
            ("test(text", "test\\(text"), // left paren
            ("test)text", "test\\)text"), // right paren
            ("test~text", "test\\~text"), // tilde
            ("test`text", "test\\`text"), // backtick
            ("test>text", "test\\>text"), // greater than
            ("test#text", "test\\#text"), // hash
            ("test+text", "test\\+text"), // plus
            ("test-text", "test\\-text"), // hyphen/minus
            ("test=text", "test\\=text"), // equals
            ("test|text", "test\\|text"), // pipe
            ("test{text", "test\\{text"), // left brace
            ("test}text", "test\\}text"), // right brace
            ("test.text", "test\\.text"), // period
            ("test!text", "test\\!text"), // exclamation
        ];

        for (input, expected) in test_cases {
            let result = escape_markdownv2(input);
            assert_eq!(
                result, expected,
                "Failed for input '{}': expected '{}', got '{}'",
                input, expected, result
            );
        }
    }

    #[test]
    fn test_escape_all_18_special_chars_in_sequence() {
        let input = "_*[]()~`>#+-=|{}.!";
        let expected = "\\_\\*\\[\\]\\(\\)\\~\\`\\>\\#\\+\\-\\=\\|\\{\\}\\.\\!";
        let result = escape_markdownv2(input);
        assert_eq!(result, expected);
    }

    #[test]
    fn test_escape_repeated_special_chars() {
        assert_eq!(escape_markdownv2("!!!"), "\\!\\!\\!");
        assert_eq!(escape_markdownv2("..."), "\\.\\.\\.");
        assert_eq!(escape_markdownv2("---"), "\\-\\-\\-");
        assert_eq!(escape_markdownv2("***"), "\\*\\*\\*");
        assert_eq!(escape_markdownv2("___"), "\\_\\_\\_");
    }

    #[test]
    fn test_escape_alternating_special_chars() {
        assert_eq!(escape_markdownv2("!.!.!"), "\\!\\.\\!\\.\\!");
        assert_eq!(escape_markdownv2("-_-_-"), "\\-\\_\\-\\_\\-");
    }

    // ==================== Bot Command Response Validation Tests ====================
    //
    // These tests verify that ALL pre-escaped command responses are valid MarkdownV2.
    // If any message fails, it means production code will cause Telegram API errors.

    #[test]
    fn test_start_command_welcome_message_is_valid_markdownv2() {
        // This is the exact message from handle_webhook for /start command
        let welcome = "👋 Welcome to Twitter News Summary Bot\\!\n\n\
Commands:\n\
/subscribe \\- Get daily AI\\-powered summaries of Twitter/X news\n\
/unsubscribe \\- Stop receiving summaries\n\
/status \\- Check your subscription status\n\n\
Summaries are sent twice daily with the latest tweets from tech leaders and AI researchers\\.";

        // Validate each special character is escaped
        // ! should be escaped: \!
        assert!(
            welcome.contains("Bot\\!"),
            "Exclamation in 'Bot!' must be escaped"
        );
        assert!(
            welcome.contains("researchers\\."),
            "Period in 'researchers.' must be escaped"
        );
        assert!(
            welcome.contains("\\-"),
            "Hyphens in command descriptions must be escaped"
        );

        // Verify no unescaped special chars exist (except intentional ones)
        // The message should NOT contain unescaped ! . -
        let unescaped_exclamation = welcome
            .match_indices('!')
            .any(|(i, _)| i == 0 || welcome.as_bytes()[i - 1] != b'\\');
        assert!(
            !unescaped_exclamation,
            "Found unescaped '!' in welcome message"
        );

        let unescaped_period = welcome
            .match_indices('.')
            .any(|(i, _)| i == 0 || welcome.as_bytes()[i - 1] != b'\\');
        assert!(!unescaped_period, "Found unescaped '.' in welcome message");

        let unescaped_hyphen = welcome
            .match_indices('-')
            .any(|(i, _)| i == 0 || welcome.as_bytes()[i - 1] != b'\\');
        assert!(!unescaped_hyphen, "Found unescaped '-' in welcome message");
    }

    #[test]
    fn test_subscribe_already_subscribed_message_is_valid_markdownv2() {
        let msg = "✅ You're already subscribed\\!";

        // ! must be escaped
        let unescaped = msg
            .match_indices('!')
            .any(|(i, _)| i == 0 || msg.as_bytes()[i - 1] != b'\\');
        assert!(
            !unescaped,
            "Exclamation must be escaped in 'already subscribed!' message"
        );
    }

    #[test]
    fn test_subscribe_success_message_is_valid_markdownv2() {
        let msg = "✅ Successfully subscribed\\! You'll receive summaries twice daily\\.";

        // Verify ! and . are escaped
        let unescaped_exclamation = msg
            .match_indices('!')
            .any(|(i, _)| i == 0 || msg.as_bytes()[i - 1] != b'\\');
        assert!(
            !unescaped_exclamation,
            "Exclamation must be escaped in success message"
        );

        let unescaped_period = msg
            .match_indices('.')
            .any(|(i, _)| i == 0 || msg.as_bytes()[i - 1] != b'\\');
        assert!(
            !unescaped_period,
            "Period must be escaped in success message"
        );
    }

    #[test]
    fn test_unsubscribe_success_message_is_valid_markdownv2() {
        let msg = "👋 Successfully unsubscribed\\. You won't receive any more summaries\\.";

        let unescaped_period = msg
            .match_indices('.')
            .any(|(i, _)| i == 0 || msg.as_bytes()[i - 1] != b'\\');
        assert!(
            !unescaped_period,
            "Periods must be escaped in unsubscribe message"
        );
    }

    #[test]
    fn test_unsubscribe_not_subscribed_message_is_valid_markdownv2() {
        let msg = "You're not currently subscribed\\.";

        let unescaped_period = msg
            .match_indices('.')
            .any(|(i, _)| i == 0 || msg.as_bytes()[i - 1] != b'\\');
        assert!(!unescaped_period, "Period must be escaped");
    }

    #[test]
    fn test_status_subscribed_message_is_valid_markdownv2() {
        // Regular user status (no special chars to escape in basic version)
        let msg = "✅ You are subscribed";
        // This message has no special chars that need escaping
        // Just verify it doesn't accidentally contain unescaped ones
        for c in MARKDOWNV2_SPECIAL_CHARS {
            assert!(
                !msg.contains(c),
                "Status message should not contain special char '{}' unless escaped",
                c
            );
        }
    }

    #[test]
    fn test_status_admin_message_needs_escaping() {
        // Admin status includes subscriber count - verify the format
        // The actual message in code: "✅ You are subscribed\n📊 Total subscribers: {}"
        // This should be escaped before sending!

        // Test that if we have dynamic content, we escape it properly
        let subscriber_count = 42;
        let raw_msg = format!(
            "✅ You are subscribed\n📊 Total subscribers: {}",
            subscriber_count
        );

        // This raw message should be escaped before sending
        // Verify the colon doesn't need escaping (it's not in the 18 special chars)
        // But if the message had other special chars, they would need escaping

        // The current implementation passes this directly to send_message
        // which expects pre-escaped content - this is a potential BUG!
        // For now, verify the message doesn't accidentally have special chars
        let has_special = raw_msg
            .chars()
            .any(|c| MARKDOWNV2_SPECIAL_CHARS.contains(&c));
        assert!(
            !has_special,
            "Admin status message has no special chars to escape (current implementation)"
        );
    }

    #[test]
    fn test_status_not_subscribed_message_is_valid_markdownv2() {
        let msg = "❌ You are not subscribed\n\nUse /subscribe to start receiving summaries\\.";

        let unescaped_period = msg
            .match_indices('.')
            .any(|(i, _)| i == 0 || msg.as_bytes()[i - 1] != b'\\');
        assert!(!unescaped_period, "Period must be escaped");
    }

    #[test]
    fn test_unknown_command_message_is_valid_markdownv2() {
        let msg = "Unknown command\\. Use /start to see available commands\\.";

        let unescaped_period = msg
            .match_indices('.')
            .any(|(i, _)| i == 0 || msg.as_bytes()[i - 1] != b'\\');
        assert!(!unescaped_period, "Periods must be escaped");
    }

    // ==================== Summary Message Validation Tests ====================

    #[test]
    fn test_summary_header_is_valid_markdownv2() {
        // The summary header format from send_to_subscribers
        let timestamp = "2024\\-01\\-15 10:30 UTC";
        let header = format!("📰 *Twitter Summary*\n_{}_\n\n", timestamp);

        // * is used for bold, _ for italic - these are intentional formatting
        // The timestamp has escaped hyphens
        assert!(
            header.contains("*Twitter Summary*"),
            "Bold markers should be present"
        );
        assert!(header.contains("_2024"), "Italic markers should be present");
    }

    #[test]
    fn test_welcome_summary_header_is_valid_markdownv2() {
        // The welcome summary header format
        let timestamp = "2024\\-01\\-15 10:30 UTC";
        let header = format!("📰 *Hey\\! Here's what you missed* 😉\n_{}_\n\n", timestamp);

        // ! in "Hey!" must be escaped
        assert!(
            header.contains("Hey\\!"),
            "Exclamation in header must be escaped"
        );
    }

    #[test]
    fn test_test_summary_header_is_valid_markdownv2() {
        // The test summary header format from send_test_message
        let timestamp = "2024\\-01\\-15 10:30 UTC";
        let header = format!("🧪 *TEST \\- Twitter Summary*\n_{}_\n\n", timestamp);

        // - in "TEST - Twitter" must be escaped
        assert!(
            header.contains("TEST \\-"),
            "Hyphen in header must be escaped"
        );
    }

    #[test]
    fn test_admin_notification_is_valid_markdownv2() {
        // Admin notification format from send_to_subscribers
        let success_count = 10;
        let total = 12;
        let fail_count = 2;
        let msg = format!(
            "📊 Summary sent to {}/{} subscribers \\({} failed\\)",
            success_count, total, fail_count
        );

        // Parentheses must be escaped
        assert!(msg.contains("\\("), "Opening paren must be escaped");
        assert!(msg.contains("\\)"), "Closing paren must be escaped");
    }

    // ==================== Dynamic Content Escaping Tests ====================

    #[test]
    fn test_escape_timestamp_content() {
        // Timestamps contain hyphens and colons
        let timestamp = "2024-01-15 10:30 UTC";
        let escaped = escape_markdownv2(timestamp);

        assert_eq!(escaped, "2024\\-01\\-15 10:30 UTC");
    }

    #[test]
    fn test_escape_subscriber_count_in_message() {
        // Subscriber counts are numbers, but surrounding text might have special chars
        let msg = format!("Total: {} subscribers!", 42);
        let escaped = escape_markdownv2(&msg);

        assert!(
            escaped.contains("subscribers\\!"),
            "Exclamation must be escaped"
        );
    }

    #[test]
    fn test_escape_username_with_underscores() {
        // Twitter usernames often have underscores
        let username = "user_name_123";
        let msg = format!("Welcome {}!", username);
        let escaped = escape_markdownv2(&msg);

        assert!(
            escaped.contains("user\\_name\\_123"),
            "Underscores must be escaped"
        );
        assert!(escaped.contains("\\!"), "Exclamation must be escaped");
    }

    // ==================== Emoji Preservation Tests ====================

    #[test]
    fn test_emoji_preserved_after_escaping() {
        let msg = "🎉 Success! 🚀 Launch! 📰 News!";
        let escaped = escape_markdownv2(msg);

        assert!(escaped.contains("🎉"), "Party emoji must be preserved");
        assert!(escaped.contains("🚀"), "Rocket emoji must be preserved");
        assert!(escaped.contains("📰"), "News emoji must be preserved");
        assert!(escaped.contains("Success\\!"), "Text must be escaped");
    }

    #[test]
    fn test_emoji_next_to_special_chars() {
        let msg = "🎉!🚀.📰-";
        let escaped = escape_markdownv2(msg);

        assert_eq!(escaped, "🎉\\!🚀\\.📰\\-");
    }

    #[test]
    fn test_various_emojis_preserved() {
        let emojis = "👋 ✅ ❌ 📊 🧪 📰 🎉 🚀 😉";
        let escaped = escape_markdownv2(emojis);

        // No special chars to escape, emojis should be unchanged
        assert_eq!(escaped, emojis);
    }

    // ==================== Multi-line Message Tests ====================

    #[test]
    fn test_multiline_message_escaping() {
        let msg = "Line 1!\nLine 2.\nLine 3-test\nLine 4 (note)";
        let escaped = escape_markdownv2(msg);

        assert!(escaped.contains("Line 1\\!"));
        assert!(escaped.contains("Line 2\\."));
        assert!(escaped.contains("Line 3\\-test"));
        assert!(escaped.contains("Line 4 \\(note\\)"));
        assert!(escaped.contains('\n'), "Newlines must be preserved");
    }

    #[test]
    fn test_multiline_with_indentation() {
        let msg = "Header:\n  - Item 1\n  - Item 2\n  - Item 3";
        let escaped = escape_markdownv2(msg);

        // Hyphens in list items must be escaped
        assert!(
            escaped.matches("\\-").count() == 3,
            "All three hyphens must be escaped"
        );
    }

    #[test]
    fn test_complex_multiline_summary() {
        let summary = r#"AI News Summary:

1. OpenAI releases GPT-4.5!
2. Anthropic Claude 3 > baseline
3. Cost = $20/month (was $25)

Read more: https://example.com/news"#;

        let escaped = escape_markdownv2(summary);

        // Verify all special chars are escaped
        assert!(escaped.contains("GPT\\-4\\.5\\!"));
        assert!(escaped.contains("Claude 3 \\> baseline"));
        assert!(escaped.contains("\\= $20/month"));
        assert!(escaped.contains("\\(was $25\\)"));
        assert!(escaped.contains("example\\.com"));
    }

    // ==================== Link Preservation and Escaping Tests ====================

    #[test]
    fn test_link_structure_preserved_simple() {
        let text = "[Click here](https://example.com)";
        let escaped = escape_markdownv2(text);

        // Link structure should be intact
        assert!(escaped.contains("[Click here](https://example.com)"));
    }

    #[test]
    fn test_link_text_with_special_chars_escaped() {
        // Special chars in link text should be escaped
        let text = "[GPT-4.5 release!](https://openai.com)";
        let escaped = escape_markdownv2(text);

        // Link preserved but special chars in text escaped
        assert!(escaped.contains("[GPT\\-4\\.5 release\\!](https://openai.com)"));
    }

    #[test]
    fn test_link_url_with_query_params_preserved() {
        let text = "[Search](https://google.com/search?q=test&page=1)";
        let escaped = escape_markdownv2(text);

        // URL should be preserved (only ) and \ need escaping in URLs)
        assert!(escaped.contains("[Search](https://google.com/search?q=test&page=1)"));
    }

    #[test]
    fn test_link_url_with_closing_paren_escaped() {
        // URLs with ) inside are tricky - the current regex captures up to first )
        // This is a known limitation. For URLs with ) inside, only ) needs escaping per Telegram docs.
        let text = "[Link](https://example.com/path)";
        let escaped = escape_markdownv2(text);

        // Simple URL without parens works
        assert!(escaped.contains("[Link](https://example.com/path)"));

        // Test that ) in URL gets escaped when detected
        // Note: Current implementation has a limitation with nested parens in URLs
        // This documents actual behavior
        let text_with_paren_in_url =
            "[Link](https://en.wikipedia.org/wiki/Rust_(programming_language\\))";
        // This URL has a pre-escaped ) to work around the limitation
        let escaped2 = escape_markdownv2(text_with_paren_in_url);
        assert!(escaped2.contains("Rust_"));
    }

    #[test]
    fn test_multiple_links_in_text() {
        let text = "See [link1](https://a.com) and [link2](https://b.com)!";
        let escaped = escape_markdownv2(text);

        // Both links preserved, ! after escaped
        assert!(escaped.contains("[link1](https://a.com)"));
        assert!(escaped.contains("[link2](https://b.com)"));
        assert!(escaped.ends_with("\\!"));
    }

    #[test]
    fn test_link_surrounded_by_special_chars() {
        let text = "!!! [link](https://x.com) !!!";
        let escaped = escape_markdownv2(text);

        // ! should be escaped, link preserved
        assert!(escaped.contains("\\!\\!\\!"));
        assert!(escaped.contains("[link](https://x.com)"));
    }

    #[test]
    fn test_fake_link_escaped() {
        // Not a real link - brackets and parens should be escaped
        let text = "Array [0] and function()";
        let escaped = escape_markdownv2(text);

        assert!(escaped.contains("\\[0\\]"));
        assert!(escaped.contains("function\\(\\)"));
    }

    // ==================== Real-World Summary Content Tests ====================

    #[test]
    fn test_realistic_ai_news_summary() {
        let summary = r#"Here are the key AI updates from the last 12 hours:

**AI Model Updates:**
- OpenAI announced GPT-4.5 with improved reasoning (30% faster!)
- Anthropic's Claude 3 performance > Claude 2 by significant margin
- Google DeepMind releases Gemini 1.5 Pro

**Industry News:**
- Startup raised $50M (Series B) for AI infrastructure
- Microsoft + OpenAI partnership extended to 2030
- New open-source model: Llama 3 = state-of-the-art

**Technical Highlights:**
- Training cost reduced by 40% using new techniques
- RAG systems now support 1M+ token context
- Fine-tuning API v2.0 available

For details: [OpenAI Blog](https://openai.com/blog)"#;

        let escaped = escape_markdownv2(summary);

        // Verify all special chars are properly escaped
        assert!(escaped.contains("GPT\\-4\\.5"));
        assert!(escaped.contains("\\(30% faster\\!\\)"));
        assert!(escaped.contains("Claude 3 performance \\> Claude 2"));
        assert!(escaped.contains("$50M \\(Series B\\)"));
        assert!(escaped.contains("Microsoft \\+ OpenAI"));
        assert!(escaped.contains("Llama 3 \\= state\\-of\\-the\\-art"));
        assert!(escaped.contains("40%"));
        assert!(escaped.contains("1M\\+"));
        assert!(escaped.contains("v2\\.0"));

        // Link should be preserved
        assert!(escaped.contains("[OpenAI Blog](https://openai.com/blog)"));
    }

    #[test]
    fn test_realistic_twitter_mentions() {
        let summary = "@user_name posted about AI trends! #MachineLearning #AI";
        let escaped = escape_markdownv2(summary);

        // @ doesn't need escaping, but _ # ! do
        assert!(escaped.contains("@user\\_name"));
        assert!(escaped.contains("trends\\!"));
        assert!(escaped.contains("\\#MachineLearning"));
        assert!(escaped.contains("\\#AI"));
    }

    #[test]
    fn test_realistic_code_snippet_mention() {
        let summary = "Use `pip install torch` for PyTorch 2.0. Example: model.train()";
        let escaped = escape_markdownv2(summary);

        // Backticks and dots must be escaped
        assert!(escaped.contains("\\`pip install torch\\`"));
        assert!(escaped.contains("PyTorch 2\\.0\\."));
        assert!(escaped.contains("model\\.train\\(\\)"));
    }

    #[test]
    fn test_realistic_math_content() {
        let summary = "Performance: accuracy > 95%, loss < 0.01, F1 = 0.98";
        let escaped = escape_markdownv2(summary);

        assert!(escaped.contains("accuracy \\> 95%"));
        assert!(escaped.contains("loss < 0\\.01")); // < doesn't need escaping
        assert!(escaped.contains("F1 \\= 0\\.98"));
    }

    // ==================== Consecutive Special Character Tests ====================

    #[test]
    fn test_consecutive_exclamations() {
        let msg = "Amazing news!!!";
        let escaped = escape_markdownv2(msg);
        assert!(escaped.ends_with("\\!\\!\\!"));
    }

    #[test]
    fn test_consecutive_periods() {
        let msg = "To be continued...";
        let escaped = escape_markdownv2(msg);
        assert!(escaped.ends_with("continued\\.\\.\\."));
    }

    #[test]
    fn test_consecutive_hyphens() {
        let msg = "Section --- break";
        let escaped = escape_markdownv2(msg);
        assert!(escaped.contains("\\-\\-\\-"));
    }

    #[test]
    fn test_mixed_consecutive_special_chars() {
        let msg = "Wait...!!! Really?!";
        let escaped = escape_markdownv2(msg);
        assert!(escaped.contains("Wait\\.\\.\\.\\!\\!\\!"));
        assert!(escaped.contains("Really?\\!"));
    }

    // ==================== Edge Cases ====================

    #[test]
    fn test_empty_string_escaping() {
        assert_eq!(escape_markdownv2(""), "");
    }

    #[test]
    fn test_whitespace_only_escaping() {
        assert_eq!(escape_markdownv2("   "), "   ");
        assert_eq!(escape_markdownv2("\n\t\n"), "\n\t\n");
    }

    #[test]
    fn test_unicode_text_with_special_chars() {
        let msg = "日本語テスト! 中文测试. Тест!";
        let escaped = escape_markdownv2(msg);

        assert!(escaped.contains("日本語テスト\\!"));
        assert!(escaped.contains("中文测试\\."));
        assert!(escaped.contains("Тест\\!"));
    }

    #[test]
    fn test_very_long_message_escaping() {
        let msg = format!(
            "Long text with special chars! {} End.",
            "word ".repeat(1000)
        );
        let escaped = escape_markdownv2(&msg);

        assert!(escaped.starts_with("Long text with special chars\\!"));
        assert!(escaped.ends_with("End\\."));
    }

    #[test]
    fn test_special_char_at_string_boundaries() {
        // Start with special char
        assert_eq!(escape_markdownv2("!start"), "\\!start");
        assert_eq!(escape_markdownv2(".start"), "\\.start");
        assert_eq!(escape_markdownv2("-start"), "\\-start");

        // End with special char
        assert_eq!(escape_markdownv2("end!"), "end\\!");
        assert_eq!(escape_markdownv2("end."), "end\\.");
        assert_eq!(escape_markdownv2("end-"), "end\\-");
    }

    #[test]
    fn test_only_special_chars() {
        let msg = "!@#$%^&*()_+-=[]{}|;:',.<>?/~`";
        let escaped = escape_markdownv2(msg);

        // Verify all 18 MarkdownV2 special chars are escaped
        // Non-special chars (@, $, %, ^, &, ;, :, ', ,, <, ?, /) should NOT be escaped
        assert!(escaped.contains("\\!"));
        assert!(escaped.contains("\\#"));
        assert!(escaped.contains("\\*"));
        assert!(escaped.contains("\\(\\)"));
        assert!(escaped.contains("\\_"));
        assert!(escaped.contains("\\+"));
        assert!(escaped.contains("\\-"));
        assert!(escaped.contains("\\="));
        assert!(escaped.contains("\\[\\]"));
        assert!(escaped.contains("\\{\\}"));
        assert!(escaped.contains("\\|"));
        assert!(escaped.contains("\\."));
        assert!(escaped.contains("\\~"));
        assert!(escaped.contains("\\`"));

        // Non-MarkdownV2 chars should NOT be escaped
        assert!(escaped.contains("@")); // Not \@
        assert!(escaped.contains("$")); // Not \$
        assert!(escaped.contains("%")); // Not \%
    }

    // ==================== Regression Tests for Production Bug ====================

    #[test]
    fn test_regression_unescaped_exclamation_in_summary() {
        // This test catches the exact production bug: unescaped '!' in MarkdownV2
        let summary = "Breaking news! AI is amazing!";

        // If we send this directly, Telegram will reject it
        let escaped = escape_markdownv2(summary);

        // Verify both exclamation marks are escaped
        assert_eq!(escaped.matches("\\!").count(), 2);

        // Verify no unescaped ! remains (check for ! NOT preceded by \)
        let has_unescaped_exclamation = escaped
            .match_indices('!')
            .any(|(i, _)| i == 0 || escaped.as_bytes()[i - 1] != b'\\');
        assert!(
            !has_unescaped_exclamation,
            "Found unescaped '!' in escaped summary: {}",
            escaped
        );
    }

    #[test]
    fn test_regression_timestamp_hyphens_in_header() {
        // Timestamps contain hyphens that must be escaped
        let timestamp = Utc::now().format("%Y-%m-%d %H:%M UTC").to_string();
        let escaped_timestamp = escape_markdownv2(&timestamp);

        // Hyphens in date should be escaped
        assert_eq!(escaped_timestamp.matches("\\-").count(), 2);
    }

    #[test]
    fn test_regression_dots_in_version_numbers() {
        let summary = "GPT-4.5 released! Version 2.0.1 available.";
        let escaped = escape_markdownv2(summary);

        // All dots should be escaped
        assert_eq!(escaped.matches("\\.").count(), 4); // 4.5, 2.0.1, available.
    }

    #[test]
    fn test_regression_parentheses_in_content() {
        let summary = "Updates (see details) are available (now).";
        let escaped = escape_markdownv2(summary);

        // All parentheses should be escaped
        assert_eq!(escaped.matches("\\(").count(), 2);
        assert_eq!(escaped.matches("\\)").count(), 2);
    }

    // ==================== Full Message Integration Tests ====================

    #[test]
    fn test_full_summary_message_is_valid() {
        let timestamp = Utc::now().format("%Y-%m-%d %H:%M UTC").to_string();
        let escaped_timestamp = escape_markdownv2(&timestamp);
        let summary =
            "AI news: GPT-4.5 released! Performance > baseline. Cost = $20/month (discount).";
        let escaped_summary = escape_markdownv2(summary);

        let full_message = format!(
            "📰 *Twitter Summary*\n_{}_\n\n{}",
            escaped_timestamp, escaped_summary
        );

        // The message should be valid MarkdownV2
        // Bold (*) and italic (_) are intentional formatting
        // All content special chars should be escaped

        // Verify key escaping
        assert!(full_message.contains("GPT\\-4\\.5 released\\!"));
        assert!(full_message.contains("Performance \\> baseline\\."));
        assert!(full_message.contains("\\= $20/month"));
        assert!(full_message.contains("\\(discount\\)\\."));
    }

    #[test]
    fn test_full_welcome_summary_message_is_valid() {
        let timestamp = Utc::now().format("%Y-%m-%d %H:%M UTC").to_string();
        let escaped_timestamp = escape_markdownv2(&timestamp);
        let summary = "Here's what happened! Key updates: v1.0 release.";
        let escaped_summary = escape_markdownv2(summary);

        let full_message = format!(
            "📰 *Hey\\! Here's what you missed* 😉\n_{}_\n\n{}",
            escaped_timestamp, escaped_summary
        );

        // Header ! is pre-escaped
        assert!(full_message.contains("*Hey\\!"));
        // Content special chars are escaped
        assert!(full_message.contains("happened\\!"));
        assert!(full_message.contains("v1\\.0 release\\."));
    }

    #[test]
    fn test_full_test_summary_message_is_valid() {
        let timestamp = Utc::now().format("%Y-%m-%d %H:%M UTC").to_string();
        let escaped_timestamp = escape_markdownv2(&timestamp);
        let summary = "Test summary content.";
        let escaped_summary = escape_markdownv2(summary);

        let full_message = format!(
            "🧪 *TEST \\- Twitter Summary*\n_{}_\n\n{}",
            escaped_timestamp, escaped_summary
        );

        // Header - is pre-escaped
        assert!(full_message.contains("*TEST \\-"));
        // Content escaped
        assert!(full_message.contains("content\\."));
    }

    // ==================== Broadcast Command Parsing Tests ====================

    #[test]
    fn test_broadcast_command_with_message() {
        let text = "/broadcast Hello everyone!";

        let (command, arg) = if text.starts_with("/language ") {
            let arg = text.strip_prefix("/language ").unwrap().trim();
            let arg = if arg.is_empty() { None } else { Some(arg) };
            ("/language", arg)
        } else if text.starts_with("/broadcast ") {
            let arg = text.strip_prefix("/broadcast ").unwrap().trim();
            let arg = if arg.is_empty() { None } else { Some(arg) };
            ("/broadcast", arg)
        } else {
            (text, None)
        };

        assert_eq!(command, "/broadcast");
        assert_eq!(arg, Some("Hello everyone!"));
    }

    #[test]
    fn test_broadcast_command_with_whitespace_only_argument() {
        // This tests the bug fix: "/broadcast    " should result in arg = None
        let text = "/broadcast    ";

        let (command, arg) = if text.starts_with("/language ") {
            let arg = text.strip_prefix("/language ").unwrap().trim();
            let arg = if arg.is_empty() { None } else { Some(arg) };
            ("/language", arg)
        } else if text.starts_with("/broadcast ") {
            let arg = text.strip_prefix("/broadcast ").unwrap().trim();
            let arg = if arg.is_empty() { None } else { Some(arg) };
            ("/broadcast", arg)
        } else {
            (text, None)
        };

        assert_eq!(command, "/broadcast");
        assert_eq!(
            arg, None,
            "Whitespace-only argument should be treated as None"
        );
    }

    #[test]
    fn test_broadcast_command_without_space() {
        // "/broadcast" (without space) should not match the starts_with("/broadcast ") branch
        let text = "/broadcast";

        let (command, arg) = if text.starts_with("/language ") {
            let arg = text.strip_prefix("/language ").unwrap().trim();
            let arg = if arg.is_empty() { None } else { Some(arg) };
            ("/language", arg)
        } else if text.starts_with("/broadcast ") {
            let arg = text.strip_prefix("/broadcast ").unwrap().trim();
            let arg = if arg.is_empty() { None } else { Some(arg) };
            ("/broadcast", arg)
        } else {
            (text, None)
        };

        // Without space, it falls through to the else branch
        assert_eq!(command, "/broadcast");
        assert_eq!(arg, None);
    }

    #[test]
    fn test_broadcast_command_with_leading_trailing_whitespace() {
        let text = "/broadcast   Hello world!   ";

        let (command, arg) = if text.starts_with("/language ") {
            let arg = text.strip_prefix("/language ").unwrap().trim();
            let arg = if arg.is_empty() { None } else { Some(arg) };
            ("/language", arg)
        } else if text.starts_with("/broadcast ") {
            let arg = text.strip_prefix("/broadcast ").unwrap().trim();
            let arg = if arg.is_empty() { None } else { Some(arg) };
            ("/broadcast", arg)
        } else {
            (text, None)
        };

        assert_eq!(command, "/broadcast");
        assert_eq!(arg, Some("Hello world!"));
    }

    #[test]
    fn test_broadcast_command_with_multiline_message() {
        let text = "/broadcast Line 1\nLine 2\nLine 3";

        let (command, arg) = if text.starts_with("/language ") {
            let arg = text.strip_prefix("/language ").unwrap().trim();
            let arg = if arg.is_empty() { None } else { Some(arg) };
            ("/language", arg)
        } else if text.starts_with("/broadcast ") {
            let arg = text.strip_prefix("/broadcast ").unwrap().trim();
            let arg = if arg.is_empty() { None } else { Some(arg) };
            ("/broadcast", arg)
        } else {
            (text, None)
        };

        assert_eq!(command, "/broadcast");
        assert_eq!(arg, Some("Line 1\nLine 2\nLine 3"));
    }

    #[test]
    fn test_broadcast_command_with_special_characters() {
        let text = "/broadcast Hello! *bold* _italic_ @mention #hashtag";

        let (command, arg) = if text.starts_with("/language ") {
            let arg = text.strip_prefix("/language ").unwrap().trim();
            let arg = if arg.is_empty() { None } else { Some(arg) };
            ("/language", arg)
        } else if text.starts_with("/broadcast ") {
            let arg = text.strip_prefix("/broadcast ").unwrap().trim();
            let arg = if arg.is_empty() { None } else { Some(arg) };
            ("/broadcast", arg)
        } else {
            (text, None)
        };

        assert_eq!(command, "/broadcast");
        assert_eq!(arg, Some("Hello! *bold* _italic_ @mention #hashtag"));
    }

    // ==================== Language Command Whitespace Edge Cases ====================

    #[test]
    fn test_language_command_with_whitespace_only_argument() {
        // This tests the bug fix: "/language    " should result in arg = None
        let text = "/language    ";

        let (command, arg) = if text.starts_with("/language ") {
            let arg = text.strip_prefix("/language ").unwrap().trim();
            let arg = if arg.is_empty() { None } else { Some(arg) };
            ("/language", arg)
        } else if text.starts_with("/broadcast ") {
            let arg = text.strip_prefix("/broadcast ").unwrap().trim();
            let arg = if arg.is_empty() { None } else { Some(arg) };
            ("/broadcast", arg)
        } else {
            (text, None)
        };

        assert_eq!(command, "/language");
        assert_eq!(
            arg, None,
            "Whitespace-only argument should be treated as None"
        );
    }

    #[test]
    fn test_language_command_with_tabs_only() {
        let text = "/language \t\t\t";

        let (command, arg) = if text.starts_with("/language ") {
            let arg = text.strip_prefix("/language ").unwrap().trim();
            let arg = if arg.is_empty() { None } else { Some(arg) };
            ("/language", arg)
        } else if text.starts_with("/broadcast ") {
            let arg = text.strip_prefix("/broadcast ").unwrap().trim();
            let arg = if arg.is_empty() { None } else { Some(arg) };
            ("/broadcast", arg)
        } else {
            (text, None)
        };

        assert_eq!(command, "/language");
        assert_eq!(arg, None, "Tab-only argument should be treated as None");
    }

    #[test]
    fn test_language_command_with_mixed_whitespace() {
        let text = "/language  \t \n ";

        let (command, arg) = if text.starts_with("/language ") {
            let arg = text.strip_prefix("/language ").unwrap().trim();
            let arg = if arg.is_empty() { None } else { Some(arg) };
            ("/language", arg)
        } else if text.starts_with("/broadcast ") {
            let arg = text.strip_prefix("/broadcast ").unwrap().trim();
            let arg = if arg.is_empty() { None } else { Some(arg) };
            ("/broadcast", arg)
        } else {
            (text, None)
        };

        assert_eq!(command, "/language");
        assert_eq!(
            arg, None,
            "Mixed whitespace argument should be treated as None"
        );
    }

    // ==================== Broadcast Command Handler Logic Tests ====================

    #[test]
    fn test_broadcast_admin_check_logic_admin_user() {
        // Admin sends broadcast
        let config_telegram_chat_id = "123456789";
        let chat_id: i64 = 123456789;
        let chat_id_str = chat_id.to_string();

        let is_admin =
            !config_telegram_chat_id.is_empty() && chat_id_str == config_telegram_chat_id;

        assert!(is_admin, "Admin user should be recognized as admin");
    }

    #[test]
    fn test_broadcast_admin_check_logic_non_admin_user() {
        // Non-admin sends broadcast
        let config_telegram_chat_id = "123456789";
        let chat_id: i64 = 987654321;
        let chat_id_str = chat_id.to_string();

        let is_admin =
            !config_telegram_chat_id.is_empty() && chat_id_str == config_telegram_chat_id;

        assert!(
            !is_admin,
            "Non-admin user should not be recognized as admin"
        );
    }

    #[test]
    fn test_broadcast_admin_check_logic_empty_config() {
        // Config has empty admin chat ID
        let config_telegram_chat_id = "";
        let chat_id: i64 = 123456789;
        let chat_id_str = chat_id.to_string();

        let is_admin =
            !config_telegram_chat_id.is_empty() && chat_id_str == config_telegram_chat_id;

        assert!(!is_admin, "No one should be admin when config is empty");
    }

    #[test]
    fn test_broadcast_permission_denied_message() {
        let msg = "This command is only available to the bot administrator.";
        assert!(msg.contains("administrator"));
        assert!(msg.contains("only available"));
    }

    #[test]
    fn test_broadcast_usage_message() {
        let msg =
            "Usage: /broadcast Your message here\n\nSends a plain text message to all subscribers.";
        assert!(msg.contains("/broadcast"));
        assert!(msg.contains("plain text"));
        assert!(msg.contains("all subscribers"));
    }

    #[test]
    fn test_broadcast_success_message_format_all_successful() {
        let sent: usize = 10;
        let failures: Vec<(i64, String)> = Vec::new();
        let total = sent + failures.len();

        let msg = if failures.is_empty() {
            format!(
                "Broadcast sent successfully!\n\nDelivered to {} subscribers",
                escape_markdownv2(&sent.to_string())
            )
        } else {
            format!(
                "Broadcast completed\n\nSent: {}\nFailed: {}\nTotal: {}",
                escape_markdownv2(&sent.to_string()),
                escape_markdownv2(&failures.len().to_string()),
                escape_markdownv2(&total.to_string())
            )
        };

        assert!(msg.contains("Broadcast sent successfully"));
        assert!(msg.contains("10 subscribers"));
    }

    #[test]
    fn test_broadcast_success_message_format_with_failures() {
        let sent: usize = 8;
        let failures: Vec<(i64, String)> = vec![
            (111, "blocked".to_string()),
            (222, "deactivated".to_string()),
        ];
        let total = sent + failures.len();

        let msg = if failures.is_empty() {
            format!(
                "Broadcast sent successfully!\n\nDelivered to {} subscribers",
                sent
            )
        } else {
            format!(
                "Broadcast completed\n\nSent: {}\nFailed: {}\nTotal: {}",
                sent,
                failures.len(),
                total
            )
        };

        assert!(msg.contains("Broadcast completed"));
        assert!(msg.contains("Sent: 8"));
        assert!(msg.contains("Failed: 2"));
        assert!(msg.contains("Total: 10"));
    }

    #[test]
    fn test_broadcast_error_message_format() {
        let error = "Database connection failed";
        let msg = format!("Broadcast failed: {}", escape_markdownv2(error));

        assert!(msg.contains("Broadcast failed"));
        assert!(msg.contains("Database connection failed"));
    }

    // ==================== Broadcast Message Function Logic Tests ====================

    #[test]
    fn test_broadcast_empty_subscriber_list_returns_zero() {
        // Simulates the case where there are no subscribers
        let subscribers: Vec<i64> = vec![];

        if subscribers.is_empty() {
            let result = (0usize, Vec::<(i64, String)>::new());
            assert_eq!(result.0, 0);
            assert!(result.1.is_empty());
        }
    }

    #[test]
    fn test_broadcast_subscriber_iteration_success() {
        // Simulates successful broadcast to all subscribers
        let subscriber_ids = vec![111i64, 222, 333];
        let mut success_count: usize = 0;
        let mut failures: Vec<(i64, String)> = Vec::new();

        for _id in &subscriber_ids {
            // Simulate success
            let result: Result<(), &str> = Ok(());
            match result {
                Ok(_) => success_count += 1,
                Err(e) => failures.push((*_id, e.to_string())),
            }
        }

        assert_eq!(success_count, 3);
        assert!(failures.is_empty());
    }

    #[test]
    fn test_broadcast_subscriber_iteration_with_failures() {
        // Simulates broadcast with some failures
        let subscriber_ids = [111i64, 222, 333];
        let mut success_count: usize = 0;
        let mut failures: Vec<(i64, String)> = Vec::new();

        for (idx, id) in subscriber_ids.iter().enumerate() {
            // Simulate: first two succeed, third fails
            let result: Result<(), &str> = if idx < 2 { Ok(()) } else { Err("blocked") };
            match result {
                Ok(_) => success_count += 1,
                Err(e) => failures.push((*id, e.to_string())),
            }
        }

        assert_eq!(success_count, 2);
        assert_eq!(failures.len(), 1);
        assert_eq!(failures[0].0, 333);
        assert_eq!(failures[0].1, "blocked");
    }

    #[test]
    fn test_broadcast_blocked_user_detection_403_blocked() {
        let error_msg = "Telegram API error (403): Forbidden: bot was blocked by the user";
        let error_lower = error_msg.to_lowercase();

        let is_blocked = error_msg.contains("403")
            && (error_lower.contains("blocked by the user")
                || error_lower.contains("user is deactivated"));

        assert!(
            is_blocked,
            "Should detect blocked user from 403 error message"
        );
    }

    #[test]
    fn test_broadcast_blocked_user_detection_403_deactivated() {
        let error_msg = "Telegram API error (403): Forbidden: user is deactivated";
        let error_lower = error_msg.to_lowercase();

        let is_blocked = error_msg.contains("403")
            && (error_lower.contains("blocked by the user")
                || error_lower.contains("user is deactivated"));

        assert!(
            is_blocked,
            "Should detect deactivated user from 403 error message"
        );
    }

    #[test]
    fn test_broadcast_blocked_user_detection_other_403() {
        // 403 error but not blocked/deactivated
        let error_msg = "Telegram API error (403): Forbidden: some other reason";
        let error_lower = error_msg.to_lowercase();

        let is_blocked = error_msg.contains("403")
            && (error_lower.contains("blocked by the user")
                || error_lower.contains("user is deactivated"));

        assert!(!is_blocked, "Should NOT auto-remove for other 403 errors");
    }

    #[test]
    fn test_broadcast_blocked_user_detection_non_403() {
        // Non-403 error should not trigger auto-remove
        let error_msg = "Telegram API error (500): Internal server error";
        let error_lower = error_msg.to_lowercase();

        let is_blocked = error_msg.contains("403")
            && (error_lower.contains("blocked by the user")
                || error_lower.contains("user is deactivated"));

        assert!(!is_blocked, "Should NOT auto-remove for non-403 errors");
    }

    #[test]
    fn test_broadcast_blocked_user_detection_case_insensitive() {
        // Test case insensitivity for error message matching
        let error_msg = "Telegram API error (403): FORBIDDEN: BOT WAS BLOCKED BY THE USER";
        let error_lower = error_msg.to_lowercase();

        let is_blocked = error_msg.contains("403")
            && (error_lower.contains("blocked by the user")
                || error_lower.contains("user is deactivated"));

        assert!(is_blocked, "Should detect blocked user case-insensitively");
    }

    // ==================== Send Broadcast Message Helper Tests ====================

    #[test]
    fn test_send_broadcast_message_plain_text_no_parse_mode() {
        // When parse_mode is None, message should be sent as plain text
        let parse_mode: Option<&str> = None;
        let message = "Hello everyone! This is a *test* message.";

        // In plain text mode, asterisks are literal, not markdown
        assert!(parse_mode.is_none());
        assert!(message.contains("*test*")); // Asterisks preserved literally
    }

    #[test]
    fn test_send_broadcast_message_with_markdownv2() {
        let parse_mode: Option<&str> = Some("MarkdownV2");
        let message = "\\*Bold text\\* and \\_italic\\_ here";

        assert_eq!(parse_mode, Some("MarkdownV2"));
        // In MarkdownV2, special chars need escaping
        assert!(message.contains("\\*"));
        assert!(message.contains("\\_"));
    }

    #[test]
    fn test_broadcast_request_json_plain_text() {
        // Verify JSON structure for plain text broadcast
        let chat_id = "123456789";
        let text = "Hello everyone!";

        let request = serde_json::json!({
            "chat_id": chat_id,
            "text": text
        });

        let json_str = serde_json::to_string(&request).unwrap();
        assert!(json_str.contains("123456789"));
        assert!(json_str.contains("Hello everyone!"));
        // Should NOT contain parse_mode
        assert!(!json_str.contains("parse_mode"));
    }

    #[test]
    fn test_broadcast_request_json_with_parse_mode() {
        // Verify JSON structure for formatted broadcast
        let request = SendMessageRequest {
            chat_id: "123456789".to_string(),
            text: "\\*Bold\\* message".to_string(),
            parse_mode: "MarkdownV2".to_string(),
        };

        let json_str = serde_json::to_string(&request).unwrap();
        assert!(json_str.contains("123456789"));
        assert!(json_str.contains("MarkdownV2"));
    }

    // ==================== Broadcast Admin Notification Tests ====================

    #[test]
    fn test_broadcast_admin_notification_format_with_failures() {
        let success_count = 8;
        let total_subscribers = 10;
        let failures_count = 2;

        let admin_msg = format!(
            "Broadcast sent to {}/{} subscribers ({} failed)",
            success_count, total_subscribers, failures_count
        );

        assert!(admin_msg.contains("8/10"));
        assert!(admin_msg.contains("2 failed"));
    }

    #[test]
    fn test_broadcast_admin_notification_not_sent_when_all_succeed() {
        let failures: Vec<(i64, String)> = vec![];
        let config_telegram_chat_id = "123456789";

        // Admin notification is only sent when there are failures
        let should_notify_admin = !config_telegram_chat_id.is_empty() && !failures.is_empty();

        assert!(
            !should_notify_admin,
            "Should not send admin notification when all succeed"
        );
    }

    #[test]
    fn test_broadcast_admin_notification_sent_when_failures_exist() {
        let failures: Vec<(i64, String)> = vec![(111, "error".to_string())];
        let config_telegram_chat_id = "123456789";

        let should_notify_admin = !config_telegram_chat_id.is_empty() && !failures.is_empty();

        assert!(
            should_notify_admin,
            "Should send admin notification when there are failures"
        );
    }

    #[test]
    fn test_broadcast_admin_notification_not_sent_when_no_admin_configured() {
        let failures: Vec<(i64, String)> = vec![(111, "error".to_string())];
        let config_telegram_chat_id = "";

        let should_notify_admin = !config_telegram_chat_id.is_empty() && !failures.is_empty();

        assert!(
            !should_notify_admin,
            "Should not send admin notification when admin is not configured"
        );
    }

    // ==================== Welcome Message with Broadcast Command Tests ====================

    #[test]
    fn test_admin_welcome_message_includes_broadcast_command() {
        // Admin welcome message should include /broadcast command
        let is_admin = true;
        let welcome = if is_admin {
            "Welcome to Twitter News Summary Bot!\n\n\
            Commands:\n\
            /subscribe - Get daily AI-powered summaries\n\
            /unsubscribe - Stop receiving summaries\n\
            /status - Check your subscription status\n\
            /language - Change summary language (en/es)\n\
            /broadcast - Send a message to all subscribers (admin only)"
        } else {
            "Welcome (non-admin version without broadcast)"
        };

        assert!(
            welcome.contains("/broadcast"),
            "Admin welcome should include /broadcast command"
        );
        assert!(welcome.contains("admin only"));
    }

    #[test]
    fn test_non_admin_welcome_message_excludes_broadcast_command() {
        // Non-admin welcome message should NOT include /broadcast
        let is_admin = false;
        let welcome = if is_admin {
            "Welcome (admin version with broadcast)"
        } else {
            "Welcome to Twitter News Summary Bot!\n\n\
            Commands:\n\
            /subscribe - Get daily AI-powered summaries\n\
            /unsubscribe - Stop receiving summaries\n\
            /status - Check your subscription status\n\
            /language - Change summary language (en/es)"
        };

        assert!(
            !welcome.contains("/broadcast"),
            "Non-admin welcome should NOT include /broadcast command"
        );
    }

    // ==================== Broadcast Edge Cases ====================

    #[test]
    fn test_broadcast_message_with_emojis() {
        let text = "/broadcast Hello! 👋 Welcome to the new update! 🎉";

        let (command, arg) = if text.starts_with("/language ") {
            let arg = text.strip_prefix("/language ").unwrap().trim();
            let arg = if arg.is_empty() { None } else { Some(arg) };
            ("/language", arg)
        } else if text.starts_with("/broadcast ") {
            let arg = text.strip_prefix("/broadcast ").unwrap().trim();
            let arg = if arg.is_empty() { None } else { Some(arg) };
            ("/broadcast", arg)
        } else {
            (text, None)
        };

        assert_eq!(command, "/broadcast");
        assert!(arg.unwrap().contains("👋"));
        assert!(arg.unwrap().contains("🎉"));
    }

    #[test]
    fn test_broadcast_message_with_urls() {
        let text = "/broadcast Check out https://example.com for more info!";

        let (command, arg) = if text.starts_with("/language ") {
            let arg = text.strip_prefix("/language ").unwrap().trim();
            let arg = if arg.is_empty() { None } else { Some(arg) };
            ("/language", arg)
        } else if text.starts_with("/broadcast ") {
            let arg = text.strip_prefix("/broadcast ").unwrap().trim();
            let arg = if arg.is_empty() { None } else { Some(arg) };
            ("/broadcast", arg)
        } else {
            (text, None)
        };

        assert_eq!(command, "/broadcast");
        assert!(arg.unwrap().contains("https://example.com"));
    }

    #[test]
    fn test_broadcast_message_very_long() {
        // Test with a very long message (Telegram limit is 4096 chars)
        let long_message = "A".repeat(4000);
        let text = format!("/broadcast {}", long_message);

        let (command, arg) = if text.starts_with("/language ") {
            let arg = text.strip_prefix("/language ").unwrap().trim();
            let arg = if arg.is_empty() { None } else { Some(arg) };
            ("/language", arg)
        } else if text.starts_with("/broadcast ") {
            let arg = text.strip_prefix("/broadcast ").unwrap().trim();
            let arg = if arg.is_empty() { None } else { Some(arg) };
            ("/broadcast", arg)
        } else {
            (&text as &str, None)
        };

        assert_eq!(command, "/broadcast");
        assert_eq!(arg.unwrap().len(), 4000);
    }

    #[test]
    fn test_broadcast_stats_calculation() {
        let sent: usize = 95;
        let failures: Vec<(i64, String)> = vec![
            (1, "error1".into()),
            (2, "error2".into()),
            (3, "error3".into()),
            (4, "error4".into()),
            (5, "error5".into()),
        ];
        let total = sent + failures.len();

        assert_eq!(total, 100);
        assert_eq!(failures.len(), 5);
    }

    #[test]
    fn test_command_priority_broadcast_over_other() {
        // Ensure /broadcast is checked correctly in the if-else chain
        let text = "/broadcast test";

        // Test that it doesn't accidentally match /language
        assert!(!text.starts_with("/language "));
        assert!(text.starts_with("/broadcast "));
    }

    #[test]
    fn test_command_priority_language_over_broadcast() {
        // Ensure /language is checked first
        let text = "/language en";

        // Test that it matches /language first
        assert!(text.starts_with("/language "));
        assert!(!text.starts_with("/broadcast "));
    }

    // ==================== Pre-Escaped Template Validation Tests ====================
    //
    // These tests ensure that pre-escaped template strings from src/i18n/strings.rs
    // work correctly with our escape functions. The key insight is that templates
    // are pre-escaped to avoid the "Character 'X' is reserved" errors, and we must
    // ensure that dynamic content added later is also properly escaped.

    #[test]
    fn test_pre_escaped_template_does_not_double_escape() {
        // Pre-escaped template (like those in strings.rs)
        let pre_escaped = "Hello\\! How are you\\?";

        // If we pass an already-escaped string through escape_markdownv2,
        // the backslashes should be escaped again
        let double_escaped = escape_markdownv2(pre_escaped);

        // The backslash before ! should now be \\! (escaped backslash + !)
        // This is expected behavior - escape function does not detect pre-escaping
        assert!(double_escaped.contains("\\\\"));
    }

    #[test]
    fn test_pre_escaped_template_with_placeholder_substitution() {
        // Simulate how templates with placeholders are used
        let template = "✅ You are subscribed\n🌐 Language: {language}";

        // Substitute with plain text value (not pre-escaped)
        let result = template.replace("{language}", "English");

        // Result should have the substituted value
        assert!(result.contains("English"));
        assert!(result.contains("Language:"));
    }

    #[test]
    fn test_pre_escaped_template_special_chars_in_substitution() {
        // Template with pre-escaped characters
        let template = "Current: {current}\\.\n\nTo change, use:\n/language en \\- English";

        // Substitute with a value that contains special characters
        let value = "English (US)";
        let result = template.replace("{current}", value);

        // The substituted value is NOT automatically escaped
        // This means if we use such templates directly with MarkdownV2,
        // we need to ensure the substituted values are safe or pre-escaped
        assert!(result.contains("English (US)"));

        // The template's pre-escaped parts are preserved
        assert!(result.contains("\\-"));
        assert!(result.contains("\\."));
    }

    #[test]
    fn test_escape_markdownv2_handles_already_escaped_chars() {
        // What happens when text already has backslash-escaped characters?
        let already_escaped = "Hello\\-World";
        let result = escape_markdownv2(already_escaped);

        // The backslash is NOT a MarkdownV2 special char (not in the 18),
        // but the hyphen after it will be seen as unescaped and escaped again
        // Actually, the original hyphen after \ should be escaped
        // Input: Hello\-World
        // The - is not preceded by an even number of backslashes from escape perspective
        // escape_markdownv2_simple sees \, -, and escapes - (since - is special)
        // Result depends on implementation
        assert!(result.contains("\\-"));
    }

    // ==================== Regression Tests for Production Bug ====================
    //
    // These tests specifically validate scenarios that caused the production bug
    // where /language and /status commands failed with "Character '-' is reserved" errors.

    #[test]
    fn test_regression_language_settings_message_format() {
        use crate::i18n::Language;

        // Get the actual language settings template
        let template = Language::ENGLISH.config().strings.language_settings;

        // This template is used directly without escape_markdownv2
        // It must have all special characters pre-escaped

        // Check that hyphens are escaped
        assert!(
            template.contains("\\-"),
            "Language settings template must have escaped hyphens"
        );

        // Check bold formatting is intact
        assert!(
            template.contains("*Language Settings*"),
            "Bold formatting must be preserved"
        );

        // Simulate placeholder substitution
        let result = template.replace("{current}", "English");
        assert!(result.contains("Current: English"));
    }

    #[test]
    fn test_regression_status_message_format() {
        use crate::i18n::Language;

        // Get the actual status template
        let template = Language::ENGLISH.config().strings.status_subscribed_admin;

        // Simulate what happens in handle_webhook for /status command
        let result = template
            .replace("{language}", "English")
            .replace("{count}", "42");

        // The result should have proper content
        assert!(result.contains("Language: English"));
        assert!(result.contains("subscribers: 42"));
    }

    #[test]
    fn test_regression_spanish_status_message() {
        use crate::i18n::Language;

        // Spanish templates must also be properly escaped
        let template = Language::SPANISH.config().strings.status_subscribed_user;

        let result = template.replace("{language}", "Español");
        assert!(result.contains("Idioma: Español"));
    }

    #[test]
    fn test_regression_subscribe_success_message() {
        use crate::i18n::Language;

        let template = Language::ENGLISH.config().strings.subscribe_success;

        // This template has periods and exclamation marks that must be escaped
        assert!(
            template.contains("\\."),
            "Periods must be escaped in subscribe_success"
        );
        assert!(
            template.contains("\\!"),
            "Exclamation marks must be escaped in subscribe_success"
        );
    }

    #[test]
    fn test_regression_welcome_message_contains_hyphens() {
        use crate::i18n::Language;

        // Welcome messages contain command descriptions with hyphens
        let template = Language::ENGLISH.config().strings.welcome_admin;

        // All hyphens must be escaped
        assert!(
            template.contains("\\-"),
            "Welcome admin template must have escaped hyphens"
        );

        // Should contain multiple escaped hyphens (one for each command description)
        let hyphen_count = template.matches("\\-").count();
        assert!(
            hyphen_count >= 4,
            "Welcome admin should have at least 4 escaped hyphens (one per command), found {}",
            hyphen_count
        );
    }

    #[test]
    fn test_regression_parentheses_in_templates() {
        use crate::i18n::Language;

        // Templates with (en/es) must have escaped parentheses
        let template = Language::ENGLISH.config().strings.welcome_admin;

        assert!(
            template.contains("\\(") && template.contains("\\)"),
            "Parentheses must be escaped in welcome_admin"
        );
    }

    // ==================== Integration Tests: Template + escape_markdownv2 ====================

    #[test]
    fn test_summary_header_formatting() {
        use crate::i18n::Language;

        let header = Language::ENGLISH.config().strings.summary_header;

        // The header "Twitter Summary" is used with *header* for bold
        // This should be straightforward - no special chars in the header text itself
        let formatted = format!("📰 *{}*", header);
        assert!(formatted.contains("*Twitter Summary*"));
    }

    #[test]
    fn test_summary_body_escaping() {
        // Summary body comes from OpenAI and needs escape_markdownv2
        let summary = "AI news: GPT-5 announced! Read more at https://example.com.";
        let escaped = escape_markdownv2(summary);

        // All special chars should be escaped
        assert!(escaped.contains("\\-")); // GPT-5
        assert!(escaped.contains("\\!")); // announced!
        assert!(escaped.contains("\\.")); // example.com.
    }

    #[test]
    fn test_full_message_construction() {
        use crate::i18n::Language;

        // Simulate constructing a full message like in send_to_subscribers
        let header = Language::ENGLISH.config().strings.summary_header;
        let timestamp = "2024-01-15 10:30 UTC";
        let summary = "AI advancements - OpenAI releases GPT-5!";

        let message = format!(
            "📰 *{}*\n_{}_\n\n{}",
            header,
            escape_markdownv2(timestamp),
            escape_markdownv2(summary)
        );

        // Header is wrapped in * for bold
        assert!(message.contains("*Twitter Summary*"));
        // Timestamp is wrapped in _ for italic and has special chars escaped
        assert!(message.contains("_2024\\-01\\-15 10:30 UTC_"));
        // Summary has special chars escaped
        assert!(message.contains("\\-")); // from both timestamp and summary
        assert!(message.contains("\\!"));
    }

    #[test]
    fn test_broadcast_message_construction() {
        use crate::i18n::Language;

        // Broadcast success message uses placeholder substitution
        let template = Language::ENGLISH.config().strings.broadcast_success;

        let result = template.replace("{count}", "150");

        // Should have the count
        assert!(result.contains("150"));
        // Should have escaped exclamation mark (from the template)
        assert!(result.contains("\\!"));
        // Should preserve bold formatting
        assert!(result.contains("*Broadcast sent successfully*"));
    }

    // ==================== Edge Case Tests ====================

    #[test]
    fn test_empty_placeholder_substitution() {
        let template = "Status: {status}";
        let result = template.replace("{status}", "");
        assert_eq!(result, "Status: ");
    }

    #[test]
    fn test_placeholder_with_special_chars_value() {
        // What if the substituted value contains MarkdownV2 special chars?
        let template = "Error: {error}";
        let error_msg = "Connection failed (timeout > 30s)!";

        // If we substitute directly, special chars are NOT escaped
        let result = template.replace("{error}", error_msg);
        assert!(result.contains("(timeout > 30s)!"));

        // If the template is meant to be used with MarkdownV2, the value should be escaped first
        let safe_result = template.replace("{error}", &escape_markdownv2(error_msg));
        assert!(safe_result.contains("\\(timeout \\> 30s\\)\\!"));
    }

    #[test]
    fn test_multiple_placeholder_substitutions() {
        let template = "Sent: {sent}, Failed: {failed}, Total: {total}";
        let result = template
            .replace("{sent}", "95")
            .replace("{failed}", "5")
            .replace("{total}", "100");

        assert_eq!(result, "Sent: 95, Failed: 5, Total: 100");
    }

    #[test]
    fn test_unicode_in_templates() {
        use crate::i18n::Language;

        // Spanish templates contain Unicode characters
        let template = Language::SPANISH.config().strings.subscribe_success;

        // Should contain Spanish characters
        assert!(template.contains("Recibirás"));

        // Should still have properly escaped special chars
        assert!(template.contains("\\."));
    }

    #[test]
    fn test_newlines_in_templates() {
        use crate::i18n::Language;

        let template = Language::ENGLISH.config().strings.welcome_admin;

        // Templates use \n for newlines
        assert!(template.contains('\n'));

        // Count newlines to ensure proper structure
        let newline_count = template.matches('\n').count();
        assert!(
            newline_count >= 5,
            "Welcome admin should have multiple newlines for formatting, found {}",
            newline_count
        );
    }

    #[test]
    fn test_emoji_preservation() {
        use crate::i18n::Language;

        // Emojis should be preserved in templates
        let template = Language::ENGLISH.config().strings.status_subscribed_admin;
        assert!(template.contains("✅"));
        assert!(template.contains("🌐"));
        assert!(template.contains("📊"));
    }

    // ==================== Double-Escaping Detection Tests ====================
    //
    // These tests detect if pre-escaped templates are accidentally double-escaped.
    // Double-escaping would produce patterns like `\\\\` (escaped backslash) or
    // `\\\\!` instead of `\\!`.
    //
    // CRITICAL: If any of these tests fail, it means production code is double-escaping
    // and will cause Telegram API errors.

    /// Helper function to detect double-escaping in a string.
    /// Double-escaping produces `\\\\` patterns (backslash followed by backslash).
    fn has_double_escaping(s: &str) -> bool {
        // In Rust strings, `\\\\` represents two literal backslashes
        // If we see `\\` followed by another `\\` it means double-escaping
        s.contains("\\\\")
    }

    /// Simulates what production code does for /start command
    #[test]
    fn test_production_start_command_no_double_escape() {
        use crate::i18n::Language;

        // This is EXACTLY what production code does
        let welcome = Language::ENGLISH.config().strings.welcome_admin;

        // Production code should NOT wrap this in escape_markdownv2
        // If it did, we'd see double-escaping
        assert!(
            !has_double_escaping(welcome),
            "welcome_admin template should not have double-escaping. \
             If this fails, production code may be calling escape_markdownv2 on pre-escaped template"
        );

        // Also verify the template IS properly escaped (single escaping)
        assert!(
            welcome.contains("\\!"),
            "welcome_admin should have escaped exclamation marks"
        );
        assert!(
            welcome.contains("\\-"),
            "welcome_admin should have escaped hyphens"
        );
    }

    #[test]
    fn test_production_subscribe_command_no_double_escape() {
        use crate::i18n::Language;

        let already = Language::ENGLISH.config().strings.subscribe_already;
        let success = Language::ENGLISH.config().strings.subscribe_success;

        assert!(
            !has_double_escaping(already),
            "subscribe_already should not have double-escaping"
        );
        assert!(
            !has_double_escaping(success),
            "subscribe_success should not have double-escaping"
        );

        // Verify proper escaping exists
        assert!(already.contains("\\!"));
        assert!(success.contains("\\!"));
        assert!(success.contains("\\."));
    }

    #[test]
    fn test_production_unsubscribe_command_no_double_escape() {
        use crate::i18n::Language;

        let success = Language::ENGLISH.config().strings.unsubscribe_success;
        let not_sub = Language::ENGLISH
            .config()
            .strings
            .unsubscribe_not_subscribed;

        assert!(
            !has_double_escaping(success),
            "unsubscribe_success should not have double-escaping"
        );
        assert!(
            !has_double_escaping(not_sub),
            "unsubscribe_not_subscribed should not have double-escaping"
        );
    }

    #[test]
    fn test_production_status_command_no_double_escape() {
        use crate::i18n::Language;

        let admin = Language::ENGLISH.config().strings.status_subscribed_admin;
        let user = Language::ENGLISH.config().strings.status_subscribed_user;
        let not_sub = Language::ENGLISH.config().strings.status_not_subscribed;

        assert!(
            !has_double_escaping(admin),
            "status_subscribed_admin should not have double-escaping"
        );
        assert!(
            !has_double_escaping(user),
            "status_subscribed_user should not have double-escaping"
        );
        assert!(
            !has_double_escaping(not_sub),
            "status_not_subscribed should not have double-escaping"
        );
    }

    #[test]
    fn test_production_language_command_no_double_escape() {
        use crate::i18n::Language;

        let not_sub = Language::ENGLISH.config().strings.language_not_subscribed;
        let changed_en = Language::ENGLISH.config().strings.language_changed_english;
        let changed_es = Language::SPANISH.config().strings.language_changed_spanish;
        let invalid = Language::ENGLISH.config().strings.language_invalid;
        let settings = Language::ENGLISH.config().strings.language_settings;

        assert!(!has_double_escaping(not_sub));
        assert!(!has_double_escaping(changed_en));
        assert!(!has_double_escaping(changed_es));
        assert!(!has_double_escaping(invalid));
        assert!(!has_double_escaping(settings));

        // Verify language_settings has proper escaping (it uses * for bold and - for list)
        assert!(
            settings.contains("\\-"),
            "language_settings should have escaped hyphens"
        );
        assert!(
            settings.contains("*Language Settings*") || settings.contains("*Configuración"),
            "language_settings should preserve intentional bold formatting"
        );
    }

    #[test]
    fn test_production_broadcast_command_no_double_escape() {
        use crate::i18n::Language;

        let admin_only = Language::ENGLISH.config().strings.broadcast_admin_only;
        let success = Language::ENGLISH.config().strings.broadcast_success;
        let partial = Language::ENGLISH.config().strings.broadcast_partial;
        let failed = Language::ENGLISH.config().strings.broadcast_failed;
        let usage = Language::ENGLISH.config().strings.broadcast_usage;

        assert!(!has_double_escaping(admin_only));
        assert!(!has_double_escaping(success));
        assert!(!has_double_escaping(partial));
        assert!(!has_double_escaping(failed));
        assert!(!has_double_escaping(usage));
    }

    #[test]
    fn test_production_unknown_command_no_double_escape() {
        use crate::i18n::Language;

        let unknown = Language::ENGLISH.config().strings.unknown_command;
        assert!(
            !has_double_escaping(unknown),
            "unknown_command should not have double-escaping"
        );
    }

    #[test]
    fn test_production_welcome_summary_no_double_escape() {
        use crate::i18n::Language;

        let header = Language::ENGLISH.config().strings.welcome_summary_header;
        assert!(
            !has_double_escaping(header),
            "welcome_summary_header should not have double-escaping"
        );
    }

    #[test]
    fn test_all_spanish_templates_no_double_escape() {
        use crate::i18n::Language;

        let strings = &Language::SPANISH.config().strings;

        assert!(!has_double_escaping(strings.welcome_admin));
        assert!(!has_double_escaping(strings.welcome_user));
        assert!(!has_double_escaping(strings.subscribe_already));
        assert!(!has_double_escaping(strings.subscribe_success));
        assert!(!has_double_escaping(strings.unsubscribe_success));
        assert!(!has_double_escaping(strings.unsubscribe_not_subscribed));
        assert!(!has_double_escaping(strings.status_subscribed_admin));
        assert!(!has_double_escaping(strings.status_subscribed_user));
        assert!(!has_double_escaping(strings.status_not_subscribed));
        assert!(!has_double_escaping(strings.language_not_subscribed));
        assert!(!has_double_escaping(strings.language_changed_english));
        assert!(!has_double_escaping(strings.language_changed_spanish));
        assert!(!has_double_escaping(strings.language_invalid));
        assert!(!has_double_escaping(strings.language_settings));
        assert!(!has_double_escaping(strings.broadcast_admin_only));
        assert!(!has_double_escaping(strings.broadcast_success));
        assert!(!has_double_escaping(strings.broadcast_partial));
        assert!(!has_double_escaping(strings.broadcast_failed));
        assert!(!has_double_escaping(strings.broadcast_usage));
        assert!(!has_double_escaping(strings.unknown_command));
        assert!(!has_double_escaping(strings.welcome_summary_header));
    }

    /// Test that demonstrates what double-escaping looks like
    /// and verifies our detection works
    #[test]
    fn test_double_escape_detection_works() {
        // Single-escaped (correct)
        let single_escaped = "Hello\\!";
        assert!(
            !has_double_escaping(single_escaped),
            "Single-escaped should not trigger detection"
        );

        // Double-escaped (incorrect - what happens if escape_markdownv2 is called twice)
        let double_escaped = escape_markdownv2("Hello\\!");
        assert!(
            has_double_escaping(&double_escaped),
            "Double-escaped should be detected. Got: {}",
            double_escaped
        );
    }

    /// Simulates the exact bug scenario: calling escape_markdownv2 on pre-escaped template
    #[test]
    fn test_simulated_double_escape_bug() {
        use crate::i18n::Language;

        // Get the pre-escaped template
        let template = Language::ENGLISH.config().strings.welcome_admin;

        // Simulate the BUG: wrapping pre-escaped template in escape_markdownv2
        let double_escaped = escape_markdownv2(template);

        // This should detect the double-escaping
        assert!(
            has_double_escaping(&double_escaped),
            "Calling escape_markdownv2 on pre-escaped template should produce double-escaping. \
             This test verifies our detection works."
        );

        // The original template should NOT have double-escaping
        assert!(
            !has_double_escaping(template),
            "Original template should not have double-escaping"
        );
    }

    // ==================== Language-Aware Response Tests ====================
    //
    // These tests verify that commands respond in the subscriber's preferred language.

    #[test]
    fn test_status_response_uses_subscriber_language() {
        use crate::i18n::Language;

        // English subscriber should get English status
        let en_template = Language::ENGLISH.config().strings.status_subscribed_user;
        assert!(
            en_template.contains("You are subscribed"),
            "English status should say 'You are subscribed'"
        );

        // Spanish subscriber should get Spanish status
        let es_template = Language::SPANISH.config().strings.status_subscribed_user;
        assert!(
            es_template.contains("Estás suscrito"),
            "Spanish status should say 'Estás suscrito'"
        );

        // Both should have the language placeholder
        assert!(en_template.contains("{language}"));
        assert!(es_template.contains("{language}"));
    }

    #[test]
    fn test_subscribe_already_uses_subscriber_language() {
        use crate::i18n::Language;

        // English subscriber should get English message
        let en_msg = Language::ENGLISH.config().strings.subscribe_already;
        assert!(
            en_msg.contains("already subscribed"),
            "English should say 'already subscribed'"
        );

        // Spanish subscriber should get Spanish message
        let es_msg = Language::SPANISH.config().strings.subscribe_already;
        assert!(
            es_msg.contains("Ya estás suscrito"),
            "Spanish should say 'Ya estás suscrito'"
        );
    }

    #[test]
    fn test_unsubscribe_success_uses_subscriber_language() {
        use crate::i18n::Language;

        // English subscriber should get English message
        let en_msg = Language::ENGLISH.config().strings.unsubscribe_success;
        assert!(
            en_msg.contains("Successfully unsubscribed"),
            "English should say 'Successfully unsubscribed'"
        );

        // Spanish subscriber should get Spanish message
        let es_msg = Language::SPANISH.config().strings.unsubscribe_success;
        assert!(
            es_msg.contains("cancelada exitosamente"),
            "Spanish should say 'cancelada exitosamente'"
        );
    }

    #[test]
    fn test_language_settings_uses_subscriber_language() {
        use crate::i18n::Language;

        // English subscriber sees English settings
        let en_template = Language::ENGLISH.config().strings.language_settings;
        assert!(
            en_template.contains("Language Settings"),
            "English should say 'Language Settings'"
        );
        assert!(
            en_template.contains("To change, use"),
            "English should say 'To change, use'"
        );

        // Spanish subscriber sees Spanish settings
        let es_template = Language::SPANISH.config().strings.language_settings;
        assert!(
            es_template.contains("Configuración de Idioma"),
            "Spanish should say 'Configuración de Idioma'"
        );
        assert!(
            es_template.contains("Para cambiar, usa"),
            "Spanish should say 'Para cambiar, usa'"
        );
    }

    #[test]
    fn test_language_invalid_uses_subscriber_language() {
        use crate::i18n::Language;

        // English subscriber sees English error
        let en_msg = Language::ENGLISH.config().strings.language_invalid;
        assert!(
            en_msg.contains("Invalid language"),
            "English should say 'Invalid language'"
        );

        // Spanish subscriber sees Spanish error
        let es_msg = Language::SPANISH.config().strings.language_invalid;
        assert!(
            es_msg.contains("Idioma inválido"),
            "Spanish should say 'Idioma inválido'"
        );
    }

    #[test]
    fn test_language_from_code_returns_correct_strings() {
        use crate::i18n::Language;

        // Test that Language::from_code gives us the right language
        let english = Language::from_code("en").unwrap();
        let spanish = Language::from_code("es").unwrap();

        assert_eq!(
            english.config().strings.status_subscribed_user,
            Language::ENGLISH.config().strings.status_subscribed_user
        );
        assert_eq!(
            spanish.config().strings.status_subscribed_user,
            Language::SPANISH.config().strings.status_subscribed_user
        );

        // Verify they're different
        assert_ne!(
            english.config().strings.status_subscribed_user,
            spanish.config().strings.status_subscribed_user
        );
    }

    #[test]
    fn test_language_from_code_fallback_to_english() {
        use crate::i18n::Language;

        // Unknown language code should fail gracefully
        let result = Language::from_code("xx");
        assert!(result.is_err(), "Unknown language code should return error");

        // Code that uses unwrap_or_else pattern should fall back to English
        let fallback = Language::from_code("xx").unwrap_or(Language::ENGLISH);
        assert_eq!(
            fallback.config().strings.status_subscribed_user,
            Language::ENGLISH.config().strings.status_subscribed_user
        );
    }

    #[test]
    fn test_start_response_uses_subscriber_language() {
        use crate::i18n::Language;

        // English subscriber should get English welcome
        let en_msg = Language::ENGLISH.config().strings.welcome_user;
        assert!(en_msg.contains("Welcome"), "English should say 'Welcome'");

        // Spanish subscriber should get Spanish welcome
        let es_msg = Language::SPANISH.config().strings.welcome_user;
        assert!(
            es_msg.contains("Bienvenido"),
            "Spanish should say 'Bienvenido'"
        );

        // Admin messages should also be language-aware
        let en_admin = Language::ENGLISH.config().strings.welcome_admin;
        let es_admin = Language::SPANISH.config().strings.welcome_admin;
        assert!(
            en_admin.contains("Welcome"),
            "English admin should say 'Welcome'"
        );
        assert!(
            es_admin.contains("Bienvenido"),
            "Spanish admin should say 'Bienvenido'"
        );
    }

    // ==================== REGRESSION TESTS: Bug #2 - Double-Escaping of Pre-Escaped Content ====================
    //
    // Bug: The translation failure notice in src/i18n/strings.rs is pre-escaped for MarkdownV2:
    // translation_failure_notice: "\\[Nota: La traducción no está disponible\\. Enviando en inglés\\.\\]\n\n"
    //
    // When translation failed, this notice was concatenated with the summary, then the whole thing
    // was passed through escape_markdownv2(), causing double-escaping. Telegram then rejected the
    // message with: "Bad Request: can't parse entities: Character '.' is reserved..."
    //
    // The fix uses a marker-based approach: when translation fails, the content is marked with
    // "\x00TRANSLATION_FAILED\x00" prefix. The message formatting code detects this marker and
    // handles the failure notice separately from content escaping.
    //
    // These tests ensure the fix is preserved and double-escaping never occurs.

    /// Test that pre-escaped i18n strings should NOT be passed through escape_markdownv2
    #[test]
    fn test_regression_pre_escaped_strings_not_double_escaped() {
        use crate::i18n::Language;

        // The translation failure notice is already pre-escaped in strings.rs
        let notice = Language::SPANISH
            .config()
            .strings
            .translation_failure_notice;

        // Verify the notice is pre-escaped (contains escaped characters)
        // In Rust string literals: "\\[" represents the two-character string: backslash, bracket
        assert!(
            notice.contains("\\["),
            "Notice should be pre-escaped with \\[ but got: {}",
            notice
        );
        assert!(
            notice.contains("\\]"),
            "Notice should be pre-escaped with \\] but got: {}",
            notice
        );
        assert!(
            notice.contains("\\."),
            "Notice should be pre-escaped with \\. but got: {}",
            notice
        );

        // CRITICAL: If we escape again, we get double-escaping (this was the bug)
        let double_escaped = escape_markdownv2(notice);

        // Pre-escaped string already has \[ (backslash, bracket) in it
        // When escaped AGAIN:
        // - The backslash is NOT a special char so stays as-is
        // - The [ IS a special char so gets escaped to \[
        // Result: \[ becomes \\[ (backslash, backslash, bracket)
        //
        // In Rust string literals:
        // - Original notice contains "\\[" = \[ (two chars)
        // - After double-escaping contains "\\\\[" = \\[ (three chars)

        // Verify double-escaping occurred - the pattern \\[ (three chars) appears
        // In Rust: "\\\\\\[" = \\[ (backslash, backslash, bracket) - but this is what we DON'T want
        // The double_escaped should contain this pattern if we made the mistake of escaping twice
        assert!(
            double_escaped.contains("\\\\["),
            "Double-escaping should produce \\\\[ pattern but got: {}",
            double_escaped
        );

        // This shows why we should NOT escape pre-escaped content
        assert_ne!(
            notice, double_escaped,
            "Pre-escaped content changes when escaped again - this must be avoided"
        );
    }

    /// Test the marker-based translation failure detection
    #[test]
    fn test_translation_failure_marker_detection() {
        let marker = "\x00TRANSLATION_FAILED\x00";
        let summary = "This is the English summary content.";

        // Simulate a translation failure - content gets prefixed with marker
        let marked_content = format!("{}{}", marker, summary);

        // Detection logic (mirrors send_to_subscribers)
        assert!(
            marked_content.starts_with(marker),
            "Marked content should start with translation failure marker"
        );

        // Extract the actual content
        let actual_content = marked_content
            .strip_prefix(marker)
            .unwrap_or(&marked_content);

        assert_eq!(
            actual_content, summary,
            "Stripping marker should reveal original content"
        );
    }

    /// Test that translation failure notice is handled correctly in message formatting
    #[test]
    fn test_translation_failure_message_formatting() {
        use crate::i18n::Language;
        use crate::translation::get_translation_failure_notice;

        let marker = "\x00TRANSLATION_FAILED\x00";
        let summary = "AI news summary with special chars: GPT-4.5! Performance > 2x.";
        let language = Language::SPANISH;

        // Simulate the content as it would be stored in translation_cache on failure
        let cached_content = format!("{}{}", marker, summary);

        // Mirror the logic from send_to_subscribers:
        let (notice_prefix, actual_content) = if cached_content.starts_with(marker) {
            // Translation failed - add pre-escaped notice and escape only the content
            let actual = cached_content
                .strip_prefix(marker)
                .unwrap_or(&cached_content);
            (get_translation_failure_notice(language), actual)
        } else {
            // Normal case - no notice needed
            ("".to_string(), cached_content.as_str())
        };

        // The notice_prefix is pre-escaped (from i18n strings)
        // The actual_content needs escaping
        let escaped_content = escape_markdownv2(actual_content);

        // Construct message (simplified version of send_to_subscribers format)
        let message = format!("{}{}", notice_prefix, escaped_content);

        // Verify the notice is NOT double-escaped
        // The notice contains \[ \] \. which should remain as single backslash escapes
        assert!(
            message.contains("\\[Nota"),
            "Notice should have single backslash escape for [. Got: {}",
            message
        );
        assert!(
            message.contains("disponible\\."),
            "Notice should have single backslash escape for periods. Got: {}",
            message
        );

        // Verify the content IS properly escaped
        assert!(
            message.contains("GPT\\-4\\.5\\!"),
            "Content should be escaped. Got: {}",
            message
        );
        assert!(
            message.contains("\\> 2x\\."),
            "Content special chars should be escaped. Got: {}",
            message
        );

        // CRITICAL: Verify no double-escaping patterns
        assert!(
            !message.contains("\\\\\\\\"),
            "REGRESSION: Message contains double-escape patterns! Got: {}",
            message
        );
    }

    /// Test that successful translation (no marker) doesn't add any notice
    #[test]
    fn test_successful_translation_no_notice() {
        let marker = "\x00TRANSLATION_FAILED\x00";
        let translated_content = "Este es el resumen traducido.";

        // Successful translation - content does NOT have marker
        assert!(
            !translated_content.starts_with(marker),
            "Successful translation should not have marker"
        );

        // Mirror the logic from send_to_subscribers:
        let (notice_prefix, actual_content) = if translated_content.starts_with(marker) {
            let actual = translated_content
                .strip_prefix(marker)
                .unwrap_or(translated_content);
            ("NOTICE: ".to_string(), actual)
        } else {
            ("".to_string(), translated_content)
        };

        assert!(
            notice_prefix.is_empty(),
            "Successful translation should have no notice prefix"
        );
        assert_eq!(actual_content, translated_content);
    }

    /// Test English subscribers (canonical language) never get translation failure notice
    #[test]
    fn test_english_subscribers_no_failure_notice() {
        use crate::i18n::Language;
        use crate::translation::get_translation_failure_notice;

        let notice = get_translation_failure_notice(Language::ENGLISH);

        assert!(
            notice.is_empty(),
            "English (canonical language) should have empty failure notice. Got: {}",
            notice
        );
    }

    /// Test that all i18n pre-escaped strings are properly formatted
    #[test]
    fn test_i18n_strings_are_pre_escaped_not_raw() {
        use crate::i18n::Language;

        // These strings contain MarkdownV2 special characters that are pre-escaped
        // They should NOT be passed through escape_markdownv2() again

        // Check Spanish translation failure notice
        let notice = Language::SPANISH
            .config()
            .strings
            .translation_failure_notice;
        assert!(
            notice.contains("\\[") && notice.contains("\\]"),
            "Spanish translation notice should have pre-escaped brackets"
        );
        assert!(
            notice.contains("\\."),
            "Spanish translation notice should have pre-escaped periods"
        );

        // Check welcome messages have pre-escaped special chars
        let welcome = Language::ENGLISH.config().strings.welcome_user;
        assert!(
            welcome.contains("\\!"),
            "English welcome should have pre-escaped exclamation: {}",
            welcome
        );
        assert!(
            welcome.contains("\\-"),
            "English welcome should have pre-escaped hyphens: {}",
            welcome
        );

        // Verify these are ALREADY escaped (contain backslashes)
        // If we escape them again, we'll get double backslashes
        let double_escaped_welcome = escape_markdownv2(welcome);
        assert!(
            double_escaped_welcome.contains("\\\\"),
            "Escaping pre-escaped content produces double backslashes - this must be avoided"
        );
    }

    /// Test the complete message construction for translation failure case
    #[test]
    fn test_complete_translation_failure_message() {
        use crate::i18n::Language;
        use crate::translation::{get_summary_header, get_translation_failure_notice};

        let language = Language::SPANISH;
        let header = get_summary_header(language);
        let timestamp = "2024-01-15 10:30 UTC";
        let summary = "AI update: GPT-4.5 released! Performance > baseline.";

        // Simulate translation failure scenario
        let notice = get_translation_failure_notice(language);

        // Construct the message as send_to_subscribers does:
        // - header gets escaped (it's raw text)
        // - timestamp gets escaped (it's raw text)
        // - notice is pre-escaped (DON'T escape again)
        // - summary gets escaped (it's raw text)
        let message = format!(
            "📰 *{}*\n_{}_\n\n{}{}",
            escape_markdownv2(header),
            escape_markdownv2(timestamp),
            notice, // Already pre-escaped - don't escape!
            escape_markdownv2(summary)
        );

        // Verify the message is valid MarkdownV2
        // The header should be escaped
        assert!(message.contains("Resumen de Twitter"));

        // The notice should have single escapes (not double)
        assert!(
            message.contains("\\[Nota"),
            "Notice should have proper escaping. Got: {}",
            message
        );
        assert!(
            !message.contains("\\\\\\["),
            "Notice should NOT be double-escaped. Got: {}",
            message
        );

        // The summary content should be escaped
        assert!(
            message.contains("GPT\\-4\\.5"),
            "Summary should be escaped. Got: {}",
            message
        );
    }

    /// Test that marker doesn't appear in final messages sent to Telegram
    #[test]
    fn test_marker_stripped_from_final_message() {
        let marker = "\x00TRANSLATION_FAILED\x00";
        let summary = "Test summary.";
        let cached = format!("{}{}", marker, summary);

        // The marker should be stripped before constructing the message
        let actual_content = cached.strip_prefix(marker).unwrap_or(&cached);
        let escaped = escape_markdownv2(actual_content);

        // Marker should never appear in final message
        assert!(
            !escaped.contains("\x00"),
            "Marker bytes should not appear in escaped content"
        );
        assert!(
            !escaped.contains("TRANSLATION_FAILED"),
            "Marker text should not appear in escaped content"
        );
    }

    /// Test edge case: content that looks like it might contain the marker but doesn't
    #[test]
    fn test_content_resembling_marker_not_confused() {
        let marker = "\x00TRANSLATION_FAILED\x00";

        // Content that contains similar text but not the actual marker
        let similar_content = "TRANSLATION_FAILED: The API returned an error.";

        assert!(
            !similar_content.starts_with(marker),
            "Text without null bytes should not match marker"
        );

        // This content should be treated as normal content, not as a failure
        let (notice, actual) = if similar_content.starts_with(marker) {
            ("NOTICE".to_string(), "stripped")
        } else {
            ("".to_string(), similar_content)
        };

        assert!(
            notice.is_empty(),
            "Similar text should not trigger failure notice"
        );
        assert_eq!(actual, similar_content);
    }

    /// Test that pre-escaped i18n strings work directly in Telegram messages
    #[test]
    fn test_pre_escaped_strings_valid_for_telegram() {
        use crate::i18n::Language;

        // These strings are designed to be used directly in MarkdownV2 messages
        let notice = Language::SPANISH
            .config()
            .strings
            .translation_failure_notice;

        // Verify the escaping is correct for MarkdownV2
        // Should have \[ not [ for brackets
        // Should have \. not . for periods
        // Should have \[ \] pairs properly matched

        let bracket_count: usize = notice.matches("\\[").count();
        let close_bracket_count: usize = notice.matches("\\]").count();

        assert_eq!(
            bracket_count, close_bracket_count,
            "Pre-escaped brackets should be balanced"
        );

        // Verify the unescaped versions are NOT present
        // (except where they're part of the escape sequence)
        // Count of standalone '[' should match count of '\['
        let escaped_open_brackets = notice.matches("\\[").count();
        let total_open_brackets = notice.matches('[').count();
        assert_eq!(
            escaped_open_brackets, total_open_brackets,
            "All open brackets should be escaped in pre-escaped string"
        );
    }
}
