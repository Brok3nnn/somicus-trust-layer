//! Policy Guard engine enforcing on-device security, risk boundaries, and user intent.
//!
//! Enforces:
//! - Hard invariants: R5 always denied; denied apps always blocked; nothing runs at L0 (answer only).
//! - The autonomy cap of the active bundle: L1 for the personal default, L3 for [`PolicyBundle::personal_do`].
//! - Physical confirmation binding for R3 and R4 actions at every level, unless the person switched confirmations off
//!   and the bundle does not require them (ADR-0040).
//! - Bundle updates require a valid signature by a pinned key (ADR-0052); a bundle this device accepted before can be
//!   restored after a restart even when its validity for a first install has ended, so an organisation's device never
//!   falls back to a looser policy while it is offline.
//!
//! The person's confirmation preference is a local setting, not part of the signed bundle, so it reaches the guard
//! with each [`ActionCandidate`]. A candidate built without it asks for confirmations. A bundle can only make the guard
//! stricter: [`PolicyBundle::confirmations_required`] overrules the person's switch.
//!
//! The guard shares the application's one [`IntentVerifierHandle`], so the clicks the core records reach the same
//! verifier that approves actions.

use std::sync::{Arc, PoisonError, RwLock, RwLockReadGuard};
use std::time::Instant;

use crate::bundle::PolicyBundle;
use crate::envelope::{SignedPolicyBundle, TrustedKeys};
use crate::error::{BundleRejection, PolicyError};
use crate::input::ActionHash;
use crate::risk::{AutonomyLevel, RiskClass};
use crate::verifier::{IntentVerifierHandle, VerifiedIntent};

/// Result of evaluating an action against the active policy.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PolicyDecision {
    /// Action is permitted to proceed without user confirmation.
    Permit,
    /// Action requires physical user confirmation before execution. The desktop words the sheet from the step and the
    /// risk class with its own localised templates; the policy holds no text for people.
    RequireConfirmation {
        /// SHA-256 hash of the exact action awaiting physical confirmation.
        action_hash: ActionHash,
        /// Evaluated risk class.
        risk: RiskClass,
    },
    /// Action is strictly prohibited by policy.
    Deny {
        /// Reason explaining why the action was rejected.
        reason: String,
    },
}

/// The person's own choice whether R3 and R4 steps wait for their confirmation (ADR-0040). It is a local preference,
/// never part of the signed bundle; the device changes it to [`Self::Off`] only after the person's physical input.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub enum ConfirmationPreference {
    /// R3 and R4 steps wait for a confirmation verified as physical input: the default in every build.
    #[default]
    On,
    /// The person switched every confirmation off, R3 and R4 alike. A bundle that requires confirmations overrules
    /// it; R5, the deny list, the autonomy cap and the stop controls are unaffected.
    Off,
}

/// The preference's fixed name, as the `reason` of an evaluation that skipped a confirmation.
impl somicus_telemetry::Label for ConfirmationPreference {
    fn label(&self) -> &'static str {
        match self {
            Self::On => "confirmations_on",
            Self::Off => "confirmations_off",
        }
    }
}

/// An action candidate evaluated by the Policy Guard.
#[derive(Debug, Clone)]
pub struct ActionCandidate<'a> {
    /// App bundle ID or executable name.
    pub app_id: &'a str,
    /// Risk class determined for this action.
    pub risk: RiskClass,
    /// Autonomy level requested.
    pub requested_autonomy: AutonomyLevel,
    /// SHA-256 hash of the exact action awaiting physical confirmation.
    pub action_hash: &'a ActionHash,
    /// User-facing label of the action.
    pub label: &'a str,
    /// The person's confirmation preference when the action is evaluated; on unless set.
    pub confirmations: ConfirmationPreference,
}

