//! API Authentication Middleware
//!
//! Supports two authentication methods:
//! 1. API Key via X-API-Key header
//! 2. Bearer token via Authorization header

use axum::{
    body::Body,
    extract::State,
    http::{Request, StatusCode},
    middleware::Next,
    response::{IntoResponse, Response},
};
use tracing::warn;

/// Authentication configuration
#[derive(Debug, Clone)]
pub struct AuthConfig {
    /// API keys that are allowed (loaded from config)
    pub api_keys: Vec<String>,
    /// Whether auth is required (false for development)
    pub required: bool,
}

impl Default for AuthConfig {
    fn default() -> Self {
        Self {
            api_keys: Vec::new(),
            required: false,
        }
    }
}

/// Paths that bypass authentication
const BYPASS_PATHS: &[&str] = &["/health"];

/// Authentication middleware function
///
/// Checks for valid API key in X-API-Key header or Authorization: Bearer header.
/// Returns 401 Unauthorized if auth is required and no valid key is provided.
pub async fn auth_middleware(
    State(config): State<AuthConfig>,
    request: Request<Body>,
    next: Next,
) -> Result<Response, Response> {
    // Skip auth if not required
    if !config.required {
        return Ok(next.run(request).await);
    }

    // Check bypass paths
    let path = request.uri().path();
    if BYPASS_PATHS.iter().any(|p| path == *p) {
        return Ok(next.run(request).await);
    }

    // Check X-API-Key header
    let api_key = request
        .headers()
        .get("X-API-Key")
        .and_then(|v| v.to_str().ok());

    // Check Authorization: Bearer header
    let bearer_token = request
        .headers()
        .get("Authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "));

    // Validate API key
    if let Some(key) = api_key {
        if config.api_keys.iter().any(|k| k == key) {
            return Ok(next.run(request).await);
        }
    }

    // Validate bearer token
    if let Some(token) = bearer_token {
        if config.api_keys.iter().any(|k| k == token) {
            return Ok(next.run(request).await);
        }
    }

    warn!(
        "Unauthorized API request to {} - missing or invalid credentials",
        path
    );

    Err((
        StatusCode::UNAUTHORIZED,
        [("WWW-Authenticate", "Bearer, ApiKey")],
        "Unauthorized: valid API key required",
    )
        .into_response())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_auth_config_default() {
        let config = AuthConfig::default();
        assert!(!config.required);
        assert!(config.api_keys.is_empty());
    }

    #[test]
    fn test_bypass_paths() {
        assert!(BYPASS_PATHS.contains(&"/health"));
    }
}
