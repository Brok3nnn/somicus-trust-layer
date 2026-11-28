# somicus-net

The network egress of the Somicus core. Every request body must be a `Redacted<T>`, so only Privacy Shield output or
explicitly branded framing can be sent as a body; the URL and the credential headers are plain strings the caller
chooses. HTTPS runs inside the process through each system's own stack
([ADR-0049](../../docs/adr/ADR-0049-native-https-transports.md)); no request starts a process. It is the only crate of
the workspace that opens connections: elsewhere Clippy's `disallowed-methods` (`clippy.toml`) rejects socket, DNS and
process calls, and `deny.toml` keeps HTTP clients and the system stacks' bindings out; CI runs both.

## What it does

- **`send`**, the one request path to the Somicus services. `ServiceRequest::body` is
  `Option<Redacted<Vec<u8>>>`. Plain HTTP goes only to a loopback address; tokens and DPoP proofs travel in headers;
  no answer is read past `MAX_RESPONSE_BYTES` (8 MiB); an HTTPS request ends within 25 seconds. The shared half in
  `src/native/` checks the URL (HTTPS only, no credentials) and the headers, refuses to send while the environment
  names a TLS key log (`SSLKEYLOGFILE`), and holds the limits; each system has one module, and `conformance.rs` holds
  the checks the macOS and Linux backends' tests run against a loopback server:
  - **Windows: WinHTTP** (`src/native/winhttp.rs`). TLS 1.2 or 1.3, the Windows certificate store, the system's proxy
    settings, no cookies, automatic sign-in off (a proxy's 407 is an error), no redirects, time-outs from what is left
    of the request's budget.
  - **macOS: `NSURLSession`** (`src/native/urlsession.rs`). One ephemeral session per request with no cookies, cache
    or stored credentials, App Transport Security's TLS 1.2 or later, a delegate that refuses redirects the caller does
    not allow (and any to plain HTTP) and rejects every sign-in challenge, and a body cancelled as soon as it passes its
    limit.
  - **Linux: libcurl** (`src/native/libcurl.rs`) through the `curl` crate's safe API, linked to the distribution's
    library: TLS 1.2 or later, the distribution's CA bundle named explicitly, no cookie engine, `.netrc` or
    authentication, and no automatic redirect.
- **`fetch_site_icon`**, one of the two documented requests without a `Redacted<T>` body
  ([ADR-0027](../../docs/adr/ADR-0027-website-icons.md)): the favicon of a public host the person already has open,
  over HTTPS without cookies, at most 64 KiB, within 2 seconds, at most two redirects that stay on HTTPS, and checked
  to be a raster image.
- **`Downloader`**, the bodiless fetch of a hash-pinned file such as the name model
  ([ADR-0042](../../docs/adr/ADR-0042-on-device-name-recognition.md)): HTTPS `GET` without cookies, credentials or a
  query, streamed into a temporary file and hashed as it arrives, never longer than the pinned length, moved into place
  only when length and SHA-256 match, resumed after a break and retried a bounded number of times. It never follows
  redirects. `FileTransport` is its seam (`NativeFileTransport` on every system, fakes in tests).
- Never logs bodies or credentials: `Debug` shows neither.

## Public API

`send`, `ServiceRequest`, `ServiceResponse`, `Method`, `MAX_RESPONSE_BYTES`; `fetch_site_icon`, `is_icon_host`,
`SiteIcon`; `Downloader`, `PinnedFile`, `Retry`, `FileTransport`, `FetchRequest`, `Fetched`, `NativeFileTransport`,
`DownloadError`, `is_download_url`; `Redacted` (re-exported), `NetError`.

## Dependencies

`somicus-privacy` (the brand), `somicus-telemetry`, `sha2`, `tokio` and `thiserror`; on Windows the `windows` crate,
on macOS `objc2`, `objc2-foundation` and `block2`, on Linux `curl` with default features off. `deny.toml` bans HTTP
and WebSocket client crates, allows `curl` only here, and refuses a static or vendored libcurl. `unsafe` code exists
only in the WinHTTP and `NSURLSession` modules, each call behind a safe function with a `// SAFETY:` comment; on Linux
the crate forbids `unsafe`.

## Testing

`cargo test -p somicus-net`. `tests/service_requests.rs` sends a request to a loopback server,
`tests/egress_boundary.rs` sends Privacy Shield output to one and checks that only its tokens reach the wire, and the
unit tests cover the URL and header checks, the key-log refusal, the response limit, the icon checks, the pinned
downloads (whole, wrong-hash, resumed, bounded attempts, refused addresses, no redirects) and the conformance checks of
the macOS and Linux backends. The macOS tests run the `NSURLSession` and the libcurl backends (against macOS's
libcurl). The platform-neutral half of the WinHTTP backend is tested everywhere, and CI runs Clippy on Windows; its
Windows-only tests have not been run yet. Three tests that need the internet are `#[ignore]`d
(`cargo test -p somicus-net -- --ignored`).
