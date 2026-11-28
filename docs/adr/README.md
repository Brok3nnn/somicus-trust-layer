# Architecture decision records

The decisions that shape the trust layer, taken from the Somicus decision log and edited for this repository: product
planning, internal hosting and links to private documents are removed; the decisions, their reasons and their
consequences are unchanged.

| ADR | Decision | Status |
|---|---|---|
| [0013](ADR-0013-autonomy-levels-and-risk-classes.md) | Autonomy levels L0–L3 and risk classes R0–R5 | Proposed, implemented in `somicus-policy` |
| [0014](ADR-0014-on-device-redaction.md) | On-device redaction (Privacy Shield) before any egress | Accepted |
| [0027](ADR-0027-website-icons.md) | The website icon: a request without a redacted body | Proposed, implemented in `somicus-net` |
| [0040](ADR-0040-confirmations-the-person-can-switch-off.md) | Confirmations on by default, a switch only the person can turn off | Accepted |
| [0041](ADR-0041-policy-as-rust-rules-with-signed-bundles.md) | Policy as Rust rules with signed bundles (instead of Cedar) | Accepted |
| [0042](ADR-0042-on-device-name-recognition.md) | On-device person-name recognition with a hash-pinned DistilBERT | Accepted |
| [0049](ADR-0049-native-https-transports.md) | HTTPS inside the process through each system's own stack | Accepted |
| [0052](ADR-0052-delivering-signed-policy-bundles.md) | Signed policy bundles: a JWS envelope, pinned keys, a deny-list floor | Accepted |

Code comments also cite records of the closed-source app that are app-internal and not published here: ADR-0020,
ADR-0021, ADR-0028, ADR-0032, ADR-0037, ADR-0038, ADR-0043, ADR-0053, ADR-0055 and ADR-0056. The rules they led to are
in the code and its tests.
