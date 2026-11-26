//! HTTPS to the services through the system's own stack, `WinHTTP` on Windows and `NSURLSession`
//! on macOS (ADR-0049): no process is started, and the rules are those of `crate::native`.

use std::time::Duration;

use super::{
    MAX_RESPONSE_BYTES, MAX_SECONDS, ServiceRequest, ServiceResponse, headers_of, too_large,
};
use crate::Redacted;
use crate::error::NetError;
use crate::native::{Answer, Exchange, Limits, Redirects, exchange};

/// Sends `request` through the native stack, following no redirect, as curl does without `--location`. Blocking: run it off
/// the async runtime.
pub(super) fn send(request: ServiceRequest<'_>) -> Result<ServiceResponse, NetError> {
    let headers = headers_of(&request)?;
    let body = request.body.map(Redacted::into_inner).unwrap_or_default();
    let limits = Limits {
        redirects: Redirects::Never,
        budget: Duration::from_secs(MAX_SECONDS),
        bytes: MAX_RESPONSE_BYTES,
    };
    let prepared =
        Exchange::new(request.method.as_str(), request.url, &headers, limits)?.with_body(&body)?;
    match exchange(&prepared)? {
        Answer::Complete { status, body } => Ok(ServiceResponse { status, body }),
        Answer::TooLarge => Err(too_large()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::service::Method;

    /// Needs the internet, so it is ignored by default: run it with
    /// `cargo nextest run -p somicus-net --run-ignored only` on a Windows machine or a Mac.
    #[test]
    #[ignore = "needs the internet"]
    fn a_public_page_answers_over_https() {
        let answer = send(ServiceRequest {
            method: Method::Get,
            url: "https://example.com/",
            body: None,
            authorization: None,
            dpop: None,
        })
        .unwrap();
        assert_eq!(answer.status, 200);
        assert_ne!(answer.body, Vec::<u8>::new());
    }
}
