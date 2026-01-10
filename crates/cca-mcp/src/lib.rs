//! CCA MCP Server - Model Context Protocol integration for Claude Code
//!
//! This crate provides the MCP server that integrates CCA with Claude Code.
//! When installed as a plugin, it exposes CCA functionality through MCP tools.

pub mod client;
pub mod server;
pub mod tools;
pub mod types;

pub use client::DaemonClient;
pub use server::McpServer;
pub use types::*;
