//! Utility functions for CCA
//!
//! Provides safe string manipulation, constant-time comparison, and environment loading.

use std::path::Path;
use subtle::ConstantTimeEq;

/// Safely truncate a string at character boundaries (not byte boundaries).
/// This prevents panics when truncating multi-byte UTF-8 characters.
///
/// # Arguments
/// * `s` - The string to truncate
/// * `max_chars` - Maximum number of characters to keep
///
/// # Returns
/// A string slice containing at most `max_chars` characters
///
/// # Example
/// ```
/// use cca_core::util::safe_truncate;
///
/// let s = "Hello, world!";
/// assert_eq!(safe_truncate(s, 5), "Hello");
///
/// // Works safely with multi-byte UTF-8 characters
/// let emoji = "Hello 🌍🌎🌏";
/// assert_eq!(safe_truncate(emoji, 8), "Hello 🌍🌎");
/// ```
#[inline]
pub fn safe_truncate(s: &str, max_chars: usize) -> &str {
    match s.char_indices().nth(max_chars) {
        Some((idx, _)) => &s[..idx],
        None => s,
    }
}

/// Safely truncate a string and add ellipsis if truncated.
/// Returns an owned String since we may need to append "...".
///
/// # Arguments
/// * `s` - The string to truncate
/// * `max_chars` - Maximum number of characters before adding ellipsis
///
/// # Example
/// ```
/// use cca_core::util::safe_truncate_with_ellipsis;
///
/// let s = "Hello, world!";
/// assert_eq!(safe_truncate_with_ellipsis(s, 5), "Hello...");
/// assert_eq!(safe_truncate_with_ellipsis("Hi", 5), "Hi");
/// ```
pub fn safe_truncate_with_ellipsis(s: &str, max_chars: usize) -> String {
    let char_count = s.chars().count();
    if char_count <= max_chars {
        s.to_string()
    } else {
        format!("{}...", safe_truncate(s, max_chars))
    }
}

/// Perform constant-time comparison of two strings.
/// This prevents timing attacks when comparing secrets like API keys.
///
/// # Arguments
/// * `a` - First string
/// * `b` - Second string
///
/// # Returns
/// `true` if the strings are equal, `false` otherwise
///
/// # Security
/// This function takes constant time regardless of where strings differ,
/// preventing attackers from learning partial information about secrets
/// through timing analysis.
///
/// # Example
/// ```
/// use cca_core::util::constant_time_eq;
///
/// assert!(constant_time_eq("secret", "secret"));
/// assert!(!constant_time_eq("secret", "secre"));
/// assert!(!constant_time_eq("secret", "SECRET"));
/// ```
pub fn constant_time_eq(a: &str, b: &str) -> bool {
    // First check lengths to avoid leaking timing through length comparison
    // Both strings must be same length for constant-time comparison
    if a.len() != b.len() {
        return false;
    }
    a.as_bytes().ct_eq(b.as_bytes()).into()
}

/// Load environment variables from CCA env file if not already set.
/// Searches standard locations in order:
/// 1. /usr/local/etc/cca/cca.env
/// 2. ~/.config/cca/cca.env
/// 3. User's config directory/cca/cca.env
pub fn load_env_file() {
    let env_paths = [
        "/usr/local/etc/cca/cca.env".to_string(),
        dirs::config_dir()
            .map(|p| p.join("cca/cca.env").to_string_lossy().to_string())
            .unwrap_or_default(),
        dirs::home_dir()
            .map(|p| p.join(".config/cca/cca.env").to_string_lossy().to_string())
            .unwrap_or_default(),
    ];

    for path in &env_paths {
        if path.is_empty() {
            continue;
        }
        if Path::new(path).exists() {
            if let Ok(contents) = std::fs::read_to_string(path) {
                parse_env_file(&contents);
            }
            break;
        }
    }
}

/// Parse env file contents and set environment variables (only if not already set).
/// Supports formats:
/// - `KEY=value`
/// - `export KEY=value`
/// - `KEY="quoted value"`
/// - `KEY='single quoted'`
/// - Comments starting with #
pub fn parse_env_file(contents: &str) {
    for line in contents.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let line = line.strip_prefix("export ").unwrap_or(line);
        if let Some((key, value)) = line.split_once('=') {
            let key = key.trim();
            let value = value.trim().trim_matches('"').trim_matches('\'');
            if std::env::var(key).is_err() {
                std::env::set_var(key, value);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_safe_truncate_ascii() {
        assert_eq!(safe_truncate("hello world", 5), "hello");
        assert_eq!(safe_truncate("hi", 5), "hi");
        assert_eq!(safe_truncate("", 5), "");
    }

    #[test]
    fn test_safe_truncate_unicode() {
        // Multi-byte UTF-8 characters
        assert_eq!(safe_truncate("日本語テスト", 3), "日本語");
        assert_eq!(safe_truncate("🎉🎊🎈", 2), "🎉🎊");

        // Mixed ASCII and Unicode
        assert_eq!(safe_truncate("Hello 世界", 7), "Hello 世");
    }

    #[test]
    fn test_safe_truncate_with_ellipsis() {
        assert_eq!(safe_truncate_with_ellipsis("hello world", 5), "hello...");
        assert_eq!(safe_truncate_with_ellipsis("hi", 5), "hi");
        assert_eq!(safe_truncate_with_ellipsis("hello", 5), "hello");
    }

    #[test]
    fn test_constant_time_eq() {
        assert!(constant_time_eq("secret", "secret"));
        assert!(!constant_time_eq("secret", "secre"));
        assert!(!constant_time_eq("secret", "SECRET"));
        assert!(!constant_time_eq("", "a"));
        assert!(constant_time_eq("", ""));
    }

    #[test]
    fn test_parse_env_file() {
        // Clear any existing test vars
        std::env::remove_var("TEST_CCA_VAR1");
        std::env::remove_var("TEST_CCA_VAR2");
        std::env::remove_var("TEST_CCA_VAR3");

        let contents = r#"
            # This is a comment
            TEST_CCA_VAR1=value1
            export TEST_CCA_VAR2="quoted value"
            TEST_CCA_VAR3='single quoted'
        "#;

        parse_env_file(contents);

        assert_eq!(std::env::var("TEST_CCA_VAR1").unwrap(), "value1");
        assert_eq!(std::env::var("TEST_CCA_VAR2").unwrap(), "quoted value");
        assert_eq!(std::env::var("TEST_CCA_VAR3").unwrap(), "single quoted");
    }
}