impl<'a> ActionCandidate<'a> {
    /// Constructs a new `ActionCandidate` with confirmations on.
    #[must_use]
    pub const fn new(app_id: &'a str, action_hash: &'a ActionHash, label: &'a str) -> Self {
        Self {
            app_id,
            risk: RiskClass::R0,
            requested_autonomy: AutonomyLevel::L1,
            action_hash,
            label,
            confirmations: ConfirmationPreference::On,
        }
    }

    /// Sets the person's confirmation preference for this candidate.
    #[must_use]
    pub const fn with_confirmations(mut self, confirmations: ConfirmationPreference) -> Self {
        self.confirmations = confirmations;
        self
    }

    /// Sets the risk class for this candidate.
    #[must_use]
    pub const fn with_risk(mut self, risk: RiskClass) -> Self {
        self.risk = risk;
        self
    }

    /// Sets the requested autonomy level for this candidate.
    #[must_use]
    pub const fn with_autonomy(mut self, autonomy: AutonomyLevel) -> Self {
        self.requested_autonomy = autonomy;
        self
    }
}

/// The device a signed policy bundle is installed on, and when.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BundleInstall<'a> {
    /// The tenant this device is enrolled in.
    pub device_tenant: &'a str,
    /// The current time in milliseconds since the UNIX epoch.
    pub now_epoch_ms: u64,
}

/// The local Policy Guard trust anchor running in the Rust core. Clones share the active bundle, so the application's
/// one guard and every handle to it see the same policy.
#[derive(Debug, Clone)]
pub struct PolicyGuard {
    built_in: PolicyBundle,
    bundle: Arc<RwLock<PolicyBundle>>,
    intent_verifier: IntentVerifierHandle,
}

/// Whether the active bundle is checked for its end of validity: a first install is, a restore of a bundle this
/// device accepted before is not.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Validity {
    Checked,
    AlreadyAccepted,
}

impl PolicyGuard {
    /// Constructs a `PolicyGuard` initialized with the personal workspace default policy.
    #[must_use]
    pub fn new_personal(intent_verifier: IntentVerifierHandle) -> Self {
        Self::new_with_bundle(PolicyBundle::default_personal(), intent_verifier)
    }

    /// Constructs a `PolicyGuard` with a specified built-in policy bundle, which [`Self::reset`] returns to.
    #[must_use]
    pub fn new_with_bundle(bundle: PolicyBundle, intent_verifier: IntentVerifierHandle) -> Self {
        Self {
            bundle: Arc::new(RwLock::new(bundle.clone())),
            built_in: bundle,
            intent_verifier,
        }
    }

    /// A copy of the active policy bundle.
    #[must_use]
    pub fn active_bundle(&self) -> PolicyBundle {
        self.read().clone()
    }

    /// Whether R3 and R4 steps skip their confirmation for a person whose preference is `preference`: only when the
    /// person switched confirmations off and the active bundle does not require them (ADR-0040).
    #[must_use]
    pub fn skips_confirmations(&self, preference: ConfirmationPreference) -> bool {
        skips(&self.read(), preference)
    }

    /// Returns to the built-in bundle, when the person signs out or changes workspace.
    pub fn reset(&self) {
        *self.bundle.write().unwrap_or_else(PoisonError::into_inner) = self.built_in.clone();
    }

    /// Replaces the active policy bundle with `signed_bundle` once its signature verifies with a pinned key and it is a
    /// newer, unexpired bundle for the tenant this device is enrolled in (ADR-0052). An older bundle cannot
    /// replace a newer one, a bundle for another tenant cannot be installed, and a bundle without an end of validity
    /// is refused.
    ///
    /// # Errors
    /// Returns the envelope's error when it does not verify (see [`SignedPolicyBundle::verify`]), and
    /// `PolicyError::BundleRejected` with the reason when the bundle may not replace the active one.
    pub fn update_signed_bundle(
        &self,
        signed_bundle: &SignedPolicyBundle,
        keys: &TrustedKeys,
        install: BundleInstall<'_>,
    ) -> Result<(), PolicyError> {
        // `ok` or the error's kind; never the tenant, the bundle or its signature.
        let span = somicus_telemetry::span!(INFO, "policy_bundle_update");
        let result = span.in_scope(|| {
            let verified = signed_bundle.verify(keys)?;
            self.install_verified(verified, install, Validity::Checked)
        });
        span.record_result(&result);
        result
    }

