//! Which input events count as physical for Trusted User Intent: injected clicks and keys on the
//! confirm control are rejected on Windows and macOS, and genuine hardware input is accepted.

use std::time::{Duration, Instant};

use somicus_policy::{
    ActionHash, ConfirmationKey, IntentError, IntentVerifier,
    K_CG_EVENT_SOURCE_STATE_COMBINED_SESSION_STATE, K_CG_EVENT_SOURCE_STATE_HID_SYSTEM_STATE,
    LLKHF_INJECTED, LLKHF_LOWER_IL_INJECTED, LLMHF_INJECTED, LLMHF_LOWER_IL_INJECTED,
    ObservedInput, PendingAction, PhysicalPoint, PhysicalRect, SOMICUS_SYNTHETIC_USER_DATA,
    classify_macos_event_source, classify_windows_keyboard_flags, classify_windows_mouse_flags,
};

const TEST_ACTION_HASH: &str = "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855";

#[test]
fn an_injected_windows_click_is_rejected() {
    let mut verifier = IntentVerifier::new();
    let action_hash = ActionHash::new(TEST_ACTION_HASH).unwrap();
    let bounds = PhysicalRect::new(100, 100, 120, 36);

    verifier
        .register_pending(PendingAction::new(action_hash.clone(), Some(bounds)))
        .unwrap();

    // Windows synthetic mouse click with LLMHF_INJECTED flag
    let source = classify_windows_mouse_flags(LLMHF_INJECTED);
    assert!(source.is_injected());

    let now = Instant::now();
    let input = ObservedInput {
        kind: somicus_policy::InputEventKind::Click(PhysicalPoint::new(150, 118)),
        source,
        timestamp: now,
    };
    verifier.record_input(input);

    let result = verifier.verify_intent(&action_hash, now);
    assert_eq!(result.unwrap_err(), IntentError::InjectedInputRejected);
}

#[test]
fn a_lower_integrity_injected_windows_click_is_rejected() {
    let mut verifier = IntentVerifier::new();
    let action_hash = ActionHash::new(TEST_ACTION_HASH).unwrap();
    let bounds = PhysicalRect::new(100, 100, 120, 36);

    verifier
        .register_pending(PendingAction::new(action_hash.clone(), Some(bounds)))
        .unwrap();

    let source = classify_windows_mouse_flags(LLMHF_LOWER_IL_INJECTED);
    assert!(source.is_injected());

    let now = Instant::now();
    let input = ObservedInput {
        kind: somicus_policy::InputEventKind::Click(PhysicalPoint::new(150, 118)),
        source,
        timestamp: now,
    };
    verifier.record_input(input);

    let result = verifier.verify_intent(&action_hash, now);
    assert_eq!(result.unwrap_err(), IntentError::InjectedInputRejected);
}

#[test]
fn a_physical_windows_click_is_accepted() {
    let mut verifier = IntentVerifier::new();
    let action_hash = ActionHash::new(TEST_ACTION_HASH).unwrap();
    let bounds = PhysicalRect::new(100, 100, 120, 36);

    verifier
        .register_pending(PendingAction::new(action_hash.clone(), Some(bounds)))
        .unwrap();

    // Flags = 0 indicates hardware HID
    let source = classify_windows_mouse_flags(0);
    assert!(source.is_physical());

    let t0 = Instant::now();
    let input = ObservedInput {
        kind: somicus_policy::InputEventKind::Click(PhysicalPoint::new(150, 118)),
        source,
        timestamp: t0,
    };
    verifier.record_input(input);

    // Webview intent submitted 250 ms later (within 1.0 s)
    let t_submit = t0 + Duration::from_millis(250);
    let confirmed_intent = verifier.verify_intent(&action_hash, t_submit).unwrap();
    assert_eq!(confirmed_intent.action_hash, action_hash);
}

#[test]
fn an_injected_windows_key_is_rejected() {
    let mut verifier = IntentVerifier::new();
    let action_hash = ActionHash::new(TEST_ACTION_HASH).unwrap();

    verifier
        .register_pending(PendingAction::new(action_hash.clone(), None).with_allow_keyboard(true))
        .unwrap();

    let source = classify_windows_keyboard_flags(LLKHF_INJECTED);
    assert!(source.is_injected());

    let now = Instant::now();
    let input = ObservedInput {
        kind: somicus_policy::InputEventKind::KeyPress(ConfirmationKey::Hotkey),
        source,
        timestamp: now,
    };
    verifier.record_input(input);

    let result = verifier.verify_intent(&action_hash, now);
    assert_eq!(result.unwrap_err(), IntentError::InjectedInputRejected);
}

