//! R-suite v1 on the rules-only Privacy Shield: what a device without the name model
//! of ADR-0042 gets, and what the model's absence falls back to.
//!
//! The targets are the constants in `somicus_privacy_evals::v1`. The rules meet the identifier target (≥ 99 %)
//! and the latency target; they do not meet the names target (≥ 90 %), which needs the model (`r_suite_v1_model.rs`
//! asserts it there), and this suite says so on every run instead of lowering it. The floors below are the rules-only
//! results measured on 2026-11-07 after the recognizer fixes; each may only rise.

#![allow(
    clippy::print_stderr,
    reason = "an evaluation suite reports its measurements on every run; test output is not the product's logs"
)]

use somicus_privacy_evals::v1::{
    IDENTIFIER_RECALL_TARGET, Kind, NAME_RECALL_TARGET, SCREEN_P95_TARGET_MS, run_r_suite_v1,
    samples,
};

/// Rules-only person-name recall (38 of 272 labelled names).
const NAME_RECALL_FLOOR: f64 = 0.139;
/// Rules-only person-name precision (24 of 27 replacements).
const NAME_PRECISION_FLOOR: f64 = 0.888;
/// Rules-only contact recall (24 of 26).
const CONTACT_RECALL_FLOOR: f64 = 0.923;
/// Rules-only share of replaced letters that were personal (1,457 of 1,810).
const LETTER_PRECISION_FLOOR: f64 = 0.804;
/// Negative samples the rules change (5 of 104).
const NEGATIVES_CHANGED_CEILING: u32 = 5;

#[test]
fn the_data_set_has_honest_sizes() {
    let all = samples().expect("the data files are well formed");
    let english = all.iter().filter(|sample| !sample.form.starts_with("v0_"));
    let count = |kind: Kind| english.clone().filter(|sample| sample.kind == kind).count();
    let (names, identifiers, contacts, negatives) = (
        count(Kind::Name),
        count(Kind::Identifier),
        count(Kind::Contact),
        count(Kind::Negative),
    );
    assert!(
        names + identifiers + contacts + negatives >= 300,
        "R-suite v1 needs at least 300 English samples"
    );
    assert!(names >= 200 && identifiers >= 25 && contacts >= 20 && negatives >= 100);
    // The checksum-identifier cases of R-suite v0 stay in the suite.
    assert_eq!(all.len() - english.count(), 15);
}

#[test]
fn rules_only_results_do_not_regress() {
    let report = run_r_suite_v1().expect("the data files are well formed");
    let summary = report.summary("rules only");
    eprintln!("{summary}");
    eprintln!(
        "names recall {:.1} % against the {:.0} % target: the rules alone cannot reach it (ADR-0042)",
        report.names.recall.recall() * 100.0,
        NAME_RECALL_TARGET * 100.0
    );
    eprintln!("recall by form: {:?}", report.by_form);
    assert_eq!(report.recognizer_errors, 0, "every recognizer runs");
    assert!(
        report.identifiers.recall.recall() >= IDENTIFIER_RECALL_TARGET,
        "{summary}"
    );
    assert!(
        report.names.recall.recall() >= NAME_RECALL_FLOOR,
        "{summary}"
    );
    assert!(
        report.names.precision() >= NAME_PRECISION_FLOOR,
        "{summary}"
    );
    assert!(
        report.contacts.recall.recall() >= CONTACT_RECALL_FLOOR,
        "{summary}"
    );
    assert!(
        report.letter_precision() >= LETTER_PRECISION_FLOOR,
        "{summary}"
    );
    assert!(
        report.negatives_changed <= NEGATIVES_CHANGED_CEILING,
        "{summary}"
    );
}

#[test]
fn screen_latency_is_asserted_on_a_quiet_machine_only() {
    let report = run_r_suite_v1().expect("the data files are well formed");
    // Wall-clock time means something on a quiet machine with an optimised build, not on a busy one: opt in.
    if std::env::var_os("SOMICUS_TIMING_ASSERTIONS").is_some() {
        let target = if cfg!(debug_assertions) {
            60.0
        } else {
            SCREEN_P95_TARGET_MS
        };
        assert!(
            report.latency_p95_ms <= target,
            "{}",
            report.summary("rules only")
        );
    } else {
        eprintln!("{}", report.summary("rules only"));
    }
}
