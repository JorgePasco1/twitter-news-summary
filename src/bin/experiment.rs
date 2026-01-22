//! Experiment binary for testing OpenAI summarization with different parameters
//!
//! Usage:
//!   cargo run --bin experiment fetch       # Fetch tweets and cache
//!   cargo run --bin experiment run-all     # Run all model × temperature combinations
//!   cargo run --bin experiment summarize   # Run single summarization (uses env vars)
//!
//! The run-all command tests these combinations:
//!   Models: gpt-4o-mini, gpt-5-nano, gpt-5-mini
//!   Temperatures: 0.3, 0.7, 1.0 (filtered by supports_custom_temperature)
//!   = 5 total combinations (gpt-4o-mini×3 temps + gpt-5-nano×1 + gpt-5-mini×1)

use anyhow::{Context, Result};
use chrono::Utc;
use std::fs;
use std::path::Path;
use tracing::info;
use twitter_news_summary::{openai, rss, twitter::Tweet};

const CACHE_FILE: &str = "run-history/experiment_tweets.json";

// ==================== Experiment Combinations ====================

/// Models to test (ordered by cost: cheapest first)
const MODELS: &[&str] = &["gpt-4o-mini", "gpt-5-nano", "gpt-5-mini"];

/// Temperatures to test
const TEMPERATURES: &[f32] = &[0.3, 0.7, 1.0];

/// Number of runs per combination (to measure variation)
const RUNS_PER_COMBO: u32 = 3;

/// Check if a model supports custom temperature values
/// Some newer models (gpt-5-nano, gpt-5-mini) only support temperature=1
fn supports_custom_temperature(model: &str) -> bool {
    !matches!(model, "gpt-5-nano" | "gpt-5-mini")
}

/// Check if a model is a reasoning model (uses tokens for internal reasoning)
/// Reasoning models need much higher max_completion_tokens because they use
/// tokens for both reasoning (hidden) and output (visible)
fn is_reasoning_model(model: &str) -> bool {
    matches!(
        model,
        "gpt-5-nano" | "gpt-5-mini" | "o1" | "o1-mini" | "o1-preview"
    )
}

/// A single experiment combination
#[derive(Debug, Clone)]
struct Combination {
    model: String,
    temperature: f32,
}

impl Combination {
    fn all() -> Vec<Combination> {
        let mut combos = Vec::new();
        for &model in MODELS {
            if supports_custom_temperature(model) {
                // Model supports all temperatures
                for &temp in TEMPERATURES {
                    combos.push(Combination {
                        model: model.to_string(),
                        temperature: temp,
                    });
                }
            } else {
                // Model only supports temperature=1
                combos.push(Combination {
                    model: model.to_string(),
                    temperature: 1.0,
                });
            }
        }
        combos
    }

    /// Short label for filenames (e.g., "gpt-4o-mini_t0.3_run1")
    fn file_label(&self, run: u32) -> String {
        format!("{}_t{}_run{}", self.model, self.temperature, run)
    }

    /// Human-readable label (e.g., "gpt-4o-mini @ temperature 0.3")
    fn display_label(&self) -> String {
        format!("{} @ temperature {}", self.model, self.temperature)
    }

    /// Human-readable label with run number (e.g., "gpt-4o-mini @ temperature 0.3 (run 1)")
    fn display_label_with_run(&self, run: u32) -> String {
        format!(
            "{} @ temperature {} (run {})",
            self.model, self.temperature, run
        )
    }
}

// ==================== Config ====================

/// Minimal config for experiment (no Telegram/DB required)
#[derive(Clone)]
struct ExperimentConfig {
    openai_api_key: String,
    openai_model: String,
    openai_api_url: String,
    openai_temperature: f32,
    nitter_instance: String,
    nitter_api_key: Option<String>,
    usernames_file: String,
    max_tweets: u32,
    hours_lookback: u32,
    summary_max_tokens: u32,
    summary_max_words: u32,
}

