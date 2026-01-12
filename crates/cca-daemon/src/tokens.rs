//! Token Efficiency Module
//!
//! Provides token counting, context analysis, compression strategies,
//! and monitoring for achieving 30%+ token reduction.
//!
//! Note: Many methods are infrastructure for future features and not yet called.
#![allow(dead_code)]

use std::collections::HashMap;
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;
use tracing::{debug, info};

use cca_core::AgentId;

/// Token counter using a simple BPE-like estimation
/// Based on GPT-4/Claude tokenization patterns (~4 chars per token average)
pub struct TokenCounter {
    /// Average characters per token (adjustable)
    chars_per_token: f64,
}

impl TokenCounter {
    pub fn new() -> Self {
        Self {
            chars_per_token: 4.0, // Conservative estimate for English text
        }
    }

    /// Estimate token count for a string
    pub fn count(&self, text: &str) -> u32 {
        if text.is_empty() {
            return 0;
        }

        // More accurate estimation considering:
        // - Whitespace often tokenizes separately
        // - Code has more tokens per character
        // - Punctuation often tokenizes separately
        let words = text.split_whitespace().count();
        let chars = text.len();

        // Heuristic: max of word-based and char-based estimates
        let word_estimate = (words as f64 * 1.3) as u32; // ~1.3 tokens per word
        let char_estimate = (chars as f64 / self.chars_per_token) as u32;

        word_estimate.max(char_estimate).max(1)
    }

    /// Count tokens in a structured message (JSON-like)
    pub fn count_message(&self, content: &str, role: &str) -> u32 {
        // Message overhead: role markers, formatting
        let overhead = 4; // Approximate overhead for message structure
        self.count(content) + self.count(role) + overhead
    }

    /// Count tokens in a conversation history
    pub fn count_conversation(&self, messages: &[ConversationMessage]) -> u32 {
        messages.iter().map(|m| self.count_message(&m.content, &m.role)).sum()
    }
}

impl Default for TokenCounter {
    fn default() -> Self {
        Self::new()
    }
}

/// A message in a conversation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConversationMessage {
    pub role: String,
    pub content: String,
    #[serde(default)]
    pub timestamp: Option<i64>,
}

/// Redundancy detection result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RedundancyResult {
    /// Similarity score between contexts (0.0 - 1.0)
    pub similarity: f64,
    /// Common content that could be deduplicated
    pub common_content: Vec<String>,
    /// Estimated tokens that could be saved
    pub potential_savings: u32,
}

/// Context analyzer for detecting redundancy across agents
pub struct ContextAnalyzer {
    counter: TokenCounter,
    /// N-gram size for similarity detection
    ngram_size: usize,
}

impl ContextAnalyzer {
    pub fn new() -> Self {
        Self {
            counter: TokenCounter::new(),
            ngram_size: 3,
        }
    }

