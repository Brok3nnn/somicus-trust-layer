# Contributing

Thanks for your interest. This repository is the public trust layer of Somicus; the rest of the product is private,
so changes here are reviewed with the product's threat model in mind.

## Before you start

- For anything larger than a small fix, open an issue first so we can agree on the approach.
- Security problems go to [SECURITY.md](SECURITY.md), never to a public issue.
- Use synthetic data only, in tests, fixtures and issues. Never paste real names, addresses, keys or screenshots.

## Development

The toolchain is pinned in `rust-toolchain.toml` (Rust 1.99.1); `rustup` installs it on first use.

```bash
cargo fmt --all --check
cargo clippy --all-targets -- -D warnings
cargo test
```

`cargo nextest run` works too (`.config/nextest.toml`), and `cargo deny check` runs the dependency policy in
`deny.toml`. On Linux, `somicus-net` links the system libcurl (`libcurl4-openssl-dev` on Debian and Ubuntu).

## Rules of the road

- The workspace lints are strict: Clippy's pedantic group is denied, and so are `unwrap`, `expect`, `panic!`,
  `todo!`, `dbg!` and printing outside tests. Every `allow` needs a `reason`.
- `unsafe` is allowed only in the WinHTTP and `NSURLSession` modules of `somicus-net`, each call behind a safe function
  with a `// SAFETY:` comment.
- Changes that widen what can leave the device, what may be logged or what Somicus may do need a threat-model note in
  the pull request and a test that pins the new boundary. A decision that changes an ADR gets a new or amended ADR.
- No new HTTP or WebSocket client crate, and no socket, DNS or process call outside `somicus-net`: egress goes through
  `somicus-net` only (`deny.toml` and Clippy's `disallowed-methods` in `clippy.toml` enforce this).

By contributing you agree that your contributions are licensed under the Apache License 2.0.
