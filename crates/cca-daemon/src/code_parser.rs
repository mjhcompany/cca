//! Code parser for extracting functions, classes, and methods from source files.
//!
//! Uses tree-sitter for accurate AST parsing across multiple programming languages.

use std::collections::HashMap;
use std::path::Path;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use tracing::debug;

/// Supported programming languages
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CodeLanguage {
    Rust,
    Python,
    JavaScript,
    TypeScript,
    Go,
    Java,
    C,
    Cpp,
}

impl CodeLanguage {
    /// Get the language name as a string
    pub fn as_str(&self) -> &'static str {
        match self {
            CodeLanguage::Rust => "rust",
            CodeLanguage::Python => "python",
            CodeLanguage::JavaScript => "javascript",
            CodeLanguage::TypeScript => "typescript",
            CodeLanguage::Go => "go",
            CodeLanguage::Java => "java",
            CodeLanguage::C => "c",
            CodeLanguage::Cpp => "cpp",
        }
    }

    /// Detect language from file extension
    pub fn from_extension(ext: &str) -> Option<Self> {
        match ext.to_lowercase().as_str() {
            "rs" => Some(CodeLanguage::Rust),
            "py" => Some(CodeLanguage::Python),
            "js" | "jsx" | "mjs" => Some(CodeLanguage::JavaScript),
            "ts" | "tsx" | "mts" => Some(CodeLanguage::TypeScript),
            "go" => Some(CodeLanguage::Go),
            "java" => Some(CodeLanguage::Java),
            "c" | "h" => Some(CodeLanguage::C),
            "cpp" | "cc" | "cxx" | "hpp" | "hxx" => Some(CodeLanguage::Cpp),
            _ => None,
        }
    }

    /// Get tree-sitter language
    fn tree_sitter_language(&self) -> tree_sitter::Language {
        match self {
            CodeLanguage::Rust => tree_sitter_rust::LANGUAGE.into(),
            CodeLanguage::Python => tree_sitter_python::LANGUAGE.into(),
            CodeLanguage::JavaScript => tree_sitter_javascript::LANGUAGE.into(),
            CodeLanguage::TypeScript => tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into(),
            CodeLanguage::Go => tree_sitter_go::LANGUAGE.into(),
            CodeLanguage::Java => tree_sitter_java::LANGUAGE.into(),
            CodeLanguage::C => tree_sitter_c::LANGUAGE.into(),
            CodeLanguage::Cpp => tree_sitter_cpp::LANGUAGE.into(),
        }
    }
}

/// Type of code chunk extracted
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ChunkType {
    Function,
    Class,
    Method,
    Struct,
    Interface,
    Trait,
    Impl,
    Enum,
    Module,
}

impl ChunkType {
    pub fn as_str(&self) -> &'static str {
        match self {
            ChunkType::Function => "function",
            ChunkType::Class => "class",
            ChunkType::Method => "method",
            ChunkType::Struct => "struct",
            ChunkType::Interface => "interface",
            ChunkType::Trait => "trait",
            ChunkType::Impl => "impl",
            ChunkType::Enum => "enum",
            ChunkType::Module => "module",
        }
    }
}

/// A parsed code chunk
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodeChunk {
    pub file_path: String,
    pub chunk_type: ChunkType,
    pub name: String,
    pub signature: Option<String>,
    pub content: String,
    pub start_line: u32,
    pub end_line: u32,
    pub language: String,
    pub metadata: serde_json::Value,
}

/// Code parser service using tree-sitter
pub struct CodeParser {
    parsers: HashMap<CodeLanguage, tree_sitter::Parser>,
}

impl CodeParser {
    /// Create a new code parser with all supported languages
    pub fn new() -> Result<Self> {
        let mut parsers = HashMap::new();

        for lang in [
            CodeLanguage::Rust,
            CodeLanguage::Python,
            CodeLanguage::JavaScript,
            CodeLanguage::TypeScript,
            CodeLanguage::Go,
            CodeLanguage::Java,
            CodeLanguage::C,
            CodeLanguage::Cpp,
        ] {
            let mut parser = tree_sitter::Parser::new();
            parser
                .set_language(&lang.tree_sitter_language())
                .context(format!("Failed to set {} language", lang.as_str()))?;
            parsers.insert(lang, parser);
        }

        Ok(Self { parsers })
    }

