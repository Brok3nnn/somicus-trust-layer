//! Error types for trusted user intent and policy enforcement.

use somicus_uigraph::geometry::{PhysicalPoint, PhysicalRect};
use thiserror::Error;

/// Errors that can occur during trusted user intent verification.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum IntentError {
    /// An injected or synthetic input was observed and rejected.
    #[error("Injected or synthetic input was detected and rejected by the trust anchor")]
    InjectedInputRejected,

    /// No matching physical input was observed within the trusted 1.0-second window.
    #[error("No physical input was observed within the trusted intent window")]
    NoPhysicalInputObserved,

    /// A physical click was detected outside the confirmation control's bounds.
    #[error("Physical input at {point:?} was outside the control bounds {bounds:?}")]
    OutOfBounds {
        /// Point where the click occurred.
        point: PhysicalPoint,
        /// Expected control bounding rectangle.
        bounds: PhysicalRect,
    },

    /// The physical input or confirmation intent timed out.
    #[error("Physical input expired: {elapsed_ms} ms elapsed, maximum allowed is {max_ms} ms")]
    Expired {
        /// Elapsed time in milliseconds.
        elapsed_ms: u64,
        /// Maximum allowed time in milliseconds (1000 ms).
        max_ms: u64,
    },

    /// No pending action was registered for the provided action hash.
    #[error("No pending action found matching hash '{0}'")]
    NoPendingAction(String),

    /// The action hash has already been confirmed and consumed (replay attempt blocked).
    #[error("Intent for action hash '{0}' has already been consumed")]
    AlreadyConsumed(String),

    /// The provided action hash string is invalid.
    #[error("Invalid action hash format: {0}")]
    InvalidActionHash(String),
}

/// Errors that can occur during policy evaluation and signed bundle verification.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum PolicyError {
    /// Bundle signature verification failed (e.g. bundle was tampered with).
    #[error("Policy bundle signature verification failed: {0}")]
    SignatureVerificationFailed(String),

    /// Signature format is invalid (not valid base64 or length).
    #[error("Invalid policy bundle signature format: {0}")]
    InvalidSignatureFormat(String),

    /// Bundle JSON serialization or deserialization failed.
    #[error("Policy bundle serialization error: {0}")]
    SerializationError(String),

    /// An intent verification error occurred.
    #[error("Intent verification error: {0}")]
    Intent(#[from] IntentError),

    /// The envelope is not one this build reads: another format or algorithm, too large, malformed, or (for a pinned
    /// key) not a valid key. The fixed reason never holds any of the envelope's content.
    #[error("Policy bundle envelope is not supported: {0}")]
    UnsupportedEnvelope(&'static str),

    /// The envelope is signed with a key this build does not pin.
    #[error("Policy bundle is signed with a key this build does not trust")]
    UntrustedKey,

    /// A correctly signed bundle may not replace the active one.
    #[error("Policy bundle rejected: {0:?}")]
    BundleRejected(BundleRejection),
}

/// Why a correctly signed policy bundle is not installed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BundleRejection {
    /// The bundle is for another tenant than the device's.
    OtherTenant,
    /// The bundle is not newer than the active bundle of the same tenant, so installing it would roll policy back.
    NotNewer,
    /// The bundle's validity ended.
    Expired,
    /// The bundle carries no end of validity.
    NoExpiry,
}
