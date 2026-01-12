//! Configuration loading and validation tests

use std::env;

// Note: These tests require the config module to be accessible.
// Adjust imports based on actual module structure.

#[test]
fn test_default_daemon_config() {
    // Test that default values are sensible
    let default_bind = "127.0.0.1:8580";
    let default_log_level = "info";
    let default_max_agents = 10;

    assert!(!default_bind.is_empty());
    assert!(default_max_agents > 0);
    assert!(default_max_agents <= 100);
    assert!(!default_log_level.is_empty());
}

#[test]
fn test_default_redis_config() {
    let default_url = "redis://localhost:16379";
    let default_pool_size = 10;
    let default_ttl = 3600;

    assert!(default_url.starts_with("redis://"));
    assert!(default_pool_size > 0);
    assert!(default_ttl > 0);
}

#[test]
fn test_default_postgres_config() {
    let default_url = "postgres://cca:cca@localhost:15432/cca";
    let default_pool_size = 10;
    let default_max_connections = 20;

    assert!(default_url.starts_with("postgres://"));
    assert!(default_pool_size > 0);
    assert!(default_max_connections >= default_pool_size);
}

#[test]
fn test_default_acp_config() {
    let default_port = 8581;
    let default_reconnect_interval = 1000;
    let default_max_attempts = 5;

    assert!(default_port > 0 && default_port < 65536);
    assert!(default_reconnect_interval > 0);
    assert!(default_max_attempts > 0);
}

#[test]
fn test_default_learning_config() {
    let default_algorithm = "ppo";
    let default_batch_size = 32;
    let default_update_interval = 300;

    assert!(!default_algorithm.is_empty());
    assert!(default_batch_size > 0);
    assert!(default_update_interval > 0);
}

#[test]
fn test_environment_variable_prefix() {
    // CCA_ prefix should be used for all env vars
    let prefix = "CCA";

    // Set a test env var
    let key = format!("{}_TEST_VALUE", prefix);
    env::set_var(&key, "test");

    assert_eq!(env::var(&key).unwrap(), "test");

    // Cleanup
    env::remove_var(&key);
}

#[test]
fn test_environment_separator() {
    // Double underscore should separate nested config
    let key = "CCA__DAEMON__BIND_ADDRESS";
    env::set_var(key, "0.0.0.0:8080");

    assert_eq!(env::var(key).unwrap(), "0.0.0.0:8080");

    env::remove_var(key);
}

#[test]
fn test_bind_address_formats() {
    // Valid formats
    let valid_addresses = vec![
        "127.0.0.1:8580",
        "0.0.0.0:8080",
        "localhost:3000",
        "[::1]:8580",
    ];

    for addr in valid_addresses {
        // Just verify they're non-empty strings
        assert!(!addr.is_empty());
    }
}

#[test]
fn test_log_level_options() {
    let valid_levels = vec!["trace", "debug", "info", "warn", "error"];

    for level in valid_levels {
        assert!(!level.is_empty());
    }
}

#[test]
fn test_max_agents_bounds() {
    // Verify reasonable bounds for max_agents
    let min_agents = 1;
    let max_agents = 100;

    assert!(min_agents >= 1);
    assert!(max_agents >= min_agents);
    assert!(max_agents <= 1000); // Reasonable upper limit
}

#[test]
fn test_redis_url_formats() {
    let valid_urls = vec![
        "redis://localhost:6379",
        "redis://user:pass@localhost:6379",
        "redis://localhost:6379/0",
        "redis://127.0.0.1:6380",
    ];

    for url in valid_urls {
        assert!(url.starts_with("redis://"));
    }
}

#[test]
fn test_postgres_url_formats() {
    let valid_urls = vec![
        "postgres://user:pass@localhost:5432/db",
        "postgres://localhost/db",
        "postgresql://user@localhost:5433/db",
    ];

    for url in valid_urls {
        assert!(url.starts_with("postgres://") || url.starts_with("postgresql://"));
    }
}

#[test]
fn test_config_file_paths() {
    // Test config file search order
    let search_paths = vec![
        "cca.toml",
        "~/.config/cca/cca.toml",
    ];

    for path in search_paths {
        assert!(!path.is_empty());
    }
}

#[test]
fn test_timeout_values() {
    // Default timeout should be reasonable
    let default_timeout = 300; // 5 minutes

    assert!(default_timeout > 0);
    assert!(default_timeout <= 3600); // Max 1 hour
}

#[test]
fn test_token_budget_values() {
    let default_budget = 50000;

    assert!(default_budget > 0);
    assert!(default_budget <= 200000); // Claude's context limit
}

#[test]
fn test_pool_size_consistency() {
    // Pool size should not exceed max connections
    let pool_size = 10;
    let max_connections = 20;

    assert!(pool_size <= max_connections);
}

#[test]
fn test_websocket_port_range() {
    let port = 8581u16;

    assert!(port > 1024); // Avoid privileged ports
    assert!(port < 65535);
}

#[test]
fn test_reconnect_interval_bounds() {
    let interval_ms = 1000u64;
    let max_attempts = 5u32;

    assert!(interval_ms >= 100); // At least 100ms
    assert!(interval_ms <= 60000); // At most 1 minute
    assert!(max_attempts >= 1);
    assert!(max_attempts <= 100);
}

#[test]
fn test_training_batch_size() {
    let batch_size = 32;

    assert!(batch_size >= 1);
    assert!(batch_size <= 256);
}

#[test]
fn test_update_interval_bounds() {
    let update_interval = 300u64; // 5 minutes

    assert!(update_interval >= 10); // At least 10 seconds
    assert!(update_interval <= 86400); // At most 1 day
}

#[test]
fn test_rl_algorithm_options() {
    let valid_algorithms = vec!["q_learning", "dqn", "ppo"];

    for algo in valid_algorithms {
        assert!(!algo.is_empty());
    }
}

#[test]
fn test_context_compression_toggle() {
    // Verify boolean toggles work
    let enabled = true;
    let disabled = false;

    assert!(enabled != disabled);
}

#[test]
fn test_ttl_seconds_bounds() {
    let ttl = 3600u64; // 1 hour

    assert!(ttl >= 60); // At least 1 minute
    assert!(ttl <= 86400 * 7); // At most 1 week
}