impl ExperimentConfig {
    fn from_env() -> Result<Self> {
        Ok(Self {
            openai_api_key: std::env::var("OPENAI_API_KEY").context("OPENAI_API_KEY not set")?,
            openai_model: std::env::var("OPENAI_MODEL")
                .unwrap_or_else(|_| "gpt-5-mini".to_string()),
            openai_api_url: std::env::var("OPENAI_API_URL")
                .unwrap_or_else(|_| "https://api.openai.com/v1/chat/completions".to_string()),
            openai_temperature: std::env::var("OPENAI_TEMPERATURE")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(0.7),
            nitter_instance: std::env::var("NITTER_INSTANCE").context("NITTER_INSTANCE not set")?,
            nitter_api_key: std::env::var("NITTER_API_KEY").ok(),
            usernames_file: std::env::var("USERNAMES_FILE")
                .unwrap_or_else(|_| "data/usernames.txt".to_string()),
            max_tweets: std::env::var("MAX_TWEETS")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(100),
            hours_lookback: std::env::var("HOURS_LOOKBACK")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(12),
            summary_max_tokens: std::env::var("SUMMARY_MAX_TOKENS")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(2500),
            summary_max_words: std::env::var("SUMMARY_MAX_WORDS")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(800),
        })
    }

    /// Create a config with specific model and temperature (for run-all)
    fn with_combination(&self, combo: &Combination) -> Self {
        // Reasoning models (gpt-5-nano, gpt-5-mini, o1 series) use tokens for both
        // internal reasoning and output. They need much higher max_completion_tokens.
        // Non-reasoning models: 2500 tokens is enough for ~800 word summary
        // Reasoning models: Need 16000+ tokens (hidden reasoning + visible output)
        let summary_max_tokens = if is_reasoning_model(&combo.model) {
            16000
        } else {
            self.summary_max_tokens
        };

        Self {
            openai_api_key: self.openai_api_key.clone(),
            openai_model: combo.model.clone(),
            openai_api_url: self.openai_api_url.clone(),
            openai_temperature: combo.temperature,
            nitter_instance: self.nitter_instance.clone(),
            nitter_api_key: self.nitter_api_key.clone(),
            usernames_file: self.usernames_file.clone(),
            max_tweets: self.max_tweets,
            hours_lookback: self.hours_lookback,
            summary_max_tokens,
            summary_max_words: self.summary_max_words,
        }
    }

    /// Convert to the full Config struct (with dummy values for unused fields)
    fn to_full_config(&self) -> twitter_news_summary::config::Config {
        twitter_news_summary::config::Config {
            environment: "experiment".to_string(),
            twitter_bearer_token: None,
            twitter_list_id: None,
            openai_api_key: self.openai_api_key.clone(),
            openai_model: self.openai_model.clone(),
            openai_api_url: self.openai_api_url.clone(),
            openai_temperature: self.openai_temperature,
            telegram_bot_token: "unused".to_string(),
            telegram_chat_id: "unused".to_string(),
            telegram_webhook_secret: "unused".to_string(),
            max_tweets: self.max_tweets,
            hours_lookback: self.hours_lookback,
            summary_max_tokens: self.summary_max_tokens,
            summary_max_words: self.summary_max_words,
            nitter_instance: self.nitter_instance.clone(),
            nitter_api_key: self.nitter_api_key.clone(),
            usernames_file: self.usernames_file.clone(),
            api_key: None,
            database_url: "unused".to_string(),
            schedule_times: vec![],
            port: 8080,
        }
    }
}

// ==================== Cache Functions ====================

/// Save tweets to cache file
fn save_tweets_cache(tweets: &[Tweet]) -> Result<()> {
    let cache_dir = Path::new("run-history");
    fs::create_dir_all(cache_dir).context("Failed to create run-history directory")?;

    let cache_path = Path::new(CACHE_FILE);
    let json =
        serde_json::to_string_pretty(tweets).context("Failed to serialize tweets to JSON")?;
    fs::write(cache_path, json).context("Failed to write tweets cache")?;

    info!(
        "Saved {} tweets to cache at {}",
        tweets.len(),
        cache_path.display()
    );
    Ok(())
}