    /// Detect language from file path
    pub fn detect_language(path: &Path) -> Option<CodeLanguage> {
        path.extension()
            .and_then(|ext| ext.to_str())
            .and_then(CodeLanguage::from_extension)
    }

    /// Parse a file and extract code chunks
    pub fn parse_file(&mut self, path: &Path) -> Result<Vec<CodeChunk>> {
        let language = Self::detect_language(path)
            .ok_or_else(|| anyhow::anyhow!("Unsupported file type: {}", path.display()))?;

        let content = std::fs::read_to_string(path)
            .context(format!("Failed to read file: {}", path.display()))?;

        let file_path = path.to_string_lossy().to_string();
        self.parse_content(&content, language, &file_path)
    }

    /// Parse content with known language
    pub fn parse_content(
        &mut self,
        content: &str,
        language: CodeLanguage,
        file_path: &str,
    ) -> Result<Vec<CodeChunk>> {
        let parser = self.parsers.get_mut(&language)
            .ok_or_else(|| anyhow::anyhow!("No parser for language: {language:?}"))?;

        let tree = parser
            .parse(content, None)
            .ok_or_else(|| anyhow::anyhow!("Failed to parse content"))?;

        let root_node = tree.root_node();
        let mut chunks = Vec::new();

        self.extract_chunks(
            &root_node,
            content.as_bytes(),
            language,
            file_path,
            &mut chunks,
        );

        debug!(
            "Parsed {} chunks from {} ({})",
            chunks.len(),
            file_path,
            language.as_str()
        );

        Ok(chunks)
    }

