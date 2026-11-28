# Changelog

All notable changes to this project are documented here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and the project uses [Semantic Versioning](https://semver.org/).

## [Unreleased]

### Security

- `somicus-ner` builds on candle 0.11.1 (removes the vulnerable zip 7 from the dependency tree; the affected code
  paths were never called) and safetensors 0.8.0. The name model gives the same results on it: names recall 95.2 %,
  precision 97.9 %.
- `PrivacyShield::redact_uigraph` also redacts each node's selectors (DOM ids, accessibility paths, anchors) and
  keyboard shortcut; before, their screen text stayed in the `Redacted<UiGraph>`.
- Every HTTPS backend refuses to send while `SSLKEYLOGFILE` is set; before, only libcurl did.
- Only `somicus-net` may open connections: Clippy's `disallowed-methods` rejects socket, DNS and process calls in every
  other crate, `deny.toml` keeps the `windows` and `objc2-foundation` bindings in `somicus-net` as it does `curl`, and
  CI runs `cargo deny check`.

### Changed

- CI runs fmt, Clippy and the tests on Linux and macOS, Clippy on Windows and rustdoc with warnings denied; every
  action is pinned by commit.
- Removed the placeholder `Transport`, `SessionConnection`, `BoxFuture`, `UrlSessionTransport`, `ProxyConfig` and
  `ProxyAuth` from `somicus-net`: nothing implemented or used them.
- The `compile_fail` examples name the error they must fail with, `somicus-telemetry` has `compile_fail` examples for
  its field allow-list, and `tests/egress_boundary.rs` sends Privacy Shield output over a loopback connection and
  checks that only its tokens reach the wire. A test that only checked the package name is removed.
- Test and eval data use reserved example domains instead of real ones.
- zeroize 1.9.2 and regex 1.13.2, and ed25519-dalek without its unused `rand_core` feature; Dependabot groups its
  updates into one pull request a month.

### Fixed

- Docs and comments: what the egress brand covers (the body, not the URL or the credential headers), which backends
  the conformance suite runs on, the website icon as one of two requests without a redacted body, the size of the
  built-in deny list, stale comments and rustdoc warnings. References to unpublished documents are gone, and the ADR
  index lists the app-internal records the code cites.

## [1.0.0] - 2026-11-26

First public release of the Somicus trust layer, taken from the private Somicus monorepo.

### Added

- `somicus-privacy`: the Privacy Shield (pattern recognizers with checksums, rule-based name recognition, reversible
  pseudonymization with a zeroized token vault) and the `Redacted<T>` egress brand.
- `somicus-net`: the egress path, which sends only `Redacted<T>` bodies through each system's own HTTPS stack
  (WinHTTP, `NSURLSession`, libcurl), plus the hash-pinned downloader and the website-icon request.
- `somicus-policy`: the local Policy Guard (risk classes, autonomy levels, trusted-intent verification, value, key
  and transfer rules, Ed25519-signed policy bundles).
- `somicus-ner`: on-device person-name recognition with a hash-pinned DistilBERT model run by candle, and a
  `convert_name_model` example that produces the pinned 16-bit weights.
- `somicus-uigraph` and `somicus-telemetry`: the UI Graph types the shield and the guard read, and allow-listed,
  content-free diagnostics.
- `evals/privacy`: the R-suite (recall, precision and latency of the Privacy Shield on synthetic screens).
- `compile_fail` examples that show un-redacted values cannot be branded or sent.

### Changed

- Tests that need the name model files are marked `#[ignore]` and fail, instead of skipping, when run without them.
- Telemetry test captures register a global no-op dispatcher, so parallel `cargo test` runs record every span.

[Unreleased]: https://github.com/Brok3nnn/somicus-trust-layer/compare/v1.0.0...HEAD
[1.0.0]: https://github.com/Brok3nnn/somicus-trust-layer/releases/tag/v1.0.0
