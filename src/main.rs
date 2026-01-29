use anyhow::Result;
use axum::{
    extract::{Json, Query, State},
    http::{HeaderMap, StatusCode},
    response::IntoResponse,
    routing::{get, post},
    Router,
};
use std::sync::Arc;
use tracing::{info, warn};
use twitter_news_summary::{config, db, i18n::TranslationMetrics, scheduler, security, telegram};

struct AppState {
    config: Arc<config::Config>,
    db: Arc<db::Database>,
}

#[derive(serde::Deserialize)]
struct TestParams {
    chat_id: Option<String>,
    fresh: Option<bool>,
}

#[derive(serde::Deserialize)]
struct BroadcastRequest {
    message: String,
    parse_mode: Option<String>,
}

#[derive(serde::Serialize)]
struct BroadcastResponse {
    success: bool,
    total: usize,
    sent: usize,
    failed: usize,
    failures: Vec<BroadcastFailure>,
}

#[derive(serde::Serialize)]
struct BroadcastFailure {
    chat_id: i64,
    error: String,
}

#[tokio::main]
async fn main() -> Result<()> {
    // Load .env file (for local development)
    let _ = dotenvy::dotenv();

    // Initialize logging
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive("twitter_news_summary=info".parse()?),
        )
        .init();

    info!("🚀 Starting Twitter News Summary Service");

    // Load configuration
    let config = Arc::new(config::Config::from_env()?);
    info!(
        "✓ Configuration loaded (environment: {})",
        config.environment
    );

    // Warn if API_KEY is not configured
    if config.api_key.is_none() {
        warn!(
            "⚠️  API_KEY not configured - /trigger, /subscribers, /broadcast, and /translation-metrics endpoints will be unprotected"
        );
    }

    // Initialize database
    let db = Arc::new(db::Database::new(&config.database_url).await?);
    info!("✓ Database initialized");

    // Start scheduler
    let _scheduler = scheduler::start_scheduler(Arc::clone(&config), Arc::clone(&db)).await?;
    info!("✓ Scheduler started for times: {:?}", config.schedule_times);

    // Create app state
    let state = Arc::new(AppState {
        config: Arc::clone(&config),
        db: Arc::clone(&db),
    });

    // Build router
    let app = Router::new()
        .route("/health", get(health_check))
        .route("/webhook", post(webhook_handler))
        .route("/trigger", post(trigger_handler))
        .route("/test", post(test_handler))
        .route("/subscribers", get(subscribers_handler))
        .route("/broadcast", post(broadcast_handler))
        .route("/translation-metrics", get(translation_metrics_handler))
        .with_state(state);

    // Start server
    let addr = format!("0.0.0.0:{}", config.port);
    info!("🌐 Starting server on {}", addr);
    let listener = tokio::net::TcpListener::bind(&addr).await?;

    axum::serve(listener, app).await?;

    Ok(())
}

/// Health check endpoint
async fn health_check() -> impl IntoResponse {
    "OK"
}

/// Telegram webhook handler
async fn webhook_handler(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(update): Json<telegram::Update>,
) -> impl IntoResponse {
    // Verify Telegram webhook secret (REQUIRED)
    let expected_secret = &state.config.telegram_webhook_secret;
    match headers.get("X-Telegram-Bot-Api-Secret-Token") {
        Some(header_value) => {
            let provided_secret = header_value.to_str().unwrap_or("");
            if !security::constant_time_compare(provided_secret, expected_secret) {
                warn!("Webhook authentication failed: invalid secret token");
                return StatusCode::UNAUTHORIZED;
            }
        }
        None => {
            warn!("Webhook authentication failed: missing secret token header");
            return StatusCode::UNAUTHORIZED;
        }
    }

    info!("Received webhook: update_id={}", update.update_id);

    match telegram::handle_webhook(&state.config, &state.db, update).await {
        Ok(_) => StatusCode::OK,
        Err(e) => {
            // Log the error but return OK to prevent Telegram from retrying.
            // Returning non-200 causes Telegram to retry the same update indefinitely,
            // which can cause the bot to get "stuck" on a failing update.
            warn!("Webhook handler error: {}", e);
            StatusCode::OK
        }
    }
}

