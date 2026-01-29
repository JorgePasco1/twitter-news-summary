use crate::config::Config;
use crate::db::Database;
use crate::openai;
use crate::rss;
use crate::telegram;
use anyhow::Result;
use chrono::{NaiveTime, TimeZone, Timelike};
use std::sync::Arc;
use tokio_cron_scheduler::{Job, JobScheduler};
use tracing::{error, info};

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

    // Calculate processing time offset based on user count
    let offset_seconds = estimate_processing_seconds(usernames.len());
    info!(
        "Estimated processing time: {}s (~{} min) - jobs will start early to ensure on-time delivery",
        offset_seconds,
        offset_seconds / 60
    );

    // Create scheduled jobs for each time in schedule_times
    for time in &config.schedule_times {
        let cron_expr = time_to_cron(time, offset_seconds)?;
        info!(
            "Scheduling job for {} Peru time (cron: {}, starts ~{}s early)",
            time, cron_expr, offset_seconds
        );

        let config_clone = Arc::clone(&config);
        let db_clone = Arc::clone(&db);
        let usernames_clone = usernames.clone();
        let target_time = time.clone();

        let job = Job::new_async(cron_expr.as_str(), move |_uuid, _l| {
            let config = Arc::clone(&config_clone);
            let db = Arc::clone(&db_clone);
            let usernames = usernames_clone.clone();
            let target = target_time.clone();

            Box::pin(async move {
                info!(
                    "⏰ Scheduled job triggered (target send time: {} Peru)",
                    target
                );
                if let Err(e) = run_summary_job(&config, &db, &usernames, Some(&target)).await {
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

/// Calculate estimated processing time in seconds based on user count
/// This accounts for: rate limiting delays (3s/user), fetch time, and summarization
fn estimate_processing_seconds(user_count: usize) -> u32 {
    // 3 seconds rate limiting delay per user
    // + ~1 second average fetch time per user
    // + 30 seconds buffer for OpenAI summarization and sending
    // + 30 seconds safety buffer
    let per_user_seconds = 4;
    let base_buffer = 60;
    (user_count as u32 * per_user_seconds) + base_buffer
}

/// Convert time string (HH:MM) to cron expression in Peru time (UTC-5)
/// Optionally applies an offset (in seconds) to start the job earlier
fn time_to_cron(time: &str, offset_seconds: u32) -> Result<String> {
    let parts: Vec<&str> = time.split(':').collect();
    if parts.len() != 2 {
        anyhow::bail!("Invalid time format: {}. Expected HH:MM", time);
    }

    let hour: u8 = parts[0].parse()?;
    let minute: u8 = parts[1].parse()?;

    // Parse the target time
    let target_time = NaiveTime::from_hms_opt(hour as u32, minute as u32, 0)
        .ok_or_else(|| anyhow::anyhow!("Invalid time: {}:{}", hour, minute))?;

    // Subtract offset to get the job start time
    let offset_duration = chrono::Duration::seconds(offset_seconds as i64);
    let job_start_time = target_time - offset_duration;

    // Convert FROM Peru time (UTC-5) TO UTC (add 5 hours)
    let utc_hour = (job_start_time.hour() + 5) % 24;
    let utc_minute = job_start_time.minute();

    // Cron format: "second minute hour day month day_of_week"
    // We run daily, so: "0 <minute> <hour> * * *"
    Ok(format!("0 {} {} * * *", utc_minute, utc_hour))
}

/// Run the summary job: fetch tweets, summarize, send to subscribers
/// If target_send_time is provided (HH:MM in Peru time), waits until that time before sending
async fn run_summary_job(
    config: &Config,
    db: &Database,
    usernames: &[String],
    target_send_time: Option<&str>,
) -> Result<()> {
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
    let client = reqwest::Client::new();
    let summary = openai::summarize_tweets(&client, config, &tweets).await?;

    // Save summary to database and get the ID for translation caching
    let summary_id = db.save_summary(&summary).await?;
    info!("✓ Summary saved to database (id: {})", summary_id);

    // If we have a target send time, wait until that time before sending
    if let Some(target_time_str) = target_send_time {
        if let Err(e) = wait_until_target_time(target_time_str).await {
            // Log the error but continue - better to send slightly early than not at all
            error!(
                "Failed to wait for target time: {}. Sending immediately.",
                e
            );
        }
    }

    // Send to all subscribers (with language-specific translations)
    info!("Sending summary via Telegram");
    telegram::send_to_subscribers(config, db, &summary, summary_id).await?;

    info!("✓ Summary job completed successfully");

    Ok(())
}

/// Wait until the target time (HH:MM in Peru time, UTC-5)
async fn wait_until_target_time(target_time_str: &str) -> Result<()> {
    use chrono::{FixedOffset, Utc};

    let parts: Vec<&str> = target_time_str.split(':').collect();
    if parts.len() != 2 {
        anyhow::bail!("Invalid target time format: {}", target_time_str);
    }

    let target_hour: u32 = parts[0].parse()?;
    let target_minute: u32 = parts[1].parse()?;

    // Peru is UTC-5
    let peru_offset = FixedOffset::west_opt(5 * 3600).unwrap();
    let now_utc = Utc::now();
    let now_peru = now_utc.with_timezone(&peru_offset);

    // Build today's target time in Peru timezone
    let today_peru = now_peru.date_naive();
    let target_naive = today_peru
        .and_hms_opt(target_hour, target_minute, 0)
        .unwrap();
    let target_peru = peru_offset
        .from_local_datetime(&target_naive)
        .single()
        .ok_or_else(|| anyhow::anyhow!("Failed to create target datetime"))?;

    // Convert to UTC for comparison
    let target_utc = target_peru.with_timezone(&Utc);

    // Calculate how long to wait
    let wait_duration = target_utc.signed_duration_since(now_utc);

    if wait_duration.num_milliseconds() <= 0 {
        info!(
            "Target time {} already passed, sending immediately",
            target_time_str
        );
        return Ok(());
    }

    let wait_secs = wait_duration.num_seconds();
    info!(
        "Waiting {}s (~{} min) until target send time {} Peru",
        wait_secs,
        wait_secs / 60,
        target_time_str
    );

    tokio::time::sleep(tokio::time::Duration::from_millis(
        wait_duration.num_milliseconds() as u64,
    ))
    .await;

    info!("✓ Target time reached, proceeding to send");
    Ok(())
}

/// Manually trigger a summary job (for /trigger endpoint)
/// Sends immediately without waiting for a target time
pub async fn trigger_summary(config: &Config, db: &Database) -> Result<()> {
    // Read usernames from file
    let usernames_content = std::fs::read_to_string(&config.usernames_file)?;
    let usernames: Vec<String> = usernames_content
        .lines()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect();

    run_summary_job(config, db, &usernames, None).await
}

/// Generate a fresh summary and save to database WITHOUT broadcasting to subscribers.
/// Use this for test endpoints where you want to generate content but only send to a specific user.
pub async fn generate_summary_only(config: &Config, db: &Database) -> Result<String> {
    // Read usernames from file
    let usernames_content = std::fs::read_to_string(&config.usernames_file)?;
    let usernames: Vec<String> = usernames_content
        .lines()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect();

    info!("Starting summary generation (no broadcast)");

    // Fetch tweets
    info!("Fetching tweets from RSS feeds");
    let tweets = rss::fetch_tweets_from_rss(config, &usernames).await?;

    if tweets.is_empty() {
        anyhow::bail!("No tweets found in the specified time window");
    }

    info!("Fetched {} tweets", tweets.len());

    // Generate summary
    info!("Generating summary with OpenAI");
    let client = reqwest::Client::new();
    let summary = openai::summarize_tweets(&client, config, &tweets).await?;

    // Save summary to database
    db.save_summary(&summary).await?;
    info!("✓ Summary generated and saved (not broadcast)");

    Ok(summary)
}

/// Calculate estimated processing time for a given user count (exposed for testing)
pub fn get_estimated_processing_seconds(user_count: usize) -> u32 {
    estimate_processing_seconds(user_count)
}

#[cfg(test)]
mod tests {
    use super::*;

    // ==================== estimate_processing_seconds Tests ====================

    #[test]
    fn test_estimate_processing_seconds_zero_users() {
        let estimate = estimate_processing_seconds(0);
        assert_eq!(estimate, 60); // Just the base buffer
    }

    #[test]
    fn test_estimate_processing_seconds_10_users() {
        let estimate = estimate_processing_seconds(10);
        // 10 * 4 + 60 = 100 seconds
        assert_eq!(estimate, 100);
    }

    #[test]
    fn test_estimate_processing_seconds_50_users() {
        let estimate = estimate_processing_seconds(50);
        // 50 * 4 + 60 = 260 seconds (~4.3 minutes)
        assert_eq!(estimate, 260);
    }

    // ==================== time_to_cron Tests (no offset) ====================

    #[test]
    fn test_time_to_cron_basic() {
        // 08:00 Peru (UTC-5) = 13:00 UTC
        let cron = time_to_cron("08:00", 0).expect("Should parse");
        assert_eq!(cron, "0 0 13 * * *");
    }

    #[test]
    fn test_time_to_cron_afternoon() {
        // 20:00 Peru (UTC-5) = 01:00 UTC (next day)
        let cron = time_to_cron("20:00", 0).expect("Should parse");
        assert_eq!(cron, "0 0 1 * * *");
    }

    #[test]
    fn test_time_to_cron_midnight() {
        // 00:00 Peru (UTC-5) = 05:00 UTC
        let cron = time_to_cron("00:00", 0).expect("Should parse");
        assert_eq!(cron, "0 0 5 * * *");
    }

    #[test]
    fn test_time_to_cron_with_minutes() {
        // 08:30 Peru (UTC-5) = 13:30 UTC
        let cron = time_to_cron("08:30", 0).expect("Should parse");
        assert_eq!(cron, "0 30 13 * * *");
    }

    #[test]
    fn test_time_to_cron_late_night() {
        // 23:00 Peru (UTC-5) = 04:00 UTC (next day)
        let cron = time_to_cron("23:00", 0).expect("Should parse");
        assert_eq!(cron, "0 0 4 * * *");
    }

    #[test]
    fn test_time_to_cron_early_morning() {
        // 06:00 Peru (UTC-5) = 11:00 UTC
        let cron = time_to_cron("06:00", 0).expect("Should parse");
        assert_eq!(cron, "0 0 11 * * *");
    }

    #[test]
    fn test_time_to_cron_hour_wraparound() {
        // 19:00 Peru (UTC-5) = 00:00 UTC (midnight)
        let cron = time_to_cron("19:00", 0).expect("Should parse");
        assert_eq!(cron, "0 0 0 * * *");
    }

    // ==================== time_to_cron with offset Tests ====================

    #[test]
    fn test_time_to_cron_with_60_second_offset() {
        // 08:00 Peru - 60 seconds = 07:59 Peru = 12:59 UTC
        let cron = time_to_cron("08:00", 60).expect("Should parse");
        assert_eq!(cron, "0 59 12 * * *");
    }

    #[test]
    fn test_time_to_cron_with_5_minute_offset() {
        // 08:00 Peru - 300 seconds = 07:55 Peru = 12:55 UTC
        let cron = time_to_cron("08:00", 300).expect("Should parse");
        assert_eq!(cron, "0 55 12 * * *");
    }

    #[test]
    fn test_time_to_cron_offset_crosses_hour_boundary() {
        // 08:00 Peru - 10 minutes (600s) = 07:50 Peru = 12:50 UTC
        let cron = time_to_cron("08:00", 600).expect("Should parse");
        assert_eq!(cron, "0 50 12 * * *");
    }

    #[test]
    fn test_time_to_cron_offset_crosses_midnight_peru() {
        // 00:05 Peru - 10 minutes (600s) = 23:55 Peru (previous day) = 04:55 UTC
        let cron = time_to_cron("00:05", 600).expect("Should parse");
        assert_eq!(cron, "0 55 4 * * *");
    }

    #[test]
    fn test_time_to_cron_large_offset() {
        // 08:00 Peru - 260 seconds (~4.3 min) = 07:55:40 Peru ≈ 07:55 Peru = 12:55 UTC
        let cron = time_to_cron("08:00", 260).expect("Should parse");
        // 260 seconds = 4 minutes 20 seconds, so 08:00 - 4:20 = 07:55:40 → cron uses 07:55
        assert_eq!(cron, "0 55 12 * * *");
    }

    // ==================== Invalid Time Format Tests ====================

    #[test]
    fn test_time_to_cron_invalid_format_no_colon() {
        let result = time_to_cron("0800", 0);
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("Invalid time format"));
    }

    #[test]
    fn test_time_to_cron_invalid_format_too_many_parts() {
        let result = time_to_cron("08:00:00", 0);
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("Invalid time format"));
    }

    #[test]
    fn test_time_to_cron_invalid_hour() {
        // 25 is not a valid hour, NaiveTime::from_hms_opt will return None
        let result = time_to_cron("25:00", 0);
        assert!(result.is_err());
    }

    #[test]
    fn test_time_to_cron_invalid_minute() {
        // 60 is not a valid minute, NaiveTime::from_hms_opt will return None
        let result = time_to_cron("08:60", 0);
        assert!(result.is_err());
    }

    #[test]
    fn test_time_to_cron_non_numeric_hour() {
        let result = time_to_cron("ab:00", 0);
        assert!(result.is_err());
    }

    #[test]
    fn test_time_to_cron_non_numeric_minute() {
        let result = time_to_cron("08:cd", 0);
        assert!(result.is_err());
    }

    #[test]
    fn test_time_to_cron_empty_string() {
        let result = time_to_cron("", 0);
        assert!(result.is_err());
    }

    #[test]
    fn test_time_to_cron_only_colon() {
        let result = time_to_cron(":", 0);
        assert!(result.is_err());
    }

    // ==================== Cron Format Verification Tests ====================

    #[test]
    fn test_cron_format_structure() {
        let cron = time_to_cron("12:30", 0).expect("Should parse");

        // Cron format: "second minute hour day month day_of_week"
        let parts: Vec<&str> = cron.split_whitespace().collect();
        assert_eq!(parts.len(), 6, "Cron should have 6 parts");

        assert_eq!(parts[0], "0", "Seconds should be 0");
        assert_eq!(parts[1], "30", "Minutes should be 30");
        // Hour: 12 + 5 = 17
        assert_eq!(parts[2], "17", "Hour should be 17 (12 Peru + 5 UTC offset)");
        assert_eq!(parts[3], "*", "Day should be *");
        assert_eq!(parts[4], "*", "Month should be *");
        assert_eq!(parts[5], "*", "Day of week should be *");
    }

    // ==================== Usernames Parsing Tests ====================

    #[test]
    fn test_usernames_parsing_basic() {
        let content = "user1\nuser2\nuser3";
        let usernames: Vec<String> = content
            .lines()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();

        assert_eq!(usernames.len(), 3);
        assert_eq!(usernames[0], "user1");
        assert_eq!(usernames[1], "user2");
        assert_eq!(usernames[2], "user3");
    }

    #[test]
    fn test_usernames_parsing_with_whitespace() {
        let content = "  user1  \n  user2  \n  user3  ";
        let usernames: Vec<String> = content
            .lines()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();

        assert_eq!(usernames.len(), 3);
        assert_eq!(usernames[0], "user1");
        assert_eq!(usernames[1], "user2");
        assert_eq!(usernames[2], "user3");
    }

    #[test]
    fn test_usernames_parsing_empty_lines() {
        let content = "user1\n\nuser2\n\n\nuser3\n";
        let usernames: Vec<String> = content
            .lines()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();

        assert_eq!(usernames.len(), 3);
    }

    #[test]
    fn test_usernames_parsing_only_whitespace() {
        let content = "   \n\n  \n";
        let usernames: Vec<String> = content
            .lines()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();

        assert!(usernames.is_empty());
    }

    #[test]
    fn test_usernames_parsing_single_user() {
        let content = "singleuser";
        let usernames: Vec<String> = content
            .lines()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();

        assert_eq!(usernames.len(), 1);
        assert_eq!(usernames[0], "singleuser");
    }

    #[test]
    fn test_usernames_parsing_empty_file() {
        let content = "";
        let usernames: Vec<String> = content
            .lines()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();

        assert!(usernames.is_empty());
    }

    #[test]
    fn test_usernames_parsing_windows_line_endings() {
        let content = "user1\r\nuser2\r\nuser3";
        let usernames: Vec<String> = content
            .lines()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();

        assert_eq!(usernames.len(), 3);
        // trim() should handle \r
        assert_eq!(usernames[0], "user1");
        assert_eq!(usernames[1], "user2");
        assert_eq!(usernames[2], "user3");
    }

    // ==================== UTC Conversion Tests ====================

    #[test]
    fn test_utc_conversion_table() {
        // Peru is UTC-5, so we add 5 hours and wrap with modulo 24
        let test_cases = vec![
            ("00:00", 5),  // 00:00 Peru = 05:00 UTC
            ("06:00", 11), // 06:00 Peru = 11:00 UTC
            ("08:00", 13), // 08:00 Peru = 13:00 UTC
            ("12:00", 17), // 12:00 Peru = 17:00 UTC
            ("18:00", 23), // 18:00 Peru = 23:00 UTC
            ("19:00", 0),  // 19:00 Peru = 00:00 UTC
            ("20:00", 1),  // 20:00 Peru = 01:00 UTC
            ("23:00", 4),  // 23:00 Peru = 04:00 UTC
        ];

        for (peru_time, expected_utc_hour) in test_cases {
            let cron = time_to_cron(peru_time, 0).expect("Should parse");
            let parts: Vec<&str> = cron.split_whitespace().collect();
            let actual_hour: u8 = parts[2].parse().expect("Should parse hour");

            assert_eq!(
                actual_hour, expected_utc_hour,
                "Peru {} should be UTC {} (got {})",
                peru_time, expected_utc_hour, actual_hour
            );
        }
    }

    // ==================== Schedule Times Tests ====================

    #[test]
    fn test_multiple_schedule_times() {
        let schedule_times = vec!["08:00".to_string(), "20:00".to_string()];

        for time in &schedule_times {
            let result = time_to_cron(time, 0);
            assert!(result.is_ok(), "Should parse {}", time);
        }
    }

    #[test]
    fn test_schedule_times_default() {
        let default_times = "08:00,20:00";
        let times: Vec<String> = default_times
            .split(',')
            .map(|s| s.trim().to_string())
            .collect();

        assert_eq!(times.len(), 2);
        assert_eq!(times[0], "08:00");
        assert_eq!(times[1], "20:00");
    }

    #[test]
    fn test_schedule_times_with_spaces() {
        let times_str = " 08:00 , 12:00 , 20:00 ";
        let times: Vec<String> = times_str.split(',').map(|s| s.trim().to_string()).collect();

        assert_eq!(times.len(), 3);
        assert_eq!(times[0], "08:00");
        assert_eq!(times[1], "12:00");
        assert_eq!(times[2], "20:00");
    }

    // ==================== Edge Cases ====================

    #[test]
    fn test_single_digit_hour() {
        let cron = time_to_cron("8:00", 0).expect("Should parse");
        // 8 + 5 = 13
        assert!(cron.contains("13"));
    }

    #[test]
    fn test_single_digit_minute() {
        let cron = time_to_cron("08:5", 0).expect("Should parse");
        // Should contain "5" for minutes
        let parts: Vec<&str> = cron.split_whitespace().collect();
        assert_eq!(parts[1], "5");
    }

    #[test]
    fn test_padded_zeros() {
        let cron = time_to_cron("08:00", 0).expect("Should parse");
        let parts: Vec<&str> = cron.split_whitespace().collect();
        assert_eq!(parts[1], "0", "Minutes should be 0");
        assert_eq!(parts[2], "13", "Hour should be 13");
    }

    // ==================== Additional estimate_processing_seconds Tests ====================

    #[test]
    fn test_estimate_processing_seconds_single_user() {
        let estimate = estimate_processing_seconds(1);
        // 1 * 4 + 60 = 64 seconds
        assert_eq!(estimate, 64);
    }

    #[test]
    fn test_estimate_processing_seconds_100_users() {
        let estimate = estimate_processing_seconds(100);
        // 100 * 4 + 60 = 460 seconds (~7.7 minutes)
        assert_eq!(estimate, 460);
    }

    #[test]
    fn test_estimate_processing_seconds_500_users() {
        let estimate = estimate_processing_seconds(500);
        // 500 * 4 + 60 = 2060 seconds (~34 minutes)
        assert_eq!(estimate, 2060);
    }

    #[test]
    fn test_estimate_processing_seconds_1000_users() {
        let estimate = estimate_processing_seconds(1000);
        // 1000 * 4 + 60 = 4060 seconds (~67 minutes)
        assert_eq!(estimate, 4060);
    }

    #[test]
    fn test_estimate_processing_seconds_formula() {
        // Verify the formula: user_count * 4 + 60
        for user_count in [0, 1, 5, 10, 25, 50, 100, 250, 500] {
            let estimate = estimate_processing_seconds(user_count);
            let expected = (user_count as u32 * 4) + 60;
            assert_eq!(
                estimate, expected,
                "For {} users, expected {} but got {}",
                user_count, expected, estimate
            );
        }
    }

    #[test]
    fn test_estimate_processing_seconds_large_value_no_overflow() {
        // Test that u32 can handle very large user counts without overflow
        // Max u32 is ~4.29 billion, so 100 million users should be safe
        let estimate = estimate_processing_seconds(100_000);
        // 100,000 * 4 + 60 = 400,060 seconds (~111 hours)
        assert_eq!(estimate, 400_060);
    }

    // ==================== Additional time_to_cron with Offset Tests ====================

    #[test]
    fn test_time_to_cron_30_minute_offset() {
        // 08:00 Peru - 30 minutes (1800s) = 07:30 Peru = 12:30 UTC
        let cron = time_to_cron("08:00", 1800).expect("Should parse");
        assert_eq!(cron, "0 30 12 * * *");
    }

    #[test]
    fn test_time_to_cron_1_hour_offset() {
        // 08:00 Peru - 60 minutes (3600s) = 07:00 Peru = 12:00 UTC
        let cron = time_to_cron("08:00", 3600).expect("Should parse");
        assert_eq!(cron, "0 0 12 * * *");
    }

    #[test]
    fn test_time_to_cron_offset_crosses_utc_midnight() {
        // 19:30 Peru - 35 minutes (2100s) = 18:55 Peru
        // 18:55 Peru + 5 hours = 23:55 UTC
        let cron = time_to_cron("19:30", 2100).expect("Should parse");
        assert_eq!(cron, "0 55 23 * * *");
    }

    #[test]
    fn test_time_to_cron_offset_from_midnight_peru() {
        // 00:00 Peru - 5 minutes = 23:55 Peru (previous day) = 04:55 UTC
        let cron = time_to_cron("00:00", 300).expect("Should parse");
        assert_eq!(cron, "0 55 4 * * *");
    }

    #[test]
    fn test_time_to_cron_offset_one_second() {
        // 08:00 Peru - 1 second = 07:59:59 Peru, cron uses minutes so 07:59 = 12:59 UTC
        let cron = time_to_cron("08:00", 1).expect("Should parse");
        assert_eq!(cron, "0 59 12 * * *");
    }

    #[test]
    fn test_time_to_cron_offset_59_seconds() {
        // 08:00 Peru - 59 seconds = 07:59:01 Peru, cron uses minutes so 07:59 = 12:59 UTC
        let cron = time_to_cron("08:00", 59).expect("Should parse");
        assert_eq!(cron, "0 59 12 * * *");
    }

    #[test]
    fn test_time_to_cron_offset_exactly_at_hour() {
        // 08:00 Peru - 120 seconds (2 min) = 07:58 Peru = 12:58 UTC
        let cron = time_to_cron("08:00", 120).expect("Should parse");
        assert_eq!(cron, "0 58 12 * * *");
    }

    #[test]
    fn test_time_to_cron_offset_multiple_hours() {
        // 10:00 Peru - 2 hours (7200s) = 08:00 Peru = 13:00 UTC
        let cron = time_to_cron("10:00", 7200).expect("Should parse");
        assert_eq!(cron, "0 0 13 * * *");
    }

    #[test]
    fn test_time_to_cron_offset_crosses_peru_midnight_from_early_morning() {
        // 01:00 Peru - 2 hours (7200s) = 23:00 Peru (previous day) = 04:00 UTC
        let cron = time_to_cron("01:00", 7200).expect("Should parse");
        assert_eq!(cron, "0 0 4 * * *");
    }

    #[test]
    fn test_time_to_cron_realistic_50_user_offset() {
        // For 50 users: offset = 50 * 4 + 60 = 260 seconds (~4.3 minutes)
        // 20:00 Peru - 260s = 19:55:40 Peru ≈ 19:55 Peru = 00:55 UTC
        let offset = estimate_processing_seconds(50);
        assert_eq!(offset, 260);
        let cron = time_to_cron("20:00", offset).expect("Should parse");
        assert_eq!(cron, "0 55 0 * * *");
    }

    #[test]
    fn test_time_to_cron_realistic_100_user_offset() {
        // For 100 users: offset = 100 * 4 + 60 = 460 seconds (~7.7 minutes)
        // 08:00 Peru - 460s = 07:52:20 Peru ≈ 07:52 Peru = 12:52 UTC
        let offset = estimate_processing_seconds(100);
        assert_eq!(offset, 460);
        let cron = time_to_cron("08:00", offset).expect("Should parse");
        assert_eq!(cron, "0 52 12 * * *");
    }

    // ==================== wait_until_target_time Tests ====================

    // Note: Testing wait_until_target_time is challenging because it involves
    // real time operations. We test the parsing and error handling logic,
    // and use a helper function to extract the validation logic for testing.

    /// Helper to validate time string format (extracted for testing)
    fn validate_time_format(time_str: &str) -> Result<(u32, u32)> {
        let parts: Vec<&str> = time_str.split(':').collect();
        if parts.len() != 2 {
            anyhow::bail!("Invalid target time format: {}", time_str);
        }

        let hour: u32 = parts[0].parse()?;
        let minute: u32 = parts[1].parse()?;

        if hour > 23 {
            anyhow::bail!("Invalid hour: {}", hour);
        }
        if minute > 59 {
            anyhow::bail!("Invalid minute: {}", minute);
        }

        Ok((hour, minute))
    }

    #[test]
    fn test_wait_time_format_valid_basic() {
        let result = validate_time_format("08:00");
        assert!(result.is_ok());
        let (hour, minute) = result.unwrap();
        assert_eq!(hour, 8);
        assert_eq!(minute, 0);
    }

    #[test]
    fn test_wait_time_format_valid_midnight() {
        let result = validate_time_format("00:00");
        assert!(result.is_ok());
        let (hour, minute) = result.unwrap();
        assert_eq!(hour, 0);
        assert_eq!(minute, 0);
    }

    #[test]
    fn test_wait_time_format_valid_end_of_day() {
        let result = validate_time_format("23:59");
        assert!(result.is_ok());
        let (hour, minute) = result.unwrap();
        assert_eq!(hour, 23);
        assert_eq!(minute, 59);
    }

    #[test]
    fn test_wait_time_format_valid_with_leading_zeros() {
        let result = validate_time_format("01:05");
        assert!(result.is_ok());
        let (hour, minute) = result.unwrap();
        assert_eq!(hour, 1);
        assert_eq!(minute, 5);
    }

    #[test]
    fn test_wait_time_format_valid_single_digit() {
        let result = validate_time_format("8:5");
        assert!(result.is_ok());
        let (hour, minute) = result.unwrap();
        assert_eq!(hour, 8);
        assert_eq!(minute, 5);
    }

    #[test]
    fn test_wait_time_format_invalid_no_colon() {
        let result = validate_time_format("0800");
        assert!(result.is_err());
    }

    #[test]
    fn test_wait_time_format_invalid_too_many_colons() {
        let result = validate_time_format("08:00:00");
        assert!(result.is_err());
    }

    #[test]
    fn test_wait_time_format_invalid_empty() {
        let result = validate_time_format("");
        assert!(result.is_err());
    }

    #[test]
    fn test_wait_time_format_invalid_only_colon() {
        let result = validate_time_format(":");
        assert!(result.is_err());
    }

    #[test]
    fn test_wait_time_format_invalid_non_numeric_hour() {
        let result = validate_time_format("ab:00");
        assert!(result.is_err());
    }

    #[test]
    fn test_wait_time_format_invalid_non_numeric_minute() {
        let result = validate_time_format("08:cd");
        assert!(result.is_err());
    }

    #[test]
    fn test_wait_time_format_invalid_hour_too_large() {
        let result = validate_time_format("24:00");
        assert!(result.is_err());
    }

    #[test]
    fn test_wait_time_format_invalid_minute_too_large() {
        let result = validate_time_format("08:60");
        assert!(result.is_err());
    }

    #[test]
    fn test_wait_time_format_invalid_negative() {
        let result = validate_time_format("-1:00");
        assert!(result.is_err());
    }

    #[test]
    fn test_wait_time_format_invalid_whitespace() {
        let result = validate_time_format(" 08:00 ");
        // This will fail because " 08" is not a valid number
        assert!(result.is_err());
    }

    // ==================== Async wait_until_target_time Tests ====================

    // These tests verify the actual wait_until_target_time function behavior

    #[tokio::test]
    async fn test_wait_until_target_time_invalid_format_no_colon() {
        let result = wait_until_target_time("0800").await;
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("Invalid target time format"));
    }

    #[tokio::test]
    async fn test_wait_until_target_time_invalid_format_too_many_parts() {
        let result = wait_until_target_time("08:00:00").await;
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("Invalid target time format"));
    }

    #[tokio::test]
    async fn test_wait_until_target_time_invalid_non_numeric() {
        let result = wait_until_target_time("ab:cd").await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_wait_until_target_time_empty_string() {
        let result = wait_until_target_time("").await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_wait_until_target_time_only_colon() {
        let result = wait_until_target_time(":").await;
        assert!(result.is_err());
    }

    // Test that wait_until_target_time returns Ok when time has passed
    // This uses a time in the past (00:00 will almost always be in the past during a test run
    // unless the test runs exactly at midnight)
    #[tokio::test]
    async fn test_wait_until_target_time_past_time_returns_ok() {
        // Use a time that's very likely to be in the past for most test runs
        // We can't guarantee this 100% but it should work 99.99% of the time
        // Note: This tests the "time already passed" branch
        use chrono::{FixedOffset, Utc};

        let peru_offset = FixedOffset::west_opt(5 * 3600).unwrap();
        let now_peru = Utc::now().with_timezone(&peru_offset);

        // Get a time 2 hours before current Peru time
        let past_hour = if now_peru.hour() >= 2 {
            now_peru.hour() - 2
        } else {
            // If it's before 2 AM Peru time, skip this test scenario
            // by returning early (the test still passes)
            return;
        };

        let past_time = format!("{:02}:00", past_hour);
        let result = wait_until_target_time(&past_time).await;

        // Should return Ok immediately without waiting
        assert!(
            result.is_ok(),
            "Past time should return Ok, got: {:?}",
            result
        );
    }

    // Test valid time format parsing (doesn't actually wait long)
    #[tokio::test]
    async fn test_wait_until_target_time_valid_format_accepted() {
        use chrono::{FixedOffset, Utc};
        use std::time::Instant;

        let peru_offset = FixedOffset::west_opt(5 * 3600).unwrap();
        let now_peru = Utc::now().with_timezone(&peru_offset);

        // Use a time 1 hour in the past to ensure immediate return
        let past_hour = if now_peru.hour() >= 1 {
            now_peru.hour() - 1
        } else {
            23 // wrap to previous day
        };

        let past_time = format!("{:02}:30", past_hour);
        let start = Instant::now();
        let result = wait_until_target_time(&past_time).await;

        // Should return quickly (< 1 second) since time is in past
        assert!(result.is_ok());
        assert!(
            start.elapsed().as_secs() < 2,
            "Should return immediately for past time"
        );
    }

    // ==================== Integration Tests: Offset + Cron ====================

    #[test]
    fn test_estimate_and_cron_integration_8am() {
        // Simulate realistic scenario: 25 users at 08:00 Peru time
        let user_count = 25;
        let offset = estimate_processing_seconds(user_count);
        // 25 * 4 + 60 = 160 seconds (~2.7 minutes)
        assert_eq!(offset, 160);

        // 08:00 Peru - 160s = 07:57:20 Peru ≈ 07:57 Peru = 12:57 UTC
        let cron = time_to_cron("08:00", offset).expect("Should parse");
        assert_eq!(cron, "0 57 12 * * *");
    }

    #[test]
    fn test_estimate_and_cron_integration_8pm() {
        // Simulate realistic scenario: 25 users at 20:00 Peru time
        let user_count = 25;
        let offset = estimate_processing_seconds(user_count);
        assert_eq!(offset, 160);

        // 20:00 Peru - 160s = 19:57:20 Peru ≈ 19:57 Peru = 00:57 UTC
        let cron = time_to_cron("20:00", offset).expect("Should parse");
        assert_eq!(cron, "0 57 0 * * *");
    }

    #[test]
    fn test_estimate_and_cron_edge_case_early_morning() {
        // Edge case: scheduled at 00:30 Peru with 20 users
        let user_count = 20;
        let offset = estimate_processing_seconds(user_count);
        // 20 * 4 + 60 = 140 seconds (~2.3 minutes)
        assert_eq!(offset, 140);

        // 00:30 Peru - 140s = 00:27:40 Peru ≈ 00:27 Peru = 05:27 UTC
        let cron = time_to_cron("00:30", offset).expect("Should parse");
        assert_eq!(cron, "0 27 5 * * *");
    }

    #[test]
    fn test_estimate_and_cron_edge_case_crosses_peru_midnight() {
        // Edge case: scheduled at 00:01 Peru with 10 users (should cross to previous day)
        let user_count = 10;
        let offset = estimate_processing_seconds(user_count);
        // 10 * 4 + 60 = 100 seconds (~1.7 minutes)
        assert_eq!(offset, 100);

        // 00:01 Peru - 100s = 23:59:20 Peru (previous day) ≈ 23:59 Peru = 04:59 UTC
        let cron = time_to_cron("00:01", offset).expect("Should parse");
        assert_eq!(cron, "0 59 4 * * *");
    }

    // ==================== Boundary Value Tests ====================

    #[test]
    fn test_time_boundary_values() {
        // Test all corner times
        let boundary_times = vec![
            ("00:00", 0, "0 0 5 * * *"),   // Midnight Peru = 05:00 UTC
            ("00:59", 0, "0 59 5 * * *"),  // Just before 01:00 Peru
            ("23:00", 0, "0 0 4 * * *"),   // 23:00 Peru = 04:00 UTC
            ("23:59", 0, "0 59 4 * * *"),  // Last minute of day Peru
            ("12:00", 0, "0 0 17 * * *"),  // Noon Peru = 17:00 UTC
            ("18:59", 0, "0 59 23 * * *"), // Just before 19:00 Peru = 23:59 UTC
            ("19:00", 0, "0 0 0 * * *"),   // 19:00 Peru = 00:00 UTC (boundary)
            ("19:01", 0, "0 1 0 * * *"),   // Just after boundary
        ];

        for (peru_time, offset, expected_cron) in boundary_times {
            let cron = time_to_cron(peru_time, offset).expect("Should parse");
            assert_eq!(
                cron, expected_cron,
                "Peru {} with offset {} should produce {} but got {}",
                peru_time, offset, expected_cron, cron
            );
        }
    }

    #[test]
    fn test_offset_boundary_at_hour_change() {
        // Test offset that lands exactly on minute boundaries
        // 08:00 Peru - 60s = 07:59:00 Peru = 12:59 UTC
        let cron = time_to_cron("08:00", 60).expect("Should parse");
        assert_eq!(cron, "0 59 12 * * *");

        // 08:01 Peru - 60s = 08:00:00 Peru = 13:00 UTC
        let cron = time_to_cron("08:01", 60).expect("Should parse");
        assert_eq!(cron, "0 0 13 * * *");
    }

    #[test]
    fn test_zero_offset_produces_exact_time() {
        // Verify that zero offset doesn't change the time
        for hour in 0..24 {
            for minute in [0, 15, 30, 45] {
                let peru_time = format!("{:02}:{:02}", hour, minute);
                let cron = time_to_cron(&peru_time, 0).expect("Should parse");

                let expected_utc_hour = (hour + 5) % 24;
                let expected_cron = format!("0 {} {} * * *", minute, expected_utc_hour);

                assert_eq!(
                    cron, expected_cron,
                    "Peru {} should produce {} but got {}",
                    peru_time, expected_cron, cron
                );
            }
        }
    }
}
