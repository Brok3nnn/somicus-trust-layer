//! Trusted User Intent verification: a confirmation counts only as physical input on the bound
//! control within 1.0 s, for the action it was registered for, and once. Which system events count as physical is in
//! `input_sources.rs`.

use std::time::{Duration, Instant};

use somicus_policy::{
    ActionHash, ConfirmationKey, InputEventKind, IntentError, IntentVerifier,
    K_CG_EVENT_SOURCE_STATE_COMBINED_SESSION_STATE, K_CG_EVENT_SOURCE_STATE_HID_SYSTEM_STATE,
    ObservedInput, PendingAction, PhysicalPoint, PhysicalRect, SOMICUS_SYNTHETIC_USER_DATA,
    classify_macos_event_source, classify_windows_keyboard_flags, classify_windows_mouse_flags,
};

const TEST_ACTION_HASH: &str = "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855";
const OTHER_ACTION_HASH: &str = "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad";

/// The confirm control the tests register: 120 by 36 pixels from (100, 100).
const CONTROL: PhysicalRect = PhysicalRect::new(100, 100, 120, 36);
const ON_CONTROL: PhysicalPoint = PhysicalPoint::new(150, 118);

/// What the verifier answers when `kind`, observed from a macOS event with `source_state` and `user_data`, is
/// submitted `elapsed_ms` after it happened for an action registered with the control's bounds and keyboard approval.
fn verify_macos(
    kind: InputEventKind,
    source_state: i32,
    user_data: i64,
    elapsed_ms: u64,
) -> Result<(), IntentError> {
    let mut verifier = IntentVerifier::new();
    let action_hash = ActionHash::new(TEST_ACTION_HASH)?;
    verifier.register_pending(
        PendingAction::new(action_hash.clone(), Some(CONTROL)).with_allow_keyboard(true),
    )?;
    let t0 = Instant::now();
    verifier.record_input(ObservedInput {
        kind,
        source: classify_macos_event_source(source_state, user_data),
        timestamp: t0,
    });
    verifier
        .verify_intent(&action_hash, t0 + Duration::from_millis(elapsed_ms))
        .map(|_| ())
}

#[test]
fn injected_macos_clicks_and_keys_on_the_confirm_control_are_rejected() {
    // An event a process posted, or one Somicus itself posted with its marker, never confirms,
    // even inside the control's bounds and at once.
    for kind in [
        InputEventKind::Click(ON_CONTROL),
        InputEventKind::KeyPress(ConfirmationKey::Hotkey),
    ] {
        for (state, user_data) in [
            (K_CG_EVENT_SOURCE_STATE_COMBINED_SESSION_STATE, 0),
            (
                K_CG_EVENT_SOURCE_STATE_HID_SYSTEM_STATE,
                SOMICUS_SYNTHETIC_USER_DATA,
            ),
        ] {
            assert_eq!(
                verify_macos(kind.clone(), state, user_data, 0),
                Err(IntentError::InjectedInputRejected),
                "{kind:?} from state {state}"
            );
        }
    }
}

#[test]
fn physical_macos_clicks_and_keys_confirm_within_one_second_and_never_after() {
    for kind in [
        InputEventKind::Click(ON_CONTROL),
        InputEventKind::KeyPress(ConfirmationKey::Hotkey),
    ] {
        for elapsed_ms in [0, 500, 999, 1000] {
            assert_eq!(
                verify_macos(
                    kind.clone(),
                    K_CG_EVENT_SOURCE_STATE_HID_SYSTEM_STATE,
                    0,
                    elapsed_ms
                ),
                Ok(()),
                "{kind:?} after {elapsed_ms} ms"
            );
        }
        for elapsed_ms in [1001, 1200, 4000] {
            assert_eq!(
                verify_macos(
                    kind.clone(),
                    K_CG_EVENT_SOURCE_STATE_HID_SYSTEM_STATE,
                    0,
                    elapsed_ms
                ),
                Err(IntentError::Expired {
                    elapsed_ms,
                    max_ms: 1000
                }),
                "{kind:?} after {elapsed_ms} ms"
            );
        }
    }
}

