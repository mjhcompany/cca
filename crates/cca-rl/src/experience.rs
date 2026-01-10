//! Experience replay buffer for RL

use std::collections::VecDeque;

use rand::seq::SliceRandom;
use serde::{Deserialize, Serialize};

use crate::state::{Action, Reward, State};

/// A single experience tuple (s, a, r, s', done)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Experience {
    pub state: State,
    pub action: Action,
    pub reward: Reward,
    pub next_state: Option<State>,
    pub done: bool,
}

impl Experience {
    /// Create a new experience
    pub fn new(
        state: State,
        action: Action,
        reward: Reward,
        next_state: Option<State>,
        done: bool,
    ) -> Self {
        Self {
            state,
            action,
            reward,
            next_state,
            done,
        }
    }
}

/// Experience replay buffer
pub struct ExperienceBuffer {
    buffer: VecDeque<Experience>,
    capacity: usize,
}

impl ExperienceBuffer {
    /// Create a new experience buffer with given capacity
    pub fn new(capacity: usize) -> Self {
        Self {
            buffer: VecDeque::with_capacity(capacity),
            capacity,
        }
    }

    /// Add an experience to the buffer
    pub fn push(&mut self, experience: Experience) {
        if self.buffer.len() >= self.capacity {
            self.buffer.pop_front();
        }
        self.buffer.push_back(experience);
    }

    /// Sample a batch of experiences
    pub fn sample(&self, batch_size: usize) -> Vec<Experience> {
        let mut rng = rand::thread_rng();
        let experiences: Vec<_> = self.buffer.iter().cloned().collect();
        experiences
            .choose_multiple(&mut rng, batch_size.min(experiences.len()))
            .cloned()
            .collect()
    }

    /// Get buffer length
    pub fn len(&self) -> usize {
        self.buffer.len()
    }

    /// Check if buffer is empty
    pub fn is_empty(&self) -> bool {
        self.buffer.is_empty()
    }

    /// Clear the buffer
    pub fn clear(&mut self) {
        self.buffer.clear();
    }

    /// Get all experiences (for batch training)
    pub fn all(&self) -> Vec<Experience> {
        self.buffer.iter().cloned().collect()
    }
}

impl Default for ExperienceBuffer {
    fn default() -> Self {
        Self::new(10000)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cca_core::AgentRole;

    fn create_test_state() -> State {
        State {
            task_type: "test".to_string(),
            available_agents: vec![],
            token_usage: 0.5,
            success_history: vec![1.0, 0.8],
            complexity: 0.3,
            features: vec![0.1, 0.2],
        }
    }

    #[test]
    fn test_experience_creation() {
        let state = create_test_state();
        let action = Action::RouteToAgent(AgentRole::Backend);
        let exp = Experience::new(state.clone(), action, 1.0, Some(state), false);

        assert_eq!(exp.reward, 1.0);
        assert!(!exp.done);
        assert!(exp.next_state.is_some());
    }

    #[test]
    fn test_buffer_push_and_len() {
        let mut buffer = ExperienceBuffer::new(100);
        assert_eq!(buffer.len(), 0);
        assert!(buffer.is_empty());

        let state = create_test_state();
        let exp = Experience::new(
            state.clone(),
            Action::RouteToAgent(AgentRole::Frontend),
            0.5,
            None,
            true,
        );

        buffer.push(exp);
        assert_eq!(buffer.len(), 1);
        assert!(!buffer.is_empty());
    }

    #[test]
    fn test_buffer_capacity() {
        let mut buffer = ExperienceBuffer::new(3);

        for i in 0..5 {
            let state = create_test_state();
            let exp = Experience::new(
                state,
                Action::AllocateTokens(i as f64 * 0.1),
                i as f64,
                None,
                false,
            );
            buffer.push(exp);
        }

        // Should be capped at capacity
        assert_eq!(buffer.len(), 3);
    }

    #[test]
    fn test_buffer_sample() {
        let mut buffer = ExperienceBuffer::new(100);

        for i in 0..10 {
            let state = create_test_state();
            let exp = Experience::new(
                state,
                Action::RouteToAgent(AgentRole::Backend),
                i as f64,
                None,
                false,
            );
            buffer.push(exp);
        }

        let sample = buffer.sample(5);
        assert_eq!(sample.len(), 5);
    }

    #[test]
    fn test_buffer_sample_larger_than_buffer() {
        let mut buffer = ExperienceBuffer::new(100);

        for i in 0..3 {
            let state = create_test_state();
            let exp = Experience::new(
                state,
                Action::RouteToAgent(AgentRole::QA),
                i as f64,
                None,
                false,
            );
            buffer.push(exp);
        }

        let sample = buffer.sample(10);
        assert_eq!(sample.len(), 3); // Can only return what's available
    }

    #[test]
    fn test_buffer_clear() {
        let mut buffer = ExperienceBuffer::new(100);

        let state = create_test_state();
        buffer.push(Experience::new(
            state,
            Action::CompressContext(0.5),
            1.0,
            None,
            false,
        ));
        assert_eq!(buffer.len(), 1);

        buffer.clear();
        assert_eq!(buffer.len(), 0);
        assert!(buffer.is_empty());
    }

    #[test]
    fn test_buffer_all() {
        let mut buffer = ExperienceBuffer::new(100);

        for i in 0..5 {
            let state = create_test_state();
            buffer.push(Experience::new(
                state,
                Action::RouteToAgent(AgentRole::DevOps),
                i as f64,
                None,
                false,
            ));
        }

        let all = buffer.all();
        assert_eq!(all.len(), 5);
    }
}
