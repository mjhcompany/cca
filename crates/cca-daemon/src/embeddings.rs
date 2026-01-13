//! Embedding service for generating vector embeddings via Ollama API
//!
//! Uses Ollama's embedding API to generate vectors for semantic search.

use anyhow::{Context, Result};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use tracing::{debug, error, info};

/// Configuration for the embedding service
#[derive(Debug, Clone)]
pub struct EmbeddingConfig {
    /// Ollama API base URL (e.g., "http://192.168.33.218:11434")
    pub ollama_url: String,
    /// Model name for embeddings (e.g., "nomic-embed-text:latest")
    pub model: String,
    /// Expected embedding dimension (768 for nomic-embed-text)
    pub dimension: usize,
}

impl Default for EmbeddingConfig {
    fn default() -> Self {
        Self {
            ollama_url: "http://localhost:11434".to_string(),
            model: "nomic-embed-text:latest".to_string(),
            dimension: 768,
        }
    }
}

/// Request body for Ollama embedding API
#[derive(Debug, Serialize)]
struct OllamaEmbeddingRequest {
    model: String,
    prompt: String,
}

/// Response from Ollama embedding API
#[derive(Debug, Deserialize)]
struct OllamaEmbeddingResponse {
    embedding: Vec<f32>,
}

/// Service for generating embeddings
pub struct EmbeddingService {
    client: Client,
    config: EmbeddingConfig,
}

impl EmbeddingService {
    /// Create a new embedding service
    pub fn new(config: EmbeddingConfig) -> Self {
        let client = Client::builder()
            .timeout(std::time::Duration::from_secs(30))
            .build()
            .expect("Failed to build HTTP client");

        info!(
            "Embedding service initialized: {} with model {}",
            config.ollama_url, config.model
        );

        Self { client, config }
    }

    /// Generate embedding for a text
    pub async fn embed(&self, text: &str) -> Result<Vec<f32>> {
        let url = format!("{}/api/embeddings", self.config.ollama_url);

        let request = OllamaEmbeddingRequest {
            model: self.config.model.clone(),
            prompt: text.to_string(),
        };

        debug!("Generating embedding for {} chars of text", text.len());

        let response = self
            .client
            .post(&url)
            .json(&request)
            .send()
            .await
            .context("Failed to send embedding request to Ollama")?;

        let status = response.status();
        if !status.is_success() {
            let body = response.text().await.unwrap_or_default();
            error!("Ollama embedding API error: {} - {}", status, body);
            anyhow::bail!("Ollama embedding API returned {}: {}", status, body);
        }

        let result = response
            .json::<OllamaEmbeddingResponse>()
            .await
            .context("Failed to parse Ollama embedding response")?;

        let embedding = result.embedding;

        // Validate dimension
        if embedding.len() != self.config.dimension {
            error!(
                "Embedding dimension mismatch: expected {}, got {}",
                self.config.dimension,
                embedding.len()
            );
            anyhow::bail!(
                "Embedding dimension mismatch: expected {}, got {}",
                self.config.dimension,
                embedding.len()
            );
        }

        debug!("Generated embedding with {} dimensions", embedding.len());
        Ok(embedding)
    }

    /// Generate embeddings for multiple texts (batch)
    pub async fn embed_batch(&self, texts: &[&str]) -> Result<Vec<Vec<f32>>> {
        let mut embeddings = Vec::with_capacity(texts.len());

        for text in texts {
            let embedding = self.embed(text).await?;
            embeddings.push(embedding);
        }

        Ok(embeddings)
    }

    /// Check if the embedding service is available
    pub async fn health_check(&self) -> bool {
        // Try to get a simple embedding
        match self.embed("test").await {
            Ok(_) => true,
            Err(e) => {
                error!("Embedding service health check failed: {}", e);
                false
            }
        }
    }

    /// Get the configured dimension
    pub fn dimension(&self) -> usize {
        self.config.dimension
    }

    /// Get the model name
    pub fn model(&self) -> &str {
        &self.config.model
    }
}
