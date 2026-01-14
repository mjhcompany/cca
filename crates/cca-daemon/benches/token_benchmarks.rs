//! Token Counting and Analysis Benchmarks
//!
//! Benchmarks for critical token counting paths:
//! - TokenCounter::count() - Basic token counting
//! - TokenCounter::count_message() - Message token counting
//! - TokenCounter::count_conversation() - Conversation token counting
//! - ContextAnalyzer::analyze() - Context analysis
//! - ContextAnalyzer::compare() - Context comparison for redundancy
//!
//! ## Hot Paths Identified
//! 1. TokenCounter::count() - Called on every piece of content processed
//! 2. ContextAnalyzer::analyze() - Called for each agent context
//! 3. ContextAnalyzer::extract_ngrams() - O(n) where n = word count
//!
//! ## Performance Targets
//! - Token counting: < 1µs for typical messages (< 1KB)
//! - Context analysis: < 100µs for typical contexts (< 10KB)
//! - Redundancy detection: < 1ms for comparing two contexts

#![allow(dead_code)]

use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};

// We need to access internal types - using a local module path
mod tokens_bench {
    // Token counter using a simple BPE-like estimation
    pub struct TokenCounter {
        chars_per_token: f64,
    }

    impl TokenCounter {
        pub fn new() -> Self {
            Self {
                chars_per_token: 4.0,
            }
        }

        pub fn count(&self, text: &str) -> u32 {
            if text.is_empty() {
                return 0;
            }

            let words = text.split_whitespace().count();
            let chars = text.len();

            let word_estimate = (words as f64 * 1.3) as u32;
            let char_estimate = (chars as f64 / self.chars_per_token) as u32;

            word_estimate.max(char_estimate).max(1)
        }

        pub fn count_message(&self, content: &str, role: &str) -> u32 {
            let overhead = 4;
            self.count(content) + self.count(role) + overhead
        }

        pub fn count_conversation(&self, messages: &[ConversationMessage]) -> u32 {
            messages
                .iter()
                .map(|m| self.count_message(&m.content, &m.role))
                .sum()
        }
    }

    #[derive(Clone)]
    pub struct ConversationMessage {
        pub role: String,
        pub content: String,
    }

    pub struct ContextAnalyzer {
        counter: TokenCounter,
        ngram_size: usize,
    }

    impl ContextAnalyzer {
        pub fn new() -> Self {
            Self {
                counter: TokenCounter::new(),
                ngram_size: 3,
            }
        }

        pub fn analyze(&self, content: &str) -> ContextAnalysis {
            let total_tokens = self.counter.count(content);
            let lines: Vec<&str> = content.lines().collect();

            let mut line_counts: std::collections::HashMap<&str, usize> =
                std::collections::HashMap::new();
            for line in &lines {
                let trimmed = line.trim();
                if !trimmed.is_empty() {
                    *line_counts.entry(trimmed).or_default() += 1;
                }
            }

            let repeated_lines: Vec<_> = line_counts
                .iter()
                .filter(|(_, &count)| count > 1)
                .map(|(&line, &count)| (line.to_string(), count))
                .collect();

            let repeated_tokens: u32 = repeated_lines
                .iter()
                .map(|(line, count)| self.counter.count(line) * (*count as u32 - 1))
                .sum();

            let code_block_count = content.matches("```").count() / 2;
            let long_lines = lines.iter().filter(|l| l.len() > 200).count();

            ContextAnalysis {
                total_tokens,
                repeated_tokens,
                repeated_lines,
                code_block_count,
                long_line_count: long_lines,
                compression_potential: self.estimate_compression_potential(content),
            }
        }

        pub fn compare(&self, context_a: &str, context_b: &str) -> RedundancyResult {
            let ngrams_a = self.extract_ngrams(context_a);
            let ngrams_b = self.extract_ngrams(context_b);

            let intersection: usize = ngrams_a.iter().filter(|n| ngrams_b.contains(*n)).count();
            let union = ngrams_a.len() + ngrams_b.len() - intersection;

            let similarity = if union > 0 {
                intersection as f64 / union as f64
            } else {
                0.0
            };

            let lines_a: std::collections::HashSet<_> = context_a
                .lines()
                .map(str::trim)
                .filter(|l| !l.is_empty())
                .collect();
            let lines_b: std::collections::HashSet<_> = context_b
                .lines()
                .map(str::trim)
                .filter(|l| !l.is_empty())
                .collect();

            let common_content: Vec<String> = lines_a
                .intersection(&lines_b)
                .map(std::string::ToString::to_string)
                .collect();

            let potential_savings: u32 = common_content
                .iter()
                .map(|line| self.counter.count(line))
                .sum();

            RedundancyResult {
                similarity,
                common_content,
                potential_savings,
            }
        }

        fn extract_ngrams(&self, text: &str) -> std::collections::HashSet<String> {
            let words: Vec<&str> = text.split_whitespace().collect();
            let mut ngrams = std::collections::HashSet::new();

            if words.len() >= self.ngram_size {
                for window in words.windows(self.ngram_size) {
                    ngrams.insert(window.join(" "));
                }
            }

            ngrams
        }