    /// Installs again a bundle this device accepted before and stored, after a restart: the same checks as
    /// [`Self::update_signed_bundle`] except the end of validity, which only limits a first install.
    ///
    /// # Errors
    /// As [`Self::update_signed_bundle`], without `BundleRejection::Expired`.
    pub fn restore_signed_bundle(
        &self,
        signed_bundle: &SignedPolicyBundle,
        keys: &TrustedKeys,
        install: BundleInstall<'_>,
    ) -> Result<(), PolicyError> {
        // `ok` or the error's kind; never the tenant, the bundle or its signature.
        let span = somicus_telemetry::span!(INFO, "policy_bundle_restore");
        let result = span.in_scope(|| {
            let verified = signed_bundle.verify(keys)?;
            self.install_verified(verified, install, Validity::AlreadyAccepted)
        });
        span.record_result(&result);
        result
    }

    /// Installs a bundle whose signature verified, unless it may not replace the active one.
    fn install_verified(
        &self,
        verified: PolicyBundle,
        install: BundleInstall<'_>,
        validity: Validity,
    ) -> Result<(), PolicyError> {
        let mut active = self.bundle.write().unwrap_or_else(PoisonError::into_inner);
        let rejection = if verified.tenant_id != install.device_tenant {
            Some(BundleRejection::OtherTenant)
        } else if active.tenant_id == install.device_tenant && verified.version <= active.version {
            Some(BundleRejection::NotNewer)
        } else {
            match verified.not_after_epoch_ms {
                None => Some(BundleRejection::NoExpiry),
                Some(not_after)
                    if validity == Validity::Checked && install.now_epoch_ms > not_after =>
                {
                    Some(BundleRejection::Expired)
                }
                Some(_) => None,
            }
        };
        if let Some(rejection) = rejection {
            return Err(PolicyError::BundleRejected(rejection));
        }
        *active = verified;
        Ok(())
    }

    fn read(&self) -> RwLockReadGuard<'_, PolicyBundle> {
        self.bundle.read().unwrap_or_else(PoisonError::into_inner)
    }

    /// Evaluates an action candidate against the active policy.
    ///
    /// Its `policy_evaluate` span records the risk class, the effective autonomy level, the decision and, for a
    /// denial or a permit without the confirmation the risk class asks for, its fixed reason; never the app, the
    /// action's label or its hash.
    #[must_use]
    pub fn evaluate_action(&self, candidate: &ActionCandidate<'_>) -> PolicyDecision {
        let bundle = self.read();
        let effective_autonomy = candidate.requested_autonomy.min(bundle.autonomy_cap);
        let span = somicus_telemetry::span!(
            INFO,
            "policy_evaluate",
            risk = candidate.risk,
            autonomy = effective_autonomy,
        );
        let decision = span.in_scope(|| {
            if let Some(denial) = denial(&bundle, candidate, effective_autonomy) {
                somicus_telemetry::record!(span, reason = denial);
                return denial.decision(candidate);
            }
            let decision = permit_or_confirm(&bundle, candidate, effective_autonomy);
            if decision == PolicyDecision::Permit && candidate.risk.requires_physical_confirmation()
            {
                // The person switched confirmations off (ADR-0040).
                somicus_telemetry::record!(span, reason = candidate.confirmations);
            }
            decision
        });
        somicus_telemetry::record!(span, decision = decision);
        decision
    }

    /// Verifies physical user intent for a pending action before allowing execution.
    ///
    /// Its `policy_verify_intent` span records `ok` or the error's kind; never the action's hash.
    ///
    /// # Errors
    /// Returns `PolicyError::Intent` if physical HID confirmation is missing, injected, or expired.
    pub async fn verify_and_permit(
        &self,
        action_hash: &ActionHash,
        submission_time: Instant,
    ) -> Result<VerifiedIntent, PolicyError> {
        let span = somicus_telemetry::span!(INFO, "policy_verify_intent");
        let verified = span
            .instrument(
                self.intent_verifier
                    .verify_intent(action_hash, submission_time),
            )
            .await
            .map_err(PolicyError::from);
        span.record_result(&verified);
        verified
    }
}

