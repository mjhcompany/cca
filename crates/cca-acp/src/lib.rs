//! CCA ACP - Agent Client Protocol implementation
//!
//! This crate implements the ACP WebSocket protocol for inter-agent communication.

pub mod client;
pub mod message;
pub mod server;

pub use client::AcpClient;
pub use message::*;
pub use server::AcpServer;

// Re-export core ACP types
pub use cca_core::communication::{AcpError, AcpMessage};
