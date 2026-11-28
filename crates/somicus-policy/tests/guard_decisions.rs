//! The Policy Guard's decisions: the default policy on personal workspaces, the
//! rejection of a tampered signed bundle, the physical confirmation an R3 action is bound to, the personal Do bundle,
//! the person's confirmation switch against a bundle that requires confirmations (ADR-0040) and the autonomy levels.

use std::time::Instant;

use ed25519_dalek::SigningKey;
use somicus_policy::TrustedKeys;
use somicus_policy::bundle::{PolicyBundle, WorkspaceType};
use somicus_policy::error::{IntentError, PolicyError};
use somicus_policy::guard::{
    ActionCandidate, BundleInstall, ConfirmationPreference, PolicyDecision, PolicyGuard,
};
use somicus_policy::input::{ActionHash, InputEventKind, InputSource, ObservedInput};
use somicus_policy::risk::{AutonomyLevel, RiskClass};
use somicus_policy::verifier::{IntentVerifierHandle, PendingAction};
use somicus_policy::{PhysicalPoint, PhysicalRect};

const TEST_ACTION_HASH: &str = "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855";

#[tokio::test]
async fn the_default_policy_applies_on_personal_workspaces() {
    let verifier = IntentVerifierHandle::new();
    let guard = PolicyGuard::new_personal(verifier);
    let bundle = guard.active_bundle();

    // 1. Verify personal workspace configuration and autonomy cap L1
    assert_eq!(bundle.workspace_type, WorkspaceType::Personal);
    assert_eq!(bundle.tenant_id, "personal");
    assert_eq!(bundle.autonomy_cap, AutonomyLevel::L1);

    // 2. Allowed applications pass
    assert!(bundle.is_app_allowed("com.apple.Safari"));
    assert!(bundle.is_app_allowed("google-chrome"));
    assert!(bundle.is_app_allowed("slack"));

    // 3. Built-in denylist blocks sensitive categories (password managers, banking, system settings)
    assert!(!bundle.is_app_allowed("1Password"));
    assert!(!bundle.is_app_allowed("com.bitwarden.desktop"));
    assert!(!bundle.is_app_allowed("revolut"));
    assert!(!bundle.is_app_allowed("systemsettings"));
    assert!(!bundle.is_app_allowed("regedit"));

    // 4. PolicyGuard denies evaluation on denied apps immediately
    let hash = ActionHash::new(TEST_ACTION_HASH).unwrap();
    let candidate = ActionCandidate::new("1Password", &hash, "Copy password")
        .with_risk(RiskClass::R0)
        .with_autonomy(AutonomyLevel::L1);
    let decision = guard.evaluate_action(&candidate);
    assert!(matches!(decision, PolicyDecision::Deny { .. }));
}

#[tokio::test]
async fn a_tampered_signed_bundle_is_rejected() {
    let signing_key = SigningKey::from_bytes(&[42u8; 32]);
    let keys = TrustedKeys::new([signing_key.verifying_key()]);

    let bundle = PolicyBundle {
        tenant_id: "org-enterprise".to_owned(),
        workspace_type: WorkspaceType::Organization,
        autonomy_cap: AutonomyLevel::L2,
        not_after_epoch_ms: Some(4_102_444_800_000),
        ..PolicyBundle::default_personal()
    };
    let signed = bundle.sign(&signing_key).expect("signing should succeed");
    assert_eq!(
        signed.verify(&keys).unwrap().autonomy_cap,
        AutonomyLevel::L2
    );

    // Escalating the autonomy cap in the signed bytes breaks the signature.
    let mut tampered = signed.clone();
    let escalated = PolicyBundle {
        autonomy_cap: AutonomyLevel::L3,
        ..bundle
    };
    tampered.payload = escalated.sign(&signing_key).unwrap().payload;
    assert!(matches!(
        tampered.verify(&keys),
        Err(PolicyError::SignatureVerificationFailed(_))
    ));

    // The guard keeps its bundle when the update does not verify.
    let guard = PolicyGuard::new_personal(IntentVerifierHandle::new());
    let install = BundleInstall {
        device_tenant: "org-enterprise",
        now_epoch_ms: 0,
    };
    assert!(
        guard
            .update_signed_bundle(&tampered, &keys, install)
            .is_err()
    );
    assert_eq!(guard.active_bundle().autonomy_cap, AutonomyLevel::L1);
}

