# Security policy

This repository is the trust layer of Somicus: the code that decides what may leave the device and what Somicus may
do. Reports about it are very welcome.

## Reporting a vulnerability

Please report privately, never in a public issue:

- through GitHub's **Report a vulnerability** button on the repository's Security tab, or
- by e-mail to **vitanbalint@gmail.com**.

Include what you found, how to reproduce it (a failing test with synthetic data is ideal) and the impact you expect.
You will get an answer within a few days. Please give a reasonable time for a fix before you disclose publicly.

## In scope

- A way for un-redacted screen or user text to reach `somicus-net` without one of the named escape hatches
  (`Redacted::without_content`, `Redacted::account_data`, `Redacted::map`).
- Personal data the Privacy Shield should catch but does not, in a category it claims to cover (see
  [`crates/somicus-privacy`](crates/somicus-privacy/README.md)).
- A transport that breaks a rule of [ADR-0049](docs/adr/ADR-0049-native-https-transports.md): plain HTTP to another
  machine, a followed redirect, a stored cookie, an unbounded answer.
- A way past the Policy Guard: an R5 step permitted, a denied app reached, a bundle that loosens the built-in floor,
  a forged, replayed or synthetic confirmation accepted, a signed bundle accepted that should be refused.
- Content in a telemetry span or event.
- A name-model file accepted with the wrong length or hash.

## Out of scope

The Somicus desktop app, its services and its agent runtime are not in this repository; report issues in them to the
same address. Weaknesses that need an already compromised device (a process running as the person) are listed in the
[threat model](docs/security/threat-model.md) as residual risks.

## Supported versions

Only the latest commit on `main` is supported.
