//! Codebase indexing service for semantic code search.
//!
//! Provides background indexing of code files with embedding generation.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::Result;
use glob::Pattern;
use tokio::sync::RwLock;
use tracing::{debug, error, info, warn};
use uuid::Uuid;
use walkdir::WalkDir;

use crate::code_parser::{CodeChunk, CodeParser};
use crate::config::IndexingConfig;
use crate::embeddings::EmbeddingService;
use crate::postgres::{IndexingJobRecord, PostgresServices};

/// Status of an indexing job
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct IndexingJobStatus {
    pub job_id: String,
    pub path: String,
    pub status: String,
    pub total_files: i32,
    pub processed_files: i32,
    pub total_chunks: i32,
    pub indexed_chunks: i32,
    pub errors: Vec<String>,
    pub progress_percent: f32,
    pub started_at: Option<String>,
    pub completed_at: Option<String>,
}

impl From<IndexingJobRecord> for IndexingJobStatus {
    fn from(record: IndexingJobRecord) -> Self {
        let progress = if record.total_files > 0 {
            (record.processed_files as f32 / record.total_files as f32) * 100.0
        } else {
            0.0
        };

        let errors: Vec<String> = record
            .errors
            .as_array()
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(String::from))
                    .collect()
            })
            .unwrap_or_default();

        IndexingJobStatus {
            job_id: record.id.to_string(),
            path: record.path,
            status: record.status,
            total_files: record.total_files,
            processed_files: record.processed_files,
            total_chunks: record.total_chunks,
            indexed_chunks: record.indexed_chunks,
            errors,
            progress_percent: progress,
            started_at: record.started_at.map(|dt| dt.to_rfc3339()),
            completed_at: record.completed_at.map(|dt| dt.to_rfc3339()),
        }
    }
}

/// Request to start an indexing job
#[derive(Debug, Clone, serde::Deserialize)]
pub struct StartIndexingRequest {
    pub path: String,
    #[serde(default)]
    pub extensions: Option<Vec<String>>,
    #[serde(default)]
    pub exclude_patterns: Option<Vec<String>>,
    #[serde(default = "default_batch_size")]
    pub batch_size: usize,
}

fn default_batch_size() -> usize {
    10
}

/// Code search result
#[derive(Debug, Clone, serde::Serialize)]
pub struct CodeSearchResult {
    pub id: String,
    pub file_path: String,
    pub chunk_type: String,
    pub name: String,
    pub signature: Option<String>,
    pub content: String,
    pub start_line: i32,
    pub end_line: i32,
    pub language: String,
    pub similarity: f64,
}

/// Indexing service for managing codebase indexing
pub struct IndexingService {
    config: IndexingConfig,
    embedding_service: Arc<EmbeddingService>,
    postgres: Arc<PostgresServices>,
    /// Active job cancellation tokens
    cancellation_tokens: Arc<RwLock<HashSet<Uuid>>>,
}

impl IndexingService {
    /// Create a new indexing service
    pub fn new(
        config: IndexingConfig,
        embedding_service: Arc<EmbeddingService>,
        postgres: Arc<PostgresServices>,
    ) -> Self {
        Self {
            config,
            embedding_service,
            postgres,
            cancellation_tokens: Arc::new(RwLock::new(HashSet::new())),
        }
    }

    /// Start a new indexing job (runs in background)
    pub async fn start_indexing(&self, request: StartIndexingRequest) -> Result<Uuid> {
        let path = PathBuf::from(&request.path);
        if !path.exists() {
            anyhow::bail!("Path does not exist: {}", request.path);
        }

        // Create job record
        let job_id = self.postgres.indexing_jobs.create(&request.path).await?;

        info!("Starting indexing job {} for path: {}", job_id, request.path);

        // Clone what we need for the background task
        let postgres = Arc::clone(&self.postgres);
        let embedding_service = Arc::clone(&self.embedding_service);
        let cancellation_tokens = Arc::clone(&self.cancellation_tokens);
        let config = self.config.clone();

        let extensions = request
            .extensions
            .unwrap_or_else(|| config.default_extensions.clone());
        let exclude_patterns = request
            .exclude_patterns
            .unwrap_or_else(|| config.default_excludes.clone());
        let batch_size = request.batch_size;

        // Spawn background task
        tokio::spawn(async move {
            if let Err(e) = run_indexing_job(
                job_id,
                path,
                extensions,
                exclude_patterns,
                batch_size,
                config.max_chunk_size,
                postgres,
                embedding_service,
                cancellation_tokens,
            )
            .await
            {
                error!("Indexing job {} failed: {:?}", job_id, e);
            }
        });

        Ok(job_id)
    }

    /// Get status of an indexing job
    pub async fn get_job_status(&self, job_id: Uuid) -> Result<Option<IndexingJobStatus>> {
        let record = self.postgres.indexing_jobs.get(job_id).await?;
        Ok(record.map(IndexingJobStatus::from))
    }

