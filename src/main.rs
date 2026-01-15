use anyhow::Result;
use axum::{
    extract::{Json, State},
    http::{HeaderMap, StatusCode},
    response::IntoResponse,
    routing::{get, post},
    Router,
};
use std::sync::Arc;
use tracing::{info, warn};
use twitter_news_summary::{config, db, scheduler, security, telegram};

struct AppState {
    config: Arc<config::Config>,
    db: Arc<db::Database>,
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
    info!("✓ Configuration loaded");

    // Warn if API_KEY is not configured
    if config.api_key.is_none() {
        warn!(
            "⚠️  API_KEY not configured - /trigger and /subscribers endpoints will be unprotected"
        );
    }

    // Initialize database
    let db = Arc::new(db::Database::new(&config.database_path)?);
    info!("✓ Database initialized at {}", config.database_path);

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
        .route("/subscribers", get(subscribers_handler))
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
            warn!("Webhook handler error: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
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
            warn!("Manual trigger failed: {}", e);
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

    match state.db.list_subscribers() {
        Ok(subscribers) => {
            let chat_ids: Vec<String> = subscribers.iter().map(|s| s.chat_id.clone()).collect();
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
