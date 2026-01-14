//! API Authentication and Rate Limiting Middleware
//!
//! Supports two authentication methods:
//! 1. API Key via `X-API-Key` header
//! 2. Bearer token via Authorization header
//!
//! Rate limiting uses a token bucket algorithm with configurable rates.
//! `SEC-004`: Per-IP rate limiting to prevent DoS attacks.

use std::net::{IpAddr, SocketAddr};
use std::num::NonZeroU32;
use std::sync::Arc;

use axum::{
    body::Body,
    extract::{ConnectInfo, State},
    http::{header, Request, StatusCode},
    middleware::Next,
    response::{IntoResponse, Response},
};
use cca_core::util::constant_time_eq;
use governor::{
    clock::DefaultClock,
    middleware::NoOpMiddleware,
    state::{InMemoryState, NotKeyed, keyed::DashMapStateStore},
    Quota, RateLimiter,
};
use tracing::{debug, warn};

/// Authentication configuration
#[derive(Debug, Clone, Default)]
pub struct AuthConfig {
    /// API keys that are allowed (loaded from config)
    pub api_keys: Vec<String>,
    /// Whether auth is required (false for development)
    pub required: bool,
}

/// Paths that bypass authentication
const BYPASS_PATHS: &[&str] = &["/health", "/api/v1/health"];

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
    if BYPASS_PATHS.contains(&path) {
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

    // Validate API key using constant-time comparison to prevent timing attacks
    if let Some(key) = api_key {
        if config.api_keys.iter().any(|k| constant_time_eq(k, key)) {
            return Ok(next.run(request).await);
        }
    }

    // Validate bearer token using constant-time comparison to prevent timing attacks
    if let Some(token) = bearer_token {
        if config.api_keys.iter().any(|k| constant_time_eq(k, token)) {
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

/// Type alias for the global (non-keyed) rate limiter
pub type GlobalRateLimiter = RateLimiter<NotKeyed, InMemoryState, DefaultClock>;

/// Type alias for per-IP rate limiter (keyed by IP address)
pub type PerIpRateLimiter = RateLimiter<IpAddr, DashMapStateStore<IpAddr>, DefaultClock, NoOpMiddleware>;

/// Type alias for per-API-key rate limiter (keyed by API key string)
pub type PerApiKeyRateLimiter = RateLimiter<String, DashMapStateStore<String>, DefaultClock, NoOpMiddleware>;

/// Rate limiting configuration
/// `SEC-004`: Configurable rate limits to prevent DoS attacks
#[derive(Debug, Clone)]
pub struct RateLimitConfig {
    /// Requests per second per IP (0 = disabled)
    pub requests_per_second: u32,
    /// Burst size - max requests allowed in a burst
    pub burst_size: u32,
    /// Global rate limit across all IPs (0 = disabled)
    pub global_rps: u32,
    /// Whether to trust `X-Forwarded-For` header (only enable behind trusted proxy)
    pub trust_proxy: bool,
    /// Requests per second per API key (`0` = disabled, uses IP limit)
    /// `SEC-004`: Per-API-key rate limiting for authenticated clients
    pub api_key_rps: u32,
    /// Burst size for API key rate limiting
    pub api_key_burst: u32,
}

impl Default for RateLimitConfig {
    fn default() -> Self {
        Self {
            requests_per_second: 100,
            burst_size: 50,
            global_rps: 1000,
            trust_proxy: false,
            api_key_rps: 200,    // Higher limit for authenticated clients
            api_key_burst: 100,  // Higher burst for authenticated clients
        }
    }
}

/// Combined rate limiter state
#[derive(Clone)]
pub struct RateLimiterState {
    /// Per-IP rate limiter
    pub per_ip: Arc<PerIpRateLimiter>,
    /// Per-API-key rate limiter (optional, for authenticated clients)
    /// SEC-004: Per-API-key rate limiting for finer-grained control
    pub per_api_key: Option<Arc<PerApiKeyRateLimiter>>,
    /// Global rate limiter (optional, for overall protection)
    pub global: Option<Arc<GlobalRateLimiter>>,
    /// Whether to trust X-Forwarded-For header
    pub trust_proxy: bool,
}

/// Create a new rate limiter with the specified requests per second (legacy)
pub fn create_rate_limiter(requests_per_second: u32) -> Arc<GlobalRateLimiter> {
    let quota = Quota::per_second(
        NonZeroU32::new(requests_per_second).unwrap_or(NonZeroU32::new(100).unwrap()),
    );
    Arc::new(RateLimiter::direct(quota))
}

/// Create a per-IP rate limiter with configurable burst
/// `SEC-004`: Per-client rate limiting for DoS protection
pub fn create_per_ip_rate_limiter(config: &RateLimitConfig) -> Arc<PerIpRateLimiter> {
    let rps = NonZeroU32::new(config.requests_per_second)
        .unwrap_or(NonZeroU32::new(100).unwrap());
    let burst = NonZeroU32::new(config.burst_size)
        .unwrap_or(NonZeroU32::new(50).unwrap());

    // Allow burst requests, replenishing at rps rate
    let quota = Quota::per_second(rps).allow_burst(burst);

    Arc::new(RateLimiter::dashmap(quota))
}

/// Create a per-API-key rate limiter with configurable burst
/// SEC-004: Per-API-key rate limiting for authenticated clients
pub fn create_per_api_key_rate_limiter(config: &RateLimitConfig) -> Arc<PerApiKeyRateLimiter> {
    let rps = NonZeroU32::new(config.api_key_rps)
        .unwrap_or(NonZeroU32::new(200).unwrap());
    let burst = NonZeroU32::new(config.api_key_burst)
        .unwrap_or(NonZeroU32::new(100).unwrap());

    // Allow burst requests, replenishing at rps rate
    let quota = Quota::per_second(rps).allow_burst(burst);

    Arc::new(RateLimiter::dashmap(quota))
}

/// Create combined rate limiter state from configuration
pub fn create_rate_limiter_state(config: &RateLimitConfig) -> RateLimiterState {
    let per_ip = create_per_ip_rate_limiter(config);

    let per_api_key = if config.api_key_rps > 0 {
        Some(create_per_api_key_rate_limiter(config))
    } else {
        None
    };

    let global = if config.global_rps > 0 {
        Some(create_rate_limiter(config.global_rps))
    } else {
        None
    };

    RateLimiterState {
        per_ip,
        per_api_key,
        global,
        trust_proxy: config.trust_proxy,
    }
}

/// Extract API key from request headers
/// SEC-004: Extract API key for per-key rate limiting
fn extract_api_key(request: &Request<Body>) -> Option<String> {
    // Check X-API-Key header
    if let Some(key) = request
        .headers()
        .get("X-API-Key")
        .and_then(|v| v.to_str().ok())
    {
        return Some(key.to_string());
    }

    // Check Authorization: Bearer header
    if let Some(token) = request
        .headers()
        .get("Authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
    {
        return Some(token.to_string());
    }

    None
}

/// Extract client IP from request
/// SEC-004: Properly handle proxy scenarios while preventing IP spoofing
fn extract_client_ip(request: &Request<Body>, trust_proxy: bool) -> Option<IpAddr> {
    // If we trust the proxy, check X-Forwarded-For first
    if trust_proxy {
        if let Some(forwarded) = request.headers().get("X-Forwarded-For") {
            if let Ok(value) = forwarded.to_str() {
                // X-Forwarded-For can contain multiple IPs: client, proxy1, proxy2
                // Take the first (leftmost) which is the original client
                if let Some(first_ip) = value.split(',').next() {
                    if let Ok(ip) = first_ip.trim().parse::<IpAddr>() {
                        return Some(ip);
                    }
                }
            }
        }

        // Also check X-Real-IP (common with nginx)
        if let Some(real_ip) = request.headers().get("X-Real-IP") {
            if let Ok(value) = real_ip.to_str() {
                if let Ok(ip) = value.trim().parse::<IpAddr>() {
                    return Some(ip);
                }
            }
        }
    }

    // Fall back to connection info
    request.extensions()
        .get::<ConnectInfo<SocketAddr>>()
        .map(|ci| ci.0.ip())
}

/// Rate limiting middleware with per-IP and per-API-key tracking
/// `SEC-004`: Per-client rate limiting to prevent DoS attacks
///
/// Rate limiting strategy:
/// 1. Global rate limit - absolute cap on all requests
/// 2. Per-API-key rate limit - for authenticated requests (higher limits)
/// 3. Per-IP rate limit - for all requests (fallback for unauthenticated)
///
/// Returns 429 Too Many Requests when limit is exceeded, with proper headers:
/// - Retry-After: seconds until a request might succeed
/// - X-RateLimit-Limit: the rate limit ceiling
/// - X-RateLimit-Remaining: approximate remaining requests (best effort)
pub async fn rate_limit_middleware(
    State(limiter): State<RateLimiterState>,
    request: Request<Body>,
    next: Next,
) -> Result<Response, Response> {
    let path = request.uri().path().to_string();

    // Extract client IP
    let client_ip = extract_client_ip(&request, limiter.trust_proxy)
        .unwrap_or_else(|| {
            // Fallback to localhost if we can't determine IP
            // This shouldn't happen in normal operation
            warn!("Could not determine client IP for rate limiting, using fallback");
            IpAddr::V4(std::net::Ipv4Addr::LOCALHOST)
        });

    // Extract API key for per-key rate limiting
    let api_key = extract_api_key(&request);

    // Check global rate limit first (if configured)
    if let Some(ref global) = limiter.global {
        if global.check().is_err() {
            warn!(
                "Global rate limit exceeded for request to {} from {}",
                path, client_ip
            );
            return Err(rate_limit_response(1, 0));
        }
    }

    // Check per-API-key rate limit if:
    // 1. API key is present in request
    // 2. Per-API-key limiter is configured
    // SEC-004: Per-API-key rate limiting provides separate quotas for each authenticated client
    if let (Some(ref key), Some(ref api_key_limiter)) = (&api_key, &limiter.per_api_key) {
        match api_key_limiter.check_key(key) {
            Ok(()) => {
                debug!(
                    "API key rate limit check passed for {} from {} (key: {}...)",
                    path,
                    client_ip,
                    &key[..key.len().min(8)] // Only log first 8 chars of key
                );
            }
            Err(not_until) => {
                let wait_time = not_until.wait_time_from(governor::clock::Clock::now(&governor::clock::DefaultClock::default()));
                let retry_after = wait_time.as_secs().max(1);

                warn!(
                    "API key rate limit exceeded for {} from {} (key: {}..., retry after {}s)",
                    path,
                    client_ip,
                    &key[..key.len().min(8)],
                    retry_after
                );

                return Err(rate_limit_response_with_type(retry_after, 0, "api_key"));
            }
        }
    }

    // Check per-IP rate limit (always checked, regardless of API key)
    // SEC-004: IP rate limiting provides base protection even for authenticated requests
    match limiter.per_ip.check_key(&client_ip) {
        Ok(()) => {
            debug!("IP rate limit check passed for {} from {}", path, client_ip);
            let mut response = next.run(request).await;

            // Add informational rate limit headers on success
            // Note: governor doesn't provide remaining count easily, so we omit it
            let limit = if api_key.is_some() { "200" } else { "100" };
            response.headers_mut().insert(
                "X-RateLimit-Limit",
                header::HeaderValue::from_static(limit),
            );

            Ok(response)
        }
        Err(not_until) => {
            let wait_time = not_until.wait_time_from(governor::clock::Clock::now(&governor::clock::DefaultClock::default()));
            let retry_after = wait_time.as_secs().max(1);

            warn!(
                "IP rate limit exceeded for {} from {} (retry after {}s)",
                path, client_ip, retry_after
            );

            Err(rate_limit_response_with_type(retry_after, 0, "ip"))
        }
    }
}

/// Create a rate limit exceeded response with proper headers
fn rate_limit_response(retry_after: u64, remaining: u32) -> Response {
    rate_limit_response_with_type(retry_after, remaining, "global")
}

/// Create a rate limit exceeded response with limit type information
/// SEC-004: Include limit type to help clients understand which quota was exceeded
fn rate_limit_response_with_type(retry_after: u64, remaining: u32, limit_type: &str) -> Response {
    let retry_after_str = retry_after.to_string();
    let remaining_str = remaining.to_string();

    (
        StatusCode::TOO_MANY_REQUESTS,
        [
            ("Retry-After", retry_after_str.as_str()),
            ("X-RateLimit-Remaining", remaining_str.as_str()),
            ("X-RateLimit-Type", limit_type),
            ("Content-Type", "application/json"),
        ],
        format!(
            r#"{{"error":"Too many requests","message":"Rate limit exceeded. Please slow down.","limit_type":"{limit_type}","retry_after_seconds":{retry_after_str}}}"#
        ),
    )
        .into_response()
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

    #[test]
    fn test_rate_limiter_creation() {
        let limiter = create_rate_limiter(100);
        // Should allow at least one request
        assert!(limiter.check().is_ok());
    }

    #[test]
    fn test_rate_limit_config_default() {
        let config = RateLimitConfig::default();
        assert_eq!(config.requests_per_second, 100);
        assert_eq!(config.burst_size, 50);
        assert_eq!(config.global_rps, 1000);
        assert!(!config.trust_proxy);
    }

    #[test]
    fn test_per_ip_rate_limiter_creation() {
        let config = RateLimitConfig::default();
        let limiter = create_per_ip_rate_limiter(&config);

        // Should allow requests from different IPs
        let ip1: IpAddr = "192.168.1.1".parse().unwrap();
        let ip2: IpAddr = "192.168.1.2".parse().unwrap();

        assert!(limiter.check_key(&ip1).is_ok());
        assert!(limiter.check_key(&ip2).is_ok());
    }

    #[test]
    fn test_rate_limiter_state_creation() {
        let config = RateLimitConfig::default();
        let state = create_rate_limiter_state(&config);

        assert!(state.global.is_some());
        assert!(!state.trust_proxy);
    }

    #[test]
    fn test_rate_limiter_state_no_global() {
        let config = RateLimitConfig {
            global_rps: 0,
            ..Default::default()
        };
        let state = create_rate_limiter_state(&config);

        assert!(state.global.is_none());
    }

    #[test]
    fn test_per_ip_rate_limit_exhaustion() {
        let config = RateLimitConfig {
            requests_per_second: 10,
            burst_size: 5,
            global_rps: 0,
            trust_proxy: false,
            api_key_rps: 0,
            api_key_burst: 0,
        };
        let limiter = create_per_ip_rate_limiter(&config);

        let ip: IpAddr = "10.0.0.1".parse().unwrap();

        // Exhaust the burst
        for _ in 0..5 {
            assert!(limiter.check_key(&ip).is_ok());
        }

        // Next request should be rate limited
        assert!(limiter.check_key(&ip).is_err());

        // Different IP should still work
        let ip2: IpAddr = "10.0.0.2".parse().unwrap();
        assert!(limiter.check_key(&ip2).is_ok());
    }

    #[test]
    fn test_per_api_key_rate_limiter_creation() {
        let config = RateLimitConfig {
            api_key_rps: 200,
            api_key_burst: 100,
            ..Default::default()
        };
        let limiter = create_per_api_key_rate_limiter(&config);

        // Should allow requests from different API keys
        let key1 = "api_key_1".to_string();
        let key2 = "api_key_2".to_string();

        assert!(limiter.check_key(&key1).is_ok());
        assert!(limiter.check_key(&key2).is_ok());
    }

    #[test]
    fn test_per_api_key_rate_limit_exhaustion() {
        let config = RateLimitConfig {
            requests_per_second: 100,
            burst_size: 50,
            global_rps: 0,
            trust_proxy: false,
            api_key_rps: 10,
            api_key_burst: 5,
        };
        let limiter = create_per_api_key_rate_limiter(&config);

        let key = "test_api_key".to_string();

        // Exhaust the burst
        for _ in 0..5 {
            assert!(limiter.check_key(&key).is_ok());
        }

        // Next request should be rate limited
        assert!(limiter.check_key(&key).is_err());

        // Different API key should still work
        let key2 = "different_key".to_string();
        assert!(limiter.check_key(&key2).is_ok());
    }

    #[test]
    fn test_rate_limiter_state_with_api_key() {
        let config = RateLimitConfig::default();
        let state = create_rate_limiter_state(&config);

        // Should have per-API-key limiter enabled (default api_key_rps > 0)
        assert!(state.per_api_key.is_some());
        assert!(state.global.is_some());
        assert!(!state.trust_proxy);
    }

    #[test]
    fn test_rate_limiter_state_no_api_key_limiting() {
        let config = RateLimitConfig {
            api_key_rps: 0,  // Disabled
            api_key_burst: 0,
            ..Default::default()
        };
        let state = create_rate_limiter_state(&config);

        // Should not have per-API-key limiter when disabled
        assert!(state.per_api_key.is_none());
    }

    #[test]
    fn test_api_key_and_ip_independence() {
        // Verify that API key and IP limiters are independent
        let config = RateLimitConfig {
            requests_per_second: 10,
            burst_size: 3,
            global_rps: 0,
            trust_proxy: false,
            api_key_rps: 10,
            api_key_burst: 3,
        };

        let ip_limiter = create_per_ip_rate_limiter(&config);
        let api_key_limiter = create_per_api_key_rate_limiter(&config);

        let ip: IpAddr = "192.168.1.100".parse().unwrap();
        let key = "shared_key".to_string();

        // Exhaust IP limiter
        for _ in 0..3 {
            assert!(ip_limiter.check_key(&ip).is_ok());
        }
        assert!(ip_limiter.check_key(&ip).is_err());

        // API key limiter should still work (independent)
        for _ in 0..3 {
            assert!(api_key_limiter.check_key(&key).is_ok());
        }
        assert!(api_key_limiter.check_key(&key).is_err());
    }
}