#[tokio::test]
async fn an_r3_action_needs_a_physical_confirmation_bound_to_it() {
    let verifier = IntentVerifierHandle::new();
    let guard = PolicyGuard::new_personal(verifier.clone());

    let hash = ActionHash::new(TEST_ACTION_HASH).unwrap();
    let control_bounds = PhysicalRect::new(100, 100, 200, 50);

    // 1. Policy evaluation at L1/L2 requires confirmation for R3 action
    let candidate = ActionCandidate::new("com.acme.crm", &hash, "Delete record")
        .with_risk(RiskClass::R3)
        .with_autonomy(AutonomyLevel::L2);
    let decision = guard.evaluate_action(&candidate);
    assert!(matches!(
        decision,
        PolicyDecision::RequireConfirmation { .. }
    ));
    if let PolicyDecision::RequireConfirmation { action_hash, risk } = decision {
        assert_eq!(risk, RiskClass::R3);
        assert_eq!(action_hash, hash);
    }

    // 2. Register pending action with control bounds
    verifier
        .register_pending(PendingAction::new(hash.clone(), Some(control_bounds)))
        .await
        .unwrap();

    // 3. Attempting to verify without physical input fails
    let err = guard
        .verify_and_permit(&hash, Instant::now())
        .await
        .expect_err("must fail without input");
    assert_eq!(
        err,
        PolicyError::Intent(IntentError::NoPhysicalInputObserved)
    );

    // 4. Injected input (synthetic) is detected and rejected
    verifier
        .record_input(ObservedInput {
            kind: InputEventKind::Click(PhysicalPoint::new(150, 125)),
            source: InputSource::Injected { flags: 0x01 },
            timestamp: Instant::now(),
        })
        .await;
    let inj_err = guard
        .verify_and_permit(&hash, Instant::now())
        .await
        .expect_err("injected input must be rejected");
    assert_eq!(
        inj_err,
        PolicyError::Intent(IntentError::InjectedInputRejected)
    );

    // 5. Genuine physical HID input within bounds succeeds
    verifier
        .record_input(ObservedInput {
            kind: InputEventKind::Click(PhysicalPoint::new(150, 125)),
            source: InputSource::Physical,
            timestamp: Instant::now(),
        })
        .await;
    let confirmed_intent = guard
        .verify_and_permit(&hash, Instant::now())
        .await
        .expect("physical input must confirm action");
    assert_eq!(confirmed_intent.action_hash, hash);

    // 6. Replay attempt is blocked immediately
    let replay_err = guard
        .verify_and_permit(&hash, Instant::now())
        .await
        .expect_err("replay must be blocked");
    assert!(matches!(
        replay_err,
        PolicyError::Intent(IntentError::AlreadyConsumed(_))
    ));
}

/// What `guard` decides for a step of class `risk` in `app` at L3, for a person whose preference is `confirmations`.
fn decide(
    guard: &PolicyGuard,
    app: &str,
    risk: RiskClass,
    confirmations: ConfirmationPreference,
) -> PolicyDecision {
    let Ok(hash) = ActionHash::new(TEST_ACTION_HASH) else {
        unreachable!("the test hash is a valid action hash");
    };
    guard.evaluate_action(
        &ActionCandidate::new(app, &hash, "Send")
            .with_risk(risk)
            .with_autonomy(AutonomyLevel::L3)
            .with_confirmations(confirmations),
    )
}

fn requires_confirmation(decision: &PolicyDecision) -> bool {
    matches!(decision, PolicyDecision::RequireConfirmation { .. })
}