#[test]
fn a_physical_return_or_space_never_confirms_even_within_one_second() {
    for key in [ConfirmationKey::Enter, ConfirmationKey::Space] {
        assert_eq!(
            verify_macos(
                InputEventKind::KeyPress(key),
                K_CG_EVENT_SOURCE_STATE_HID_SYSTEM_STATE,
                0,
                100
            ),
            Err(IntentError::NoPhysicalInputObserved),
            "{key:?}"
        );
    }
}

#[test]
fn a_physical_return_never_confirms_an_action() {
    // Return or Space anywhere on the screen must not approve an action; only a click inside the bound
    // control or the dedicated hotkey can.
    let mut verifier = IntentVerifier::new();
    let action_hash = ActionHash::new(TEST_ACTION_HASH).unwrap();
    verifier
        .register_pending(PendingAction::new(action_hash.clone(), None).with_allow_keyboard(true))
        .unwrap();

    let t0 = Instant::now();
    for key in [ConfirmationKey::Enter, ConfirmationKey::Space] {
        verifier.record_input(ObservedInput {
            kind: somicus_policy::InputEventKind::KeyPress(key),
            source: classify_windows_keyboard_flags(0),
            timestamp: t0,
        });
    }

    let result = verifier.verify_intent(&action_hash, t0 + Duration::from_millis(400));
    assert_eq!(result.unwrap_err(), IntentError::NoPhysicalInputObserved);
}

#[test]
fn the_bound_hotkey_confirms_only_when_keyboard_approval_is_allowed() {
    let allowed_hash = ActionHash::new(TEST_ACTION_HASH).unwrap();
    let mut allowed = IntentVerifier::new();
    allowed
        .register_pending(PendingAction::new(allowed_hash.clone(), None).with_allow_keyboard(true))
        .unwrap();
    let t0 = Instant::now();
    allowed.record_input(ObservedInput {
        kind: somicus_policy::InputEventKind::KeyPress(ConfirmationKey::Hotkey),
        source: classify_windows_keyboard_flags(0),
        timestamp: t0,
    });
    let confirmed = allowed
        .verify_intent(&allowed_hash, t0 + Duration::from_millis(200))
        .unwrap();
    assert_eq!(confirmed.action_hash, allowed_hash);

    let refused_hash = ActionHash::new(OTHER_ACTION_HASH).unwrap();
    let mut refused = IntentVerifier::new();
    refused
        .register_pending(PendingAction::new(refused_hash.clone(), None).with_allow_keyboard(false))
        .unwrap();
    refused.record_input(ObservedInput {
        kind: somicus_policy::InputEventKind::KeyPress(ConfirmationKey::Hotkey),
        source: classify_windows_keyboard_flags(0),
        timestamp: t0,
    });
    let result = refused.verify_intent(&refused_hash, t0 + Duration::from_millis(200));
    assert_eq!(result.unwrap_err(), IntentError::NoPhysicalInputObserved);
}

#[test]
fn a_physical_click_confirms_only_a_bound_control() {
    // An action registered without control bounds cannot be approved by a click anywhere.
    let mut verifier = IntentVerifier::new();
    let action_hash = ActionHash::new(TEST_ACTION_HASH).unwrap();
    verifier
        .register_pending(PendingAction::new(action_hash.clone(), None))
        .unwrap();

    let t0 = Instant::now();
    verifier.record_input(ObservedInput {
        kind: somicus_policy::InputEventKind::Click(PhysicalPoint::new(150, 118)),
        source: classify_windows_mouse_flags(0),
        timestamp: t0,
    });

    let result = verifier.verify_intent(&action_hash, t0 + Duration::from_millis(250));
    assert_eq!(result.unwrap_err(), IntentError::NoPhysicalInputObserved);
}

