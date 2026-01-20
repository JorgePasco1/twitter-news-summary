# Investigation: Duplicate Messages in Test Environment

**Date**: 2026-01-20
**Environment**: twitter-summary-bot-test
**Issue**: Spanish subscriber received 2 different summary messages at 01:02 UTC

## Summary

The test environment sent two messages with different formats to a Spanish subscriber because **two Fly.io machine instances were running simultaneously**, each with its own independent scheduler.

## Root Cause: Multiple Instances Running Concurrent Schedulers

The test environment was running **2 separate Fly.io machine instances**:
- Instance 1: `3d8de966c31728`
- Instance 2: `83905ea7941268`

Both instances independently started their own scheduler when the application booted:
```rust
let _scheduler = scheduler::start_scheduler(Arc::clone(&config), Arc::clone(&db)).await?;
```

## Timeline of Events

| Time (UTC) | Instance | Action |
|------------|----------|--------|
| 01:02:06 | Both | Cron job triggers (01:00 UTC = 20:00 Peru time) |
| 01:02:30 | Instance 2 | Completes summary, saves to DB (id: 4), sends Spanish message |
| 01:02:32 | Instance 1 | Completes summary, saves to DB (id: 5), sends Spanish message |

## Why Different Message Formats?

The two messages had different formats because:

1. **Instance 2** (first message):
   - Fetched 22 tweets (9 failed)
   - Generated summary with old format: `# 🧠 Investigación y desarrollos en IA`
   - Likely running older code version

2. **Instance 1** (second message):
   - Fetched 35 tweets (0 failed)
   - Generated summary with new format: `🔬 Investigación en IA`
   - Running newer code with Phase 3-4 i18n changes

## Evidence

From Fly.io status:
```text
PROCESS  ID              VERSION  REGION  STATE    LAST UPDATED
app      3d8de966c31728  12       sjc     started  2026-01-19T23:08:19Z
app      83905ea7941268  12       sjc     started  2026-01-19T23:08:36Z
```

## Technical Details

### Current Scheduler Implementation (`src/scheduler.rs`)

The scheduler has **NO deduplication mechanism**:
- Each instance creates its own independent `JobScheduler`
- No database lock mechanism
- No leader election
- No distributed deduplication logic

```rust
pub async fn start_scheduler(config: Arc<Config>, db: Arc<Database>) -> Result<JobScheduler> {
    let scheduler = JobScheduler::new().await?;
    // Creates jobs for each time in schedule_times
    scheduler.add(job).await?;
    scheduler.start().await?;
    Ok(scheduler)
}
```

## What Needs to Be Fixed

### Option 1: Ensure Single Instance (Recommended for simplicity)
- Set `min_machines_running = 1` and `max_machines_running = 1` in `fly.toml`
- Prevents multiple instances from running simultaneously

### Option 2: Add Distributed Lock (More robust)
- Before running a scheduled job, acquire a PostgreSQL advisory lock
- Only the instance that acquires the lock executes the job
- Other instances skip the job

### Option 3: Leader Election
- Implement leader election using PostgreSQL or Redis
- Only the leader instance runs scheduled jobs

## Immediate Action Required

1. Check `fly.toml` configuration for both environments
2. Ensure only 1 machine is configured to run at a time
3. Consider adding a database lock for scheduled jobs as a safety measure

## Related Files

- `src/scheduler.rs` - Scheduler implementation
- `fly.toml` - Fly.io deployment configuration
- `src/main.rs` - Scheduler initialization (line 79)
