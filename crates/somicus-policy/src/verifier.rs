//! Intent verification engine correlating webview intent submissions with physical input.
//!
//! Enforces:
//! - Physical hardware input required within ≤ 1.0 second of submission.
//! - Injected or synthetic events are rejected immediately.
//! - Clicks must hit the exact physical bounding box of the confirm control; an action registered without bounds
//!   cannot be confirmed by any click.
//! - A click counts only from the moment the control is armed, which the core sets after the control is on screen
//!   at its final place and sets again after it moves or changes size, so a click aimed
//!   at another app cannot land on a control that just appeared under the pointer.
//! - Of the keys, only the dedicated confirmation hotkey confirms, and only when the pending action allows keyboard
//!   approval; Return and Space never do, because a forged submission could follow the person's next key press in
//!   their own app.
//! - Replay prevention: an action hash cannot be confirmed more than once; the record of confirmed hashes is bounded.
//!
//! Residual risk: "physical" means what the operating system reports about the event source. On macOS an
//! event whose source state is the HID system state counts as physical, and on Windows one without the injected flags
//! does. A local process with enough privilege to post events through the HID system state, or a driver that injects
//! below the flags, passes as physical input. The verifier cannot tell those apart; the defence against them is the
//! operating system's own protection of event posting (Accessibility and Input Monitoring permissions).

use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::Arc;
use std::time::{Duration, Instant};

use somicus_uigraph::geometry::PhysicalRect;
use tokio::sync::Mutex;

use crate::error::IntentError;
use crate::input::{ActionHash, InputEventKind, ObservedInput};

/// Default trusted intent window (1.0 second).
pub const DEFAULT_TRUSTED_INTENT_WINDOW: Duration = Duration::from_millis(1000);

/// Default retention period for recent observed input events (5.0 seconds).
pub const DEFAULT_EVENT_RETENTION: Duration = Duration::from_secs(5);

/// Most confirmed action hashes the verifier remembers to name a replay as such.
pub const MAX_CONSUMED_ACTIONS: usize = 4096;

/// An action awaiting physical user confirmation.
#[derive(Debug, Clone)]
pub struct PendingAction {
    /// SHA-256 hash of the exact action being confirmed.
    pub action_hash: ActionHash,
    /// Physical screen bounding box of the confirmation control. Without it no click can confirm the action.
    pub control_bounds: Option<PhysicalRect>,
    /// Whether the dedicated confirmation hotkey may confirm the action; Return and Space never can.
    pub allow_keyboard: bool,
    /// Monotonic timestamp when this pending action was created.
    pub created_at: Instant,
    /// From when a click may confirm the action, when arming is required; an earlier
    /// click never does. `None` allows a click recorded just before registration within the trusted window, as for
    /// clicks on controls that were already on screen (Send and the mode switch).
    pub armed_at: Option<Instant>,
    /// Maximum time-to-live before this pending action expires.
    pub ttl: Duration,
}

impl PendingAction {
    /// Constructs a new pending action with a 30-second TTL and keyboard approval disabled.
    #[must_use]
    pub fn new(action_hash: ActionHash, control_bounds: Option<PhysicalRect>) -> Self {
        Self {
            action_hash,
            control_bounds,
            allow_keyboard: false,
            created_at: Instant::now(),
            armed_at: None,
            ttl: Duration::from_secs(30),
        }
    }

    /// Sets from when a click may confirm the action.
    #[must_use]
    pub fn armed_at(mut self, at: Instant) -> Self {
        self.armed_at = Some(at);
        self
    }

    /// Sets whether the dedicated confirmation hotkey may confirm the action.
    #[must_use]
    pub fn with_allow_keyboard(mut self, allow: bool) -> Self {
        self.allow_keyboard = allow;
        self
    }

    /// Sets how long the action may wait for the person's input before it expires.
    #[must_use]
    pub fn with_ttl(mut self, ttl: Duration) -> Self {
        self.ttl = ttl;
        self
    }
}