/// Manual trigger endpoint (API key protected)
async fn trigger_handler(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> impl IntoResponse {
    // Check API key with constant-time comparison
    if let Some(expected_key) = &state.config.api_key {
        match headers.get("X-API-Key") {
            Some(header_value) => {
                let provided_key = header_value.to_str().unwrap_or("");
                if !security::constant_time_compare(provided_key, expected_key) {
                    warn!("Unauthorized trigger attempt: invalid API key");
                    return (StatusCode::UNAUTHORIZED, "Unauthorized").into_response();
                }
            }
            None => {
                warn!("Unauthorized trigger attempt: missing API key");
                return (StatusCode::UNAUTHORIZED, "Unauthorized").into_response();
            }
        }
    }

    info!("Manual trigger requested");

    match scheduler::trigger_summary(&state.config, &state.db).await {
        Ok(_) => {
            info!("Manual trigger completed successfully");
            (StatusCode::OK, "Summary sent").into_response()
        }
        Err(e) => {
            warn!("Manual trigger failed: {:?}", e);
            telegram::notify_admin_error(&state.config, "Manual trigger (/trigger)", &e).await;
            (StatusCode::INTERNAL_SERVER_ERROR, format!("Error: {}", e)).into_response()
        }
    }
}

/// Test message endpoint (API key protected) - sends test message to specific chat ID
async fn test_handler(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Query(params): Query<TestParams>,
) -> impl IntoResponse {
    // Check API key with constant-time comparison
    if let Some(expected_key) = &state.config.api_key {
        match headers.get("X-API-Key") {
            Some(header_value) => {
                let provided_key = header_value.to_str().unwrap_or("");
                if !security::constant_time_compare(provided_key, expected_key) {
                    warn!("Unauthorized test attempt: invalid API key");
                    return (StatusCode::UNAUTHORIZED, "Unauthorized").into_response();
                }
            }
            None => {
                warn!("Unauthorized test attempt: missing API key");
                return (StatusCode::UNAUTHORIZED, "Unauthorized").into_response();
            }
        }
    }

    // Get chat ID (from query param or default to TEST_CHAT_ID env var or config chat_id)
    let chat_id = params
        .chat_id
        .or_else(|| std::env::var("TEST_CHAT_ID").ok())
        .unwrap_or_else(|| state.config.telegram_chat_id.clone());

    // Guard against empty chat_id
    if chat_id.trim().is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            "chat_id is required (query param or TEST_CHAT_ID env var)",
        )
            .into_response();
    }

    info!("Test message requested for chat_id: {}", chat_id);

    // Get or generate summary
    let summary = if params.fresh.unwrap_or(false) {
        info!("Generating fresh summary for test (no broadcast)");
        // Generate fresh summary WITHOUT broadcasting to all subscribers
        match scheduler::generate_summary_only(&state.config, &state.db).await {
            Ok(summary) => summary,
            Err(e) => {
                warn!("Failed to generate summary: {}", e);
                return (StatusCode::INTERNAL_SERVER_ERROR, format!("Error: {}", e))
                    .into_response();
            }
        }
    } else {
        // Use latest from DB
        match state.db.get_latest_summary().await {
            Ok(Some(summary)) => summary.content,
            Ok(None) => {
                warn!("No summary available in database");
                return (StatusCode::NOT_FOUND, "No summary available").into_response();
            }
            Err(e) => {
                warn!("Failed to fetch summary from database: {}", e);
                return (StatusCode::INTERNAL_SERVER_ERROR, format!("Error: {}", e))
                    .into_response();
            }
        }
    };

    // Send test message
    match telegram::send_test_message(&state.config, &chat_id, &summary).await {
        Ok(_) => {
            info!("Test message sent successfully to {}", chat_id);
            (StatusCode::OK, format!("Test message sent to {}", chat_id)).into_response()
        }
        Err(e) => {
            warn!("Test message failed: {}", e);
            (StatusCode::INTERNAL_SERVER_ERROR, format!("Error: {}", e)).into_response()
        }
    }
}

