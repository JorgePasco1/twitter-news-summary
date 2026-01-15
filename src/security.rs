use subtle::ConstantTimeEq;

/// Constant-time string comparison to prevent timing attacks
/// Use this for comparing API keys, webhook secrets, and other sensitive values
pub fn constant_time_compare(a: &str, b: &str) -> bool {
    if a.len() != b.len() {
        return false;
    }
    a.as_bytes().ct_eq(b.as_bytes()).into()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_constant_time_compare() {
        assert!(constant_time_compare("secret123", "secret123"));
        assert!(!constant_time_compare("secret123", "secret124"));
        assert!(!constant_time_compare("secret123", "secret12"));
        assert!(!constant_time_compare("", "secret"));
    }
}