        fn estimate_compression_potential(&self, content: &str) -> f64 {
            let total = self.counter.count(content) as f64;
            if total == 0.0 {
                return 0.0;
            }

            let lines: Vec<&str> = content.lines().collect();
            let total_lines = lines.len().max(1);

            let mut seen = std::collections::HashSet::new();
            let unique_lines = lines.iter().filter(|l| seen.insert(l.trim())).count();
            let repetition_ratio = 1.0 - (unique_lines as f64 / total_lines as f64);

            let filler_words = [
                "basically",
                "essentially",
                "actually",
                "just",
                "simply",
                "obviously",
                "clearly",
                "of course",
                "as you can see",
            ];
            let filler_count: usize = filler_words
                .iter()
                .map(|w| content.to_lowercase().matches(w).count())
                .sum();
            let verbosity_score = (filler_count as f64 / (content.len() as f64 / 100.0)).min(1.0);

            let structural_chars = content
                .chars()
                .filter(|c| matches!(c, '#' | '-' | '=' | '*' | '|'))
                .count();
            let structural_overhead = (structural_chars as f64 / content.len() as f64).min(0.3);

            let redundancy_factor = repetition_ratio * 0.4;
            let verbosity_factor = verbosity_score * 0.3;
            let structure_factor = structural_overhead * 0.3;

            (redundancy_factor + verbosity_factor + structure_factor).min(0.5)
        }
    }

    #[derive(Debug)]
    pub struct ContextAnalysis {
        pub total_tokens: u32,
        pub repeated_tokens: u32,
        pub repeated_lines: Vec<(String, usize)>,
        pub code_block_count: usize,
        pub long_line_count: usize,
        pub compression_potential: f64,
    }

    #[derive(Debug)]
    pub struct RedundancyResult {
        pub similarity: f64,
        pub common_content: Vec<String>,
        pub potential_savings: u32,
    }
}

use tokens_bench::*;

/// Generate test content of specified size
fn generate_content(size_bytes: usize) -> String {
    let base = "The quick brown fox jumps over the lazy dog. ";
    let repeat_count = size_bytes / base.len() + 1;
    base.repeat(repeat_count)[..size_bytes].to_string()
}

/// Generate content with repeated lines (for redundancy testing)
fn generate_redundant_content(size_bytes: usize, repetition_ratio: f64) -> String {
    let base_line = "This is a line that will be repeated multiple times.\n";
    let unique_line = |i: usize| format!("Unique line number {i} with some text.\n");

    let target_lines = size_bytes / 50; // ~50 chars per line
    let repeated_lines = (target_lines as f64 * repetition_ratio) as usize;
    let unique_lines_count = target_lines - repeated_lines;

    let mut content = String::with_capacity(size_bytes);

    // Add repeated lines
    for _ in 0..repeated_lines {
        content.push_str(base_line);
    }

    // Add unique lines
    for i in 0..unique_lines_count {
        content.push_str(&unique_line(i));
    }

    content.truncate(size_bytes);
    content
}

