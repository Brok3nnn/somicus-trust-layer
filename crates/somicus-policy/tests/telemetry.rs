//! The Policy Guard's spans carry only decisions, risk classes, autonomy levels and fixed reasons: never the app, the
//! action's label, its hash or a consequence note.

use std::time::Instant;

use somicus_policy::error::IntentError;
use somicus_policy::guard::{ActionCandidate, ConfirmationPreference, PolicyGuard};
use somicus_policy::input::ActionHash;
use somicus_policy::risk::{AutonomyLevel, RiskClass};
use somicus_policy::verifier::IntentVerifierHandle;
use somicus_telemetry::Record;
use somicus_telemetry::testing::{capture, capture_async};

const ACTION_HASH: &str = "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855";
/// A synthetic action label with a name in it, as a step's label can have.
const LABEL: &str = "Send the invoice to Anna Kovacs";

fn assert_only_allowed(record: &Record) {
    assert_eq!(record.dropped, 0, "{record:?}");
    let line = record.to_string();
    for content in ["Anna", "invoice", "1Password", "Safari", &ACTION_HASH[..12]] {
        assert!(!line.contains(content), "{line}");
    }
}

#[tokio::test]
async fn policy_spans_carry_only_decisions_classes_and_reasons() -> Result<(), IntentError> {
    let guard = PolicyGuard::new_personal(IntentVerifierHandle::new());
    let hash = ActionHash::new(ACTION_HASH)?;
    let (_, records) = capture(|| {
        let denied = ActionCandidate::new("1Password", &hash, LABEL)
            .with_risk(RiskClass::R3)
            .with_autonomy(AutonomyLevel::L2);
        let shown = ActionCandidate::new("com.apple.Safari", &hash, LABEL).with_risk(RiskClass::R2);
        (
            guard.evaluate_action(&denied),
            guard.evaluate_action(&shown),
        )
    });
    let (verified, intent) = capture_async(guard.verify_and_permit(&hash, Instant::now())).await;
    assert!(verified.is_err());

    assert_eq!(records.len(), 2, "{records:?}");
    let (denied, shown) = (&records[0], &records[1]);
    assert_eq!(denied.name, "policy_evaluate");
    assert_eq!(
        denied.field_names(),
        ["risk", "autonomy", "reason", "decision"]
    );
    assert_eq!(denied.label("risk"), Some("r3"));
    // A personal workspace caps autonomy at L1.
    assert_eq!(denied.label("autonomy"), Some("l1"));
    assert_eq!(denied.label("reason"), Some("app_not_allowed"));
    assert_eq!(denied.label("decision"), Some("deny"));
    assert_eq!(shown.label("decision"), Some("require_confirmation"));
    assert_eq!(shown.label("reason"), None);

    assert_eq!(intent.len(), 1, "{intent:?}");
    assert_eq!(intent[0].name, "policy_verify_intent");
    assert_eq!(intent[0].label("reason"), Some("no_pending_action"));
    for record in records.iter().chain(&intent) {
        assert_only_allowed(record);
    }
    Ok(())
}

#[test]
fn a_step_permitted_without_its_confirmation_names_the_persons_switch() -> Result<(), IntentError> {
    let guard = PolicyGuard::new_with_bundle(
        somicus_policy::PolicyBundle::personal_do(),
        IntentVerifierHandle::new(),
    );
    let hash = ActionHash::new(ACTION_HASH)?;
    let ((), records) = capture(|| {
        for (risk, confirmations) in [
            (RiskClass::R4, ConfirmationPreference::Off),
            (RiskClass::R4, ConfirmationPreference::On),
            (RiskClass::R2, ConfirmationPreference::Off),
        ] {
            let candidate = ActionCandidate::new("com.apple.Safari", &hash, LABEL)
                .with_risk(risk)
                .with_autonomy(AutonomyLevel::L3)
                .with_confirmations(confirmations);
            let _ = guard.evaluate_action(&candidate);
        }
    });
    assert_eq!(records.len(), 3, "{records:?}");
    let (skipped, confirmed, ordinary) = (&records[0], &records[1], &records[2]);
    assert_eq!(
        skipped.field_names(),
        ["risk", "autonomy", "reason", "decision"]
    );
    assert_eq!(skipped.label("reason"), Some("confirmations_off"));
    assert_eq!(skipped.label("decision"), Some("permit"));
    assert_eq!(confirmed.label("reason"), None);
    assert_eq!(confirmed.label("decision"), Some("require_confirmation"));
    // An R2 step never needed a confirmation, so nothing was skipped.
    assert_eq!(ordinary.label("reason"), None);
    for record in &records {
        assert_only_allowed(record);
    }
    Ok(())
}