#[test]
fn a_lower_integrity_injected_windows_key_is_rejected() {
    let mut verifier = IntentVerifier::new();
    let action_hash = ActionHash::new(TEST_ACTION_HASH).unwrap();

    verifier
        .register_pending(PendingAction::new(action_hash.clone(), None).with_allow_keyboard(true))
        .unwrap();

    let source = classify_windows_keyboard_flags(LLKHF_LOWER_IL_INJECTED);
    assert!(source.is_injected());

    let now = Instant::now();
    let input = ObservedInput {
        kind: somicus_policy::InputEventKind::KeyPress(ConfirmationKey::Hotkey),
        source,
        timestamp: now,
    };
    verifier.record_input(input);

    let result = verifier.verify_intent(&action_hash, now);
    assert_eq!(result.unwrap_err(), IntentError::InjectedInputRejected);
}

#[test]
fn an_injected_macos_event_is_rejected() {
    let mut verifier = IntentVerifier::new();
    let action_hash = ActionHash::new(TEST_ACTION_HASH).unwrap();
    let bounds = PhysicalRect::new(200, 200, 100, 32);

    verifier
        .register_pending(PendingAction::new(action_hash.clone(), Some(bounds)))
        .unwrap();

    // macOS synthetic / process-posted event (CombinedSessionState)
    let source = classify_macos_event_source(K_CG_EVENT_SOURCE_STATE_COMBINED_SESSION_STATE, 0);
    assert!(source.is_injected());

    let now = Instant::now();
    let input = ObservedInput {
        kind: somicus_policy::InputEventKind::Click(PhysicalPoint::new(250, 215)),
        source,
        timestamp: now,
    };
    verifier.record_input(input);

    let result = verifier.verify_intent(&action_hash, now);
    assert_eq!(result.unwrap_err(), IntentError::InjectedInputRejected);
}

#[test]
fn a_macos_event_tagged_as_somicus_input_is_rejected() {
    let mut verifier = IntentVerifier::new();
    let action_hash = ActionHash::new(TEST_ACTION_HASH).unwrap();
    let bounds = PhysicalRect::new(200, 200, 100, 32);

    verifier
        .register_pending(PendingAction::new(action_hash.clone(), Some(bounds)))
        .unwrap();

    // Event tagged with Somicus synthetic marker
    let source = classify_macos_event_source(
        K_CG_EVENT_SOURCE_STATE_HID_SYSTEM_STATE,
        SOMICUS_SYNTHETIC_USER_DATA,
    );
    assert!(source.is_injected());

    let now = Instant::now();
    let input = ObservedInput {
        kind: somicus_policy::InputEventKind::Click(PhysicalPoint::new(250, 215)),
        source,
        timestamp: now,
    };
    verifier.record_input(input);

    let result = verifier.verify_intent(&action_hash, now);
    assert_eq!(result.unwrap_err(), IntentError::InjectedInputRejected);
}

#[test]
fn a_physical_macos_event_is_accepted() {
    let mut verifier = IntentVerifier::new();
    let action_hash = ActionHash::new(TEST_ACTION_HASH).unwrap();
    let bounds = PhysicalRect::new(200, 200, 100, 32);

    verifier
        .register_pending(PendingAction::new(action_hash.clone(), Some(bounds)))
        .unwrap();

    // macOS hardware HID state
    let source = classify_macos_event_source(K_CG_EVENT_SOURCE_STATE_HID_SYSTEM_STATE, 0);
    assert!(source.is_physical());

    let t0 = Instant::now();
    let input = ObservedInput {
        kind: somicus_policy::InputEventKind::Click(PhysicalPoint::new(250, 215)),
        source,
        timestamp: t0,
    };
    verifier.record_input(input);

    let t_submit = t0 + Duration::from_millis(600);
    let confirmed_intent = verifier.verify_intent(&action_hash, t_submit).unwrap();
    assert_eq!(confirmed_intent.action_hash, action_hash);
}