#[tokio::test]
async fn the_personal_do_bundle_performs_up_to_r2_and_asks_before_r3_and_r4() {
    let verifier = IntentVerifierHandle::new();
    let personal_do = PolicyGuard::new_with_bundle(PolicyBundle::personal_do(), verifier.clone());
    let personal = PolicyGuard::new_personal(verifier);
    assert_eq!(personal_do.active_bundle().autonomy_cap, AutonomyLevel::L3);
    assert_eq!(
        personal_do.active_bundle().workspace_type,
        WorkspaceType::Personal
    );
    assert_eq!(
        personal_do.active_bundle().denied_apps,
        personal.active_bundle().denied_apps
    );
    assert!(!personal_do.active_bundle().confirmations_required);

    let on = ConfirmationPreference::On;
    for risk in [RiskClass::R0, RiskClass::R1, RiskClass::R2] {
        assert_eq!(
            decide(&personal_do, "safari", risk, on),
            PolicyDecision::Permit
        );
        // The plain personal default stays at L1, where Somicus only shows.
        assert!(requires_confirmation(&decide(
            &personal, "safari", risk, on
        )));
    }
    for risk in [RiskClass::R3, RiskClass::R4] {
        // Confirmations are on by default in every build (ADR-0040).
        assert!(requires_confirmation(&decide(
            &personal_do,
            "safari",
            risk,
            on
        )));
    }
    // A candidate that does not name the person's preference asks for a confirmation.
    let hash = ActionHash::new(TEST_ACTION_HASH).unwrap();
    let unnamed = ActionCandidate::new("safari", &hash, "Send")
        .with_risk(RiskClass::R3)
        .with_autonomy(AutonomyLevel::L3);
    assert_eq!(unnamed.confirmations, ConfirmationPreference::On);
    assert!(requires_confirmation(
        &personal_do.evaluate_action(&unnamed)
    ));
}

#[tokio::test]
async fn only_the_persons_switch_skips_a_confirmation_and_only_where_the_bundle_allows_it() {
    // The four combinations of the person's switch and an organisation's requirement (ADR-0040).
    let allowing =
        PolicyGuard::new_with_bundle(PolicyBundle::personal_do(), IntentVerifierHandle::new());
    let requiring = PolicyGuard::new_with_bundle(
        PolicyBundle {
            confirmations_required: true,
            ..PolicyBundle::personal_do()
        },
        IntentVerifierHandle::new(),
    );
    let (on, off) = (ConfirmationPreference::On, ConfirmationPreference::Off);
    for risk in [RiskClass::R3, RiskClass::R4] {
        // Switch on, bundle allows the switch: the step waits for the person.
        assert!(requires_confirmation(&decide(
            &allowing, "safari", risk, on
        )));
        // Switch off, bundle allows the switch: the only combination that skips the confirmation.
        assert_eq!(
            decide(&allowing, "safari", risk, off),
            PolicyDecision::Permit
        );
        // Switch on, bundle requires confirmations.
        assert!(requires_confirmation(&decide(
            &requiring, "safari", risk, on
        )));
        // Switch off, bundle requires confirmations: the organisation's bundle wins.
        assert!(requires_confirmation(&decide(
            &requiring, "safari", risk, off
        )));
    }
    assert!(!allowing.skips_confirmations(on));
    assert!(allowing.skips_confirmations(off));
    assert!(!requiring.skips_confirmations(on));
    assert!(!requiring.skips_confirmations(off));

    // The switch changes nothing else: R5 and the deny list are refused, R0–R2 run, either way.
    for (guard, preference) in [
        (&allowing, on),
        (&allowing, off),
        (&requiring, on),
        (&requiring, off),
    ] {
        assert!(matches!(
            decide(guard, "safari", RiskClass::R5, preference),
            PolicyDecision::Deny { .. }
        ));
        assert!(matches!(
            decide(guard, "1password", RiskClass::R4, preference),
            PolicyDecision::Deny { .. }
        ));
        assert_eq!(
            decide(guard, "safari", RiskClass::R2, preference),
            PolicyDecision::Permit
        );
    }
}

