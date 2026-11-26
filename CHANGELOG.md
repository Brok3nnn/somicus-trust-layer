# Changelog

All notable changes to this project are documented here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and the project uses [Semantic Versioning](https://semver.org/).

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

[1.0.0]: https://github.com/Brok3nnn/somicus-trust-layer/releases/tag/v1.0.0