/// A successfully verified user confirmation intent.
#[derive(Debug, Clone)]
pub struct VerifiedIntent {
    /// The action hash confirmed.
    pub action_hash: ActionHash,
    /// Monotonic timestamp when verification succeeded.
    pub verified_at: Instant,
    /// The physical input event that confirmed the action.
    pub input: ObservedInput,
}

/// The core trusted user intent verifier.
#[derive(Debug)]
pub struct IntentVerifier {
    trusted_intent_window: Duration,
    event_retention: Duration,
    pending_actions: HashMap<ActionHash, PendingAction>,
    recent_inputs: VecDeque<ObservedInput>,
    consumed_actions: HashSet<ActionHash>,
    consumed_order: VecDeque<ActionHash>,
}

impl Default for IntentVerifier {
    fn default() -> Self {
        Self::new()
    }
}

/// Returns `(matches_target, out_of_bounds)` for an input against `pending`.
fn input_matches(input: &ObservedInput, pending: &PendingAction) -> (bool, bool) {
    match (&input.kind, pending.control_bounds) {
        // A click before the control was armed confirms nothing, wherever it landed.
        (InputEventKind::Click(_), Some(_))
            if pending
                .armed_at
                .is_some_and(|armed_at| input.timestamp < armed_at) =>
        {
            (false, false)
        }
        (InputEventKind::Click(pt), Some(bounds)) => {
            if bounds.contains_point(*pt) {
                (true, false)
            } else {
                (false, true)
            }
        }
        // Without bounds nothing is known about where the click landed, so it confirms nothing.
        (InputEventKind::Click(_), None) => (false, false),
        (InputEventKind::KeyPress(key), _) => (pending.allow_keyboard && key.is_confirm(), false),
    }
}

impl IntentVerifier {
    /// Constructs a new `IntentVerifier` with standard timeouts.
    #[must_use]
    pub fn new() -> Self {
        Self {
            trusted_intent_window: DEFAULT_TRUSTED_INTENT_WINDOW,
            event_retention: DEFAULT_EVENT_RETENTION,
            pending_actions: HashMap::new(),
            recent_inputs: VecDeque::new(),
            consumed_actions: HashSet::new(),
            consumed_order: VecDeque::new(),
        }
    }

    /// Configures a custom trusted intent window duration.
    #[must_use]
    pub fn with_trusted_window(mut self, window: Duration) -> Self {
        self.trusted_intent_window = window;
        self
    }

    /// Registers a pending action awaiting user confirmation.
    ///
    /// # Errors
    /// Returns `IntentError::AlreadyConsumed` if the action hash was already confirmed.
    pub fn register_pending(&mut self, action: PendingAction) -> Result<(), IntentError> {
        if self.consumed_actions.contains(&action.action_hash) {
            return Err(IntentError::AlreadyConsumed(action.action_hash.to_string()));
        }
        self.pending_actions
            .insert(action.action_hash.clone(), action);
        Ok(())
    }

    /// Ends the wait for `action_hash`, which `input` confirmed.
    fn confirm(&mut self, action_hash: &ActionHash, input: ObservedInput) -> VerifiedIntent {
        self.pending_actions.remove(action_hash);
        self.remember_consumed(action_hash.clone());
        VerifiedIntent {
            action_hash: action_hash.clone(),
            verified_at: Instant::now(),
            input,
        }
    }

    /// Remembers that `action_hash` was confirmed, so a second submission is refused as a replay, and forgets the oldest
    /// beyond [`MAX_CONSUMED_ACTIONS`]. Forgetting is safe: a confirmed action's pending entry is removed, so a late
    /// second submission still fails as `NoPendingAction`, and the core registers every action under a fresh random
    /// hash, never an old one.
    fn remember_consumed(&mut self, action_hash: ActionHash) {
        if self.consumed_actions.insert(action_hash.clone()) {
            self.consumed_order.push_back(action_hash);
        }
        while self.consumed_order.len() > MAX_CONSUMED_ACTIONS {
            if let Some(oldest) = self.consumed_order.pop_front() {
                self.consumed_actions.remove(&oldest);
            }
        }
    }