/// Generate code content with comments
fn generate_code_content(size_bytes: usize) -> String {
    let code = r#"```rust
// This is a comment that should be removed
fn main() {
    // Another comment
    println!("Hello, world!");
    let x = 42;
    // Calculate something
    let result = x * 2;
}
```
"#;
    let repeat_count = size_bytes / code.len() + 1;
    code.repeat(repeat_count)[..size_bytes.min(code.repeat(repeat_count).len())].to_string()
}

/// Generate conversation messages
fn generate_conversation(message_count: usize) -> Vec<ConversationMessage> {
    (0..message_count)
        .map(|i| ConversationMessage {
            role: if i % 2 == 0 { "user" } else { "assistant" }.to_string(),
            content: format!(
                "This is message number {i}. It contains some content about the task at hand."
            ),
        })
        .collect()
}

// ============================================================================
// Token Counting Benchmarks
// ============================================================================

fn bench_token_count_small(c: &mut Criterion) {
    let counter = TokenCounter::new();
    let content = generate_content(100); // 100 bytes

    c.bench_function("token_count/small_100B", |b| {
        b.iter(|| counter.count(black_box(&content)))
    });
}

fn bench_token_count_sizes(c: &mut Criterion) {
    let counter = TokenCounter::new();
    let sizes = [100, 500, 1_000, 5_000, 10_000, 50_000, 100_000];

    let mut group = c.benchmark_group("token_count/by_size");
    for size in sizes {
        let content = generate_content(size);
        group.throughput(Throughput::Bytes(size as u64));
        group.bench_with_input(BenchmarkId::from_parameter(size), &content, |b, content| {
            b.iter(|| counter.count(black_box(content)))
        });
    }
    group.finish();
}

fn bench_token_count_message(c: &mut Criterion) {
    let counter = TokenCounter::new();
    let content = "This is a typical user message asking for help with a coding task.";
    let role = "user";

    c.bench_function("token_count/message", |b| {
        b.iter(|| counter.count_message(black_box(content), black_box(role)))
    });
}

fn bench_token_count_conversation(c: &mut Criterion) {
    let counter = TokenCounter::new();
    let sizes = [5, 10, 25, 50, 100];

    let mut group = c.benchmark_group("token_count/conversation");
    for size in sizes {
        let messages = generate_conversation(size);
        group.bench_with_input(
            BenchmarkId::from_parameter(size),
            &messages,
            |b, messages| b.iter(|| counter.count_conversation(black_box(messages))),
        );
    }
    group.finish();
}

// ============================================================================
// Context Analysis Benchmarks
// ============================================================================

fn bench_context_analyze(c: &mut Criterion) {
    let analyzer = ContextAnalyzer::new();
    let sizes = [1_000, 5_000, 10_000, 25_000, 50_000];

    let mut group = c.benchmark_group("context_analyze/by_size");
    for size in sizes {
        let content = generate_content(size);
        group.throughput(Throughput::Bytes(size as u64));
        group.bench_with_input(BenchmarkId::from_parameter(size), &content, |b, content| {
            b.iter(|| analyzer.analyze(black_box(content)))
        });
    }
    group.finish();
}

fn bench_context_analyze_redundant(c: &mut Criterion) {
    let analyzer = ContextAnalyzer::new();
    let ratios = [0.0, 0.25, 0.5, 0.75];

    let mut group = c.benchmark_group("context_analyze/redundancy");
    for ratio in ratios {
        let content = generate_redundant_content(10_000, ratio);
        let label = format!("{}%", (ratio * 100.0) as u32);
        group.bench_with_input(BenchmarkId::from_parameter(label), &content, |b, content| {
            b.iter(|| analyzer.analyze(black_box(content)))
        });
    }
    group.finish();
}

fn bench_context_analyze_code(c: &mut Criterion) {
    let analyzer = ContextAnalyzer::new();
    let content = generate_code_content(10_000);

    c.bench_function("context_analyze/code_10KB", |b| {
        b.iter(|| analyzer.analyze(black_box(&content)))
    });
}

// ============================================================================
// Redundancy Detection Benchmarks
// ============================================================================

fn bench_redundancy_compare(c: &mut Criterion) {
    let analyzer = ContextAnalyzer::new();
    let sizes = [1_000, 5_000, 10_000];

    let mut group = c.benchmark_group("redundancy_compare/by_size");
    for size in sizes {
        let content_a = generate_content(size);
        let content_b = generate_content(size);
        group.bench_with_input(
            BenchmarkId::from_parameter(size),
            &(content_a.clone(), content_b.clone()),
            |b, (a, b_content)| b.iter(|| analyzer.compare(black_box(a), black_box(b_content))),
        );
    }
    group.finish();
}

fn bench_redundancy_compare_similar(c: &mut Criterion) {
    let analyzer = ContextAnalyzer::new();

    // Create two contexts with 50% shared content
    let shared = "Shared content that appears in both contexts.\n".repeat(100);
    let unique_a = "Unique to context A.\n".repeat(50);
    let unique_b = "Unique to context B.\n".repeat(50);

    let context_a = format!("{shared}{unique_a}");
    let context_b = format!("{shared}{unique_b}");

    c.bench_function("redundancy_compare/50%_similar", |b| {
        b.iter(|| analyzer.compare(black_box(&context_a), black_box(&context_b)))
    });
}

// ============================================================================
// N-gram Extraction Benchmarks (Hot Path)
// ============================================================================

fn bench_ngram_extraction(c: &mut Criterion) {
    let analyzer = ContextAnalyzer::new();
    let sizes = [100, 500, 1_000, 5_000, 10_000];

    let mut group = c.benchmark_group("ngram_extraction/by_size");
    for size in sizes {
        let content = generate_content(size);
        group.throughput(Throughput::Bytes(size as u64));
        // We benchmark compare since extract_ngrams is private
        // but compare calls it twice
        group.bench_with_input(BenchmarkId::from_parameter(size), &content, |b, content| {
            b.iter(|| analyzer.compare(black_box(content), black_box(content)))
        });
    }
    group.finish();
}

// ============================================================================
// Criterion Groups
// ============================================================================

criterion_group!(
    name = token_counting;
    config = Criterion::default();
    targets =
        bench_token_count_small,
        bench_token_count_sizes,
        bench_token_count_message,
        bench_token_count_conversation,
);

criterion_group!(
    name = context_analysis;
    config = Criterion::default();
    targets =
        bench_context_analyze,
        bench_context_analyze_redundant,
        bench_context_analyze_code,
);

criterion_group!(
    name = redundancy_detection;
    config = Criterion::default();
    targets =
        bench_redundancy_compare,
        bench_redundancy_compare_similar,
        bench_ngram_extraction,
);

criterion_main!(token_counting, context_analysis, redundancy_detection);