/// Load tweets from cache file
fn load_tweets_cache() -> Result<Vec<Tweet>> {
    let cache_path = Path::new(CACHE_FILE);

    if !cache_path.exists() {
        anyhow::bail!(
            "No tweets cache found at {}. Run 'cargo run --bin experiment fetch' first.",
            cache_path.display()
        );
    }

    let contents = fs::read_to_string(cache_path).context("Failed to read tweets cache")?;
    let tweets: Vec<Tweet> =
        serde_json::from_str(&contents).context("Failed to parse tweets cache")?;

    info!(
        "Loaded {} tweets from cache at {}",
        tweets.len(),
        cache_path.display()
    );
    Ok(tweets)
}

// ==================== Commands ====================

/// Fetch tweets from RSS and cache them
async fn fetch_command(config: &ExperimentConfig) -> Result<()> {
    let full_config = config.to_full_config();

    // Read usernames
    let usernames_content =
        std::fs::read_to_string(&config.usernames_file).context("Failed to read usernames file")?;
    let usernames: Vec<String> = usernames_content
        .lines()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect();

    info!("Loaded {} usernames", usernames.len());

    // Fetch tweets
    info!(
        "Fetching tweets from RSS feeds (last {} hours)...",
        config.hours_lookback
    );
    let tweets = rss::fetch_tweets_from_rss(&full_config, &usernames).await?;

    if tweets.is_empty() {
        println!(
            "\nNo tweets found in the last {} hours.",
            config.hours_lookback
        );
        return Ok(());
    }

    // Save to cache
    save_tweets_cache(&tweets)?;

    println!("\n========================================");
    println!("  FETCH COMPLETE");
    println!("========================================");
    println!("  Tweets fetched: {}", tweets.len());
    println!("  Cached to: {}", CACHE_FILE);
    println!("========================================\n");

    Ok(())
}

