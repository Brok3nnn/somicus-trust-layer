# ADR-0014: On-device redaction (Privacy Shield) before any egress

- **Status:** Accepted (2026-11-08)
- **Date:** 2026-10-17

## Context

Screens contain personal and confidential data. Sending it to cloud models creates GDPR, professional-secrecy and
trust problems. Most guidance does not need field values: it needs structure and labels.

## Decision

All outbound text passes through the on-device **Privacy Shield** (`crates/somicus-privacy`): classification of each
text (public label, content, personal, secret), deterministic recognizers with checksums, an on-device name model
([ADR-0042](ADR-0042-on-device-name-recognition.md)), and **reversible pseudonymization** with an in-memory token vault,
so the model can still refer to "⟦PERSON_1⟧" and the answer is rehydrated on the device. The result is branded
`Redacted<T>`, and the only egress crate (`crates/somicus-net`) accepts nothing else as a body. Planned on top of this:
optional organisation dictionaries, image region filling before any screenshot leaves the device, a user-viewable
egress log, and a server-side re-scan as defence in depth.

## Alternatives considered

| Option | Pros | Cons | Why not |
|---|---|---|---|
| Server-side redaction only | Simpler client | Raw data leaves the device | Data minimisation at the source |
| No redaction, rely on provider zero-retention terms | Simplest | Unacceptable to many organisations | Trust and compliance |
| Irreversible masking only | Simpler | The model cannot refer to entities ("the invoice for X"), so guidance gets worse | Quality |

## Consequences

- (+) Raw personal data does not leave the device by construction; the type system makes the boundary visible.
- (−) On-device CPU and memory cost (budget: 40 ms p95 per screen). Recall is never 100 %, hence the R-suite
  (`evals/privacy`), conservative defaults and defence in depth.
