# ADR-0041: Policy as Rust rules with signed bundles

- **Status:** Accepted. Supersedes an earlier, unimplemented plan to use the Cedar policy language.
- **Date:** 2026-11-07

## Context

An earlier proposal chose Cedar as the policy language on the device and the server, with hard safety invariants kept
in code outside Cedar. The code never used Cedar: the device's Policy Guard was plain Rust in
`crates/somicus-policy`. This record decides which of the two to keep.

What the guard does (read in the code):

- **The bundle** (`src/bundle.rs`): `PolicyBundle` is typed data, not code: a version, the workspace it belongs to,
  an autonomy cap (L0 to L3), an optional allow list, a deny list, a creation time, an end of validity and whether
  confirmations are required. App entries match whole names or bundle identifiers, never parts of a name.
- **Installing** (`src/guard.rs`): a bundle is installed only when its signature verifies, it names the device's
  workspace, its version is higher than the active one and its end of validity has not passed; otherwise the guard
  refuses it and keeps the active bundle.
- **Evaluating** (`src/guard.rs`): the effective autonomy is the lower of the requested level and the bundle's cap;
  R5 is denied, an app the bundle does not allow is denied, nothing runs at L0, and an R3 or R4 step needs a physical
  confirmation unless the person switched confirmations off and the bundle does not require them.
- **Risk** (`src/risk`, `src/action.rs`): a whole-word label lexicon in five languages and deterministic floors per
  action kind and target role; a model can only raise a class.
- **Trusted intent** (`src/verifier.rs`), and the **value, key and transfer rules** (`src/typed`, `src/numbers`,
  `src/fields.rs`, `src/keys.rs`, `src/transfer.rs`).

## Decision

1. **Device policy is Rust rules in `crates/somicus-policy`.** Cedar is not adopted.
2. **What an organisation can set is a typed, versioned bundle.** A new setting is a new typed field with a version
   increase, reviewed as a security-sensitive change.
3. **Bundles are signed, workspace-bound and forward-only**
   (format and keys: [ADR-0052](ADR-0052-delivering-signed-policy-bundles.md)).
4. **Hard invariants stay compiled in.** R5 is never performed, nothing runs at L0, a model can only raise a risk
   class, the physical-input rules of the verifier, the value, key and transfer rules, and no bundle can turn
   confirmations off ([ADR-0040](ADR-0040-confirmations-the-person-can-switch-off.md)). A bundle can make the device
   stricter, never looser.

## What Cedar would have added

| Cedar capability | What we do instead |
|---|---|
| A policy language administrators author | Fixed typed fields |
| Formal analysis and a validator | Unit and property tests of a small rule set |
| The same evaluation on device and server | No server mirror yet; it would reuse the bundle fields |
| A schema of principals, actions and resources | Rust types are the schema |

Its costs: a policy engine and its dependencies inside the device's trusted computing base, a WebAssembly build for
the services, and a schema to version with every bundle.

## When to revisit

When organisations need rules per user or group that typed fields cannot express, when the server must evaluate the
same rules, when the bundle grows beyond about ten interdependent settings, or when customers ask for policies as
reviewable code with formal analysis.

## Consequences

- (+) The policy that guards every step is one Rust crate, reviewed like any other code; no policy engine to maintain.
- (−) Organisations cannot write their own conditions; a new kind of rule ships with an app release.
