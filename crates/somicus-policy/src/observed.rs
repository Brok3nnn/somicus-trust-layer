//! Fixed names for the Policy Guard's telemetry spans: decisions, risk classes, autonomy levels and error kinds. Never
//! the app, the action's label or hash, a consequence note or a tenant.

use somicus_telemetry::Label;

use crate::error::{BundleRejection, IntentError, PolicyError};
use crate::guard::PolicyDecision;
use crate::risk::{AutonomyLevel, RiskClass};

impl Label for RiskClass {
    fn label(&self) -> &'static str {
        match self {
            Self::R0 => "r0",
            Self::R1 => "r1",
            Self::R2 => "r2",
            Self::R3 => "r3",
            Self::R4 => "r4",
            Self::R5 => "r5",
        }
    }
}

impl Label for AutonomyLevel {
    fn label(&self) -> &'static str {
        match self {
            Self::L0 => "l0",
            Self::L1 => "l1",
            Self::L2 => "l2",
            Self::L3 => "l3",
        }
    }
}

impl Label for PolicyDecision {
    fn label(&self) -> &'static str {
        match self {
            Self::Permit => "permit",
            Self::RequireConfirmation { .. } => "require_confirmation",
            Self::Deny { .. } => "deny",
        }
    }
}

impl Label for IntentError {
    fn label(&self) -> &'static str {
        match self {
            Self::InjectedInputRejected => "injected_input_rejected",
            Self::NoPhysicalInputObserved => "no_physical_input_observed",
            Self::OutOfBounds { .. } => "out_of_bounds",
            Self::Expired { .. } => "expired",
            Self::NoPendingAction(_) => "no_pending_action",
            Self::AlreadyConsumed(_) => "already_consumed",
            Self::InvalidActionHash(_) => "invalid_action_hash",
        }
    }
}

impl Label for PolicyError {
    fn label(&self) -> &'static str {
        match self {
            Self::SignatureVerificationFailed(_) => "signature_verification_failed",
            Self::InvalidSignatureFormat(_) => "invalid_signature_format",
            Self::SerializationError(_) => "serialization_error",
            Self::UnsupportedEnvelope(_) => "unsupported_envelope",
            Self::UntrustedKey => "untrusted_key",
            Self::Intent(intent) => intent.label(),
            Self::BundleRejected(rejection) => match rejection {
                BundleRejection::OtherTenant => "bundle_other_tenant",
                BundleRejection::NotNewer => "bundle_not_newer",
                BundleRejection::Expired => "bundle_expired",
                BundleRejection::NoExpiry => "bundle_no_expiry",
            },
        }
    }
}
