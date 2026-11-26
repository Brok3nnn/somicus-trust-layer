//! Policy Guard and Trusted User Intent verification for Somicus.
//!
//! Enforces:
//! - Physical user intent verification for sensitive/high-risk actions.
//! - Distinction between physical hardware HID input and synthetic/injected events.
//! - Temporal and spatial correlation within the ≤ 1.0 s trusted window.
//! - Action hash binding and replay prevention.
//!
//! See `docs/architecture.md` and ADR-0013, ADR-0040 and ADR-0041 in `docs/adr/`.

#![forbid(unsafe_code)]

pub mod action;
pub mod bundle;
pub mod classifier;
pub mod consent;
pub mod consequence;
pub mod envelope;
pub mod error;
pub mod fields;
pub mod guard;
pub mod input;
pub mod keys;
pub mod numbers;
mod observed;
pub mod risk;
pub mod transfer;
pub mod typed;
pub mod verifier;

pub use action::{ActionKind, ActionTarget, action_floor, classify_performed_action};
pub use bundle::{BUILT_IN_DENIED_APPS, PolicyBundle, WorkspaceType};
pub use classifier::{
    K_CG_EVENT_SOURCE_STATE_COMBINED_SESSION_STATE, K_CG_EVENT_SOURCE_STATE_HID_SYSTEM_STATE,
    LLKHF_INJECTED, LLKHF_LOWER_IL_INJECTED, LLMHF_INJECTED, LLMHF_LOWER_IL_INJECTED,
    SOMICUS_SYNTHETIC_USER_DATA, classify_linux_device, classify_macos_event_source,
    classify_windows_keyboard_flags, classify_windows_mouse_flags,
};
pub use consent::{
    Banner, BannerChoice, ButtonChoice, answers_banner, button_choice, names_cookies, names_terms,
};
pub use consequence::{Consequence, asks_for, asks_to_rename, consequences_of, sends_on_return};
pub use envelope::{SignedPolicyBundle, TrustedKeys, key_id};
pub use error::{IntentError, PolicyError};
pub use fields::{is_strict_target, search_words_allowed};
pub use guard::{
    ActionCandidate, BundleInstall, ConfirmationPreference, PolicyDecision, PolicyGuard,
};
pub use input::{ActionHash, ConfirmationKey, InputEventKind, InputSource, ObservedInput};
pub use keys::{ALLOWED_KEYS, allowed_key, key_floor, types_text};
pub use numbers::{KnownNumbers, evaluate};
pub use risk::{ActionContext, AutonomyLevel, RiskClass, RiskClassifier};
// The UI Graph's screen geometry, so that hit tests and the graph share one type.
pub use somicus_uigraph::geometry::{PhysicalPoint, PhysicalRect};
pub use transfer::{
    SourceSensitivity, TransferDecision, copy_decision, drag_decision, paste_decision,
};
pub use typed::{typed_draft_allowed, typed_value_allowed};
pub use verifier::{
    DEFAULT_TRUSTED_INTENT_WINDOW, IntentVerifier, IntentVerifierHandle, MAX_CONSUMED_ACTIONS,
    PendingAction, VerifiedIntent,
};