    /// Extract chunks from AST node recursively
    fn extract_chunks(
        &self,
        node: &tree_sitter::Node,
        source: &[u8],
        language: CodeLanguage,
        file_path: &str,
        chunks: &mut Vec<CodeChunk>,
    ) {
        // Check if this node is a chunk we want to extract
        if let Some(chunk) = self.node_to_chunk(node, source, language, file_path) {
            chunks.push(chunk);
        }

        // Recurse into children
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            self.extract_chunks(&child, source, language, file_path, chunks);
        }
    }

    /// Convert an AST node to a code chunk if applicable
    fn node_to_chunk(
        &self,
        node: &tree_sitter::Node,
        source: &[u8],
        language: CodeLanguage,
        file_path: &str,
    ) -> Option<CodeChunk> {
        let chunk_type = self.get_chunk_type(node.kind(), language)?;
        let name = self.extract_name(node, source, language)?;

        let content = node.utf8_text(source).ok()?.to_string();
        let signature = self.extract_signature(node, source, language);

        // Skip very small chunks (likely not meaningful)
        if content.len() < 20 {
            return None;
        }

        let start_line = node.start_position().row as u32 + 1;
        let end_line = node.end_position().row as u32 + 1;

        let metadata = serde_json::json!({
            "node_kind": node.kind(),
            "byte_range": [node.start_byte(), node.end_byte()],
        });

        Some(CodeChunk {
            file_path: file_path.to_string(),
            chunk_type,
            name,
            signature,
            content,
            start_line,
            end_line,
            language: language.as_str().to_string(),
            metadata,
        })
    }

    /// Map tree-sitter node kind to ChunkType
    fn get_chunk_type(&self, kind: &str, language: CodeLanguage) -> Option<ChunkType> {
        match language {
            CodeLanguage::Rust => match kind {
                "function_item" => Some(ChunkType::Function),
                "struct_item" => Some(ChunkType::Struct),
                "enum_item" => Some(ChunkType::Enum),
                "trait_item" => Some(ChunkType::Trait),
                "impl_item" => Some(ChunkType::Impl),
                "mod_item" => Some(ChunkType::Module),
                _ => None,
            },
            CodeLanguage::Python => match kind {
                "function_definition" => Some(ChunkType::Function),
                "class_definition" => Some(ChunkType::Class),
                _ => None,
            },
            CodeLanguage::JavaScript | CodeLanguage::TypeScript => match kind {
                "function_declaration" => Some(ChunkType::Function),
                "arrow_function" => Some(ChunkType::Function),
                "method_definition" => Some(ChunkType::Method),
                "class_declaration" => Some(ChunkType::Class),
                "interface_declaration" => Some(ChunkType::Interface),
                _ => None,
            },
            CodeLanguage::Go => match kind {
                "function_declaration" => Some(ChunkType::Function),
                "method_declaration" => Some(ChunkType::Method),
                "type_declaration" => Some(ChunkType::Struct),
                _ => None,
            },
            CodeLanguage::Java => match kind {
                "method_declaration" => Some(ChunkType::Method),
                "class_declaration" => Some(ChunkType::Class),
                "interface_declaration" => Some(ChunkType::Interface),
                "enum_declaration" => Some(ChunkType::Enum),
                _ => None,
            },
            CodeLanguage::C => match kind {
                "function_definition" => Some(ChunkType::Function),
                "struct_specifier" => Some(ChunkType::Struct),
                "enum_specifier" => Some(ChunkType::Enum),
                _ => None,
            },
            CodeLanguage::Cpp => match kind {
                "function_definition" => Some(ChunkType::Function),
                "class_specifier" => Some(ChunkType::Class),
                "struct_specifier" => Some(ChunkType::Struct),
                "enum_specifier" => Some(ChunkType::Enum),
                _ => None,
            },
        }
    }

    /// Extract the name of a code element
    fn extract_name(
        &self,
        node: &tree_sitter::Node,
        source: &[u8],
        language: CodeLanguage,
    ) -> Option<String> {
        let name_field = match language {
            CodeLanguage::Rust => match node.kind() {
                "function_item" | "struct_item" | "enum_item" | "trait_item" | "mod_item" => "name",
                "impl_item" => "type",
                _ => return None,
            },
            CodeLanguage::Python => match node.kind() {
                "function_definition" | "class_definition" => "name",
                _ => return None,
            },
            CodeLanguage::JavaScript | CodeLanguage::TypeScript => match node.kind() {
                "function_declaration" | "class_declaration" | "interface_declaration" => "name",
                "method_definition" => "name",
                "arrow_function" => {
                    // Arrow functions might be assigned to a variable
                    if let Some(parent) = node.parent() {
                        if parent.kind() == "variable_declarator" {
                            return parent
                                .child_by_field_name("name")
                                .and_then(|n| n.utf8_text(source).ok())
                                .map(std::string::ToString::to_string);
                        }
                    }
                    return Some("anonymous".to_string());
                }
                _ => return None,
            },
            CodeLanguage::Go => match node.kind() {
                "function_declaration" | "method_declaration" => "name",
                "type_declaration" => {
                    // Go type declarations have a type_spec child
                    // First try field name, then search children
                    let type_spec = node.child_by_field_name("type").or_else(|| {
                        let mut cursor = node.walk();
                        let children: Vec<_> = node.children(&mut cursor).collect();
                        children.into_iter().find(|c| c.kind() == "type_spec")
                    });
                    return type_spec
                        .and_then(|spec| spec.child_by_field_name("name"))
                        .and_then(|n| n.utf8_text(source).ok())
                        .map(std::string::ToString::to_string);
                }
                _ => return None,
            },
            CodeLanguage::Java => match node.kind() {
                "method_declaration" | "class_declaration" | "interface_declaration" | "enum_declaration" => "name",
                _ => return None,
            },
            CodeLanguage::C | CodeLanguage::Cpp => match node.kind() {
                "function_definition" => {
                    // C/C++ function definitions have declarator child
                    return node
                        .child_by_field_name("declarator")
                        .and_then(|d| Self::find_identifier(&d, source));
                }
                "struct_specifier" | "class_specifier" | "enum_specifier" => "name",
                _ => return None,
            },
        };

        node.child_by_field_name(name_field)
            .and_then(|n| n.utf8_text(source).ok())
            .map(std::string::ToString::to_string)
    }

    /// Find identifier in a declarator (for C/C++)
    fn find_identifier(node: &tree_sitter::Node, source: &[u8]) -> Option<String> {
        if node.kind() == "identifier" {
            return node.utf8_text(source).ok().map(std::string::ToString::to_string);
        }

        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            if let Some(name) = Self::find_identifier(&child, source) {
                return Some(name);
            }
        }

        None
    }

    /// Extract function/method signature
    fn extract_signature(
        &self,
        node: &tree_sitter::Node,
        source: &[u8],
        language: CodeLanguage,
    ) -> Option<String> {
        match language {
            CodeLanguage::Rust => {
                // For Rust, get the first line (usually the signature)
                let text = node.utf8_text(source).ok()?;
                let first_line = text.lines().next()?;
                Some(first_line.trim().to_string())
            }
            CodeLanguage::Python => {
                // Get def line
                let text = node.utf8_text(source).ok()?;
                let first_line = text.lines().next()?;
                Some(first_line.trim().to_string())
            }
            CodeLanguage::JavaScript | CodeLanguage::TypeScript => {
                // Get first line
                let text = node.utf8_text(source).ok()?;
                let first_line = text.lines().next()?;
                Some(first_line.trim().to_string())
            }
            CodeLanguage::Go => {
                let text = node.utf8_text(source).ok()?;
                let first_line = text.lines().next()?;
                Some(first_line.trim().to_string())
            }
            CodeLanguage::Java => {
                let text = node.utf8_text(source).ok()?;
                // Find the line with the method/class signature
                for line in text.lines() {
                    let trimmed = line.trim();
                    if trimmed.contains('(') || trimmed.starts_with("class ") || trimmed.starts_with("interface ") {
                        return Some(trimmed.to_string());
                    }
                }
                text.lines().next().map(|s| s.trim().to_string())
            }
            CodeLanguage::C | CodeLanguage::Cpp => {
                let text = node.utf8_text(source).ok()?;
                // Get up to the opening brace
                if let Some(brace_pos) = text.find('{') {
                    let sig = text[..brace_pos].trim();
                    // Collapse multiple lines
                    let sig = sig.split_whitespace().collect::<Vec<_>>().join(" ");
                    Some(sig)
                } else {
                    text.lines().next().map(|s| s.trim().to_string())
                }
            }
        }
    }
}

