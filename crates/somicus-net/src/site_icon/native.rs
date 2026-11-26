//! The icon request through the system's own stack, `WinHTTP` on Windows and `NSURLSession` on
//! macOS (ADR-0049): no process is started, and the rules are those of `crate::native`, with redirects allowed as curl's `--location --max-redirs 2 --proto-redir =https` allows them.

use std::time::Duration;

use super::{MAX_ICON_BYTES, MAX_REDIRECTS, MAX_SECONDS, SiteIcon, image_type};
use crate::error::NetError;
use crate::native::{Answer, Exchange, Failure, Limits, Redirects, exchange};

/// Asks `host`, already checked to be a public DNS name, for `/favicon.ico`. Blocking: run it off the async runtime.
///
/// As with curl's `--fail`, a status other than 2xx, a failed exchange, a redirect that is not to HTTPS and an answer
/// over the limit all give `Ok(None)`; only the native stack failing to set the request up is an error.
pub(super) fn fetch(host: &str) -> Result<Option<SiteIcon>, NetError> {
    let url = format!("https://{host}/favicon.ico");
    let limits = Limits {
        redirects: Redirects::HttpsOnly(MAX_REDIRECTS),
        budget: Duration::from_secs(MAX_SECONDS),
        bytes: MAX_ICON_BYTES,
    };
    let prepared = Exchange::new("GET", &url, &[], limits)?;
    match exchange(&prepared) {
        Ok(Answer::Complete {
            status: 200..=299,
            body,
        }) => Ok(image_type(&body).map(|media_type| SiteIcon {
            bytes: body,
            media_type,
        })),
        Err(Failure::Setup(_)) => Err(NetError::Connection(
            "the icon request could not run".to_owned(),
        )),
        Ok(_) | Err(_) => Ok(None),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Needs the internet, so it is ignored by default: run it with
    /// `cargo nextest run -p somicus-net --run-ignored only` on a Windows machine or a Mac.
    #[test]
    #[ignore = "needs the internet"]
    fn a_public_site_icon_is_fetched_over_https() {
        let icon = fetch("github.com").unwrap();
        assert!(icon.is_some(), "no icon came back");
    }
}
