//! Memory management commands

use std::time::Duration;

use anyhow::{Context, Result};
use clap::Subcommand;
use serde::{Deserialize, Serialize};

use super::http;

fn daemon_url() -> String {
    std::env::var("CCA_DAEMON_URL").unwrap_or_else(|_| "http://127.0.0.1:8580".to_string())
}

#[derive(Subcommand)]
pub enum MemoryCommands {
    /// Store a pattern
    Store {
        /// Pattern content
        pattern: String,

        /// Pattern type (code, routing, error_handling, etc.)
        #[arg(short, long, default_value = "code")]
        pattern_type: String,
    },
    /// Search patterns
    Search {
        /// Search query
        query: String,

        /// Maximum results
        #[arg(short, long, default_value = "10")]
        limit: usize,
    },
    /// Show memory statistics
    Stats,
    /// Export patterns to file
    Export {
        /// Output file path
        output: String,
    },
    /// Import patterns from file
    Import {
        /// Input file path
        input: String,
    },
    /// Index a codebase for semantic search
    Index {
        /// Path to index (directory)
        path: String,

        /// File extensions to index (comma-separated)
        #[arg(short, long, default_value = "rs,py,js,ts,go,java,c,cpp,h,hpp,jsx,tsx")]
        extensions: String,

        /// Patterns to exclude (glob format, comma-separated)
        #[arg(short = 'x', long, default_value = "**/node_modules/**,**/target/**,**/.git/**,**/vendor/**")]
        exclude: String,

        /// Batch size for embedding generation
        #[arg(short, long, default_value = "10")]
        batch_size: usize,

        /// Follow progress (poll until complete)
        #[arg(short, long)]
        follow: bool,
    },
    /// Check indexing job status
    IndexStatus {
        /// Job ID to check (omit to list recent jobs)
        job_id: Option<String>,
    },
    /// Search indexed code
    CodeSearch {
        /// Search query
        query: String,

        /// Maximum results
        #[arg(short, long, default_value = "10")]
        limit: usize,

        /// Filter by language
        #[arg(short = 'L', long)]
        language: Option<String>,
    },
}

pub async fn run(cmd: MemoryCommands) -> Result<()> {
    match cmd {
        MemoryCommands::Store {
            pattern,
            pattern_type,
        } => store(&pattern, &pattern_type).await,
        MemoryCommands::Search { query, limit } => search(&query, limit).await,
        MemoryCommands::Stats => stats().await,
        MemoryCommands::Export { output } => export(&output).await,
        MemoryCommands::Import { input } => import(&input).await,
        MemoryCommands::Index {
            path,
            extensions,
            exclude,
            batch_size,
            follow,
        } => index(&path, &extensions, &exclude, batch_size, follow).await,
        MemoryCommands::IndexStatus { job_id } => index_status(job_id.as_deref()).await,
        MemoryCommands::CodeSearch {
            query,
            limit,
            language,
        } => code_search(&query, limit, language.as_deref()).await,
    }
}

async fn store(pattern: &str, pattern_type: &str) -> Result<()> {
    println!("Storing pattern...");
    println!("Type: {pattern_type}");
    println!("Content: {pattern}");
    // TODO: Call daemon API
    println!("Pattern stored: <pattern-id>");
    Ok(())
}

async fn search(query: &str, limit: usize) -> Result<()> {
    println!("Searching patterns: \"{query}\" (limit: {limit})\n");

    let url = format!("{}/api/v1/memory/search", daemon_url());
    let resp = http::post_json(&url, &serde_json::json!({
        "query": query,
        "limit": limit as i32
    }))
    .await
    .context("Failed to search patterns")?;

    if !resp.status().is_success() {
        println!("Error: HTTP {}", resp.status());
        return Ok(());
    }

    let data: serde_json::Value = resp.json().await.context("Failed to parse response")?;

    if let Some(false) = data.get("success").and_then(serde_json::Value::as_bool) {
        println!(
            "Error: {}",
            data.get("error")
                .and_then(|v| v.as_str())
                .unwrap_or("Unknown error")
        );
        return Ok(());
    }

    println!(
        "{:<36} {:<12} {:<10} {:<30}",
        "ID", "TYPE", "SCORE", "CONTENT"
    );
    println!("{}", "-".repeat(90));

    if let Some(patterns) = data.get("patterns").and_then(|v| v.as_array()) {
        if patterns.is_empty() {
            println!("No patterns found");
        } else {
            for p in patterns {
                let id = p.get("id").and_then(|v| v.as_str()).unwrap_or("-");
                let ptype = p.get("pattern_type").and_then(|v| v.as_str()).unwrap_or("-");
                let score = p.get("similarity").and_then(serde_json::Value::as_f64).unwrap_or(0.0);
                let content = p.get("content").and_then(|v| v.as_str()).unwrap_or("-");
                let content_short = if content.len() > 27 {
                    format!("{}...", &content[..27])
                } else {
                    content.to_string()
                };
                println!("{id:<36} {ptype:<12} {score:<10.2} {content_short:<30}");
            }
        }
    }

    println!(
        "\nSearch type: {}",
        data.get("search_type").and_then(|v| v.as_str()).unwrap_or("unknown")
    );

    Ok(())
}

