//! Tests verifying the Redacted<T>-only egress boundary invariant.
//!
//! That raw values cannot reach these APIs at all is shown by the `compile_fail` examples on `ServiceRequest` and on
//! `somicus_privacy::Redacted`; that Privacy Shield output is sent is covered by the `service` and `urlsession`
//! unit tests.

use somicus_net::Redacted;

#[test]
fn the_brand_never_prints_its_value() {
    let framing = Redacted::without_content("session-frame".to_owned());
    assert_eq!(format!("{framing:?}"), "Redacted(…)");
    assert_eq!(framing.as_inner(), "session-frame");
    assert_eq!(framing.into_inner(), "session-frame");
}