#[test]
fn a_physical_click_older_than_one_second_is_rejected() {
    let mut verifier = IntentVerifier::new();
    let action_hash = ActionHash::new(TEST_ACTION_HASH).unwrap();
    let bounds = PhysicalRect::new(100, 100, 120, 36);

    verifier
        .register_pending(PendingAction::new(action_hash.clone(), Some(bounds)))
        .unwrap();

    let t0 = Instant::now();
    let input = ObservedInput::physical_click(PhysicalPoint::new(150, 118), t0);
    verifier.record_input(input);

    // Submission 1200 ms later (> 1000 ms limit)
    let t_submit = t0 + Duration::from_millis(1200);
    let err = verifier.verify_intent(&action_hash, t_submit).unwrap_err();
    assert!(matches!(
        err,
        IntentError::Expired {
            elapsed_ms: 1200,
            max_ms: 1000
        }
    ));
}

#[test]
fn a_physical_click_outside_the_control_is_rejected() {
    let mut verifier = IntentVerifier::new();
    let action_hash = ActionHash::new(TEST_ACTION_HASH).unwrap();
    let bounds = PhysicalRect::new(100, 100, 120, 36);

    verifier
        .register_pending(PendingAction::new(action_hash.clone(), Some(bounds)))
        .unwrap();

    let now = Instant::now();
    // Click is outside bounds (x=80 is < 100)
    let input = ObservedInput::physical_click(PhysicalPoint::new(80, 118), now);
    verifier.record_input(input);

    let err = verifier.verify_intent(&action_hash, now).unwrap_err();
    assert_eq!(
        err,
        IntentError::OutOfBounds {
            point: PhysicalPoint::new(80, 118),
            bounds,
        }
    );
}

#[test]
fn a_replayed_confirmation_is_rejected() {
    let mut verifier = IntentVerifier::new();
    let action_hash = ActionHash::new(TEST_ACTION_HASH).unwrap();
    let bounds = PhysicalRect::new(100, 100, 120, 36);

    verifier
        .register_pending(PendingAction::new(action_hash.clone(), Some(bounds)))
        .unwrap();

    let now = Instant::now();
    let input = ObservedInput::physical_click(PhysicalPoint::new(150, 118), now);
    verifier.record_input(input);

    // First attempt succeeds
    let first = verifier.verify_intent(&action_hash, now);
    assert!(first.is_ok());

    // Replay attempt with same action hash fails immediately
    let replay = verifier.verify_intent(&action_hash, now);
    assert_eq!(
        replay.unwrap_err(),
        IntentError::AlreadyConsumed(action_hash.to_string())
    );
}

#[test]
fn a_confirmation_for_another_action_is_rejected() {
    let mut verifier = IntentVerifier::new();
    let action_hash = ActionHash::new(TEST_ACTION_HASH).unwrap();
    let other_hash = ActionHash::new(OTHER_ACTION_HASH).unwrap();
    let bounds = PhysicalRect::new(100, 100, 120, 36);

    verifier
        .register_pending(PendingAction::new(action_hash.clone(), Some(bounds)))
        .unwrap();

    let now = Instant::now();
    let input = ObservedInput::physical_click(PhysicalPoint::new(150, 118), now);
    verifier.record_input(input);

    // Webview submits intent with a different action hash
    let err = verifier.verify_intent(&other_hash, now).unwrap_err();
    assert_eq!(err, IntentError::NoPendingAction(other_hash.to_string()));
}

#[test]
fn an_action_without_physical_input_is_rejected() {
    let mut verifier = IntentVerifier::new();
    let action_hash = ActionHash::new(TEST_ACTION_HASH).unwrap();
    let bounds = PhysicalRect::new(100, 100, 120, 36);

    verifier
        .register_pending(PendingAction::new(action_hash.clone(), Some(bounds)))
        .unwrap();

    let now = Instant::now();
    // No input was recorded at all
    let err = verifier.verify_intent(&action_hash, now).unwrap_err();
    assert_eq!(err, IntentError::NoPhysicalInputObserved);
}