/// Run all model × temperature combinations (each run multiple times)
async fn run_all_command(base_config: &ExperimentConfig) -> Result<()> {
    let tweets = load_tweets_cache()?;

    if tweets.is_empty() {
        println!("\nNo tweets in cache.");
        return Ok(());
    }

    let combinations = Combination::all();
    let combo_count = combinations.len();
    let total_runs = combo_count * RUNS_PER_COMBO as usize;

    // Create experiment folder with timestamp
    let timestamp = Utc::now().format("%Y-%m-%d_%H-%M-%S").to_string();
    let experiment_dir = Path::new("run-history").join(format!("experiment_{}", timestamp));
    fs::create_dir_all(&experiment_dir).context("Failed to create experiment directory")?;

    println!("\n╔══════════════════════════════════════════════════════════════╗");
    println!("║                    RUNNING ALL COMBINATIONS                   ║");
    println!("╠══════════════════════════════════════════════════════════════╣");
    println!("║  Tweets: {:<52} ║", tweets.len());
    println!("║  Models: {:<52} ║", MODELS.join(", "));
    println!(
        "║  Temperatures: {:<46} ║",
        TEMPERATURES
            .iter()
            .map(|t| t.to_string())
            .collect::<Vec<_>>()
            .join(", ")
    );
    println!("║  Combinations: {:<46} ║", combo_count);
    println!("║  Runs per combo: {:<44} ║", RUNS_PER_COMBO);
    println!("║  Total runs: {:<48} ║", total_runs);
    println!("║  Output folder: {:<45} ║", experiment_dir.display());
    println!("╚══════════════════════════════════════════════════════════════╝\n");

    // (combo, run_number, summary, filepath)
    let mut results: Vec<(Combination, u32, String, String)> = Vec::new();
    let mut run_counter = 0;

    for combo in combinations.iter() {
        println!("\n─── {} ───", combo.display_label());

        for run in 1..=RUNS_PER_COMBO {
            run_counter += 1;
            println!("  [{}/{}] Run {}...", run_counter, total_runs, run);

            let config = base_config.with_combination(combo);
            let full_config = config.to_full_config();
            let tweets_clone = tweets.clone();

            // Spawn each API call as a separate tokio task to avoid async runtime issues
            // Use longer timeout for reasoning models (5 minutes vs 2 minutes)
            let timeout_secs = if is_reasoning_model(&combo.model) {
                300
            } else {
                120
            };
            println!(
                "         → Spawning API call task (timeout: {}s)...",
                timeout_secs
            );
            let handle = tokio::spawn(async move {
                let client = reqwest::Client::builder()
                    .timeout(std::time::Duration::from_secs(timeout_secs))
                    .connect_timeout(std::time::Duration::from_secs(30))
                    .pool_max_idle_per_host(0)
                    .build()
                    .context("Failed to build HTTP client")?;

                openai::summarize_tweets(&client, &full_config, &tweets_clone).await
            });

            println!("         → Awaiting API call result...");
            match handle.await {
                Ok(Ok(summary)) => {
                    // Save individual result
                    let filename = format!("{}.md", combo.file_label(run));
                    let filepath = experiment_dir.join(&filename);

                    let file_content =
                        format_summary_file_with_run(combo, run, &config, tweets.len(), &summary);
                    fs::write(&filepath, &file_content)
                        .context("Failed to write experiment result to file")?;

                    println!("         ✓ Saved: {}", filename);
                    results.push((combo.clone(), run, summary, filepath.display().to_string()));
                }
                Ok(Err(e)) => {
                    println!("         ✗ API Error: {}", e);
                }
                Err(e) => {
                    println!("         ✗ Task Error: {}", e);
                }
            }
        }
    }

    // Generate index file
    let index_path = experiment_dir.join("_INDEX.md");
    let index_content = format_index_file(&timestamp, tweets.len(), base_config, &results);
    fs::write(&index_path, &index_content).context("Failed to write experiment index file")?;

    let successful = results.len();
    let failed = total_runs - successful;

    println!("\n╔══════════════════════════════════════════════════════════════╗");
    println!("║                       EXPERIMENT COMPLETE                     ║");
    println!("╠══════════════════════════════════════════════════════════════╣");
    println!("║  Successful runs: {:<43} ║", successful);
    println!("║  Failed runs: {:<47} ║", failed);
    println!("║  Results: {:<51} ║", experiment_dir.display());
    println!("║  Index: {:<53} ║", index_path.display());
    println!("╚══════════════════════════════════════════════════════════════╝\n");

    Ok(())
}