    /// List recent indexing jobs
    pub async fn list_jobs(&self, limit: i32) -> Result<Vec<IndexingJobStatus>> {
        let records = self.postgres.indexing_jobs.list_recent(limit).await?;
        Ok(records.into_iter().map(IndexingJobStatus::from).collect())
    }

    /// Cancel a running indexing job
    pub async fn cancel_job(&self, job_id: Uuid) -> Result<bool> {
        // Mark for cancellation
        {
            let mut tokens = self.cancellation_tokens.write().await;
            tokens.insert(job_id);
        }

        // Update database
        let cancelled = self.postgres.indexing_jobs.cancel(job_id).await?;

        if cancelled {
            info!("Cancelled indexing job {}", job_id);
        }

        Ok(cancelled)
    }

    /// Search indexed code chunks
    pub async fn search_code(
        &self,
        query: &str,
        limit: i32,
        language: Option<&str>,
    ) -> Result<Vec<CodeSearchResult>> {
        // Generate embedding for query
        let embedding = self.embedding_service.embed(query).await?;

        // Search similar chunks
        let chunks = self
            .postgres
            .code_chunks
            .search_similar(&embedding, limit, 0.3, language)
            .await?;

        let results = chunks
            .into_iter()
            .map(|cs| CodeSearchResult {
                id: cs.chunk.id.to_string(),
                file_path: cs.chunk.file_path,
                chunk_type: cs.chunk.chunk_type,
                name: cs.chunk.name,
                signature: cs.chunk.signature,
                content: cs.chunk.content,
                start_line: cs.chunk.start_line,
                end_line: cs.chunk.end_line,
                language: cs.chunk.language,
                similarity: cs.similarity,
            })
            .collect();

        Ok(results)
    }

    /// Get indexing statistics
    pub async fn get_stats(&self) -> Result<serde_json::Value> {
        let stats = self.postgres.code_chunks.get_stats().await?;
        let by_language = self.postgres.code_chunks.count_by_language().await?;

        Ok(serde_json::json!({
            "total_chunks": stats.total_chunks,
            "total_files": stats.total_files,
            "languages_count": stats.languages_count,
            "by_language": by_language.into_iter().collect::<std::collections::HashMap<_, _>>(),
        }))
    }
}

/// Run the actual indexing job
async fn run_indexing_job(
    job_id: Uuid,
    path: PathBuf,
    extensions: Vec<String>,
    exclude_patterns: Vec<String>,
    batch_size: usize,
    max_chunk_size: usize,
    postgres: Arc<PostgresServices>,
    embedding_service: Arc<EmbeddingService>,
    cancellation_tokens: Arc<RwLock<HashSet<Uuid>>>,
) -> Result<()> {
    let mut errors: Vec<String> = Vec::new();

    // Compile exclude patterns
    let exclude_globs: Vec<Pattern> = exclude_patterns
        .iter()
        .filter_map(|p| Pattern::new(p).ok())
        .collect();

    // Collect files to process
    let files: Vec<PathBuf> = collect_files(&path, &extensions, &exclude_globs);
    let total_files = files.len() as i32;

    info!(
        "Job {}: Found {} files to index in {:?}",
        job_id, total_files, path
    );

    // Update initial progress
    postgres
        .indexing_jobs
        .update_progress(job_id, total_files, 0, 0, 0)
        .await?;

    // Create parser
    let mut parser = CodeParser::new()?;

    let mut processed_files = 0;
    let mut total_chunks = 0;
    let mut indexed_chunks = 0;
    let mut pending_chunks: Vec<(CodeChunk, String)> = Vec::new();

    for file_path in files {
        // Check for cancellation
        {
            let tokens = cancellation_tokens.read().await;
            if tokens.contains(&job_id) {
                info!("Job {} was cancelled", job_id);
                return Ok(());
            }
        }

        // Parse file
        match parser.parse_file(&file_path) {
            Ok(chunks) => {
                for chunk in chunks {
                    // Skip chunks that are too large
                    if chunk.content.len() > max_chunk_size {
                        debug!(
                            "Skipping chunk {} (too large: {} bytes)",
                            chunk.name,
                            chunk.content.len()
                        );
                        continue;
                    }

                    total_chunks += 1;

                    // Create embedding text: combine name, signature, and content
                    let embed_text = create_embedding_text(&chunk);
                    pending_chunks.push((chunk, embed_text));

                    // Process batch if full
                    if pending_chunks.len() >= batch_size {
                        let batch_indexed = process_chunk_batch(
                            &mut pending_chunks,
                            &postgres,
                            &embedding_service,
                            &mut errors,
                        )
                        .await;
                        indexed_chunks += batch_indexed;
                    }
                }
            }
            Err(e) => {
                let error_msg = format!("Failed to parse {:?}: {}", file_path, e);
                warn!("{}", error_msg);
                errors.push(error_msg);
            }
        }

        processed_files += 1;

        // Update progress periodically
        if processed_files % 10 == 0 {
            postgres
                .indexing_jobs
                .update_progress(job_id, total_files, processed_files, total_chunks, indexed_chunks)
                .await?;
        }
    }

    // Process remaining chunks
    if !pending_chunks.is_empty() {
        let batch_indexed = process_chunk_batch(
            &mut pending_chunks,
            &postgres,
            &embedding_service,
            &mut errors,
        )
        .await;
        indexed_chunks += batch_indexed;
    }

    // Final update
    postgres
        .indexing_jobs
        .update_progress(job_id, total_files, processed_files, total_chunks, indexed_chunks)
        .await?;

    // Mark as complete
    let success = errors.len() < (total_files as usize / 2); // Allow some errors
    postgres.indexing_jobs.complete(job_id, success, errors).await?;

    info!(
        "Job {} completed: {} files, {} chunks indexed ({})",
        job_id,
        processed_files,
        indexed_chunks,
        if success { "success" } else { "with errors" }
    );

    Ok(())
}