/// An action registered at `t0` on the control, waiting `ttl`.
fn pending_since(t0: Instant, ttl: Duration) -> PendingAction {
    let Ok(action_hash) = ActionHash::new(TEST_ACTION_HASH) else {
        unreachable!("the test hash is a valid action hash");
    };
    let mut pending = PendingAction::new(action_hash, Some(CONTROL)).with_ttl(ttl);
    pending.created_at = t0;
    pending
}

#[test]
fn more_time_lets_a_later_physical_click_confirm_and_changes_nothing_else() {
    let t0 = Instant::now();
    let action_hash = ActionHash::new(TEST_ACTION_HASH).unwrap();
    // Without more time, a click 30 s in comes after the 20 s the action waits.
    let mut waiting = IntentVerifier::new();
    waiting
        .register_pending(pending_since(t0, Duration::from_secs(20)))
        .unwrap();
    let late = t0 + Duration::from_secs(30);
    waiting.record_input(ObservedInput::physical_click(ON_CONTROL, late));
    assert!(matches!(
        waiting.verify_intent(&action_hash, late),
        Err(IntentError::Expired { .. })
    ));

    // With 60 s more, asked for 15 s in, the same click confirms.
    let mut extended = IntentVerifier::new();
    extended
        .register_pending(pending_since(t0, Duration::from_secs(20)))
        .unwrap();
    assert!(extended.extend_pending(
        &action_hash,
        Duration::from_secs(60),
        t0 + Duration::from_secs(15)
    ));
    // A click beside the control still never confirms; the action keeps waiting.
    extended.record_input(ObservedInput::physical_click(
        PhysicalPoint::new(80, 118),
        late,
    ));
    let refused = extended.verify_intent(&action_hash, late).unwrap_err();
    assert!(
        !matches!(refused, IntentError::Expired { .. }),
        "{refused:?}"
    );
    let on_time = late + Duration::from_millis(100);
    extended.record_input(ObservedInput::physical_click(ON_CONTROL, on_time));
    assert!(extended.verify_intent(&action_hash, on_time).is_ok());
}

#[test]
fn more_time_never_brings_back_an_expired_or_unknown_action() {
    let t0 = Instant::now();
    let action_hash = ActionHash::new(TEST_ACTION_HASH).unwrap();
    let mut verifier = IntentVerifier::new();
    verifier
        .register_pending(pending_since(t0, Duration::from_secs(20)))
        .unwrap();
    assert!(!verifier.extend_pending(
        &action_hash,
        Duration::from_secs(60),
        t0 + Duration::from_secs(21)
    ));
    let unknown = ActionHash::new(OTHER_ACTION_HASH).unwrap();
    assert!(!verifier.extend_pending(&unknown, Duration::from_secs(60), t0));
    let late = t0 + Duration::from_secs(30);
    verifier.record_input(ObservedInput::physical_click(ON_CONTROL, late));
    assert!(matches!(
        verifier.verify_intent(&action_hash, late),
        Err(IntentError::Expired { .. })
    ));
}

#[test]
fn one_physical_click_confirms_one_action_only() -> Result<(), Box<dyn std::error::Error>> {
    // The same click cannot back a second action submitted within the trusted window.
    let mut verifier = IntentVerifier::new();
    let first = ActionHash::new(TEST_ACTION_HASH)?;
    let second = ActionHash::new(OTHER_ACTION_HASH)?;
    verifier.register_pending(PendingAction::new(first.clone(), Some(CONTROL)))?;
    verifier.register_pending(PendingAction::new(second.clone(), Some(CONTROL)))?;
    let t0 = Instant::now();
    verifier.record_input(ObservedInput::physical_click(ON_CONTROL, t0));

    assert!(
        verifier
            .verify_intent(&first, t0 + Duration::from_millis(100))
            .is_ok()
    );
    assert!(matches!(
        verifier.verify_intent(&second, t0 + Duration::from_millis(200)),
        Err(IntentError::NoPhysicalInputObserved)
    ));
    Ok(())
}

