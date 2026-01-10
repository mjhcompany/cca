//! CCA Core - Core types, traits, and shared functionality
//!
//! This crate provides the foundational types used across all CCA components.

pub mod agent;
pub mod communication;
pub mod error;
pub mod memory;
pub mod task;
pub mod types;

pub use agent::{Agent, AgentId, AgentRole, AgentState};
pub use error::{CCAError, Result};
pub use task::{Task, TaskId, TaskResult, TaskStatus};
pub use types::*;