/// Whether R3 and R4 steps skip their confirmation under `bundle` for a person whose preference is `preference`.
fn skips(bundle: &PolicyBundle, preference: ConfirmationPreference) -> bool {
    preference == ConfirmationPreference::Off && !bundle.confirmations_required
}

/// Why `candidate` is denied outright under `bundle`, if it is.
fn denial(
    bundle: &PolicyBundle,
    candidate: &ActionCandidate<'_>,
    effective_autonomy: AutonomyLevel,
) -> Option<Denial> {
    if candidate.risk == RiskClass::R5 {
        // 1. Hard Invariant: R5 is ALWAYS prohibited under all policies
        Some(Denial::Prohibited)
    } else if !bundle.is_app_allowed(candidate.app_id) {
        // 2. App allow/deny check, with the built-in deny list as a floor
        Some(Denial::AppNotAllowed)
    } else if effective_autonomy == AutonomyLevel::L0 {
        // 3. At L0 (answer only) nothing is performed, whatever the risk.
        Some(Denial::AnswerOnly)
    } else {
        None
    }
}

/// The decision under `bundle` for a candidate that is not denied outright.
fn permit_or_confirm(
    bundle: &PolicyBundle,
    candidate: &ActionCandidate<'_>,
    effective_autonomy: AutonomyLevel,
) -> PolicyDecision {
    // 4. Physical confirmation binding
    // At L1 (Show): Somicus never executes directly; only points & explains
    if effective_autonomy == AutonomyLevel::L1 {
        return PolicyDecision::RequireConfirmation {
            action_hash: candidate.action_hash.clone(),
            risk: candidate.risk,
        };
    }

    // At L2 / L3: a commit or an irreversible step needs a confirmation, unless the person switched them off and the
    // bundle does not require them.
    if candidate.risk.requires_physical_confirmation() && !skips(bundle, candidate.confirmations) {
        PolicyDecision::RequireConfirmation {
            action_hash: candidate.action_hash.clone(),
            risk: candidate.risk,
        }
    } else {
        PolicyDecision::Permit
    }
}

/// Why an action is denied before its risk and autonomy are weighed.
#[derive(Debug, Clone, Copy)]
enum Denial {
    /// R5 is prohibited by a hard invariant.
    Prohibited,
    /// The workspace policy does not allow the app.
    AppNotAllowed,
    /// At L0 Somicus answers only.
    AnswerOnly,
}

impl Denial {
    fn decision(self, candidate: &ActionCandidate<'_>) -> PolicyDecision {
        let reason = match self {
            Self::Prohibited => {
                "Action risk class R5 (Prohibited) is blocked by hard security invariant".to_owned()
            }
            Self::AppNotAllowed => format!(
                "Application '{}' is prohibited by workspace policy",
                candidate.app_id
            ),
            Self::AnswerOnly => "Autonomy level L0 answers only; no step is performed".to_owned(),
        };
        PolicyDecision::Deny { reason }
    }
}

/// The denial's fixed name, as the `reason` of the evaluation's span: never the app it names.
impl somicus_telemetry::Label for Denial {
    fn label(&self) -> &'static str {
        match self {
            Self::Prohibited => "r5_prohibited",
            Self::AppNotAllowed => "app_not_allowed",
            Self::AnswerOnly => "answer_only",
        }
    }
}
