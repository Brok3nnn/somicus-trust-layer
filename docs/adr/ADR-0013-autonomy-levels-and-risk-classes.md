# ADR-0013: Autonomy levels L0–L3 and risk classes R0–R5

- **Status:** Proposed; implemented in `crates/somicus-policy`
- **Date:** 2026-10-17

## Context

Somicus combines showing (a pointer to the next click), telling (an explanation) and doing (performing steps). People
and organisations need predictable, configurable limits on what the agent may do; many want guidance only.

## Decision

- **Autonomy levels:** L0 Answer, L1 Show (default), L2 Assist (single steps on request), L3 Autopilot (procedures
  with checkpoints). There is no unattended mode.
- **Risk classes:** R0 Observe, R1 Navigate, R2 Edit (a reversible draft), R3 Commit, R4 Critical, R5 Prohibited. A
  step's class is the maximum of the local heuristics (a whole-word label lexicon and floors per action kind and
  target role), any organisation override and the model's proposal: a model can raise a class, never lower it.
- **Hard invariants:** R5 is never performed; R3 and R4 wait for a confirmation rendered locally and verified as
  physical input (amended by [ADR-0040](ADR-0040-confirmations-the-person-can-switch-off.md): the person may switch
  confirmations off); physical input by the person pauses execution.
- Effective autonomy = min(the person's choice, the organisation's cap, the policy result, any safety downgrade).

## Alternatives considered

| Option | Pros | Cons | Why not |
|---|---|---|---|
| Guidance only (no execution) | Simplest, safest | Leaves the value of doing on the table | Showing and doing together is the product |
| Binary "assist on/off" | Simple to explain | Too coarse for organisation policies | Governance needs |
| Model-decided autonomy | Flexible | Unpredictable, unsafe | Safety |

## Consequences

- (+) Clear, explainable governance with safe defaults.
- (−) The quality of risk classification is critical: it needs multilingual lexicons and evaluations.
