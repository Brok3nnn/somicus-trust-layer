//! Errors of loading, converting and running the name model. None of them carries screen text.

use thiserror::Error;

/// Why the name model could not be loaded, converted or run.
#[derive(Debug, Error)]
pub enum NerError {
    /// A model file could not be read.
    #[error("a name model file could not be read ({0})")]
    Read(std::io::ErrorKind),
    /// A file did not have the pinned length and SHA-256, so it was not used.
    #[error("a name model file does not have the pinned length and SHA-256")]
    Verification,
    /// The tokenizer file is not the `WordPiece` tokenizer the model was trained with.
    #[error("the tokenizer is not the expected WordPiece tokenizer: {0}")]
    Tokenizer(&'static str),
    /// The weights could not be read as the model, or the computation failed. The message names tensors and shapes.
    #[error("the name model cannot run: {0}")]
    Model(String),
    /// The weights could not be converted to half precision.
    #[error("the name model cannot be converted: {0}")]
    Conversion(String),
}

impl From<std::io::Error> for NerError {
    fn from(error: std::io::Error) -> Self {
        Self::Read(error.kind())
    }
}

/// The error's kind, for telemetry and for the name-protection reason; never its message.
impl somicus_telemetry::Label for NerError {
    fn label(&self) -> &'static str {
        match self {
            Self::Read(_) => "read",
            Self::Verification => "verification",
            Self::Tokenizer(_) => "tokenizer",
            Self::Model(_) => "model",
            Self::Conversion(_) => "conversion",
        }
    }
}