    /// Analyze a single context for token usage
    pub fn analyze(&self, content: &str) -> ContextAnalysis {
        let total_tokens = self.counter.count(content);
        let lines: Vec<&str> = content.lines().collect();

        // Detect repeated patterns
        let mut line_counts: HashMap<&str, usize> = HashMap::new();
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

        // Detect code blocks (often high token density)
        let code_block_count = content.matches("```").count() / 2;

        // Detect long lines (potential for summarization)
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

    /// Compare two contexts for redundancy
    pub fn compare(&self, context_a: &str, context_b: &str) -> RedundancyResult {
        let ngrams_a = self.extract_ngrams(context_a);
        let ngrams_b = self.extract_ngrams(context_b);

        // Jaccard similarity of n-grams
        let intersection: usize = ngrams_a.iter().filter(|n| ngrams_b.contains(*n)).count();
        let union = ngrams_a.len() + ngrams_b.len() - intersection;

        let similarity = if union > 0 {
            intersection as f64 / union as f64
        } else {
            0.0
        };

        // Find common lines
        let lines_a: std::collections::HashSet<_> = context_a.lines().map(str::trim).filter(|l| !l.is_empty()).collect();
        let lines_b: std::collections::HashSet<_> = context_b.lines().map(str::trim).filter(|l| !l.is_empty()).collect();

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

    /// Extract n-grams from text for similarity detection
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

    /// Estimate how much the context could be compressed
    fn estimate_compression_potential(&self, content: &str) -> f64 {
        let total = self.counter.count(content) as f64;
        if total == 0.0 {
            return 0.0;
        }

        let analysis = self.analyze_compressibility(content);

        // Weight different factors
        let redundancy_factor = analysis.repetition_ratio * 0.4;
        let verbosity_factor = analysis.verbosity_score * 0.3;
        let structure_factor = analysis.structural_overhead * 0.3;

        (redundancy_factor + verbosity_factor + structure_factor).min(0.5) // Cap at 50%
    }

    fn analyze_compressibility(&self, content: &str) -> CompressibilityFactors {
        let lines: Vec<&str> = content.lines().collect();
        let total_lines = lines.len().max(1);

        // Repetition ratio
        let mut seen = std::collections::HashSet::new();
        let unique_lines = lines.iter().filter(|l| seen.insert(l.trim())).count();
        let repetition_ratio = 1.0 - (unique_lines as f64 / total_lines as f64);

        // Verbosity score (long explanations, filler words)
        let filler_words = ["basically", "essentially", "actually", "just", "simply",
                          "obviously", "clearly", "of course", "as you can see"];
        let filler_count: usize = filler_words
            .iter()
            .map(|w| content.to_lowercase().matches(w).count())
            .sum();
        let verbosity_score = (filler_count as f64 / (content.len() as f64 / 100.0)).min(1.0);

        // Structural overhead (headers, separators, etc.)
        let structural_chars = content.chars().filter(|c| matches!(c, '#' | '-' | '=' | '*' | '|')).count();
        let structural_overhead = (structural_chars as f64 / content.len() as f64).min(0.3);

        CompressibilityFactors {
            repetition_ratio,
            verbosity_score,
            structural_overhead,
        }
    }
}

impl Default for ContextAnalyzer {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug)]
struct CompressibilityFactors {
    repetition_ratio: f64,
    verbosity_score: f64,
    structural_overhead: f64,
}

/// Analysis results for a context
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContextAnalysis {
    pub total_tokens: u32,
    pub repeated_tokens: u32,
    pub repeated_lines: Vec<(String, usize)>,
    pub code_block_count: usize,
    pub long_line_count: usize,
    pub compression_potential: f64,
}

/// Context compression strategies
pub struct ContextCompressor {
    counter: TokenCounter,
}

impl ContextCompressor {
    pub fn new() -> Self {
        Self {
            counter: TokenCounter::new(),
        }
    }

    /// Prune old messages from conversation history
    /// Keeps system message, recent messages, and important context
    pub fn prune_history(
        &self,
        messages: &[ConversationMessage],
        max_tokens: u32,
        keep_recent: usize,
    ) -> Vec<ConversationMessage> {
        if messages.is_empty() {
            return vec![];
        }

        let mut result = Vec::new();
        let mut current_tokens = 0u32;

        // Always keep system message if present
        if let Some(first) = messages.first() {
            if first.role == "system" {
                let tokens = self.counter.count_message(&first.content, &first.role);
                result.push(first.clone());
                current_tokens += tokens;
            }
        }

        // Keep recent messages
        let recent_start = messages.len().saturating_sub(keep_recent);
        for msg in messages.iter().skip(recent_start) {
            if msg.role == "system" && result.iter().any(|m| m.role == "system") {
                continue; // Skip duplicate system message
            }

            let tokens = self.counter.count_message(&msg.content, &msg.role);
            if current_tokens + tokens <= max_tokens {
                result.push(msg.clone());
                current_tokens += tokens;
            }
        }

        result
    }

    /// Summarize a long message to reduce tokens
    #[allow(clippy::no_effect_underscore_binding)]
    pub fn summarize(&self, content: &str, target_reduction: f64) -> String {
        let original_tokens = self.counter.count(content);
        let _target_tokens = (original_tokens as f64 * (1.0 - target_reduction)) as u32;

        // Simple summarization: keep first and last parts, remove middle
        let lines: Vec<&str> = content.lines().collect();
        if lines.len() <= 5 {
            return content.to_string();
        }

        let keep_lines = (lines.len() as f64 * (1.0 - target_reduction)) as usize;
        let keep_start = keep_lines / 2;
        let keep_end = keep_lines - keep_start;

        let mut result = Vec::new();
        result.extend(lines.iter().take(keep_start));
        result.push("... [content summarized] ...");
        result.extend(lines.iter().rev().take(keep_end).rev());

        let summarized = result.join("\n");
        let new_tokens = self.counter.count(&summarized);

        debug!(
            "Summarized {} tokens to {} tokens ({:.1}% reduction)",
            original_tokens,
            new_tokens,
            (1.0 - new_tokens as f64 / original_tokens as f64) * 100.0
        );

        summarized
    }