/// Run single summarization with current config
async fn summarize_command(config: &ExperimentConfig) -> Result<()> {
    let tweets = load_tweets_cache()?;

    if tweets.is_empty() {
        println!("\nNo tweets in cache.");
        return Ok(());
    }

    // Adjust token limit for reasoning models
    let mut adjusted_config = config.clone();
    if is_reasoning_model(&config.openai_model) {
        adjusted_config.summary_max_tokens = 16000;
    }

    let full_config = adjusted_config.to_full_config();

    println!("\n========================================");
    println!("  EXPERIMENT PARAMETERS");
    println!("========================================");
    println!("  Model:       {}", adjusted_config.openai_model);
    println!("  Temperature: {}", adjusted_config.openai_temperature);
    println!("  Max Tokens:  {}", adjusted_config.summary_max_tokens);
    println!("  Max Words:   {}", adjusted_config.summary_max_words);
    println!("  Tweets:      {}", tweets.len());
    println!("========================================\n");

    info!("Generating summary with {} tweets...", tweets.len());

    // Use longer timeout for reasoning models (5 minutes vs 2 minutes)
    let timeout_secs = if is_reasoning_model(&adjusted_config.openai_model) {
        300
    } else {
        120
    };
    println!("  Using timeout: {}s", timeout_secs);

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(timeout_secs))
        .connect_timeout(std::time::Duration::from_secs(30))
        .pool_max_idle_per_host(0) // Disable connection pooling
        .build()
        .context("Failed to build HTTP client")?;

    let summary = openai::summarize_tweets(&client, &full_config, &tweets).await?;

    // Save summary to file
    let history_dir = Path::new("run-history");
    fs::create_dir_all(history_dir).context("Failed to create run-history directory")?;

    let combo = Combination {
        model: adjusted_config.openai_model.clone(),
        temperature: adjusted_config.openai_temperature,
    };
    let filename = format!(
        "single_{}_{}_t{}.md",
        Utc::now().format("%Y-%m-%d_%H-%M-%S"),
        combo.model,
        combo.temperature
    );
    let filepath = history_dir.join(&filename);

    let file_content = format_summary_file(&combo, &adjusted_config, tweets.len(), &summary);
    fs::write(&filepath, &file_content).context("Failed to write summary to run-history")?;

    println!("========== SUMMARY ==========\n");
    println!("{}", summary);
    println!("\n========== END SUMMARY ==========\n");
    println!("Saved to: {}", filepath.display());
    println!();

    Ok(())
}

// ==================== File Formatting ====================

/// Format a summary file with readable header (for single runs)
fn format_summary_file(
    combo: &Combination,
    config: &ExperimentConfig,
    tweet_count: usize,
    summary: &str,
) -> String {
    format!(
        r#"# {}

## Configuration

| Parameter | Value |
|-----------|-------|
| **Model** | `{}` |
| **Temperature** | `{}` |
| **Max Tokens** | `{}` |
| **Max Words** | `{}` |
| **Tweets Processed** | `{}` |
| **Generated At** | `{}` |

---

## Summary

{}
"#,
        combo.display_label(),
        combo.model,
        combo.temperature,
        config.summary_max_tokens,
        config.summary_max_words,
        tweet_count,
        Utc::now().format("%Y-%m-%d %H:%M:%S UTC"),
        summary
    )
}

/// Format a summary file with run number in header (for run-all)
fn format_summary_file_with_run(
    combo: &Combination,
    run: u32,
    config: &ExperimentConfig,
    tweet_count: usize,
    summary: &str,
) -> String {
    format!(
        r#"# {}

## Configuration

| Parameter | Value |
|-----------|-------|
| **Model** | `{}` |
| **Temperature** | `{}` |
| **Run** | `{} of {}` |
| **Max Tokens** | `{}` |
| **Max Words** | `{}` |
| **Tweets Processed** | `{}` |
| **Generated At** | `{}` |

---

## Summary

{}
"#,
        combo.display_label_with_run(run),
        combo.model,
        combo.temperature,
        run,
        RUNS_PER_COMBO,
        config.summary_max_tokens,
        config.summary_max_words,
        tweet_count,
        Utc::now().format("%Y-%m-%d %H:%M:%S UTC"),
        summary
    )
}

