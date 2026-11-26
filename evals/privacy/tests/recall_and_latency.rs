//! The R-suite v0 thresholds: checksum identifiers and personal names recalled on synthetic DE,
//! EN and HU screens, and the redaction latency of a 2,000-node screen.

#![allow(
    clippy::print_stderr,
    reason = "an evaluation suite reports its measurements on every run; test output is not the product's logs"
)]

use somicus_privacy_evals::run_r_suite;

#[test]
fn r_suite_v0_meets_its_recall_targets_and_reports_latency() {
    let report = run_r_suite();

    // Verification requirement 1: Checksum IDs >= 99%
    assert!(
        report.overall_checksum_recall >= 0.99,
        "DE={:.2}%, EN={:.2}%, HU={:.2}%, Overall={:.2}%",
        report.de_checksum_recall * 100.0,
        report.en_checksum_recall * 100.0,
        report.hu_checksum_recall * 100.0,
        report.overall_checksum_recall * 100.0
    );
    assert!(
        report.de_checksum_recall >= 0.99,
        "DE checksum ID recall must be >= 99%, got {:.2}%",
        report.de_checksum_recall * 100.0
    );
    assert!(
        report.en_checksum_recall >= 0.99,
        "EN checksum ID recall must be >= 99%, got {:.2}%",
        report.en_checksum_recall * 100.0
    );
    assert!(
        report.hu_checksum_recall >= 0.99,
        "HU checksum ID recall must be >= 99%, got {:.2}%",
        report.hu_checksum_recall * 100.0
    );

    // Verification requirement 2: Names >= 90% (v0 target)
    assert!(
        report.overall_name_recall >= 0.90,
        "Personal name recall must be >= 90%, got {:.2}%",
        report.overall_name_recall * 100.0
    );
    assert!(
        report.de_name_recall >= 0.90,
        "DE name recall must be >= 90%, got {:.2}%",
        report.de_name_recall * 100.0
    );
    assert!(
        report.en_name_recall >= 0.90,
        "EN name recall must be >= 90%, got {:.2}%",
        report.en_name_recall * 100.0
    );
    assert!(
        report.hu_name_recall >= 0.90,
        "HU name recall must be >= 90%, got {:.2}%",
        report.hu_name_recall * 100.0
    );

    // Verification requirement 3: latency p95 <= 40 ms on a 2,000-node screen (<= 60 ms in an unoptimized debug
    // build). Wall-clock timing means something on a quiet machine, not on a shared CI runner, so the assertion is
    // opt-in; the recall assertions above always hold.
    let max_latency = if cfg!(debug_assertions) { 60.0 } else { 40.0 };
    if std::env::var_os("SOMICUS_TIMING_ASSERTIONS").is_some() {
        assert!(
            report.latency_p95_ms <= max_latency,
            "p95 latency on 2,000-node screen must be <= {max_latency} ms, got {:.2} ms",
            report.latency_p95_ms
        );
        assert!(report.all_passed, "All R-suite v0 criteria must pass");
    } else {
        eprintln!(
            "R-suite v0 p95 latency on a 2,000-node screen: {:.2} ms (target {max_latency} ms; set SOMICUS_TIMING_ASSERTIONS=1 to assert it)",
            report.latency_p95_ms
        );
    }
}