/// Collect files matching extensions and not excluded
fn collect_files(path: &Path, extensions: &[String], exclude_globs: &[Pattern]) -> Vec<PathBuf> {
    let ext_set: HashSet<String> = extensions.iter().map(|e| e.to_lowercase()).collect();

    WalkDir::new(path)
        .follow_links(false)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_file())
        .filter(|e| {
            // Check extension
            e.path()
                .extension()
                .and_then(|ext| ext.to_str())
                .map(|ext| ext_set.contains(&ext.to_lowercase()))
                .unwrap_or(false)
        })
        .filter(|e| {
            // Check exclude patterns
            let path_str = e.path().to_string_lossy();
            !exclude_globs.iter().any(|g| g.matches(&path_str))
        })
        .map(|e| e.path().to_path_buf())
        .collect()
}

/// Create text for embedding from a code chunk
fn create_embedding_text(chunk: &CodeChunk) -> String {
    let mut text = String::new();

    // Add chunk type and name
    text.push_str(&format!(
        "{} {}: ",
        chunk.chunk_type.as_str(),
        chunk.name
    ));

    // Add signature if available
    if let Some(ref sig) = chunk.signature {
        text.push_str(sig);
        text.push('\n');
    }

    // Add content (truncated if too long)
    let content = if chunk.content.len() > 2000 {
        &chunk.content[..2000]
    } else {
        &chunk.content
    };
    text.push_str(content);

    text
}

/// Process a batch of chunks: generate embeddings and store
async fn process_chunk_batch(
    pending: &mut Vec<(CodeChunk, String)>,
    postgres: &Arc<PostgresServices>,
    embedding_service: &Arc<EmbeddingService>,
    errors: &mut Vec<String>,
) -> i32 {
    if pending.is_empty() {
        return 0;
    }

    let texts: Vec<&str> = pending.iter().map(|(_, t)| t.as_str()).collect();

    // Generate embeddings
    match embedding_service.embed_batch(&texts).await {
        Ok(embeddings) => {
            let mut indexed = 0;
            for ((chunk, _), embedding) in pending.drain(..).zip(embeddings) {
                // Store chunk with embedding
                if let Err(e) = postgres
                    .code_chunks
                    .upsert(
                        &chunk.file_path,
                        chunk.chunk_type.as_str(),
                        &chunk.name,
                        chunk.signature.as_deref(),
                        &chunk.content,
                        chunk.start_line as i32,
                        chunk.end_line as i32,
                        &chunk.language,
                        &embedding,
                        chunk.metadata.clone(),
                    )
                    .await
                {
                    errors.push(format!("Failed to store chunk {}: {}", chunk.name, e));
                } else {
                    indexed += 1;
                }
            }
            indexed
        }
        Err(e) => {
            let error_msg = format!("Failed to generate embeddings: {}", e);
            warn!("{}", error_msg);
            errors.push(error_msg);
            pending.clear();
            0
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_embedding_text_creation() {
        let chunk = CodeChunk {
            file_path: "test.rs".to_string(),
            chunk_type: crate::code_parser::ChunkType::Function,
            name: "hello".to_string(),
            signature: Some("fn hello(name: &str) -> String".to_string()),
            content: "fn hello(name: &str) -> String { format!(\"Hello, {}!\", name) }".to_string(),
            start_line: 1,
            end_line: 3,
            language: "rust".to_string(),
            metadata: serde_json::json!({}),
        };

        let text = create_embedding_text(&chunk);
        assert!(text.contains("function hello"));
        assert!(text.contains("fn hello(name: &str)"));
    }
}
