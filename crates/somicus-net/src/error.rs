//! Error types for network transport operations and egress connections.

use thiserror::Error;

/// Errors returned by `somicus-net` transport and egress operations.
#[derive(Debug, Error)]
pub enum NetError {
    /// A network connection error occurred.
    #[error("network connection error: {0}")]
    Connection(String),

    /// Standard I/O failure.
    #[error("network I/O error: {0}")]
    Io(#[from] std::io::Error),
}

/// The error's kind, for telemetry: never its message, which can name an address.
impl somicus_telemetry::Label for NetError {
    fn label(&self) -> &'static str {
        match self {
            Self::Connection(_) => "connection",
            Self::Io(_) => "io",
        }
    }
}
