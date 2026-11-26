//! The `NSURLSession` transport against a loopback HTTP server on this Mac, through the real session and its delegate.

use super::{run, stream};
use crate::native::conformance;

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
