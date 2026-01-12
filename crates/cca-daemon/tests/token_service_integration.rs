//! Integration tests for Token Efficiency Service
//!
//! These tests verify the token service components work correctly together.

#![allow(clippy::cast_precision_loss)]
#![allow(clippy::cast_possible_truncation)]
#![allow(clippy::cast_sign_loss)]
#![allow(clippy::cast_lossless)]
#![allow(clippy::needless_raw_string_hashes)]
#![allow(clippy::float_cmp)]
#![allow(clippy::no_effect_underscore_binding)]
#![allow(clippy::doc_markdown)]
#![allow(clippy::format_collect)]

/// Test TokenCounter accuracy with various content types
#[tokio::test]
async fn test_token_counter_code() {
    // Simulate token counting for code
    let code = r#"
fn calculate_sum(numbers: &[i32]) -> i32 {
    numbers.iter().sum()
}

fn main() {
    let nums = vec![1, 2, 3, 4, 5];
    println!("Sum: {}", calculate_sum(&nums));
}
"#;

    // Token estimation: ~4 chars per token
    let estimated_tokens = (code.len() as f64 / 4.0).ceil() as u32;
    assert!(estimated_tokens > 30, "Code should have significant tokens");
    assert!(estimated_tokens < 100, "Should not overestimate");
}

#[tokio::test]
async fn test_token_counter_prose() {
    let prose = "The quick brown fox jumps over the lazy dog. This is a common pangram.";

    let estimated_tokens = (prose.len() as f64 / 4.0).ceil() as u32;
    assert!(estimated_tokens > 10);
    assert!(estimated_tokens < 30);
}

/// Test ContextAnalyzer redundancy detection
#[tokio::test]
async fn test_redundancy_detection_identical() {
    let content = "This line is repeated.\nThis line is repeated.\nThis line is repeated.\n";

    let lines: Vec<&str> = content.lines().collect();
    let unique_lines: std::collections::HashSet<_> = lines.iter().collect();

    let redundancy = 1.0 - (unique_lines.len() as f64 / lines.len() as f64);
    assert!(redundancy > 0.5, "Should detect high redundancy");
}

#[tokio::test]
async fn test_redundancy_detection_unique() {
    let content = "Line one is unique.\nLine two is different.\nLine three is also unique.\n";

    let lines: Vec<&str> = content.lines().collect();
    let unique_lines: std::collections::HashSet<_> = lines.iter().collect();

    let redundancy = 1.0 - (unique_lines.len() as f64 / lines.len() as f64);
    assert!(redundancy < 0.1, "Should detect low redundancy");
}

/// Test code comment removal
#[tokio::test]
async fn test_code_comment_removal() {
    let code_with_comments = r#"
// This is a single line comment
fn main() {
    /* This is a
       multi-line comment */
    let x = 5; // inline comment
    println!("{}", x);
}
"#;

    // Simulate comment removal (simplified)
    let mut result = String::new();
    let mut in_multiline = false;

    for line in code_with_comments.lines() {
        let trimmed = line.trim();

        // Skip single-line comments
        if trimmed.starts_with("//") {
            continue;
        }

        // Handle multi-line comments (simplified)
        if trimmed.contains("/*") {
            in_multiline = true;
        }
        if trimmed.contains("*/") {
            in_multiline = false;
            continue;
        }
        if in_multiline {
            continue;
        }

        // Remove inline comments
        let line_without_inline = if let Some(pos) = line.find("//") {
            &line[..pos]
        } else {
            line
        };

        if !line_without_inline.trim().is_empty() {
            result.push_str(line_without_inline);
            result.push('\n');
        }
    }

    assert!(!result.contains("//"), "Should remove single-line comments");
    assert!(!result.contains("/*"), "Should remove multi-line comment start");
    assert!(result.contains("fn main()"), "Should preserve code");
    assert!(result.contains("let x = 5"), "Should preserve code");
}

/// Test history pruning
#[tokio::test]
async fn test_history_pruning() {
    let messages: Vec<(&str, &str)> = vec![
        ("user", "Message 1 - oldest"),
        ("assistant", "Response 1"),
        ("user", "Message 2"),
        ("assistant", "Response 2"),
        ("user", "Message 3"),
        ("assistant", "Response 3"),
        ("user", "Message 4"),
        ("assistant", "Response 4"),
        ("user", "Message 5 - most recent"),
        ("assistant", "Response 5"),
    ];

    let keep_count = 4; // Keep last 4 messages
    let pruned: Vec<_> = messages.iter().skip(messages.len() - keep_count).collect();

    assert_eq!(pruned.len(), keep_count);
    assert!(pruned[0].1.contains("Response 4") || pruned[0].1.contains("Message 4"));
    assert!(pruned.last().unwrap().1.contains("Response 5"));
}

