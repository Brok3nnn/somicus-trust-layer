# ADR-0027: The website icon, the one request without a redacted body

- **Status:** Proposed (security review pending); implemented in `crates/somicus-net`
- **Date:** 2026-10-22

## Context

When Somicus works in a web page, the app shows which site a step is on. The real site icon needs a request that is
not Privacy Shield output: it has no body to redact, but it reveals the host to that host.

## Decision

1. **One request to the page's own host.** `somicus_net::fetch_site_icon` asks `https://<host>/favicon.ico` of the
   page Somicus works on, once per host and session. It is the only request that leaves the device without a
   `Redacted<T>` body: no body, no cookies, no identifying header beyond a fixed user agent; only a host the person
   already has open; HTTPS only, at most two redirects that stay on HTTPS; at most 64 KiB within 2 seconds, which must
   start like a known raster image (PNG, ICO, GIF, JPEG or WebP; never SVG). Nothing about the request is logged.
2. **Only public hosts** (`is_icon_host`): lower-case DNS labels of letters, digits and hyphens, at least two labels,
   no IP address, no `localhost`, no port or user.
3. Until the icon arrives, or when the site has none, the app shows the site's initial.

## Alternatives considered

| Option | Pros | Cons | Why not |
|---|---|---|---|
| Read the icon from screen pixels | No request | A screen-recording permission | Permission cost |
| A letter tile only | No request | Not the real icon | Recognisability |

## Consequences

- (+) The exception is one function, bounded and tested (`crates/somicus-net/src/site_icon.rs`).
- (−) The page's host learns that this device has the page open, which its own page load already told it.