async fn stats() -> Result<()> {
    println!("Memory Statistics");
    println!("=================\n");
    println!("Total patterns: 0");
    println!("Pattern types:");
    println!("  - code: 0");
    println!("  - routing: 0");
    println!("  - error_handling: 0");
    println!("\nRedis:");
    println!("  - Connected: checking...");
    println!("  - Memory used: N/A");
    println!("\nPostgreSQL:");
    println!("  - Connected: checking...");
    println!("  - Total embeddings: N/A");
    // TODO: Call daemon API
    Ok(())
}

async fn export(output: &str) -> Result<()> {
    println!("Exporting patterns to {output}...");
    // TODO: Implement export
    println!("Export complete: 0 patterns");
    Ok(())
}

async fn import(input: &str) -> Result<()> {
    println!("Importing patterns from {input}...");
    // TODO: Implement import
    println!("Import complete: 0 patterns");
    Ok(())
}

// ============================================================================
// Codebase Indexing Commands
// ============================================================================

#[derive(Debug, Serialize)]
struct IndexRequest {
    path: String,
    extensions: Option<Vec<String>>,
    exclude_patterns: Option<Vec<String>>,
    batch_size: usize,
}

#[derive(Debug, Deserialize)]
struct IndexResponse {
    success: bool,
    job_id: Option<String>,
    message: Option<String>,
    error: Option<String>,
}

#[derive(Debug, Deserialize)]
struct JobStatus {
    job_id: String,
    path: String,
    status: String,
    total_files: i32,
    processed_files: i32,
    total_chunks: i32,
    indexed_chunks: i32,
    errors: Vec<String>,
    progress_percent: f32,
    started_at: Option<String>,
    completed_at: Option<String>,
}

#[derive(Debug, Deserialize)]
struct JobStatusResponse {
    success: bool,
    job: Option<JobStatus>,
    #[allow(dead_code)]
    error: Option<String>,
}

async fn index(
    path: &str,
    extensions: &str,
    exclude: &str,
    batch_size: usize,
    follow: bool,
) -> Result<()> {
    // Canonicalize path
    let abs_path = std::fs::canonicalize(path).unwrap_or_else(|_| path.into());

    println!("Indexing codebase: {}", abs_path.display());
    println!("Extensions: {extensions}");
    println!("Excluding: {exclude}");

    let ext_list: Vec<String> = extensions
        .split(',')
        .map(|s| s.trim().to_string())
        .collect();
    let exclude_list: Vec<String> = exclude
        .split(',')
        .map(|s| s.trim().to_string())
        .collect();

    let request = IndexRequest {
        path: abs_path.to_string_lossy().to_string(),
        extensions: Some(ext_list),
        exclude_patterns: Some(exclude_list),
        batch_size,
    };

    let url = format!("{}/api/v1/memory/index", daemon_url());
    let resp = http::post_json(&url, &request)
        .await
        .context("Failed to start indexing")?;

    if !resp.status().is_success() {
        println!("Error: HTTP {}", resp.status());
        return Ok(());
    }

    let response: IndexResponse = resp.json().await.context("Failed to parse response")?;

    if !response.success {
        println!(
            "Failed to start indexing: {}",
            response.error.unwrap_or_else(|| "Unknown error".to_string())
        );
        return Ok(());
    }

    let job_id = response.job_id.unwrap();
    if let Some(msg) = response.message {
        println!("{msg}");
    }
    println!("Indexing job started: {job_id}");

    if follow {
        println!("\nFollowing progress (Ctrl+C to stop)...\n");
        loop {
            tokio::time::sleep(Duration::from_secs(2)).await;

            let status_url = format!("{}/api/v1/memory/index/{}", daemon_url(), job_id);
            let status_resp = http::get(&status_url)
                .await
                .context("Failed to get job status")?;

            if !status_resp.status().is_success() {
                println!("Error checking status: HTTP {}", status_resp.status());
                break;
            }

            let data: JobStatusResponse = status_resp.json().await?;
            if let Some(job) = data.job {
                print!(
                    "\rProgress: {}/{} files, {} chunks indexed ({:.1}%)    ",
                    job.processed_files, job.total_files, job.indexed_chunks, job.progress_percent
                );

                if job.status == "completed" || job.status == "failed" || job.status == "cancelled" {
                    println!("\n\nIndexing {}", job.status);
                    if !job.errors.is_empty() {
                        println!("\nErrors ({}):", job.errors.len());
                        for (i, err) in job.errors.iter().take(5).enumerate() {
                            println!("  {}. {}", i + 1, err);
                        }
                        if job.errors.len() > 5 {
                            println!("  ... and {} more", job.errors.len() - 5);
                        }
                    }
                    break;
                }
            }
        }
    } else {
        println!("\nCheck status with: cca memory index-status {job_id}");
    }

    Ok(())
}