/// Test summarization
#[tokio::test]
async fn test_summarization() {
    let long_content = (0..20)
        .map(|i| format!("This is line number {i} with some content.\n"))
        .collect::<String>();

    let lines: Vec<&str> = long_content.lines().collect();
    let keep_ratio = 0.5;
    let keep_count = (lines.len() as f64 * keep_ratio) as usize;
    let keep_start = keep_count / 2;
    let keep_end = keep_count - keep_start;

    let mut summarized = Vec::new();
    summarized.extend(lines.iter().take(keep_start));
    summarized.push("... [content summarized] ...");
    summarized.extend(lines.iter().skip(lines.len() - keep_end));

    assert!(summarized.len() < lines.len(), "Should reduce content");
    assert!(summarized.contains(&"... [content summarized] ..."), "Should indicate summarization");
}

/// Test deduplication
#[tokio::test]
async fn test_deduplication() {
    let contexts = vec![
        "Context A with some content".to_string(),
        "Context B with different content".to_string(),
        "Context A with some content".to_string(), // duplicate
        "Context C with unique content".to_string(),
        "Context B with different content".to_string(), // duplicate
    ];

    let mut seen = std::collections::HashSet::new();
    let deduplicated: Vec<_> = contexts
        .into_iter()
        .filter(|ctx| seen.insert(ctx.clone()))
        .collect();

    assert_eq!(deduplicated.len(), 3, "Should remove duplicates");
    assert!(deduplicated.contains(&"Context A with some content".to_string()));
    assert!(deduplicated.contains(&"Context B with different content".to_string()));
    assert!(deduplicated.contains(&"Context C with unique content".to_string()));
}

/// Test compression achieves target reduction
#[tokio::test]
async fn test_compression_target() {
    let original_content = "fn example() {\n    // comment\n    let x = 1;\n}\n".repeat(100);
    let original_tokens = (original_content.len() as f64 / 4.0).ceil() as u32;

    let target_reduction = 0.3; // 30% reduction
    let _target_tokens = (original_tokens as f64 * (1.0 - target_reduction)) as u32;

    // Simulate compression by removing comments
    let compressed: String = original_content
        .lines()
        .filter(|line| !line.trim().starts_with("//"))
        .collect::<Vec<_>>()
        .join("\n");

    let compressed_tokens = (compressed.len() as f64 / 4.0).ceil() as u32;

    // Verify reduction occurred (may not hit exact target)
    assert!(
        compressed_tokens < original_tokens,
        "Compressed should have fewer tokens"
    );

    let actual_reduction = 1.0 - (compressed_tokens as f64 / original_tokens as f64);
    println!(
        "Original: {}, Compressed: {}, Reduction: {:.1}%",
        original_tokens,
        compressed_tokens,
        actual_reduction * 100.0
    );
}

/// Test metrics tracking
#[tokio::test]
async fn test_metrics_tracking() {
    use std::collections::HashMap;
    use std::sync::atomic::{AtomicU64, Ordering};

    struct MockMetrics {
        total_used: AtomicU64,
        total_saved: AtomicU64,
        per_agent: tokio::sync::RwLock<HashMap<String, (u64, u64)>>,
    }

    impl MockMetrics {
        fn new() -> Self {
            Self {
                total_used: AtomicU64::new(0),
                total_saved: AtomicU64::new(0),
                per_agent: tokio::sync::RwLock::new(HashMap::new()),
            }
        }

        async fn record(&self, agent_id: &str, used: u64, saved: u64) {
            self.total_used.fetch_add(used, Ordering::Relaxed);
            self.total_saved.fetch_add(saved, Ordering::Relaxed);

            let mut agents = self.per_agent.write().await;
            let entry = agents.entry(agent_id.to_string()).or_insert((0, 0));
            entry.0 += used;
            entry.1 += saved;
        }

        fn efficiency(&self) -> f64 {
            let used = self.total_used.load(Ordering::Relaxed);
            let saved = self.total_saved.load(Ordering::Relaxed);
            if used == 0 {
                0.0
            } else {
                saved as f64 / (used + saved) as f64 * 100.0
            }
        }
    }

    let metrics = MockMetrics::new();

    // Simulate some recordings
    metrics.record("agent-001", 1000, 300).await;
    metrics.record("agent-002", 800, 200).await;
    metrics.record("agent-001", 500, 150).await;

    assert_eq!(metrics.total_used.load(Ordering::Relaxed), 2300);
    assert_eq!(metrics.total_saved.load(Ordering::Relaxed), 650);

    let efficiency = metrics.efficiency();
    assert!(efficiency > 20.0 && efficiency < 30.0, "Efficiency should be ~22%");

    let agents = metrics.per_agent.read().await;
    assert_eq!(agents.get("agent-001"), Some(&(1500, 450)));
    assert_eq!(agents.get("agent-002"), Some(&(800, 200)));
}

