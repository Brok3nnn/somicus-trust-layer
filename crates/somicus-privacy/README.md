# somicus-privacy

The on-device Privacy Shield: find personal data in text, replace it with reversible tokens, and brand the result as
`Redacted<T>`, the only body type [`somicus-net`](../somicus-net/README.md) will send.

## What it does

- **Recognizes** e-mail addresses, phone numbers, payment cards (Luhn), IBANs (mod 97, in one block or printed in
  groups), US social security numbers (with hyphens or spaces), UK National Insurance numbers, German tax IDs,
  Hungarian tax and TAJ numbers, secrets (API keys, bearer tokens, JSON Web Tokens, labelled passwords, private-key
  armour), postal addresses and person names. Names are found by rules: German, English and Hungarian titles, cue
  words and a short name list (a title or cue marks the capitalised name after it, not the rest of the sentence).
- **Asks a name model**, when one is given (`PrivacyShield::with_name_model`, implemented by
  [`somicus-ner`](../somicus-ner/README.md)), for person names in what the rules left. Identifiers, secrets, contact
  details and addresses are blanked out first, so the model never reads them; texts replaced whole never reach it; it
  can only add replacements. A turn gives it a deadline, a screen is scanned in one batch (`scan_names`), and texts the
  model did not reach are counted (`names_left_to_rules`) and go with the rules only.
- **Pseudonymizes** what it finds with per-session tokens (`⟦PERSON_1⟧`, `⟦EMAIL_2⟧`, …) kept in a `TokenVault` that
  is zeroized on drop, and **rehydrates** model answers for display on the device. The same value maps to the same
  token within a session; overlapping matches become one token; when a recognizer cannot run, the whole text becomes
  one token. Window URLs keep only their origin and executable paths only their file name.
- **Owns the `Redacted<T>` brand**: no public constructor, no `From<T>`, no `Deserialize`, and a `Debug` that never
  prints the value. The named ways in besides the shield are `Redacted::without_content` (framing, identifiers,
  credentials), `Redacted::account_data` (the product's own sign-in forms) and `Redacted::map` on an already branded
  value; their call sites are the audit points. `compile_fail` examples on the type show the rest does not compile.
- Does not send anything, log content or persist the vault.

## Public API

- `PrivacyShield`: `new`, `with_name_model`, `scan_names`, `names_left_to_rules`, `redact_text`, `redact_digest`,
  `redact_uigraph` and `rehydrate`.
- The name-model seam: `NameRecognizer`, `NameProtection`, `RulesOnlyReason` and `detect_entities_with_names`.
- `Redacted<T>`, `TokenVault` and `PrivacyError`.
- The recognizers: `detect_all_entities`, `detect_names`, `find_pattern_matches`, the checksum functions
  (`passes_luhn_checksum`, `passes_iban_checksum`, `passes_de_steuer_id_checksum`, `passes_hu_tax_id_checksum`,
  `passes_hu_taj_checksum`), `EntityType`, `PatternCategory`, `RecognizedEntity`, `PatternMatch` and `NameMatch`.

## Dependencies

`somicus-uigraph` (`ClassifiedText`, `PrivacyClass` and the graph types), `somicus-telemetry`, `regex`, `serde`,
`serde_json`, `zeroize` and `thiserror`. It forbids `unsafe` code.

## Decisions

[ADR-0014: on-device redaction](../../docs/adr/ADR-0014-on-device-redaction.md) ·
[ADR-0042: on-device name recognition](../../docs/adr/ADR-0042-on-device-name-recognition.md) ·
[Privacy Shield](../../docs/security/privacy.md)

## Testing

`cargo test -p somicus-privacy`. `tests/redaction_and_rehydration.rs` and the unit tests in `src/shield` cover the
vault, the redaction of texts and UI Graphs and rehydration; `src/shield/name_model_tests.rs` covers the name-model
seam with a fake recognizer (what it is shown, the deadline, invalid ranges, the batch scan). Recall and latency are
measured by the R-suite in [`evals/privacy`](../../evals/privacy/README.md), outside this crate.