    /// Remove redundant content between contexts
    pub fn deduplicate(&self, contexts: &[String]) -> Vec<String> {
        if contexts.len() <= 1 {
            return contexts.to_vec();
        }

        // Find common lines across all contexts
        let mut common_lines: std::collections::HashSet<&str> = contexts[0]
            .lines()
            .map(str::trim)
            .filter(|l| !l.is_empty())
            .collect();

        for ctx in contexts.iter().skip(1) {
            let ctx_lines: std::collections::HashSet<_> = ctx
                .lines()
                .map(str::trim)
                .filter(|l| !l.is_empty())
                .collect();
            common_lines = common_lines.intersection(&ctx_lines).copied().collect();
        }

        // Remove common lines from all but first context
        let mut result = vec![contexts[0].clone()];

        for ctx in contexts.iter().skip(1) {
            let deduped: Vec<&str> = ctx
                .lines()
                .filter(|line| !common_lines.contains(line.trim()))
                .collect();

            if deduped.is_empty() {
                result.push("[See shared context above]".to_string());
            } else {
                result.push(deduped.join("\n"));
            }
        }

        result
    }

    /// Compress code blocks by removing comments and extra whitespace
    pub fn compress_code(&self, content: &str) -> String {
        let mut result = String::new();
        let mut in_code_block = false;
        let mut code_lang = String::new();

        for line in content.lines() {
            if line.trim().starts_with("```") {
                in_code_block = !in_code_block;
                if in_code_block {
                    code_lang = line.trim().trim_start_matches("```").to_string();
                }
                result.push_str(line);
                result.push('\n');
                continue;
            }

            if in_code_block {
                // Remove single-line comments based on language
                let trimmed = line.trim();
                let is_comment = match code_lang.as_str() {
                    "rust" | "javascript" | "typescript" | "java" | "c" | "cpp" | "go" => {
                        trimmed.starts_with("//")
                    }
                    "python" | "ruby" | "shell" | "bash" => {
                        trimmed.starts_with('#') && !trimmed.starts_with("#!")
                    }
                    _ => false,
                };

                if !is_comment && !trimmed.is_empty() {
                    result.push_str(line);
                    result.push('\n');
                }
            } else {
                result.push_str(line);
                result.push('\n');
            }
        }

        result.trim_end().to_string()
    }
}

impl Default for ContextCompressor {
    fn default() -> Self {
        Self::new()
    }
}

/// Token usage metrics for monitoring
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenUsage {
    pub agent_id: AgentId,
    pub input_tokens: u32,
    pub output_tokens: u32,
    pub total_tokens: u32,
    pub context_tokens: u32,
    pub timestamp: i64,
}

/// Aggregated metrics for an agent
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AgentTokenMetrics {
    pub total_input: u64,
    pub total_output: u64,
    pub total_context: u64,
    pub message_count: u64,
    pub avg_input_per_message: f64,
    pub avg_output_per_message: f64,
    pub peak_context_size: u32,
    pub compression_savings: u64,
}

/// Token metrics tracker
pub struct TokenMetrics {
    /// Per-agent metrics
    agent_metrics: Arc<RwLock<HashMap<AgentId, AgentTokenMetrics>>>,
    /// Global metrics
    global_metrics: Arc<RwLock<GlobalTokenMetrics>>,
    counter: TokenCounter,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct GlobalTokenMetrics {
    pub total_tokens_used: u64,
    pub total_tokens_saved: u64,
    pub compression_ratio: f64,
    pub agents_tracked: usize,
}

impl TokenMetrics {
    pub fn new() -> Self {
        Self {
            agent_metrics: Arc::new(RwLock::new(HashMap::new())),
            global_metrics: Arc::new(RwLock::new(GlobalTokenMetrics::default())),
            counter: TokenCounter::new(),
        }
    }

    /// Record token usage for an agent
    pub async fn record(&self, usage: TokenUsage) {
        let mut metrics = self.agent_metrics.write().await;
        let agent = metrics.entry(usage.agent_id).or_default();

        agent.total_input += usage.input_tokens as u64;
        agent.total_output += usage.output_tokens as u64;
        agent.total_context += usage.context_tokens as u64;
        agent.message_count += 1;
        agent.avg_input_per_message = agent.total_input as f64 / agent.message_count as f64;
        agent.avg_output_per_message = agent.total_output as f64 / agent.message_count as f64;
        agent.peak_context_size = agent.peak_context_size.max(usage.context_tokens);

        // Update global metrics
        let mut global = self.global_metrics.write().await;
        global.total_tokens_used += usage.total_tokens as u64;
        global.agents_tracked = metrics.len();
    }

