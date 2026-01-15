use anyhow::Result;
use tokio_cron_scheduler::{Job, JobScheduler};
use tracing::{info, error};
use std::sync::Arc;
use crate::config::Config;
use crate::db::Database;
use crate::rss;
use crate::openai;
use crate::telegram;

/// Initialize and start the scheduler
pub async fn start_scheduler(config: Arc<Config>, db: Arc<Database>) -> Result<JobScheduler> {
    let scheduler = JobScheduler::new().await?;

    // Read usernames from file once at startup
    let usernames_content = std::fs::read_to_string(&config.usernames_file)?;
    let usernames: Vec<String> = usernames_content
        .lines()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect();

    info!("Loaded {} usernames for scheduled fetches", usernames.len());

    // Create scheduled jobs for each time in schedule_times
    for time in &config.schedule_times {
        let cron_expr = time_to_cron(time)?;
        info!("Scheduling job for {} (cron: {})", time, cron_expr);

        let config_clone = Arc::clone(&config);
        let db_clone = Arc::clone(&db);
        let usernames_clone = usernames.clone();

        let job = Job::new_async(cron_expr.as_str(), move |_uuid, _l| {
            let config = Arc::clone(&config_clone);
            let db = Arc::clone(&db_clone);
            let usernames = usernames_clone.clone();

            Box::pin(async move {
                info!("⏰ Scheduled job triggered");
                if let Err(e) = run_summary_job(&config, &db, &usernames).await {
                    error!("Scheduled job failed: {}", e);
                }
            })
        })?;

        scheduler.add(job).await?;
    }

    scheduler.start().await?;
    info!("✓ Scheduler started");

    Ok(scheduler)
}

/// Convert time string (HH:MM) to cron expression in Peru time (UTC-5)
fn time_to_cron(time: &str) -> Result<String> {
    let parts: Vec<&str> = time.split(':').collect();
    if parts.len() != 2 {
        anyhow::bail!("Invalid time format: {}. Expected HH:MM", time);
    }

    let hour: u8 = parts[0].parse()?;
    let minute: u8 = parts[1].parse()?;

    // Convert Peru time (UTC-5) to UTC
    // For example: 08:00 Peru = 13:00 UTC
    let utc_hour = (hour + 5) % 24;

    // Cron format: "second minute hour day month day_of_week"
    // We run daily, so: "0 <minute> <hour> * * *"
    Ok(format!("0 {} {} * * *", minute, utc_hour))
}

/// Run the summary job: fetch tweets, summarize, send to subscribers
async fn run_summary_job(config: &Config, db: &Database, usernames: &[String]) -> Result<()> {
    info!("Starting summary job");

    // Fetch tweets
    info!("Fetching tweets from RSS feeds");
    let tweets = rss::fetch_tweets_from_rss(config, usernames).await?;

    if tweets.is_empty() {
        info!("No tweets found in the specified time window");
        return Ok(());
    }

    info!("Fetched {} tweets", tweets.len());

    // Generate summary
    info!("Generating summary with OpenAI");
    let summary = openai::summarize_tweets(config, &tweets).await?;

    // Send to all subscribers
    info!("Sending summary via Telegram");
    telegram::send_to_subscribers(config, db, &summary).await?;

    info!("✓ Summary job completed successfully");

    Ok(())
}

/// Manually trigger a summary job (for /trigger endpoint)
pub async fn trigger_summary(config: &Config, db: &Database) -> Result<()> {
    // Read usernames from file
    let usernames_content = std::fs::read_to_string(&config.usernames_file)?;
    let usernames: Vec<String> = usernames_content
        .lines()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect();

    run_summary_job(config, db, &usernames).await
}