/// List subscribers endpoint (API key protected, for debugging)
async fn subscribers_handler(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> impl IntoResponse {
    // Check API key with constant-time comparison
    if let Some(expected_key) = &state.config.api_key {
        match headers.get("X-API-Key") {
            Some(header_value) => {
                let provided_key = header_value.to_str().unwrap_or("");
                if !security::constant_time_compare(provided_key, expected_key) {
                    warn!("Unauthorized subscribers list attempt: invalid API key");
                    return (
                        StatusCode::UNAUTHORIZED,
                        Json(serde_json::json!({
                            "error": "Unauthorized"
                        })),
                    )
                        .into_response();
                }
            }
            None => {
                warn!("Unauthorized subscribers list attempt: missing API key");
                return (
                    StatusCode::UNAUTHORIZED,
                    Json(serde_json::json!({
                        "error": "Unauthorized"
                    })),
                )
                    .into_response();
            }
        }
    }

    match state.db.list_subscribers().await {
        Ok(subscribers) => {
            let chat_ids: Vec<i64> = subscribers.iter().map(|s| s.chat_id).collect();
            (
                StatusCode::OK,
                Json(serde_json::json!({
                    "subscribers": chat_ids,
                    "count": subscribers.len()
                })),
            )
                .into_response()
        }
        Err(e) => {
            warn!("Failed to list subscribers: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({
                    "error": format!("Error: {}", e)
                })),
            )
                .into_response()
        }
    }
}

/// Broadcast message endpoint (API key protected) - sends custom message to all subscribers
async fn broadcast_handler(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(request): Json<BroadcastRequest>,
) -> impl IntoResponse {
    // Check API key with constant-time comparison
    if let Some(expected_key) = &state.config.api_key {
        match headers.get("X-API-Key") {
            Some(header_value) => {
                let provided_key = header_value.to_str().unwrap_or("");
                if !security::constant_time_compare(provided_key, expected_key) {
                    warn!("Unauthorized broadcast attempt: invalid API key");
                    return (
                        StatusCode::UNAUTHORIZED,
                        Json(serde_json::json!({
                            "error": "Unauthorized"
                        })),
                    )
                        .into_response();
                }
            }
            None => {
                warn!("Unauthorized broadcast attempt: missing API key");
                return (
                    StatusCode::UNAUTHORIZED,
                    Json(serde_json::json!({
                        "error": "Unauthorized"
                    })),
                )
                    .into_response();
            }
        }
    }

    // Validate message is not empty
    if request.message.trim().is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({
                "error": "Message cannot be empty"
            })),
        )
            .into_response();
    }

    info!("Broadcast message requested");

    match telegram::broadcast_message(
        &state.config,
        &state.db,
        &request.message,
        request.parse_mode.as_deref(),
    )
    .await
    {
        Ok((sent, failures)) => {
            let total = sent + failures.len();
            let response = BroadcastResponse {
                success: true,
                total,
                sent,
                failed: failures.len(),
                failures: failures
                    .into_iter()
                    .map(|(chat_id, error)| BroadcastFailure { chat_id, error })
                    .collect(),
            };
            info!(
                "Broadcast completed: {}/{} sent successfully",
                response.sent, response.total
            );
            (StatusCode::OK, Json(response)).into_response()
        }
        Err(e) => {
            warn!("Broadcast failed: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({
                    "error": format!("Error: {}", e)
                })),
            )
                .into_response()
        }
    }
}

/// Translation metrics endpoint (API key protected) - returns translation statistics
async fn translation_metrics_handler(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> impl IntoResponse {
    // Check API key with constant-time comparison
    if let Some(expected_key) = &state.config.api_key {
        match headers.get("X-API-Key") {
            Some(header_value) => {
                let provided_key = header_value.to_str().unwrap_or("");
                if !security::constant_time_compare(provided_key, expected_key) {
                    warn!("Unauthorized translation metrics attempt: invalid API key");
                    return (
                        StatusCode::UNAUTHORIZED,
                        Json(serde_json::json!({
                            "error": "Unauthorized"
                        })),
                    )
                        .into_response();
                }
            }
            None => {
                warn!("Unauthorized translation metrics attempt: missing API key");
                return (
                    StatusCode::UNAUTHORIZED,
                    Json(serde_json::json!({
                        "error": "Unauthorized"
                    })),
                )
                    .into_response();
            }
        }
    }

    // Get metrics report
    let report = TranslationMetrics::global().report();
    (StatusCode::OK, Json(report)).into_response()
}
