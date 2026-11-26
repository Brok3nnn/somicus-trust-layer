# ADR-0040: Confirmations on by default, a switch only the person can turn off

- **Status:** Accepted
- **Date:** 2026-11-07
- **Amends:** [ADR-0013](ADR-0013-autonomy-levels-and-risk-classes.md) ("R4 always freshly confirmed")

## Context

Confirming every R3 and R4 step keeps a human in front of every consequential action, but some people want Somicus to
finish a task they asked for without stopping. The question is who may remove that control, and what must hold
without it.

## Decision

1. **On by default.** R3 and R4 steps wait for a locally rendered confirmation, verified as physical input on the
   confirm control and bound to the action's hash. This holds on every new installation and whenever the stored
   choice is missing or unreadable.
2. **The person may switch every confirmation off.** While it is off, the Policy Guard permits R3 and R4 steps at L2
   and L3 without the confirmation (`ConfirmationPreference::Off` in `somicus-policy`).
3. **What the switch never skips:** R5 is always denied; the app deny and allow lists; nothing is performed at L0; the
   autonomy cap; the deterministic risk floors and the lexicon, which a model can only raise; the typed-value, key and
   transfer rules; a task runs only in a session the person started; the emergency stop; and the Privacy Shield.
4. **Only an organisation's signed bundle can force confirmations on** (`confirmations_required`). No bundle, signed or
   built in, can turn confirmations off: only the person can.
5. **The setting changes only by the person**, in locally rendered UI with physical input; never through a model, a
   server message, screen text or another window. While it is off, an indicator shows it, and one click turns
   confirmations back on.

**This is a deliberate weakening of the default control** for a person who switches it off: no human approves a send,
a delete or a payment before Somicus performs it. The controls of decision 3 replace that approval.

## Alternatives considered

| Option | Pros | Cons | Why not |
|---|---|---|---|
| Confirmations always on | Strongest control | People who asked for a task are stopped at every step | Usability |
| A bundle may turn them off | Central control | A server key could remove a person's protection | Only the person may weaken their own protection |

## Consequences

- (+) The default stays safe, organisations can lock it, and the guard's other invariants hold either way
  (`tests/guard_decisions.rs`, `tests/policy_matching.rs`).
- (−) For a person with confirmations off, a manipulated screen or model meets only the deterministic rules; the
  [threat model](../security/threat-model.md) lists this as a residual risk.
