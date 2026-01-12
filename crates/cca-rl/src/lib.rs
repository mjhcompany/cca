//! CCA RL - Reinforcement Learning algorithms for task optimization
//!
//! This crate provides RL algorithms for optimizing task routing,
//! token budgeting, and pattern recognition in CCA.

// Clippy pedantic allows - these are intentional design choices
#![allow(clippy::doc_markdown)]
#![allow(clippy::missing_errors_doc)]
#![allow(clippy::missing_panics_doc)]
#![allow(clippy::must_use_candidate)]
#![allow(clippy::return_self_not_must_use)]
#![allow(clippy::cast_precision_loss)]
#![allow(clippy::cast_possible_truncation)]
#![allow(clippy::cast_sign_loss)]
#![allow(clippy::cast_lossless)]
#![allow(clippy::float_cmp)]
#![allow(clippy::similar_names)]

pub mod algorithm;
pub mod engine;
pub mod experience;
pub mod state;

pub use algorithm::RLAlgorithm;
pub use engine::RLEngine;
pub use experience::{Experience, ExperienceBuffer};
pub use state::{Action, Reward, State};