    /// Cancels and removes a pending action.
    pub fn cancel_pending(&mut self, action_hash: &ActionHash) -> bool {
        self.pending_actions.remove(action_hash).is_some()
    }

    /// Arms a pending action's control again from `at`, after it moved or changed size: no earlier click confirms it.
    /// Whether the action was pending.
    pub fn rearm(&mut self, action_hash: &ActionHash, at: Instant) -> bool {
        self.pending_actions
            .get_mut(action_hash)
            .map(|pending| pending.armed_at = Some(at))
            .is_some()
    }

    /// Lets a pending action wait `more` longer, as the person asked for more time (WCAG 2.2.1). Only time changes: the
    /// bounds, the keyboard rule and what counts as physical input stay as they were. Whether the action was pending at
    /// `now` and not yet expired; an expired or unknown action is not brought back.
    pub fn extend_pending(
        &mut self,
        action_hash: &ActionHash,
        more: Duration,
        now: Instant,
    ) -> bool {
        match self.pending_actions.get_mut(action_hash) {
            Some(pending) if now.saturating_duration_since(pending.created_at) <= pending.ttl => {
                pending.ttl = pending.ttl.saturating_add(more);
                true
            }
            _ => false,
        }
    }

    /// Records an observed input event from the OS low-level hook.
    pub fn record_input(&mut self, input: ObservedInput) {
        let now = input.timestamp;
        self.recent_inputs
            .retain(|ev| now.duration_since(ev.timestamp) <= self.event_retention);
        self.recent_inputs.push_back(input);
    }

    /// Verifies whether a submitted confirmation intent was backed by genuine physical input.
    ///
    /// # Errors
    /// Returns `IntentError` if:
    /// - The action hash was already consumed (`AlreadyConsumed`).
    /// - No matching pending action exists or it expired (`NoPendingAction`, `Expired`).
    /// - The input was injected or synthetic (`InjectedInputRejected`).
    /// - The click was outside the control's bounds (`OutOfBounds`).
    /// - No physical input that can confirm this action occurred within the 1.0 s trusted window
    ///   (`NoPhysicalInputObserved`): a click only counts inside registered bounds, and of the keys only the
    ///   confirmation hotkey counts, when the action allows it.
    pub fn verify_intent(
        &mut self,
        action_hash: &ActionHash,
        submission_time: Instant,
    ) -> Result<VerifiedIntent, IntentError> {
        if self.consumed_actions.contains(action_hash) {
            return Err(IntentError::AlreadyConsumed(action_hash.to_string()));
        }

        let pending = self
            .pending_actions
            .get(action_hash)
            .ok_or_else(|| IntentError::NoPendingAction(action_hash.to_string()))?
            .clone();

        if submission_time.duration_since(pending.created_at) > pending.ttl {
            self.pending_actions.remove(action_hash);
            return Err(IntentError::Expired {
                elapsed_ms: u64::try_from(
                    submission_time
                        .duration_since(pending.created_at)
                        .as_millis(),
                )
                .unwrap_or(u64::MAX),
                max_ms: u64::try_from(pending.ttl.as_millis()).unwrap_or(u64::MAX),
            });
        }

        let mut out_of_bounds_candidate = None;
        let mut expired_candidate = None;
        let mut confirming = None;

        for (index, input) in self.recent_inputs.iter().enumerate().rev() {
            let delta = if input.timestamp <= submission_time {
                submission_time.duration_since(input.timestamp)
            } else {
                input.timestamp.duration_since(submission_time)
            };

            let (matches_target, out_of_bounds) = input_matches(input, &pending);

            if matches_target {
                if input.source.is_injected() {
                    return Err(IntentError::InjectedInputRejected);
                }

                if delta <= self.trusted_intent_window {
                    confirming = Some(index);
                    break;
                }

                if expired_candidate.is_none() {
                    expired_candidate = Some(delta);
                }
            } else if out_of_bounds
                && delta <= self.trusted_intent_window
                && out_of_bounds_candidate.is_none()
                && let (InputEventKind::Click(pt), Some(bounds)) =
                    (&input.kind, pending.control_bounds)
            {
                out_of_bounds_candidate = Some((*pt, bounds));
            }
        }

        // One physical input confirms one action: it is taken out, so the same click cannot confirm a second action
        // submitted within the window.
        if let Some(input) = confirming.and_then(|index| self.recent_inputs.remove(index)) {
            return Ok(self.confirm(action_hash, input));
        }

        if let Some(delta) = expired_candidate {
            return Err(IntentError::Expired {
                elapsed_ms: u64::try_from(delta.as_millis()).unwrap_or(u64::MAX),
                max_ms: u64::try_from(self.trusted_intent_window.as_millis()).unwrap_or(u64::MAX),
            });
        }

        if let Some((point, bounds)) = out_of_bounds_candidate {
            return Err(IntentError::OutOfBounds { point, bounds });
        }

        Err(IntentError::NoPhysicalInputObserved)
    }
}