#[tokio::test]
async fn a_bundle_can_never_turn_confirmations_off() {
    // The prototype's retired field is no longer part of a bundle: a signed bundle that carries it is refused, and
    // only the person's own switch skips a confirmation (ADR-0040, ADR-0052).
    let signing_key = SigningKey::from_bytes(&[11u8; 32]);
    let payload = br#"{"version":2,"tenant_id":"org-acme","workspace_type":"organization","autonomy_cap":"l3","allowed_apps":null,"denied_apps":[],"created_at_epoch_ms":1,"not_after_epoch_ms":4102444800000,"confirmations_off":true}"#;
    let signed = somicus_policy::envelope::sign_payload(payload, &signing_key).unwrap();
    let guard =
        PolicyGuard::new_with_bundle(PolicyBundle::personal_do(), IntentVerifierHandle::new());
    let install = BundleInstall {
        device_tenant: "org-acme",
        now_epoch_ms: 0,
    };
    assert!(matches!(
        guard.update_signed_bundle(
            &signed,
            &TrustedKeys::new([signing_key.verifying_key()]),
            install
        ),
        Err(PolicyError::SerializationError(_))
    ));
    for risk in [RiskClass::R3, RiskClass::R4] {
        assert!(requires_confirmation(&decide(
            &guard,
            "safari",
            risk,
            ConfirmationPreference::On
        )));
    }
}

#[tokio::test]
async fn l0_never_permits_execution() {
    // An organisation bundle capped at L0 ("answer only") must never let a step run, whatever its risk, also
    // for a person who switched confirmations off.
    let bundle = PolicyBundle {
        autonomy_cap: AutonomyLevel::L0,
        ..PolicyBundle::personal_do()
    };
    let guard = PolicyGuard::new_with_bundle(bundle, IntentVerifierHandle::new());
    let hash = ActionHash::new(TEST_ACTION_HASH).unwrap();
    for risk in [
        RiskClass::R0,
        RiskClass::R1,
        RiskClass::R2,
        RiskClass::R3,
        RiskClass::R4,
    ] {
        let candidate = ActionCandidate::new("safari", &hash, "Search")
            .with_risk(risk)
            .with_autonomy(AutonomyLevel::L3)
            .with_confirmations(ConfirmationPreference::Off);
        assert!(
            matches!(
                guard.evaluate_action(&candidate),
                PolicyDecision::Deny { .. }
            ),
            "{risk:?} must be denied at L0"
        );
    }
}

#[tokio::test]
async fn r3_requires_confirmation_at_every_level_when_confirmations_are_on() {
    // A commit (R3) is confirmed at L2 and L3 alike once confirmations are on.
    let guard =
        PolicyGuard::new_with_bundle(PolicyBundle::personal_do(), IntentVerifierHandle::new());
    let hash = ActionHash::new(TEST_ACTION_HASH).unwrap();
    for autonomy in [AutonomyLevel::L2, AutonomyLevel::L3] {
        for risk in [RiskClass::R3, RiskClass::R4] {
            let candidate = ActionCandidate::new("safari", &hash, "Send")
                .with_risk(risk)
                .with_autonomy(autonomy);
            assert!(
                matches!(
                    guard.evaluate_action(&candidate),
                    PolicyDecision::RequireConfirmation { .. }
                ),
                "{risk:?} at {autonomy:?} must require a confirmation"
            );
        }
        let safe = ActionCandidate::new("safari", &hash, "Search")
            .with_risk(RiskClass::R2)
            .with_autonomy(autonomy);
        assert_eq!(guard.evaluate_action(&safe), PolicyDecision::Permit);
    }
}

#[tokio::test]
async fn a_bundle_that_requires_confirmations_is_covered_by_its_signature() {
    let signing_key = SigningKey::from_bytes(&[9u8; 32]);
    let bundle = PolicyBundle {
        tenant_id: "org-acme".to_owned(),
        workspace_type: WorkspaceType::Organization,
        confirmations_required: true,
        not_after_epoch_ms: Some(4_102_444_800_000),
        ..PolicyBundle::personal_do()
    };
    let keys = TrustedKeys::new([signing_key.verifying_key()]);
    let signed = bundle.sign(&signing_key).unwrap();
    assert!(signed.verify(&keys).unwrap().confirmations_required);
    // Removing the requirement breaks the signature, so nobody but the organisation can lift it.
    let mut lifted = signed;
    lifted.payload = PolicyBundle {
        confirmations_required: false,
        ..bundle
    }
    .sign(&signing_key)
    .unwrap()
    .payload;
    assert!(matches!(
        lifted.verify(&keys),
        Err(PolicyError::SignatureVerificationFailed(_))
    ));
}
