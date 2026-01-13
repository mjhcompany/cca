//! Authenticated HTTP client for daemon communication

use reqwest::{Client, RequestBuilder, Response};
use serde::Deserialize;
use std::path::PathBuf;

/// Minimal config to extract API key
#[derive(Debug, Deserialize, Default)]
struct MinimalConfig {
    #[serde(default)]
    daemon: MinimalDaemonConfig,
}

#[derive(Debug, Deserialize, Default)]
struct MinimalDaemonConfig {
    #[serde(default)]
    api_keys: Vec<String>,
}

/// Get API key from config file
pub fn load_api_key() -> Option<String> {
    let config_path = find_config_file()?;
    let content = std::fs::read_to_string(&config_path).ok()?;
    let config: MinimalConfig = toml::from_str(&content).ok()?;
    config.daemon.api_keys.into_iter().next()
}

/// Find config file (same locations as daemon)
fn find_config_file() -> Option<PathBuf> {
    // CCA_CONFIG env var
    if let Ok(path) = std::env::var("CCA_CONFIG") {
        let path = PathBuf::from(path);
        if path.exists() {
            return Some(path);
        }
    }

    // System-wide config
    let system_config = PathBuf::from("/usr/local/etc/cca/cca.toml");
    if system_config.exists() {
        return Some(system_config);
    }

    // User config
    if let Some(home) = dirs::home_dir() {
        let user_config = home.join(".config").join("cca").join("cca.toml");
        if user_config.exists() {
            return Some(user_config);
        }
    }

    // Current directory
    let local = PathBuf::from("cca.toml");
    if local.exists() {
        return Some(local);
    }

    None
}

/// Create authenticated GET request
pub fn auth_get(url: &str) -> RequestBuilder {
    let client = Client::new();
    let mut request = client.get(url);
    if let Some(api_key) = load_api_key() {
        request = request.header("X-API-Key", api_key);
    }
    request
}

/// Create authenticated POST request
pub fn auth_post(url: &str) -> RequestBuilder {
    let client = Client::new();
    let mut request = client.post(url);
    if let Some(api_key) = load_api_key() {
        request = request.header("X-API-Key", api_key);
    }
    request
}

/// Execute authenticated GET request
pub async fn get(url: &str) -> Result<Response, reqwest::Error> {
    auth_get(url).send().await
}

/// Execute authenticated POST request with JSON body
pub async fn post_json<T: serde::Serialize>(url: &str, body: &T) -> Result<Response, reqwest::Error> {
    auth_post(url).json(body).send().await
}
