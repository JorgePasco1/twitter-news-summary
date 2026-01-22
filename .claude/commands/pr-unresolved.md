# Get Unresolved PR Comments

Get all unresolved review comments from a specific pull request using the GitHub CLI.

## Usage

```
/pr-unresolved <pr-number>
```

## What it does

Uses the GitHub CLI (`gh`) to fetch all review threads from a PR, filtering for only unresolved comments.

## ⚠️ CRITICAL SAFETY REQUIREMENT

**NEVER post replies to PR comments without first:**
1. Showing a verification table mapping Thread ID → Comment ID → File:Line → Topic
2. Getting explicit user confirmation with "CONFIRM" or similar

This prevents posting replies to wrong comments, which cannot be easily undone.

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
                 id
                 databaseId
                 path
                 position
                 originalPosition
                 line
                 originalLine
                 body
                 diffHunk
                 author {
                   login
                 }
               }
             }
           }
         }
       }
     }
   }' -F owner="{owner}" -F repo="{repo}" -F pr={pr_number}
   ```

   **IMPORTANT**: The `databaseId` field (numeric) is what you use for `-F in_reply_to=` when posting replies. The `id` field (starts with `PRRC_`) is NOT used for replies.

4. Filter the results to show only threads where `isResolved: false`

5. Display the results in a clear format showing:
   - Total number of unresolved review threads
   - For each unresolved thread:
     - Thread number (for reference)
     - Thread ID (starts with `PRRT_`) - used for resolving
     - Comment ID / databaseId (numeric) - used for posting replies
     - File path and line number
     - First 50-100 chars of comment body as topic summary
     - Whether it's outdated or collapsed

   **Example format:**
   ```
   ## Thread 1
   - Thread ID: PRRT_kwDOQ4vS9s5qff_4
   - Comment ID: 2715647849
   - File: src/bin/experiment.rs:26
   - Topic: "Documentation mismatch..."
   - Status: Active
   ```

6. Evaluate the comments and give a justified decision on whether we should fix each one or not.

7. **Create a clear mapping table** (see "After Fixing/Skipping Comments" section) showing Thread ID → Comment ID → File:Line → Topic before asking for confirmation.

8. Ask the user to confirm which ones they want to fix.

## After Fixing/Skipping Comments

After the user confirms which comments to fix and you've completed the work:

### CRITICAL: Verification Before Posting Replies

**NEVER post replies without first showing this verification table and getting user confirmation:**

1. **Create a verification mapping table** from the GraphQL data:
   ```
   | Thread # | Thread ID | Comment ID (databaseId) | File:Line | Topic Summary | Action | Reply Message |
   |----------|-----------|-------------------------|-----------|---------------|--------|---------------|
   | 1 | PRRT_kwD... | 271564... | experiment.rs:26 | Doc mismatch | Fix | "Fixed! Added .context()..." |
   | 2 | PRRT_kwD... | 271564... | experiment.rs:37 | Temperature logic | Fix | "Fixed! MODELS includes..." |
   ```

2. **Extract the correct IDs from GraphQL response**:
   - Thread ID: `reviewThreads.nodes[].id` (starts with `PRRT_`)
   - Comment ID for replies: `reviewThreads.nodes[].comments.nodes[0].databaseId` (numeric)
   - File:Line: `reviewThreads.nodes[].comments.nodes[0].path:line`
   - Topic: First 20 chars of comment body

3. **Show the verification table to the user** with this exact format:
   ```markdown
   ## ⚠️ VERIFICATION: Reply Mapping Before Posting

   Please confirm this mapping is correct before I post replies:

   | Thread | Comment ID | File:Line | Topic | Reply Preview |
   |--------|------------|-----------|-------|---------------|
   | 1 | 2715647849 | experiment.rs:26 | Documentation mismatch | "Fixed! Added .context()..." |
   | 2 | 2715647855 | experiment.rs:37 | Temperature logic | "Fixed! MODELS includes..." |

   Reply with "CONFIRM" to proceed, or point out any issues.
   ```

4. **WAIT for user confirmation** - DO NOT proceed until user responds with "CONFIRM" or similar

### Posting Replies (Only After Confirmation)

5. **Commit changes first**:
   ```bash
   git add <files>
   git commit -m "fix: address PR review comments"
   git push
   ```

6. **Reply to each comment** using the VERIFIED comment ID:
   ```bash
   gh api repos/{owner}/{repo}/pulls/{pr_number}/comments \
     -f body="Fixed! <brief description of what was done>" \
     -F in_reply_to={comment_id} \
     -X POST
   ```

7. **React to comments** to indicate status:
   - For comments being fixed: Add a 👍 reaction
     ```bash
     gh api repos/{owner}/{repo}/pulls/comments/{comment_id}/reactions \
       -f content="+1" -X POST
     ```
   - For comments being skipped: Add a 👎 reaction and explain why in the reply
     ```bash
     gh api repos/{owner}/{repo}/pulls/comments/{comment_id}/reactions \
       -f content="-1" -X POST
     ```

8. **Resolve threads** using the GraphQL API (for both fixed and skipped comments):
   ```bash
   gh api graphql -f query='
   mutation {
     resolveReviewThread(input: {threadId: "{thread_node_id}"}) {
       thread { isResolved }
     }
   }'
   ```

### Important Notes

- **ALWAYS use `databaseId` (numeric) for comment replies**, NOT the GraphQL `id` field
- **Thread ID** (starts with `PRRT_`) is only for resolving threads, NOT for posting replies
- **Verify file:line matches the topic** before posting to catch any ID confusion
- If you're unsure about ANY mapping, STOP and ask the user to verify

## Example Output

```
Found 3 unresolved review threads on PR#4:

## Thread 1
- Thread ID: PRRT_kwDOQ4vS9s5abc123
- Comment ID: 2715647849
- File: src/rss.rs:205
- Topic: "Consider adding error handling for RSS parse failures..."
- Status: Active (not outdated)

**Full Comment:**
Consider adding error handling for RSS parse failures...
[full comment body]

## Thread 2
- Thread ID: PRRT_kwDOQ4vS9s5def456
- Comment ID: 2715647855
- File: src/openai.rs:393
- Topic: "The token limit might need adjustment for longer summaries..."
- Status: Active (not outdated)

**Full Comment:**
The token limit might need adjustment for longer summaries...
[full comment body]

## Thread 3
- Thread ID: PRRT_kwDOQ4vS9s5ghi789
- Comment ID: 2715647858
- File: src/telegram.rs:458
- Topic: "Add retry logic for failed Telegram sends..."
- Status: Active (not outdated)

**Full Comment:**
Add retry logic for failed Telegram sends...
[full comment body]

---

**My Evaluation:**
- Thread 1: Should fix - error handling is important
- Thread 2: Should fix - affects user experience
- Thread 3: Should fix - reliability improvement

Would you like me to fix these issues?

**After you confirm, I will show you a verification table mapping Thread IDs → Comment IDs → Topics before posting any replies.**
```

## Notes

- Uses GitHub CLI which requires `gh auth login` to be run first
- Returns grouped review threads (not individual comments)
- Provides metadata like `isOutdated` and `isCollapsed` for context
- Supports any GitHub repository via environment variables
