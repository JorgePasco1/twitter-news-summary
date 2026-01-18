use crate::config::Config;
use crate::db::Database;
use crate::openai;
use crate::rss;
use crate::telegram;
use anyhow::Result;
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

    // Convert FROM Peru time (UTC-5) TO UTC
    // For example: 08:00 Peru becomes 13:00 UTC (add 5 hours)
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
    let client = reqwest::Client::new();
    let summary = openai::summarize_tweets(&client, config, &tweets).await?;

    // Save summary to database and get the ID for translation caching
    let summary_id = db.save_summary(&summary).await?;
    info!("✓ Summary saved to database (id: {})", summary_id);

    // Send to all subscribers (with language-specific translations)
    info!("Sending summary via Telegram");
    telegram::send_to_subscribers(config, db, &summary, summary_id).await?;

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

#[cfg(test)]
mod tests {
    use super::*;

    // ==================== time_to_cron Tests ====================

    #[test]
    fn test_time_to_cron_basic() {
        // 08:00 Peru (UTC-5) = 13:00 UTC
        let cron = time_to_cron("08:00").expect("Should parse");
        assert_eq!(cron, "0 0 13 * * *");
    }

    #[test]
    fn test_time_to_cron_afternoon() {
        // 20:00 Peru (UTC-5) = 01:00 UTC (next day)
        let cron = time_to_cron("20:00").expect("Should parse");
        assert_eq!(cron, "0 0 1 * * *");
    }

    #[test]
    fn test_time_to_cron_midnight() {
        // 00:00 Peru (UTC-5) = 05:00 UTC
        let cron = time_to_cron("00:00").expect("Should parse");
        assert_eq!(cron, "0 0 5 * * *");
    }

    #[test]
    fn test_time_to_cron_with_minutes() {
        // 08:30 Peru (UTC-5) = 13:30 UTC
        let cron = time_to_cron("08:30").expect("Should parse");
        assert_eq!(cron, "0 30 13 * * *");
    }

    #[test]
    fn test_time_to_cron_late_night() {
        // 23:00 Peru (UTC-5) = 04:00 UTC (next day)
        let cron = time_to_cron("23:00").expect("Should parse");
        assert_eq!(cron, "0 0 4 * * *");
    }

    #[test]
    fn test_time_to_cron_early_morning() {
        // 06:00 Peru (UTC-5) = 11:00 UTC
        let cron = time_to_cron("06:00").expect("Should parse");
        assert_eq!(cron, "0 0 11 * * *");
    }

    #[test]
    fn test_time_to_cron_hour_wraparound() {
        // 19:00 Peru (UTC-5) = 00:00 UTC (midnight)
        let cron = time_to_cron("19:00").expect("Should parse");
        assert_eq!(cron, "0 0 0 * * *");
    }

    // ==================== Invalid Time Format Tests ====================

    #[test]
    fn test_time_to_cron_invalid_format_no_colon() {
        let result = time_to_cron("0800");
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("Invalid time format"));
    }

    #[test]
    fn test_time_to_cron_invalid_format_too_many_parts() {
        let result = time_to_cron("08:00:00");
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("Invalid time format"));
    }

    #[test]
    fn test_time_to_cron_invalid_hour() {
        let result = time_to_cron("25:00");
        // This will wrap around due to modulo, but the parse will succeed
        // 25 + 5 = 30, 30 % 24 = 6
        let cron = result.expect("Should parse (wraps)");
        assert!(cron.contains("6"));
    }

    #[test]
    fn test_time_to_cron_invalid_minute() {
        let result = time_to_cron("08:60");
        // This will parse but might not be valid cron
        // The function doesn't validate minute range
        let cron = result.expect("Should parse");
        assert!(cron.contains("60")); // Invalid but accepted
    }

    #[test]
    fn test_time_to_cron_non_numeric_hour() {
        let result = time_to_cron("ab:00");
        assert!(result.is_err());
    }

    #[test]
    fn test_time_to_cron_non_numeric_minute() {
        let result = time_to_cron("08:cd");
        assert!(result.is_err());
    }

    #[test]
    fn test_time_to_cron_empty_string() {
        let result = time_to_cron("");
        assert!(result.is_err());
    }

    #[test]
    fn test_time_to_cron_only_colon() {
        let result = time_to_cron(":");
        assert!(result.is_err());
    }

    // ==================== Cron Format Verification Tests ====================

    #[test]
    fn test_cron_format_structure() {
        let cron = time_to_cron("12:30").expect("Should parse");

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
            let cron = time_to_cron(peru_time).expect("Should parse");
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
            let result = time_to_cron(time);
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
        let cron = time_to_cron("8:00").expect("Should parse");
        // 8 + 5 = 13
        assert!(cron.contains("13"));
    }

    #[test]
    fn test_single_digit_minute() {
        let cron = time_to_cron("08:5").expect("Should parse");
        // Should contain "5" for minutes
        let parts: Vec<&str> = cron.split_whitespace().collect();
        assert_eq!(parts[1], "5");
    }

    #[test]
    fn test_padded_zeros() {
        let cron = time_to_cron("08:00").expect("Should parse");
        let parts: Vec<&str> = cron.split_whitespace().collect();
        assert_eq!(parts[1], "0", "Minutes should be 0");
        assert_eq!(parts[2], "13", "Hour should be 13");
    }
}
