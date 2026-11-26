//! A website's own icon (ADR-0027): `https://<host>/favicon.ico` of the page Somicus works on, so the Dock's activity
//! card can show the site rather than the browser.
//!
//! It is accepted as the one request that leaves the Mac without a [`Redacted`](crate::Redacted) payload:
//! it carries no body, goes only to the host of the page the person already has open, over HTTPS, without cookies, and
//! returns at most a small image whose bytes must start like a known image format. Nothing about it is logged.

use crate::error::NetError;

// The HTTPS transport: the system's own stack inside the process (ADR-0049).
mod native;
use native as https;

/// The largest icon accepted, in bytes.
pub const MAX_ICON_BYTES: usize = 64 * 1024;
/// The longest a request may take, in seconds.
const MAX_SECONDS: u64 = 2;
/// How many redirects a request may follow, each only to HTTPS.
const MAX_REDIRECTS: u32 = 2;

/// A website's icon: its image bytes and their media type.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SiteIcon {
    /// The image, at most [`MAX_ICON_BYTES`].
    pub bytes: Vec<u8>,
    /// Its media type, such as `image/png`.
    pub media_type: &'static str,
}

/// Whether `host` is a public DNS name an icon may be asked from: lower-case letters, digits and hyphens in dot-separated
/// labels of 1 to 63 characters, at least two labels, at most 253 characters, and not an IP address or `localhost`.
#[must_use]
pub fn is_icon_host(host: &str) -> bool {
    let labels: Vec<&str> = host.split('.').collect();
    host.len() <= 253
        && labels.len() >= 2
        && labels.iter().all(|label| {
            (1..=63).contains(&label.len())
                && !label.starts_with('-')
                && !label.ends_with('-')
                && label
                    .bytes()
                    .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
        })
        && labels
            .last()
            .is_some_and(|top| top.bytes().any(|byte| byte.is_ascii_lowercase()))
        && host != "localhost"
}

/// The media type of `bytes` when they start like an image a webview may show: PNG, ICO, GIF, JPEG or WebP. SVG is
/// never accepted.
#[must_use]
pub fn image_type(bytes: &[u8]) -> Option<&'static str> {
    if bytes.starts_with(&[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A]) {
        Some("image/png")
    } else if bytes.starts_with(&[0, 0, 1, 0]) {
        Some("image/x-icon")
    } else if bytes.starts_with(b"GIF87a") || bytes.starts_with(b"GIF89a") {
        Some("image/gif")
    } else if bytes.starts_with(&[0xFF, 0xD8, 0xFF]) {
        Some("image/jpeg")
    } else if bytes.len() >= 12 && bytes.starts_with(b"RIFF") && &bytes[8..12] == b"WEBP" {
        Some("image/webp")
    } else {
        None
    }
}

/// Asks `host` for `/favicon.ico` over HTTPS, following at most two redirects that stay on HTTPS. Blocking: run it off
/// the async runtime.
///
/// Its `net_site_icon` span records the outcome and the icon's size; never the host, which says what the person views.
///
/// # Errors
/// Returns [`NetError::Connection`] for a host that is not a public DNS name or when the request cannot run; a site
/// without an icon, or with something that is not an image, gives `Ok(None)`.
pub fn fetch_site_icon(host: &str) -> Result<Option<SiteIcon>, NetError> {
    let span = somicus_telemetry::span!(INFO, "net_site_icon", transport = "https");
    let result = span.in_scope(|| fetch_unobserved(host));
    if let Ok(Some(icon)) = &result {
        somicus_telemetry::record!(span, bytes = icon.bytes.len());
    }
    span.record_result(&result);
    result
}

fn fetch_unobserved(host: &str) -> Result<Option<SiteIcon>, NetError> {
    if !is_icon_host(host) {
        return Err(NetError::Connection("not a public host name".to_owned()));
    }
    https::fetch(host)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_a_public_dns_name_may_be_asked() {
        for host in [
            "telex.hu",
            "www.youtube.com",
            "open.spotify.com",
            "a-b.example.co.uk",
        ] {
            assert!(is_icon_host(host), "{host}");
        }
        for host in [
            "",
            "localhost",
            "127.0.0.1",
            "192.168.1.1",
            "telex",
            "Telex.hu",
            "telex.hu:8443",
            "-bad.hu",
            "bad-.hu",
            "a..hu",
            "evil.com/../x",
            "evil.com\"\nurl = \"https://other.com",
            "user@evil.com",
        ] {
            assert!(!is_icon_host(host), "{host:?}");
        }
        assert!(!is_icon_host(&format!("{}.hu", "a".repeat(64))));
        assert!(fetch_site_icon("127.0.0.1").is_err());
    }

    #[test]
    fn only_known_image_formats_are_accepted() {
        assert_eq!(
            image_type(&[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A, 0]),
            Some("image/png")
        );
        assert_eq!(image_type(&[0, 0, 1, 0, 1]), Some("image/x-icon"));
        assert_eq!(image_type(b"GIF89a..."), Some("image/gif"));
        assert_eq!(image_type(&[0xFF, 0xD8, 0xFF, 0xE0]), Some("image/jpeg"));
        assert_eq!(image_type(b"RIFF\0\0\0\0WEBPVP8 "), Some("image/webp"));
        assert_eq!(
            image_type(b"<svg xmlns=\"http://www.w3.org/2000/svg\"/>"),
            None
        );
        assert_eq!(image_type(b"<!doctype html>"), None);
        assert_eq!(image_type(&[]), None);
    }
}
