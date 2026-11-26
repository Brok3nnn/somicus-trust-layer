//! Error types for UI Graph operations.

use thiserror::Error;

/// Errors that can occur when building, querying, or manipulating a UI Graph.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum UiGraphError {
    /// A referenced node ID does not exist in the graph.
    #[error("node not found: {0}")]
    NodeNotFound(String),

    /// A node ID is duplicate within a single snapshot.
    #[error("duplicate node ID: {0}")]
    DuplicateNodeId(String),

    /// A cycle was detected in the parent-child hierarchy.
    #[error("cycle detected in hierarchy involving node: {0}")]
    CycleDetected(String),

    /// The requested token budget for a screen digest is invalid.
    #[error("invalid token budget: {0} (must be > 0)")]
    InvalidTokenBudget(usize),

    /// An error occurred during graph serialization or deserialization.
    #[error("serialization error: {0}")]
    Serialization(String),

    /// Coordinate transformation error.
    #[error("coordinate error: {0}")]
    InvalidCoordinate(String),
}
