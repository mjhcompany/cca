//! CCA Core - Core types, traits, and shared functionality
//!
//! This crate provides the foundational types used across all CCA components.

// Clippy pedantic allows - these are intentional design choices
#![allow(clippy::doc_markdown)]
#![allow(clippy::missing_errors_doc)]
#![allow(clippy::missing_panics_doc)]
#![allow(clippy::must_use_candidate)]
#![allow(clippy::return_self_not_must_use)]
#![allow(clippy::module_name_repetitions)]

pub mod agent;
pub mod communication;
pub mod error;
pub mod memory;
pub mod task;
pub mod types;
pub mod util;

pub use agent::{Agent, AgentId, AgentRole, AgentState};
pub use error::{CCAError, Result};
pub use task::{Task, TaskId, TaskResult, TaskStatus};
pub use types::*;
