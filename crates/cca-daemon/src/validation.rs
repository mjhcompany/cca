//! Input validation middleware and extractors for the CCA API
//!
//! SEC-012: Provides validated JSON extraction and input sanitization
//!
//! This module provides:
//! - `ValidatedJson<T>` - An Axum extractor that automatically validates request bodies
//! - Validation constants for field length limits
//! - Custom validator functions for UUIDs, priorities, algorithms, paths, and timeouts

// Allow unused items - these are exported utilities for use across the API
#![allow(dead_code)]

use axum::{
    async_trait,
    extract::{rejection::JsonRejection, FromRequest, Request},
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use serde::de::DeserializeOwned;
use validator::Validate;

/// Default body size limit: 1MB
pub const DEFAULT_BODY_LIMIT: usize = 1024 * 1024;

/// Maximum string length for task descriptions
pub const MAX_TASK_DESCRIPTION_LEN: usize = 100_000;
/// Maximum string length for broadcast messages
pub const MAX_BROADCAST_MESSAGE_LEN: usize = 10_000;
/// Maximum string length for content fields (token analysis, compression)
pub const MAX_CONTENT_LEN: usize = 1_000_000;
/// Maximum string length for search queries
pub const MAX_QUERY_LEN: usize = 1_000;
/// Maximum role name length
pub const MAX_ROLE_LEN: usize = 64;
/// Maximum algorithm name length
pub const MAX_ALGORITHM_LEN: usize = 32;
/// Maximum priority string length
pub const MAX_PRIORITY_LEN: usize = 16;
/// Maximum filesystem path length
pub const MAX_PATH_LEN: usize = 4096;
/// Maximum timeout in seconds
pub const MAX_TIMEOUT_SECONDS: u64 = 3600;
/// Minimum timeout in seconds
pub const MIN_TIMEOUT_SECONDS: u64 = 1;

/// Valid priority values
pub const VALID_PRIORITIES: &[&str] = &["low", "normal", "high", "critical"];
/// Valid RL algorithms
pub const VALID_RL_ALGORITHMS: &[&str] = &["q_learning", "dqn", "ppo"];

/// Error type for validated JSON extraction
#[derive(Debug)]
pub struct ValidationError {
    pub message: String,
}

impl IntoResponse for ValidationError {
    fn into_response(self) -> Response {
        let body = serde_json::json!({
            "success": false,
            "error": self.message,
            "error_type": "validation_error"
        });
        (StatusCode::BAD_REQUEST, Json(body)).into_response()
    }
}

/// A JSON extractor that validates the request body using the validator crate
///
/// Usage:
/// ```ignore
/// async fn handler(ValidatedJson(payload): ValidatedJson<MyRequest>) -> impl IntoResponse {
///     // payload is guaranteed to be valid
/// }
/// ```
#[derive(Debug, Clone, Copy, Default)]
pub struct ValidatedJson<T>(pub T);

#[async_trait]
impl<S, T> FromRequest<S> for ValidatedJson<T>
where
    T: DeserializeOwned + Validate,
    S: Send + Sync,
{
    type Rejection = ValidationError;

    async fn from_request(req: Request, state: &S) -> Result<Self, Self::Rejection> {
        // First, extract the JSON body
        let Json(value) = Json::<T>::from_request(req, state)
            .await
            .map_err(|rejection: JsonRejection| ValidationError {
                message: format!("Invalid JSON: {}", rejection),
            })?;

        // Then validate using the validator crate
        value.validate().map_err(|e| ValidationError {
            message: format!("Validation failed: {}", e),
        })?;

        Ok(ValidatedJson(value))
    }
}

/// Custom validator for UUID strings
pub fn validate_uuid(value: &str) -> Result<(), validator::ValidationError> {
    uuid::Uuid::parse_str(value).map_err(|_| {
        let mut err = validator::ValidationError::new("invalid_uuid");
        err.message = Some("Invalid UUID format".into());
        err
    })?;
    Ok(())
}

/// Custom validator for priority values
pub fn validate_priority(value: &str) -> Result<(), validator::ValidationError> {
    if VALID_PRIORITIES.contains(&value.to_lowercase().as_str()) {
        Ok(())
    } else {
        let mut err = validator::ValidationError::new("invalid_priority");
        err.message = Some(format!("Must be one of: {}", VALID_PRIORITIES.join(", ")).into());
        Err(err)
    }
}

/// Custom validator for RL algorithm names
pub fn validate_algorithm(value: &str) -> Result<(), validator::ValidationError> {
    if VALID_RL_ALGORITHMS.contains(&value.to_lowercase().as_str()) {
        Ok(())
    } else {
        let mut err = validator::ValidationError::new("invalid_algorithm");
        err.message = Some(format!("Must be one of: {}", VALID_RL_ALGORITHMS.join(", ")).into());
        Err(err)
    }
}

/// Custom validator for filesystem paths (no path traversal)
pub fn validate_path(value: &str) -> Result<(), validator::ValidationError> {
    // Check for path traversal attempts
    if value.contains("..") {
        let mut err = validator::ValidationError::new("path_traversal");
        err.message = Some("Path traversal not allowed".into());
        return Err(err);
    }

    // Require absolute path
    if !value.starts_with('/') {
        let mut err = validator::ValidationError::new("relative_path");
        err.message = Some("Path must be absolute".into());
        return Err(err);
    }

    Ok(())
}

/// Custom validator for timeout values
pub fn validate_timeout(value: u64) -> Result<(), validator::ValidationError> {
    if value < MIN_TIMEOUT_SECONDS {
        let mut err = validator::ValidationError::new("timeout_too_small");
        err.message = Some(format!("Timeout must be at least {} seconds", MIN_TIMEOUT_SECONDS).into());
        return Err(err);
    }
    if value > MAX_TIMEOUT_SECONDS {
        let mut err = validator::ValidationError::new("timeout_too_large");
        err.message = Some(format!("Timeout must be at most {} seconds", MAX_TIMEOUT_SECONDS).into());
        return Err(err);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validate_uuid() {
        assert!(validate_uuid("550e8400-e29b-41d4-a716-446655440000").is_ok());
        assert!(validate_uuid("not-a-uuid").is_err());
        assert!(validate_uuid("").is_err());
    }

    #[test]
    fn test_validate_priority() {
        assert!(validate_priority("low").is_ok());
        assert!(validate_priority("normal").is_ok());
        assert!(validate_priority("HIGH").is_ok()); // Case insensitive
        assert!(validate_priority("critical").is_ok());
        assert!(validate_priority("invalid").is_err());
    }

    #[test]
    fn test_validate_algorithm() {
        assert!(validate_algorithm("q_learning").is_ok());
        assert!(validate_algorithm("DQN").is_ok()); // Case insensitive
        assert!(validate_algorithm("ppo").is_ok());
        assert!(validate_algorithm("invalid").is_err());
    }

    #[test]
    fn test_validate_path() {
        assert!(validate_path("/home/user/code").is_ok());
        assert!(validate_path("/tmp/test").is_ok());
        assert!(validate_path("relative/path").is_err());
        assert!(validate_path("/home/../etc/passwd").is_err());
        assert!(validate_path("/home/user/..").is_err());
    }

    #[test]
    fn test_validate_timeout() {
        assert!(validate_timeout(1).is_ok());
        assert!(validate_timeout(60).is_ok());
        assert!(validate_timeout(3600).is_ok());
        assert!(validate_timeout(0).is_err());
        assert!(validate_timeout(3601).is_err());
    }
}
