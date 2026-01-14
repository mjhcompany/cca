//! CCA ACP - Agent Client Protocol implementation
//!
//! This crate implements the ACP WebSocket protocol for inter-agent communication.
//!
//! ## Features
//!
//! - WebSocket server with JSON-RPC 2.0 support
//! - WebSocket client with automatic reconnection
//! - Per-agent connection tracking
//! - Heartbeat mechanism
//! - Request/response correlation

// Clippy pedantic allows - these are intentional design choices
#![allow(clippy::doc_markdown)]
#![allow(clippy::missing_errors_doc)]
#![allow(clippy::missing_panics_doc)]
#![allow(clippy::must_use_candidate)]
#![allow(clippy::return_self_not_must_use)]
#![allow(clippy::unused_async)]
#![allow(clippy::unused_self)]
#![allow(clippy::map_unwrap_or)]
#![allow(clippy::cast_lossless)]
#![allow(clippy::single_match_else)]
#![allow(clippy::match_same_arms)]
#![allow(clippy::redundant_else)]
#![allow(clippy::cast_precision_loss)]
#![allow(clippy::manual_let_else)]
#![allow(clippy::too_many_lines)]

pub mod client;
pub mod message;
pub mod server;

pub use client::{AcpClient, AcpClientConfig, ConnectionState};
pub use message::*;
pub use server::{
    AcpAuthConfig, AcpServer, AgentConnection, ApiKeyMetadata, BackpressureConfig,
    BackpressureMetrics, BroadcastResult, ConnectionBackpressureInfo, DefaultHandler,
    MessageHandler, SendResult, TaskResponse,
};

// Re-export core ACP types
pub use cca_core::communication::{AcpError, AcpMessage};