    /// Record compression savings
    pub async fn record_savings(&self, agent_id: AgentId, tokens_saved: u32) {
        let mut metrics = self.agent_metrics.write().await;
        if let Some(agent) = metrics.get_mut(&agent_id) {
            agent.compression_savings += tokens_saved as u64;
        }

        let mut global = self.global_metrics.write().await;
        global.total_tokens_saved += tokens_saved as u64;
        if global.total_tokens_used > 0 {
            global.compression_ratio =
                global.total_tokens_saved as f64 /
                (global.total_tokens_used + global.total_tokens_saved) as f64;
        }
    }

    /// Get metrics for a specific agent
    pub async fn get_agent_metrics(&self, agent_id: AgentId) -> Option<AgentTokenMetrics> {
        let metrics = self.agent_metrics.read().await;
        metrics.get(&agent_id).cloned()
    }

    /// Get all agent metrics
    pub async fn get_all_metrics(&self) -> HashMap<AgentId, AgentTokenMetrics> {
        let metrics = self.agent_metrics.read().await;
        metrics.clone()
    }

    /// Get global metrics
    pub async fn get_global_metrics(&self) -> GlobalTokenMetrics {
        let global = self.global_metrics.read().await;
        global.clone()
    }

    /// Get efficiency recommendations
    pub async fn get_recommendations(&self) -> Vec<TokenRecommendation> {
        let metrics = self.agent_metrics.read().await;
        let mut recommendations = Vec::new();

        for (agent_id, agent) in metrics.iter() {
            // High context usage
            if agent.peak_context_size > 50000 {
                recommendations.push(TokenRecommendation {
                    agent_id: *agent_id,
                    category: "context_size".to_string(),
                    severity: "high".to_string(),
                    message: format!(
                        "Agent has peak context of {} tokens. Consider pruning history.",
                        agent.peak_context_size
                    ),
                    potential_savings: agent.peak_context_size / 3,
                });
            }

            // Low compression ratio
            let total = agent.total_input + agent.total_output + agent.total_context;
            if total > 10000 && agent.compression_savings < total / 10 {
                recommendations.push(TokenRecommendation {
                    agent_id: *agent_id,
                    category: "compression".to_string(),
                    severity: "medium".to_string(),
                    message: "Low compression savings. Enable context compression.".to_string(),
                    potential_savings: (total / 5) as u32,
                });
            }

            // High average tokens per message
            if agent.avg_input_per_message > 2000.0 {
                recommendations.push(TokenRecommendation {
                    agent_id: *agent_id,
                    category: "verbosity".to_string(),
                    severity: "medium".to_string(),
                    message: format!(
                        "Average input is {:.0} tokens/message. Consider summarization.",
                        agent.avg_input_per_message
                    ),
                    potential_savings: (agent.avg_input_per_message * 0.3) as u32,
                });
            }
        }

        recommendations
    }
}

impl Default for TokenMetrics {
    fn default() -> Self {
        Self::new()
    }
}

/// A token efficiency recommendation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenRecommendation {
    pub agent_id: AgentId,
    pub category: String,
    pub severity: String,
    pub message: String,
    pub potential_savings: u32,
}

/// Token efficiency service combining all components
pub struct TokenService {
    pub counter: TokenCounter,
    pub analyzer: ContextAnalyzer,
    pub compressor: ContextCompressor,
    pub metrics: TokenMetrics,
}

impl TokenService {
    pub fn new() -> Self {
        info!("Token efficiency service initialized");
        Self {
            counter: TokenCounter::new(),
            analyzer: ContextAnalyzer::new(),
            compressor: ContextCompressor::new(),
            metrics: TokenMetrics::new(),
        }
    }

    /// Analyze and optionally compress a context
    pub async fn process_context(
        &self,
        agent_id: AgentId,
        content: &str,
        compress: bool,
    ) -> ProcessedContext {
        let analysis = self.analyzer.analyze(content);
        let original_tokens = analysis.total_tokens;

        let (processed_content, tokens_saved) = if compress && analysis.compression_potential > 0.1 {
            let compressed = self.compressor.compress_code(content);
            let new_tokens = self.counter.count(&compressed);
            let saved = original_tokens.saturating_sub(new_tokens);

            if saved > 0 {
                self.metrics.record_savings(agent_id, saved).await;
            }

            (compressed, saved)
        } else {
            (content.to_string(), 0)
        };

        ProcessedContext {
            content: processed_content,
            original_tokens,
            final_tokens: original_tokens - tokens_saved,
            tokens_saved,
            analysis,
        }
    }