/// What the verifier answers for a physical click on the control `click_ms` after the action was registered, armed
/// `armed_ms` after it, submitted `submit_ms` after it.
fn verify_armed(click_ms: u64, armed_ms: u64, submit_ms: u64) -> Result<(), IntentError> {
    let mut verifier = IntentVerifier::new();
    let action_hash = ActionHash::new(TEST_ACTION_HASH)?;
    let pending = PendingAction::new(action_hash.clone(), Some(CONTROL));
    let t0 = pending.created_at;
    verifier.register_pending(pending.armed_at(t0 + Duration::from_millis(armed_ms)))?;
    verifier.record_input(ObservedInput {
        kind: InputEventKind::Click(ON_CONTROL),
        source: classify_macos_event_source(K_CG_EVENT_SOURCE_STATE_HID_SYSTEM_STATE, 0),
        timestamp: t0 + Duration::from_millis(click_ms),
    });
    verifier
        .verify_intent(&action_hash, t0 + Duration::from_millis(submit_ms))
        .map(|_| ())
}

#[test]
fn a_click_before_the_control_is_armed_never_confirms() {
    // 499 ms after the panel showed is too early, 500 ms is not.
    assert_eq!(
        verify_armed(499, 500, 600),
        Err(IntentError::NoPhysicalInputObserved)
    );
    assert_eq!(verify_armed(500, 500, 600), Ok(()));
}

#[test]
fn rearming_after_a_move_makes_an_earlier_click_too_early() -> Result<(), IntentError> {
    let mut verifier = IntentVerifier::new();
    let action_hash = ActionHash::new(TEST_ACTION_HASH)?;
    let pending = PendingAction::new(action_hash.clone(), Some(CONTROL));
    let t0 = pending.created_at;
    verifier.register_pending(pending.armed_at(t0 + Duration::from_millis(500)))?;
    // The panel moved at 700 ms: it is armed again from 1200 ms.
    assert!(verifier.rearm(&action_hash, t0 + Duration::from_millis(1_200)));
    verifier.record_input(ObservedInput {
        kind: InputEventKind::Click(ON_CONTROL),
        source: classify_macos_event_source(K_CG_EVENT_SOURCE_STATE_HID_SYSTEM_STATE, 0),
        timestamp: t0 + Duration::from_millis(800),
    });
    assert_eq!(
        verifier
            .verify_intent(&action_hash, t0 + Duration::from_millis(900))
            .map(|_| ()),
        Err(IntentError::NoPhysicalInputObserved)
    );
    let Ok(other) = ActionHash::new(OTHER_ACTION_HASH) else {
        unreachable!("the test hash is valid");
    };
    assert!(!verifier.rearm(&other, t0));
    Ok(())
}

#[test]
fn an_action_without_arming_accepts_a_recent_click_recorded_just_before_registration()
-> Result<(), IntentError> {
    let mut verifier = IntentVerifier::new();
    let click_at = Instant::now();
    verifier.record_input(ObservedInput {
        kind: InputEventKind::Click(ON_CONTROL),
        source: classify_macos_event_source(K_CG_EVENT_SOURCE_STATE_HID_SYSTEM_STATE, 0),
        timestamp: click_at,
    });
    let action_hash = ActionHash::new(TEST_ACTION_HASH)?;
    let pending = PendingAction::new(action_hash.clone(), Some(CONTROL));
    let created_at = pending.created_at;
    verifier.register_pending(pending)?;
    assert!(
        verifier
            .verify_intent(&action_hash, created_at + Duration::from_millis(10))
            .is_ok()
    );
    Ok(())
}
