//! Code Parsing and Indexing Benchmarks
//!
//! Benchmarks for tree-sitter parsing and code chunk extraction.
//! These are CPU-intensive operations used when indexing codebases.
//!
//! ## Hot Paths
//! - Language detection from file extension
//! - Tree-sitter parsing of source files
//! - AST traversal for chunk extraction
//! - Code signature generation

use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use std::collections::HashMap;

// ============================================================================
// Simulated Code Parser Components
// ============================================================================

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum CodeLanguage {
    Rust,
    Python,
    JavaScript,
    TypeScript,
    Go,
    Java,
}

impl CodeLanguage {
    fn from_extension(ext: &str) -> Option<Self> {
        match ext.to_lowercase().as_str() {
            "rs" => Some(CodeLanguage::Rust),
            "py" => Some(CodeLanguage::Python),
            "js" | "jsx" | "mjs" => Some(CodeLanguage::JavaScript),
            "ts" | "tsx" | "mts" => Some(CodeLanguage::TypeScript),
            "go" => Some(CodeLanguage::Go),
            "java" => Some(CodeLanguage::Java),
            _ => None,
        }
    }

    fn as_str(&self) -> &'static str {
        match self {
            CodeLanguage::Rust => "rust",
            CodeLanguage::Python => "python",
            CodeLanguage::JavaScript => "javascript",
            CodeLanguage::TypeScript => "typescript",
            CodeLanguage::Go => "go",
            CodeLanguage::Java => "java",
        }
    }
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
struct CodeChunk {
    name: String,
    chunk_type: String,
    language: String,
    content: String,
    start_line: usize,
    end_line: usize,
    signature: String,
}

/// Simulated code parser (exercises similar algorithms as real parser)
struct CodeParser {
    language_patterns: HashMap<CodeLanguage, Vec<&'static str>>,
}

impl CodeParser {
    fn new() -> Self {
        let mut language_patterns = HashMap::new();

        // Patterns for identifying code structures
        language_patterns.insert(
            CodeLanguage::Rust,
            vec!["fn ", "struct ", "impl ", "trait ", "enum ", "mod "],
        );
        language_patterns.insert(
            CodeLanguage::Python,
            vec!["def ", "class ", "async def "],
        );
        language_patterns.insert(
            CodeLanguage::JavaScript,
            vec!["function ", "class ", "const ", "let ", "async function "],
        );
        language_patterns.insert(
            CodeLanguage::TypeScript,
            vec!["function ", "class ", "interface ", "type ", "const "],
        );
        language_patterns.insert(CodeLanguage::Go, vec!["func ", "type ", "struct "]);
        language_patterns.insert(
            CodeLanguage::Java,
            vec!["public ", "private ", "class ", "interface "],
        );

        Self { language_patterns }
    }

    fn parse(&self, content: &str, language: CodeLanguage) -> Vec<CodeChunk> {
        let mut chunks = Vec::new();
        let patterns = self.language_patterns.get(&language).unwrap_or(&vec![]);
        let lines: Vec<&str> = content.lines().collect();

        let mut current_chunk: Option<(usize, String, String)> = None;
        let mut brace_depth = 0;

        for (line_num, line) in lines.iter().enumerate() {
            let trimmed = line.trim();

            // Check for new chunk start
            for pattern in patterns {
                if trimmed.starts_with(pattern) {
                    // Save previous chunk if any
                    if let Some((start, name, chunk_type)) = current_chunk.take() {
                        chunks.push(CodeChunk {
                            name: name.clone(),
                            chunk_type,
                            language: language.as_str().to_string(),
                            content: lines[start..line_num].join("\n"),
                            start_line: start,
                            end_line: line_num - 1,
                            signature: self.generate_signature(&name, language),
                        });
                    }

                    // Extract name (simplified)
                    let name = self.extract_name(trimmed, pattern);
                    current_chunk = Some((line_num, name, pattern.trim().to_string()));
                    break;
                }
            }

            // Track brace depth for chunk boundaries
            brace_depth += line.matches('{').count();
            brace_depth = brace_depth.saturating_sub(line.matches('}').count());
        }

        // Handle last chunk
        if let Some((start, name, chunk_type)) = current_chunk {
            chunks.push(CodeChunk {
                name: name.clone(),
                chunk_type,
                language: language.as_str().to_string(),
                content: lines[start..].join("\n"),
                start_line: start,
                end_line: lines.len() - 1,
                signature: self.generate_signature(&name, language),
            });
        }

        chunks
    }

    fn extract_name(&self, line: &str, pattern: &str) -> String {
        let after_pattern = line.strip_prefix(pattern).unwrap_or(line);
        after_pattern
            .split(|c: char| !c.is_alphanumeric() && c != '_')
            .next()
            .unwrap_or("unknown")
            .to_string()
    }

    fn generate_signature(&self, name: &str, language: CodeLanguage) -> String {
        format!("{}::{}", language.as_str(), name)
    }

    fn detect_language(&self, filename: &str) -> Option<CodeLanguage> {
        let ext = filename.rsplit('.').next()?;
        CodeLanguage::from_extension(ext)
    }
}

// ============================================================================
// Test Data Generators
// ============================================================================

fn generate_rust_code(functions: usize) -> String {
    let mut code = String::new();
    code.push_str("//! Example Rust module\n\n");

    for i in 0..functions {
        code.push_str(&format!(
            r#"
/// Function {i} documentation
fn function_{i}(x: i32, y: i32) -> i32 {{
    let result = x + y;
    if result > 100 {{
        return result * 2;
    }}
    result
}}
"#
        ));
    }

    // Add some structs
    for i in 0..(functions / 5).max(1) {
        code.push_str(&format!(
            r#"
/// Struct {i}
struct MyStruct{i} {{
    field_a: String,
    field_b: i32,
    field_c: Vec<u8>,
}}

impl MyStruct{i} {{
    fn new() -> Self {{
        Self {{
            field_a: String::new(),
            field_b: 0,
            field_c: Vec::new(),
        }}
    }}

    fn process(&self) -> i32 {{
        self.field_b * 2
    }}
}}
"#
        ));
    }

    code
}

