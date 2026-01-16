# Get Unresolved PR Comments

Get all unresolved review comments from a specific pull request using the GitHub CLI.

## Usage

```
/pr-unresolved <pr-number>
```

## What it does

Uses the GitHub CLI (`gh`) to fetch all review threads from a PR, filtering for only unresolved comments.

## Configuration

The skill uses these environment variables (optional):
- `GITHUB_OWNER`: GitHub repository owner (defaults to "JorgePasco1")
- `GITHUB_REPO`: GitHub repository name (defaults to "twitter-news-summary")

## Instructions

When the user runs this command with a PR number:

1. Get the owner and repo from environment variables, or use defaults:
   - owner: `$GITHUB_OWNER` or "JorgePasco1"
   - repo: `$GITHUB_REPO` or "twitter-news-summary"

2. Use the `gh` CLI to fetch PR review comments:
   ```bash
   gh api repos/{owner}/{repo}/pulls/{pr_number}/comments
   ```

3. Also fetch the review threads to check resolution status:
   ```bash
   gh api graphql -f query='
   query($owner: String!, $repo: String!, $pr: Int!) {
     repository(owner: $owner, name: $repo) {
       pullRequest(number: $pr) {
         reviewThreads(first: 100) {
           nodes {
             id
             isResolved
             isOutdated
             isCollapsed
             comments(first: 10) {
               nodes {
                 path
                 position
                 originalPosition
                 line
                 originalLine
                 body
                 diffHunk
               }
             }
           }
         }
       }
     }
   }' -F owner="{owner}" -F repo="{repo}" -F pr={pr_number}
   ```

4. Filter the results to show only threads where `isResolved: false`

5. Display the results in a clear format showing:
   - Total number of unresolved review threads
   - For each unresolved thread:
     - File path
     - Line number
     - Full comment body from the thread
     - Whether it's outdated or collapsed

6. Evaluate the comments and give a justified decision on wether we should fix each one or not.
7. Ask the user to confirm which ones they want to fix.

## Example Output

```
Found 3 unresolved review threads on PR#4:

## Thread 1: Line 205
**File**: src/rss.rs
**Status**: Active (not outdated)
**Comment**:
Consider adding error handling for RSS parse failures...

## Thread 2: Line 393
**File**: src/openai.rs
**Status**: Active (not outdated)
**Comment**:
The token limit might need adjustment for longer summaries...

## Thread 3: Line 458
**File**: src/telegram.rs
**Status**: Active (not outdated)
**Comment**:
Add retry logic for failed Telegram sends...

Would you like me to fix these issues?
```

## Notes

- Uses GitHub CLI which requires `gh auth login` to be run first
- Returns grouped review threads (not individual comments)
- Provides metadata like `isOutdated` and `isCollapsed` for context
- Supports any GitHub repository via environment variables