    /// Get a summary of token efficiency
    pub async fn get_efficiency_summary(&self) -> EfficiencySummary {
        let global = self.metrics.get_global_metrics().await;
        let recommendations = self.metrics.get_recommendations().await;
        let agent_metrics = self.metrics.get_all_metrics().await;

        let total_potential_savings: u32 = recommendations
            .iter()
            .map(|r| r.potential_savings)
            .sum();

        EfficiencySummary {
            total_tokens_used: global.total_tokens_used,
            total_tokens_saved: global.total_tokens_saved,
            compression_ratio: global.compression_ratio,
            agents_tracked: agent_metrics.len(),
            recommendations_count: recommendations.len(),
            total_potential_savings,
            target_reduction: 0.30, // 30% target
            current_reduction: global.compression_ratio,
            on_track: global.compression_ratio >= 0.25, // Within 5% of target
        }
    }
}

impl Default for TokenService {
    fn default() -> Self {
        Self::new()
    }
}

/// Processed context result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProcessedContext {
    pub content: String,
    pub original_tokens: u32,
    pub final_tokens: u32,
    pub tokens_saved: u32,
    pub analysis: ContextAnalysis,
}

/// Overall efficiency summary
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EfficiencySummary {
    pub total_tokens_used: u64,
    pub total_tokens_saved: u64,
    pub compression_ratio: f64,
    pub agents_tracked: usize,
    pub recommendations_count: usize,
    pub total_potential_savings: u32,
    pub target_reduction: f64,
    pub current_reduction: f64,
    pub on_track: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_token_counter() {
        let counter = TokenCounter::new();

        // Empty string
        assert_eq!(counter.count(""), 0);

        // Simple text
        let tokens = counter.count("Hello world");
        assert!(tokens > 0);
        assert!(tokens < 10);

        // Longer text
        let long_text = "The quick brown fox jumps over the lazy dog. ".repeat(10);
        let long_tokens = counter.count(&long_text);
        assert!(long_tokens > 50);
    }

    #[test]
    fn test_context_analyzer() {
        let analyzer = ContextAnalyzer::new();

        let content = "Line one\nLine two\nLine one\nLine three";
        let analysis = analyzer.analyze(content);

        assert!(analysis.total_tokens > 0);
        assert!(analysis.repeated_tokens > 0);
        assert!(!analysis.repeated_lines.is_empty());
    }

    #[test]
    fn test_redundancy_detection() {
        let analyzer = ContextAnalyzer::new();

        let ctx_a = "The quick brown fox jumps over the lazy dog.";
        let ctx_b = "The quick brown fox runs through the forest.";

        let result = analyzer.compare(ctx_a, ctx_b);
        assert!(result.similarity > 0.0);
        assert!(result.similarity < 1.0);
    }

    #[test]
    fn test_context_compressor() {
        let compressor = ContextCompressor::new();

        // Test code compression
        let code = "```rust\n// This is a comment\nfn main() {\n    println!(\"Hello\");\n}\n```";
        let compressed = compressor.compress_code(code);
        assert!(!compressed.contains("// This is a comment"));
        assert!(compressed.contains("fn main()"));
    }

    #[test]
    fn test_history_pruning() {
        let compressor = ContextCompressor::new();

        let messages = vec![
            ConversationMessage { role: "system".to_string(), content: "You are helpful.".to_string(), timestamp: None },
            ConversationMessage { role: "user".to_string(), content: "Hello".to_string(), timestamp: None },
            ConversationMessage { role: "assistant".to_string(), content: "Hi there!".to_string(), timestamp: None },
            ConversationMessage { role: "user".to_string(), content: "How are you?".to_string(), timestamp: None },
            ConversationMessage { role: "assistant".to_string(), content: "I'm doing well!".to_string(), timestamp: None },
        ];

        let pruned = compressor.prune_history(&messages, 100, 2);
        assert!(pruned.len() <= 3); // System + 2 recent
        assert_eq!(pruned[0].role, "system");
    }

    #[tokio::test]
    async fn test_token_metrics() {
        let metrics = TokenMetrics::new();
        let agent_id = AgentId::new();

        let usage = TokenUsage {
            agent_id,
            input_tokens: 100,
            output_tokens: 50,
            total_tokens: 150,
            context_tokens: 1000,
            timestamp: 0,
        };

        metrics.record(usage).await;

        let agent_metrics = metrics.get_agent_metrics(agent_id).await.unwrap();
        assert_eq!(agent_metrics.total_input, 100);
        assert_eq!(agent_metrics.total_output, 50);
    }
}