/// Thread-safe shared handle to an `IntentVerifier`.
#[derive(Debug, Clone, Default)]
pub struct IntentVerifierHandle {
    inner: Arc<Mutex<IntentVerifier>>,
}

impl IntentVerifierHandle {
    /// Constructs a new thread-safe verifier handle.
    #[must_use]
    pub fn new() -> Self {
        Self {
            inner: Arc::new(Mutex::new(IntentVerifier::new())),
        }
    }

    /// Registers a pending action awaiting confirmation.
    ///
    /// # Errors
    /// Returns `IntentError` if registration fails.
    pub async fn register_pending(&self, action: PendingAction) -> Result<(), IntentError> {
        let mut verifier = self.inner.lock().await;
        verifier.register_pending(action)
    }

    /// Records an observed input event into the verifier.
    pub async fn record_input(&self, input: ObservedInput) {
        let mut verifier = self.inner.lock().await;
        verifier.record_input(input);
    }

    /// Verifies whether an intent submission is backed by valid physical input.
    ///
    /// # Errors
    /// Returns `IntentError` if intent verification fails.
    pub async fn verify_intent(
        &self,
        action_hash: &ActionHash,
        submission_time: Instant,
    ) -> Result<VerifiedIntent, IntentError> {
        let mut verifier = self.inner.lock().await;
        verifier.verify_intent(action_hash, submission_time)
    }

    /// Cancels a pending action.
    pub async fn cancel_pending(&self, action_hash: &ActionHash) -> bool {
        let mut verifier = self.inner.lock().await;
        verifier.cancel_pending(action_hash)
    }

    /// Arms a pending action's control again from `at`; see [`IntentVerifier::rearm`].
    pub async fn rearm(&self, action_hash: &ActionHash, at: Instant) -> bool {
        let mut verifier = self.inner.lock().await;
        verifier.rearm(action_hash, at)
    }

    /// Lets a pending action wait `more` longer; see [`IntentVerifier::extend_pending`].
    pub async fn extend_pending(&self, action_hash: &ActionHash, more: Duration) -> bool {
        let mut verifier = self.inner.lock().await;
        verifier.extend_pending(action_hash, more, Instant::now())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_replay_record_keeps_only_the_most_recent_confirmations() {
        let mut verifier = IntentVerifier::new();
        let hash = |n: usize| ActionHash::new(format!("{n:064x}"));
        for n in 0..MAX_CONSUMED_ACTIONS + 10 {
            let Ok(action_hash) = hash(n) else {
                unreachable!("a hexadecimal number is a valid action hash");
            };
            verifier.remember_consumed(action_hash);
        }
        assert_eq!(verifier.consumed_actions.len(), MAX_CONSUMED_ACTIONS);
        assert_eq!(verifier.consumed_order.len(), MAX_CONSUMED_ACTIONS);
        let (Ok(oldest), Ok(newest)) = (hash(0), hash(MAX_CONSUMED_ACTIONS + 9)) else {
            unreachable!("a hexadecimal number is a valid action hash");
        };
        assert!(!verifier.consumed_actions.contains(&oldest));
        assert!(verifier.consumed_actions.contains(&newest));
    }
}
