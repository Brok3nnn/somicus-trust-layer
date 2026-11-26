# somicus-policy

The on-device Policy Guard: whether Somicus may perform a step, at which risk class, with which values, and whether a
confirmation came from the person's physical input. It is plain Rust rules
([ADR-0041](../../docs/adr/ADR-0041-policy-as-rust-rules-with-signed-bundles.md)), independent of any model.

## What it does

- **Hard invariants**: R5 is always denied, denied apps are always blocked, nothing runs at L0, the autonomy cap
  holds, and R3 and R4 steps need a physical confirmation unless the person switched confirmations off
  (`ConfirmationPreference`, a local preference passed with each `ActionCandidate`) and the bundle does not require
  them ([ADR-0040](../../docs/adr/ADR-0040-confirmations-the-person-can-switch-off.md)). A bundle can only make the
  guard stricter. The built-in deny list (`BUILT_IN_DENIED_APPS`: password managers, banking and brokerage apps,
  security settings) applies under every bundle.
- **Classifies risk locally** (`RiskClassifier`, a whole-word label lexicon in English, German, Hungarian, French and
  Spanish, and `action_floor` from the action kind and the target's role). A model's estimate can raise the class,
  never lower it ([ADR-0013](../../docs/adr/ADR-0013-autonomy-levels-and-risk-classes.md)).
- **Decides which values Somicus may type** (`typed_value_allowed`, `typed_draft_allowed`), which numbers a composed
  text may hold (`KnownNumbers`), which keys it may press (`ALLOWED_KEYS`), and whether a copy, paste or drag may move
  content (`copy_decision`, `paste_decision`, `drag_decision`).
- **Verifies trusted intent**: a confirmation must follow physical input inside the confirm control's bounds within
  1 s, bound to the action's hash and never replayed; synthetic events (including those Somicus posts itself, marked
  with `SOMICUS_SYNTHETIC_USER_DATA`) are rejected.
- **Verifies signed policy bundles**
  ([ADR-0052](../../docs/adr/ADR-0052-delivering-signed-policy-bundles.md)): a JWS (`EdDSA`, `kid` = the key's JWK
  thumbprint) over the transmitted bytes, checked with a pinned key (`TrustedKeys`) before the payload is read
  strictly; installed only when newer, for the device's workspace and within its first-install validity; restored
  after a restart; reset to the built-in bundle.
- Does not observe input or act: the app feeds observed clicks to the verifier and performs the permitted steps.

## Public API

- `PolicyGuard`, `ActionCandidate`, `ConfirmationPreference`, `PolicyDecision`, `BundleInstall`; `PolicyBundle`,
  `BUILT_IN_DENIED_APPS`, `WorkspaceType`; `SignedPolicyBundle`, `TrustedKeys`, `key_id`.
- `RiskClassifier`, `RiskClass`, `AutonomyLevel`, `ActionContext`, `ActionKind`, `ActionTarget`, `action_floor`,
  `classify_performed_action`.
- `IntentVerifier`, `IntentVerifierHandle`, `PendingAction`, `VerifiedIntent`, `ObservedInput`, `InputSource`,
  `ActionHash`, `ConfirmationKey`.
- `typed_value_allowed`, `typed_draft_allowed`, `is_strict_target`, `search_words_allowed`, `KnownNumbers`,
  `evaluate`, `allowed_key`, `key_floor`, `types_text`, the transfer decisions and the consequence checks.
- The input classifiers for macOS, Windows and Linux events, `PolicyError` and `IntentError`.

## Dependencies

`somicus-uigraph` (shared geometry), `somicus-telemetry`, `ed25519-dalek`, `sha2` and `base64` (bundle signatures),
`serde`, `serde_json`, `tokio` and `thiserror`. It forbids `unsafe` code.

## Testing

`cargo test -p somicus-policy`. `tests/guard_decisions.rs`, `tests/policy_matching.rs`, `tests/trusted_intent.rs`,
`tests/input_sources.rs`, `tests/signed_bundles.rs` and `tests/consequences.rs` cover the invariants, the person's
switch against a bundle that requires confirmations, signed envelopes (headers, keys, strict payloads, the golden
fixture `tests/fixtures/policy-bundle-v1.json`, restore and reset), intent verification (timing, bounds, replay,
synthetic input) and the input classifiers; the unit tests in `src/numbers` (with `proptest` properties) and
`src/typed` cover the value rules. The German and Hungarian words in the lexicon and the tests are functional data:
the guard reads labels of third-party apps in those languages.