impl Default for CodeParser {
    fn default() -> Self {
        Self::new().expect("Failed to create default CodeParser")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_language_detection() {
        assert_eq!(
            CodeLanguage::from_extension("rs"),
            Some(CodeLanguage::Rust)
        );
        assert_eq!(
            CodeLanguage::from_extension("py"),
            Some(CodeLanguage::Python)
        );
        assert_eq!(
            CodeLanguage::from_extension("ts"),
            Some(CodeLanguage::TypeScript)
        );
        assert_eq!(
            CodeLanguage::from_extension("tsx"),
            Some(CodeLanguage::TypeScript)
        );
        assert_eq!(CodeLanguage::from_extension("unknown"), None);
    }

    #[test]
    fn test_parse_rust_function() {
        let mut parser = CodeParser::new().unwrap();
        let content = r#"
fn hello_world(name: &str) -> String {
    format!("Hello, {}!", name)
}
"#;

        let chunks = parser
            .parse_content(content, CodeLanguage::Rust, "test.rs")
            .unwrap();

        assert_eq!(chunks.len(), 1);
        assert_eq!(chunks[0].chunk_type, ChunkType::Function);
        assert_eq!(chunks[0].name, "hello_world");
        assert_eq!(chunks[0].language, "rust");
    }

    #[test]
    fn test_parse_python_class() {
        let mut parser = CodeParser::new().unwrap();
        let content = r#"
class MyClass:
    def __init__(self, value):
        self.value = value

    def get_value(self):
        return self.value
"#;

        let chunks = parser
            .parse_content(content, CodeLanguage::Python, "test.py")
            .unwrap();

        assert!(!chunks.is_empty());
        assert!(chunks.iter().any(|c| c.chunk_type == ChunkType::Class && c.name == "MyClass"));
    }

    #[test]
    fn test_parse_typescript_interface() {
        let mut parser = CodeParser::new().unwrap();
        let content = r#"
interface User {
    id: number;
    name: string;
    email: string;
}

function getUser(id: number): User {
    return { id, name: "Test", email: "test@example.com" };
}
"#;

        let chunks = parser
            .parse_content(content, CodeLanguage::TypeScript, "test.ts")
            .unwrap();

        assert!(chunks.len() >= 2);
        assert!(chunks.iter().any(|c| c.chunk_type == ChunkType::Interface));
        assert!(chunks.iter().any(|c| c.chunk_type == ChunkType::Function));
    }
}
