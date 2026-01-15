# Security Guidelines

This document outlines security best practices for the Twitter News Summary application.

## Critical Security Requirements

### 1. Telegram Webhook Secret (CRITICAL)

**Problem**: Without webhook authentication, anyone who discovers your webhook URL can send fake requests to:
- Subscribe/unsubscribe users
- Trigger your OpenAI API (costing you money)
- Abuse your service

**Solution**: Use Telegram's webhook secret token verification.

#### Setup

1. **Generate a secure secret**:
   ```bash
   ./scripts/generate-secrets.sh
   ```

2. **Add to your environment**:
   - Local: Add `TELEGRAM_WEBHOOK_SECRET=...` to `.env`
   - GitHub Actions: Add as secret in Settings → Secrets → prod environment

3. **Configure Telegram webhook**:
   ```bash
   curl -X POST "https://api.telegram.org/bot$TELEGRAM_BOT_TOKEN/setWebhook" \
     -H "Content-Type: application/json" \
     -d '{
       "url": "https://your-webhook-url/webhook",
       "secret_token": "YOUR_TELEGRAM_WEBHOOK_SECRET"
     }'
   ```

4. **Verify in your code**:
   When handling webhook requests, verify the `X-Telegram-Bot-Api-Secret-Token` header matches your secret using **constant-time comparison**:

   ```rust
   use crate::security::constant_time_compare;

   // In webhook handler
   let expected_secret = config.telegram_webhook_secret;
   let provided_secret = headers.get("X-Telegram-Bot-Api-Secret-Token");

   if !constant_time_compare(provided_secret, &expected_secret) {
       return Err("Unauthorized");
   }
   ```

### 2. Constant-Time Comparison

**Problem**: Using `==` for secret comparison leaks timing information that attackers can use to guess secrets.

**Solution**: Always use `security::constant_time_compare()` for comparing:
- Webhook secrets
- API keys
- Any sensitive tokens

```rust
// ❌ WRONG - Vulnerable to timing attacks
if api_key == expected_key {
    // ...
}

// ✅ CORRECT - Constant-time comparison
use crate::security::constant_time_compare;
if constant_time_compare(&api_key, &expected_key) {
    // ...
}
```

### 3. Input Validation

Always validate webhook inputs:

```rust
// Validate update_id
if update.update_id <= 0 {
    return Err("Invalid update_id");
}

// Validate text length (Telegram max is 4096)
if message.text.len() > 4096 {
    return Err("Message too long");
}

// Sanitize usernames (alphanumeric + underscore only)
let username = username.chars()
    .filter(|c| c.is_alphanumeric() || *c == '_')
    .take(32)
    .collect::<String>();
```

### 4. Error Handling

**Never expose internal details in error responses**:

```rust
// ❌ WRONG - Leaks internal information
Err(e) => {
    return Response::new(format!("Error: {}", e));
}

// ✅ CORRECT - Generic error to client, details in logs only
Err(e) => {
    tracing::error!("Webhook processing failed: {}", e);
    return Response::new("Internal server error");
}
```

### 5. Logging Sensitive Data

**Never log**:
- Full API keys or secrets
- User passwords or tokens
- Full message content (unless necessary for debugging)

**Safe logging**:
```rust
// ❌ WRONG
tracing::info!("API key: {}", api_key);

// ✅ CORRECT
tracing::info!("API key configured: {}", api_key.is_some());

// ❌ WRONG
tracing::info!("Message: {}", message.text);

// ✅ CORRECT (for commands only)
let command = message.text.split_whitespace().next();
tracing::info!("Received command: {:?}", command);
```

## Security Checklist

Before deploying:

- [ ] `TELEGRAM_WEBHOOK_SECRET` is set (min 32 bytes, preferably 64 hex chars)
- [ ] Telegram webhook configured with `secret_token`
- [ ] All secret comparisons use `constant_time_compare()`
- [ ] Input validation implemented for all webhook payloads
- [ ] Error messages don't leak internal details
- [ ] Logs don't contain secrets or sensitive user data
- [ ] `.env` file is in `.gitignore` and never committed

## Incident Response

If you suspect your webhook is compromised:

1. **Rotate the secret immediately**:
   ```bash
   # Generate new secret
   ./scripts/generate-secrets.sh

   # Update in all environments
   # Then reconfigure Telegram webhook
   ```

2. **Check logs** for suspicious activity:
   - Failed webhook authentication attempts
   - Unusual subscription patterns
   - Unexpected API usage

3. **Review subscribers** for any suspicious entries

## Resources

- [Telegram Bot API Security](https://core.telegram.org/bots/api#setwebhook)
- [OWASP Secure Coding Practices](https://owasp.org/www-project-secure-coding-practices-quick-reference-guide/)
- [Rust Security Guidelines](https://anssi-fr.github.io/rust-guide/)

## Reporting Security Issues

If you discover a security vulnerability, please report it privately:
- Do not open a public issue
- Contact the maintainers directly
- Allow time for a fix before public disclosure