async fn index_status(job_id: Option<&str>) -> Result<()> {
    if let Some(id) = job_id {
        // Get specific job status
        let url = format!("{}/api/v1/memory/index/{}", daemon_url(), id);
        let resp = http::get(&url).await.context("Failed to get job status")?;

        if !resp.status().is_success() {
            println!("Error: HTTP {}", resp.status());
            return Ok(());
        }

        let data: JobStatusResponse = resp.json().await?;

        if !data.success {
            println!("Error: Job not found");
            return Ok(());
        }

        if let Some(job) = data.job {
            println!("Indexing Job Status");
            println!("==================\n");
            println!("Job ID: {}", job.job_id);
            println!("Path: {}", job.path);
            println!("Status: {}", job.status);
            println!(
                "Progress: {}/{} files ({:.1}%)",
                job.processed_files, job.total_files, job.progress_percent
            );
            println!(
                "Chunks: {}/{}",
                job.indexed_chunks, job.total_chunks
            );
            if let Some(started) = job.started_at {
                println!("Started: {started}");
            }
            if let Some(completed) = job.completed_at {
                println!("Completed: {completed}");
            }
        }
    } else {
        // List recent jobs
        let url = format!("{}/api/v1/memory/index/jobs", daemon_url());
        let resp = http::get(&url).await.context("Failed to list jobs")?;

        if !resp.status().is_success() {
            println!("Error: HTTP {}", resp.status());
            return Ok(());
        }

        let data: serde_json::Value = resp.json().await?;

        println!("Recent Indexing Jobs");
        println!("====================\n");
        println!(
            "{:<36} {:<12} {:<10} {:<20}",
            "JOB ID", "STATUS", "PROGRESS", "PATH"
        );
        println!("{}", "-".repeat(80));

        if let Some(jobs) = data.get("jobs").and_then(|v| v.as_array()) {
            if jobs.is_empty() {
                println!("No indexing jobs found");
            } else {
                for job in jobs {
                    let id = job.get("job_id").and_then(|v| v.as_str()).unwrap_or("-");
                    let status = job.get("status").and_then(|v| v.as_str()).unwrap_or("-");
                    let progress = job.get("progress_percent").and_then(serde_json::Value::as_f64).unwrap_or(0.0);
                    let path = job.get("path").and_then(|v| v.as_str()).unwrap_or("-");
                    let path_short = if path.len() > 17 {
                        format!("...{}", &path[path.len() - 17..])
                    } else {
                        path.to_string()
                    };
                    println!(
                        "{id:<36} {status:<12} {progress:<10.1}% {path_short:<20}"
                    );
                }
            }
        }
    }

    Ok(())
}

#[derive(Debug, Serialize)]
struct CodeSearchRequest {
    query: String,
    limit: i32,
    #[serde(skip_serializing_if = "Option::is_none")]
    language: Option<String>,
}

async fn code_search(query: &str, limit: usize, language: Option<&str>) -> Result<()> {
    println!("Searching code: \"{query}\"\n");

    let request = CodeSearchRequest {
        query: query.to_string(),
        limit: limit as i32,
        language: language.map(String::from),
    };

    let url = format!("{}/api/v1/code/search", daemon_url());
    let resp = http::post_json(&url, &request)
        .await
        .context("Failed to search code")?;

    if !resp.status().is_success() {
        println!("Error: HTTP {}", resp.status());
        return Ok(());
    }

    let data: serde_json::Value = resp.json().await.context("Failed to parse response")?;

    if let Some(false) = data.get("success").and_then(serde_json::Value::as_bool) {
        println!(
            "Error: {}",
            data.get("error")
                .and_then(|v| v.as_str())
                .unwrap_or("Unknown error")
        );
        return Ok(());
    }

    if let Some(results) = data.get("results").and_then(|v| v.as_array()) {
        if results.is_empty() {
            println!("No results found");
        } else {
            for (i, r) in results.iter().enumerate() {
                let name = r.get("name").and_then(|v| v.as_str()).unwrap_or("-");
                let chunk_type = r.get("chunk_type").and_then(|v| v.as_str()).unwrap_or("-");
                let file_path = r.get("file_path").and_then(|v| v.as_str()).unwrap_or("-");
                let start_line = r.get("start_line").and_then(serde_json::Value::as_i64).unwrap_or(0);
                let end_line = r.get("end_line").and_then(serde_json::Value::as_i64).unwrap_or(0);
                let language = r.get("language").and_then(|v| v.as_str()).unwrap_or("-");
                let similarity = r.get("similarity").and_then(serde_json::Value::as_f64).unwrap_or(0.0);
                let signature = r.get("signature").and_then(|v| v.as_str());

                println!("{}. {} {} ({})", i + 1, chunk_type, name, language);
                println!("   Location: {file_path}:{start_line}-{end_line}");
                println!("   Similarity: {similarity:.2}");
                if let Some(sig) = signature {
                    let sig_short = if sig.len() > 70 {
                        format!("{}...", &sig[..70])
                    } else {
                        sig.to_string()
                    };
                    println!("   Signature: {sig_short}");
                }
                println!();
            }
        }
    }

    println!(
        "Found {} result(s)",
        data.get("count").and_then(serde_json::Value::as_i64).unwrap_or(0)
    );

    Ok(())
}
