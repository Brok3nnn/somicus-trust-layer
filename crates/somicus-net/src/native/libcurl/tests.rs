//! The libcurl transport against a loopback HTTP server, through the real library: on Linux the distribution's, on the
//! Mac its own. The same checks run for `NSURLSession`.

use std::ffi::OsStr;

use super::{refuse_key_log, run, stream};
use crate::native::{Failure, conformance};

#[test]
fn answers_with_the_status_and_body_and_sends_the_request_as_given() {
    conformance::answers_with_the_status_and_body_and_sends_the_request_as_given(run, stream);
}

#[test]
fn follows_no_redirect_unless_allowed_and_then_only_to_https() {
    conformance::follows_no_redirect_unless_allowed_and_then_only_to_https(run, stream);
}

#[test]
fn cancels_an_answer_over_its_limit() {
    conformance::cancels_an_answer_over_its_limit(run, stream);
}

#[test]
fn ends_within_its_time_budget() {
    conformance::ends_within_its_time_budget(run, stream);
}

#[test]
fn leaves_a_sign_in_unanswered() {
    conformance::leaves_a_sign_in_unanswered(run, stream);
}

#[test]
fn keeps_no_cookie_between_requests() {
    conformance::keeps_no_cookie_between_requests(run, stream);
}

#[test]
fn streams_a_file_answer_and_reports_a_broken_or_wrong_one() {
    conformance::streams_a_file_answer_and_reports_a_broken_or_wrong_one(run, stream);
}

#[test]
fn passes_a_sink_failure_on() {
    conformance::passes_a_sink_failure_on(run, stream);
}

#[test]
fn sends_nothing_when_the_environment_asks_for_a_tls_key_log() {
    assert!(matches!(
        refuse_key_log(Some(OsStr::new("/tmp/keys.log"))),
        Err(Failure::KeyLog)
    ));
    assert!(refuse_key_log(None).is_ok());
}

#[test]
fn follows_an_allowed_redirect_only_to_https() {
    use super::redirect_target;
    assert_eq!(
        redirect_target("https://example.com/a", "/b").as_deref(),
        Some("https://example.com/b")
    );
    assert_eq!(
        redirect_target("https://example.com/a", "https://other.example/c").as_deref(),
        Some("https://other.example/c")
    );
    assert_eq!(
        redirect_target("https://example.com/a", "http://example.com/b"),
        None
    );
    assert_eq!(redirect_target("http://127.0.0.1/a", "/b"), None);
    assert_eq!(redirect_target("https://example.com/a", "b"), None);
}

/// Needs the internet, so it is ignored by default: run it with `cargo nextest run -p somicus-net --run-ignored only`
/// on Linux, where libcurl is the transport, or on a Mac against its own libcurl.
#[test]
#[ignore = "needs the internet"]
fn a_public_page_answers_over_tls_with_the_systems_roots() {
    use crate::native::{Answer, Exchange, Limits, Redirects};
    let exchange = Exchange::new(
        "GET",
        "https://example.com/",
        &[],
        Limits {
            redirects: Redirects::Never,
            budget: std::time::Duration::from_secs(20),
            bytes: 1024 * 1024,
        },
    )
    .unwrap();
    assert!(matches!(
        run(&exchange, "https").unwrap(),
        Answer::Complete { status: 200, .. }
    ));
}

/// A build machine without the libcurl headers makes curl-sys build its own copy, without TLS; this fails such a build's
/// tests before HTTPS fails for the person (ADR-0049: the distribution's library, never a bundled one).
#[test]
fn the_linked_libcurl_speaks_tls() {
    let version = curl::Version::get();
    assert!(version.feature_ssl(), "the linked libcurl has no TLS");
    assert!(
        version.ssl_version().is_some(),
        "the linked libcurl names no TLS library"
    );
}
