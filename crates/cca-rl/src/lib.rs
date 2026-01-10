//! CCA RL - Reinforcement Learning algorithms for task optimization
//!
//! This crate provides RL algorithms for optimizing task routing,
//! token budgeting, and pattern recognition in CCA.

pub mod algorithm;
pub mod engine;
pub mod experience;
pub mod state;

pub use algorithm::RLAlgorithm;
pub use engine::RLEngine;
pub use experience::{Experience, ExperienceBuffer};
pub use state::{Action, Reward, State};
