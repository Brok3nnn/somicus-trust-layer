//! Error types for the on-device Privacy Shield.

use thiserror::Error;

/// Errors produced during Privacy Shield operations.
#[derive(Debug, Error)]
pub enum PrivacyError {
    /// Token vault lookup failure or mismatched entity type.
    #[error("Token vault error: {0}")]
    VaultError(String),

    /// Entity recognition failure or unparseable input.
    #[error("Recognition failure: {0}")]
    RecognitionError(String),

    /// Serialization error during redaction or rehydration.
    #[error("Serialization error: {0}")]
    SerializationError(#[from] serde_json::Error),

    /// Evaluation suite execution error.
    #[error("R-suite evaluation error: {0}")]
    EvalError(String),
}