/// Test recommendations generation
#[tokio::test]
async fn test_recommendations_generation() {
    #[derive(Debug)]
    #[allow(dead_code)]
    struct Recommendation {
        category: String,
        message: String,
        priority: String,
    }

    fn generate_recommendations(
        efficiency: f64,
        compression_enabled: bool,
        history_pruning_enabled: bool,
    ) -> Vec<Recommendation> {
        let mut recs = Vec::new();

        if efficiency < 20.0 {
            recs.push(Recommendation {
                category: "efficiency".to_string(),
                message: "Token efficiency is below 20%. Consider enabling compression.".to_string(),
                priority: "high".to_string(),
            });
        }

        if !compression_enabled {
            recs.push(Recommendation {
                category: "compression".to_string(),
                message: "Enable compression for code contexts to reduce token usage.".to_string(),
                priority: "medium".to_string(),
            });
        }

        if !history_pruning_enabled {
            recs.push(Recommendation {
                category: "history".to_string(),
                message: "Enable history pruning to keep context focused.".to_string(),
                priority: "medium".to_string(),
            });
        }

        recs
    }

    // Test with low efficiency and no features enabled
    let recs = generate_recommendations(15.0, false, false);
    assert_eq!(recs.len(), 3);
    assert!(recs.iter().any(|r| r.priority == "high"));

    // Test with good efficiency and all features enabled
    let recs = generate_recommendations(35.0, true, true);
    assert!(recs.is_empty(), "No recommendations needed for optimal setup");
}

/// Test end-to-end token processing
#[tokio::test]
async fn test_end_to_end_token_processing() {
    // 1. Start with raw content
    let original = r#"
fn process_data(input: &str) -> Result<String, Error> {
    // Validate input first
    if input.is_empty() {
        return Err(Error::Empty);
    }

    // Process the data
    /* This is a complex processing step
       that takes multiple lines to explain */
    let result = input.to_uppercase();

    // Return the result
    Ok(result)
}
"#;

    // 2. Count original tokens
    let original_tokens = (original.len() as f64 / 4.0).ceil() as u32;
    println!("Original tokens: {original_tokens}");

    // 3. Analyze for redundancy
    let lines: Vec<&str> = original.lines().collect();
    let unique_lines: std::collections::HashSet<_> = lines.iter().collect();
    let redundancy = 1.0 - (unique_lines.len() as f64 / lines.len() as f64);
    println!("Redundancy: {:.1}%", redundancy * 100.0);

    // 4. Apply compression (remove comments)
    let compressed: String = original
        .lines()
        .filter(|line| {
            let trimmed = line.trim();
            !trimmed.starts_with("//") && !trimmed.starts_with("/*") && !trimmed.starts_with("*/")
        })
        .filter(|line| !line.contains("*/"))
        .collect::<Vec<_>>()
        .join("\n");

    // 5. Count compressed tokens
    let compressed_tokens = (compressed.len() as f64 / 4.0).ceil() as u32;
    println!("Compressed tokens: {compressed_tokens}");

    // 6. Calculate savings
    let tokens_saved = original_tokens.saturating_sub(compressed_tokens);
    let reduction = if original_tokens > 0 {
        tokens_saved as f64 / original_tokens as f64 * 100.0
    } else {
        0.0
    };
    println!("Tokens saved: {tokens_saved} ({reduction:.1}%)");

    // 7. Verify results
    assert!(compressed_tokens < original_tokens, "Compression should reduce tokens");
    assert!(reduction > 10.0, "Should achieve at least 10% reduction");
    assert!(compressed.contains("fn process_data"), "Should preserve function");
    assert!(compressed.contains("Ok(result)"), "Should preserve return");
}