/// Format the index file that lists all results
fn format_index_file(
    timestamp: &str,
    tweet_count: usize,
    config: &ExperimentConfig,
    results: &[(Combination, u32, String, String)],
) -> String {
    let combo_count = Combination::all().len();

    let mut content = format!(
        r#"# Experiment Results: {}

## Overview

| Setting | Value |
|---------|-------|
| **Timestamp** | `{}` |
| **Tweets Processed** | `{}` |
| **Max Tokens** | `{}` |
| **Max Words** | `{}` |
| **Models Tested** | {} |
| **Temperatures Tested** | {} |
| **Runs per Combo** | {} |
| **Total Combinations** | {} |
| **Total Runs** | {} |

---

## Results

| # | Model | Temp | Run | File |
|---|-------|------|-----|------|
"#,
        timestamp,
        timestamp,
        tweet_count,
        config.summary_max_tokens,
        config.summary_max_words,
        MODELS.join(", "),
        TEMPERATURES
            .iter()
            .map(|t| t.to_string())
            .collect::<Vec<_>>()
            .join(", "),
        RUNS_PER_COMBO,
        combo_count,
        results.len()
    );

    for (i, (combo, run, _, filepath)) in results.iter().enumerate() {
        let filename = Path::new(filepath)
            .file_name()
            .unwrap_or_default()
            .to_string_lossy();
        content.push_str(&format!(
            "| {} | {} | {} | {} | [{}]({}) |\n",
            i + 1,
            combo.model,
            combo.temperature,
            run,
            filename,
            filename
        ));
    }

    content.push_str("\n---\n\n## Quick Comparison\n\n");

    // Add first ~300 chars of each summary for quick comparison
    for (combo, run, summary, _) in results {
        let preview: String = summary.chars().take(300).collect();
        let preview = preview.replace('\n', " ");
        content.push_str(&format!(
            "### {} (run {})\n\n{}{}\n\n",
            combo.display_label(),
            run,
            preview,
            if summary.len() > 300 { "..." } else { "" }
        ));
    }

    content
}

// ==================== Main ====================

fn print_usage() {
    let models = MODELS.join(", ");
    let temps: String = TEMPERATURES
        .iter()
        .map(|t| t.to_string())
        .collect::<Vec<_>>()
        .join(", ");
    let combo_count = Combination::all().len();
    let total_runs = combo_count * RUNS_PER_COMBO as usize;

    println!(
        r#"
Experiment binary for testing OpenAI summarization parameters

USAGE:
    cargo run --bin experiment <COMMAND>

COMMANDS:
    fetch      Fetch tweets from RSS and cache to {cache}
    run-all    Run all combinations ({total_runs} total runs)
    summarize  Run single summarization with env var settings

COMBINATIONS (run-all):
    Models:         {models}
    Temperatures:   {temps}
    Runs per combo: {runs_per_combo}
    ─────────────────────────────
    Combinations:   {combo_count}
    Total runs:     {total_runs}

ENVIRONMENT VARIABLES (for 'summarize' command):
    OPENAI_MODEL          Model to use (default: gpt-5-mini)
    OPENAI_TEMPERATURE    Temperature 0.0-2.0 (default: 0.7)
    SUMMARY_MAX_TOKENS    Max tokens in response (default: 2500; 16000 for reasoning models)
    SUMMARY_MAX_WORDS     Max words in summary (default: 800)

EXAMPLES:
    # Step 1: Fetch tweets once
    cargo run --bin experiment fetch

    # Step 2a: Run ALL combinations at once (3 runs each to measure variation)
    cargo run --bin experiment run-all

    # Step 2b: Or run a single experiment
    OPENAI_MODEL=gpt-4o OPENAI_TEMPERATURE=0.5 cargo run --bin experiment summarize
"#,
        cache = CACHE_FILE,
        models = models,
        temps = temps,
        runs_per_combo = RUNS_PER_COMBO,
        combo_count = combo_count,
        total_runs = total_runs
    );
}

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize logging
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive("twitter_news_summary=info".parse().unwrap()),
        )
        .init();

    // Load environment from .env file
    dotenvy::dotenv().ok();

    // Parse CLI arguments
    let args: Vec<String> = std::env::args().collect();

    if args.len() < 2 {
        print_usage();
        return Ok(());
    }

    let command = &args[1];

    let config = ExperimentConfig::from_env()?;

    match command.as_str() {
        "fetch" => fetch_command(&config).await,
        "run-all" => run_all_command(&config).await,
        "summarize" => summarize_command(&config).await,
        "--help" | "-h" | "help" => {
            print_usage();
            Ok(())
        }
        _ => {
            eprintln!("Unknown command: {}", command);
            print_usage();
            std::process::exit(1);
        }
    }
}