fn generate_python_code(functions: usize) -> String {
    let mut code = String::new();
    code.push_str("\"\"\"Example Python module\"\"\"\n\n");

    for i in 0..functions {
        code.push_str(&format!(
            r#"
def function_{i}(x: int, y: int) -> int:
    """Function {i} documentation."""
    result = x + y
    if result > 100:
        return result * 2
    return result
"#
        ));
    }

    // Add some classes
    for i in 0..(functions / 5).max(1) {
        code.push_str(&format!(
            r#"
class MyClass{i}:
    """Class {i} documentation."""

    def __init__(self):
        self.field_a = ""
        self.field_b = 0

    def process(self) -> int:
        return self.field_b * 2
"#
        ));
    }

    code
}

fn generate_javascript_code(functions: usize) -> String {
    let mut code = String::new();
    code.push_str("// Example JavaScript module\n\n");

    for i in 0..functions {
        code.push_str(&format!(
            r#"
/**
 * Function {i} documentation
 */
function function_{i}(x, y) {{
    const result = x + y;
    if (result > 100) {{
        return result * 2;
    }}
    return result;
}}
"#
        ));
    }

    // Add some classes
    for i in 0..(functions / 5).max(1) {
        code.push_str(&format!(
            r#"
/**
 * Class {i}
 */
class MyClass{i} {{
    constructor() {{
        this.fieldA = "";
        this.fieldB = 0;
    }}

    process() {{
        return this.fieldB * 2;
    }}
}}
"#
        ));
    }

    code
}

fn generate_filenames(count: usize) -> Vec<String> {
    let extensions = ["rs", "py", "js", "ts", "go", "java", "txt", "md", "json"];
    (0..count)
        .map(|i| format!("file_{}.{}", i, extensions[i % extensions.len()]))
        .collect()
}

// ============================================================================
// Benchmarks
// ============================================================================

fn bench_language_detection(c: &mut Criterion) {
    let parser = CodeParser::new();
    let filenames = generate_filenames(1000);

    let mut group = c.benchmark_group("code_indexing/language_detection");

    group.throughput(Throughput::Elements(1000));
    group.bench_function("1000_files", |b| {
        b.iter(|| {
            for filename in &filenames {
                let _ = parser.detect_language(black_box(filename));
            }
        })
    });

    group.finish();
}

fn bench_code_parsing(c: &mut Criterion) {
    let parser = CodeParser::new();

    let mut group = c.benchmark_group("code_indexing/parsing");

    // Benchmark different file sizes
    for func_count in [10, 25, 50, 100].iter() {
        let rust_code = generate_rust_code(*func_count);
        group.throughput(Throughput::Bytes(rust_code.len() as u64));
        group.bench_with_input(
            BenchmarkId::new("rust", func_count),
            &rust_code,
            |b, code| b.iter(|| parser.parse(black_box(code), CodeLanguage::Rust)),
        );
    }

    // Compare languages with same function count
    let func_count = 50;
    let python_code = generate_python_code(func_count);
    let js_code = generate_javascript_code(func_count);

    group.bench_function("python_50_funcs", |b| {
        b.iter(|| parser.parse(black_box(&python_code), CodeLanguage::Python))
    });

    group.bench_function("javascript_50_funcs", |b| {
        b.iter(|| parser.parse(black_box(&js_code), CodeLanguage::JavaScript))
    });

    group.finish();
}

fn bench_chunk_extraction(c: &mut Criterion) {
    let parser = CodeParser::new();
    let rust_code = generate_rust_code(100);

    c.bench_function("code_indexing/chunk_extraction_100_items", |b| {
        b.iter(|| {
            let chunks = parser.parse(black_box(&rust_code), CodeLanguage::Rust);
            // Simulate further processing
            for chunk in &chunks {
                let _ = black_box(&chunk.signature);
            }
            chunks.len()
        })
    });
}

fn bench_signature_generation(c: &mut Criterion) {
    let parser = CodeParser::new();
    let names: Vec<String> = (0..1000).map(|i| format!("function_{}", i)).collect();

    let mut group = c.benchmark_group("code_indexing/signature_gen");

    group.throughput(Throughput::Elements(1000));
    group.bench_function("1000_signatures", |b| {
        b.iter(|| {
            for name in &names {
                let _ = parser.generate_signature(black_box(name), CodeLanguage::Rust);
            }
        })
    });

    group.finish();
}

fn bench_full_indexing_pipeline(c: &mut Criterion) {
    let parser = CodeParser::new();

    // Simulate indexing a small project
    let files = vec![
        ("main.rs", generate_rust_code(20)),
        ("lib.rs", generate_rust_code(30)),
        ("utils.py", generate_python_code(15)),
        ("app.js", generate_javascript_code(25)),
    ];

    c.bench_function("code_indexing/full_pipeline_4_files", |b| {
        b.iter(|| {
            let mut total_chunks = 0;
            for (filename, content) in &files {
                if let Some(lang) = parser.detect_language(filename) {
                    let chunks = parser.parse(content, lang);
                    total_chunks += chunks.len();
                }
            }
            total_chunks
        })
    });
}

// ============================================================================
// Criterion Groups
// ============================================================================

criterion_group!(
    benches,
    bench_language_detection,
    bench_code_parsing,
    bench_chunk_extraction,
    bench_signature_generation,
    bench_full_indexing_pipeline,
);

criterion_main!(benches);
