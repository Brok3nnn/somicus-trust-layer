# ADR-0049: HTTPS inside the process through each system's own stack

- **Status:** Accepted (2026-11-08)
- **Date:** 2026-11-09

## Context

Before this decision, macOS and Linux started a `curl` process for every request. A process per request costs time,
puts the request's shape on a command line the system can see, and depends on a program the person can replace.

## Decision

1. **One stack per system, inside the Somicus process**, all in `somicus-net`, the only crate allowed to open
   connections:
   - **Windows:** WinHTTP.
   - **macOS:** `NSURLSession` with an ephemeral configuration, through the `objc2` bindings.
   - **Linux:** libcurl through the `curl` crate, linked to the distribution's own libcurl (its TLS library, trusted
     roots and proxy environment), never a bundled or static copy.
2. **The same rules on every stack:**
   - HTTPS only; plain HTTP only to a loopback address;
   - TLS 1.2 or later, checked against the system's trusted roots;
   - no cookies, cache, stored credentials, automatic sign-in or `.netrc`;
   - service requests and downloads never follow a redirect; site icons follow at most two, HTTPS only;
   - an answer larger than its cap is cancelled while it streams: 8 MiB for service requests, 64 KiB for icons, and a
     download never runs past its pinned length;
   - each request ends within its budget: 25 s for service requests, 2 s for icons, 15 minutes per download attempt;
   - a proxy that asks for sign-in (407) is an error;
   - errors carry fixed words and a code, never a URL with a query, a header or a body.
3. **TLS secrets stay private.** A process whose environment names a TLS key log file (`SSLKEYLOGFILE`) sends nothing.
   On Linux, the trusted roots are set explicitly to the distribution's bundle instead of any path the environment
   names.
4. **`unsafe` stays where the platform needs it**: only the macOS module that calls Objective-C and the Windows module
   that calls WinHTTP, every call behind a safe function with a `// SAFETY:` comment. The Linux module uses the `curl`
   crate's safe API, and the crate forbids `unsafe` there.
5. **Dependencies.** The `curl` crate is allowed only as a dependency of `somicus-net`, with default features off, so
   OpenSSL is not compiled; `deny.toml` enforces this and bans HTTP and WebSocket client crates.

## Alternatives considered

| Option | Cons | Why not |
|---|---|---|
| Keep the `curl` process | A process per request; the request on a command line; a replaceable program | The reasons for this record |
| reqwest with rustls | Ignores the system's roots and proxy settings unless configured | Enterprise networks expect the system stack |
| libcurl on macOS too | macOS's managed proxies, VPN rules and certificate trust live in `NSURLSession`'s stack | Same |

## Consequences

- (+) No request starts a process; administrators configure Somicus the way they configure every other app.
- (−) More `unsafe` code on macOS, and backends that run for real only on their own systems: the WinHTTP backend's
  Windows-only tests have not been run yet.

## Threat-model note

No second program sits between the core and the network, so a replaced `curl` binary, its configuration file and its
command line drop out of the threat model. A backend that followed a redirect, stored a cookie or read an unbounded
answer would reopen exfiltration and denial-of-service paths; the shared conformance tests pin each rule.
