//! CCA MCP Server - Model Context Protocol integration for Claude Code
//!
//! This crate provides the MCP server that integrates CCA with Claude Code.
//! When installed as a plugin, it exposes CCA functionality through MCP tools.

// Clippy pedantic allows - these are intentional design choices
#![allow(clippy::doc_markdown)]
#![allow(clippy::missing_errors_doc)]
#![allow(clippy::missing_panics_doc)]
#![allow(clippy::must_use_candidate)]
#![allow(clippy::return_self_not_must_use)]
#![allow(clippy::unused_self)]
#![allow(clippy::trivially_copy_pass_by_ref)]
#![allow(clippy::too_many_lines)]
#![allow(clippy::format_push_string)]
#![allow(clippy::cast_possible_truncation)]

pub mod client;
pub mod server;
pub mod tools;
pub mod types;

pub use client::DaemonClient;
pub use server::McpServer;
pub use types::*;
