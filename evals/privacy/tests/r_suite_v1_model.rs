//! R-suite v1 with the name model of ADR-0042 in the Privacy Shield.
//!
//! The model files are never part of the repository, so these tests are ignored by default. Run them with
//! `cargo test --release -p somicus-privacy-evals --test r_suite_v1_model -- --ignored --test-threads 1 --nocapture`
//! once `SOMICUS_NAME_MODEL_DIR` names a folder with the pinned `model.safetensors` (16-bit) and `tokenizer.json`;
//! without it they fail.

#![allow(
    clippy::print_stderr,
    reason = "an evaluation suite reports its measurements on every run; test output is not the product's logs"
)]

use std::sync::{Arc, OnceLock};

use somicus_ner::{NameModel, NerError};
use somicus_privacy_evals::v1::{
    IDENTIFIER_RECALL_TARGET, NAME_RECALL_TARGET, RSuiteV1Report, SCREEN_P95_TARGET_MS,
    model_folder, run_r_suite_v1_with_model,
};

/// Person-name recall with the model, measured on 2026-11-07 (259 of 272); it may only rise.
const NAME_RECALL_FLOOR: f64 = 0.952;
/// Person-name precision with the model (277 of 283 replacements).
const NAME_PRECISION_FLOOR: f64 = 0.978;
/// Share of replaced letters that were personal, with the model (3,692 of 4,116).
const LETTER_PRECISION_FLOOR: f64 = 0.896;
/// Negative samples changed with the model (6 of 104: the rules' five and one the model adds).
const NEGATIVES_CHANGED_CEILING: u32 = 6;
/// Per new English screen with the model, p95 in a release build: 56.7 to 57.7 ms alone on an Apple M2 Pro on
/// 2026-11-08. Above the 40 ms target, accepted that day with the target held on the cached screen and the turn's
/// 40 ms deadline on a new one (ADR-0042 §Measured); it may only fall.
const COLD_SCREEN_P95_CEILING_MS: f64 = 60.0;

/// The verified model, loaded once (or the reason it does not load), or `None` when `SOMICUS_NAME_MODEL_DIR` is not
/// set. The tests unwrap it, so a model that is missing or fails its check fails them.
fn model() -> Option<&'static Result<Arc<NameModel>, NerError>> {
    static MODEL: OnceLock<Option<Result<Arc<NameModel>, NerError>>> = OnceLock::new();
    MODEL
        .get_or_init(|| {
            let folder = model_folder()?;
            Some(NameModel::load(&folder).map(Arc::new))
        })
        .as_ref()
}

/// The report, measured once for all the tests below, or what kept it from being measured.
fn report() -> Option<&'static Result<RSuiteV1Report, String>> {
    static REPORT: OnceLock<Option<Result<RSuiteV1Report, String>>> = OnceLock::new();
    REPORT
        .get_or_init(|| {
            let model = match model()? {
                Ok(model) => model,
                Err(error) => {
                    return Some(Err(format!("the pinned name model does not load: {error}")));
                }
            };
            Some(
                run_r_suite_v1_with_model(model)
                    .inspect(|report| {
                        eprintln!("{}", report.summary("rules and name model"));
                        eprintln!("recall by form: {:?}", report.by_form);
                    })
                    .map_err(|error| format!("the data files are not well formed: {error}")),
            )
        })
        .as_ref()
}

#[test]
#[ignore = "needs the pinned name model: set SOMICUS_NAME_MODEL_DIR (see the README)"]
fn the_name_model_meets_the_plans_recall_targets() {
    let report =
        report().expect("SOMICUS_NAME_MODEL_DIR names the folder of the pinned name model");
    let report = report
        .as_ref()
        .expect("the R-suite runs with the name model");
    assert_eq!(report.recognizer_errors, 0, "every recognizer runs");
    assert_eq!(report.model_skipped, 0, "the model scans every sample");
    assert!(
        report.names.recall.recall() >= NAME_RECALL_TARGET,
        "{}",
        report.summary("rules and name model")
    );
    assert!(
        report.identifiers.recall.recall() >= IDENTIFIER_RECALL_TARGET,
        "{}",
        report.summary("rules and name model")
    );
}

#[test]
#[ignore = "needs the pinned name model: set SOMICUS_NAME_MODEL_DIR (see the README)"]
fn results_with_the_name_model_do_not_regress() {
    let report =
        report().expect("SOMICUS_NAME_MODEL_DIR names the folder of the pinned name model");
    let report = report
        .as_ref()
        .expect("the R-suite runs with the name model");
    let summary = report.summary("rules and name model");
    assert!(
        report.names.recall.recall() >= NAME_RECALL_FLOOR,
        "{summary}"
    );
    assert!(
        report.names.precision() >= NAME_PRECISION_FLOOR,
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

/// ADR-0042 decision 2: the 16-bit weights the app downloads find the same names as the upstream 32-bit weights on
/// every sample. Runs when `SOMICUS_NAME_MODEL_UPSTREAM` names the pinned upstream `model.safetensors` too.
#[test]
#[ignore = "needs the pinned name model: set SOMICUS_NAME_MODEL_DIR (see the README)"]
fn the_half_precision_model_finds_the_same_names_as_the_upstream_one() {
    let model = model().expect("SOMICUS_NAME_MODEL_DIR names the folder of the pinned name model");
    let model = model.as_ref().expect("the pinned name model loads");
    let Some(upstream) =
        std::env::var_os("SOMICUS_NAME_MODEL_UPSTREAM").map(std::path::PathBuf::from)
    else {
        eprintln!(
            "skipped: set SOMICUS_NAME_MODEL_UPSTREAM to the pinned upstream model.safetensors to compare"
        );
        return;
    };
    let folder = model_folder().expect("the model folder is set");
    let full = NameModel::load_pinned(
        &upstream,
        &somicus_ner::pins::UPSTREAM_MODEL,
        &folder.join(somicus_ner::pins::TOKENIZER.name),
    )
    .expect("the pinned upstream model loads");
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(600);
    let samples = somicus_privacy_evals::v1::samples().expect("the data files are well formed");
    let texts: Vec<&str> = samples.iter().map(|sample| sample.text).collect();
    model.clear_cache();
    let half = somicus_privacy::NameRecognizer::find_names(model.as_ref(), &texts, deadline);
    let whole = somicus_privacy::NameRecognizer::find_names(&full, &texts, deadline);
    let differing: Vec<&str> = texts
        .iter()
        .zip(half.iter().zip(&whole))
        .filter(|(_, (half, whole))| half != whole)
        .map(|(text, _)| *text)
        .collect();
    eprintln!(
        "half precision against upstream: {} of {} samples differ",
        differing.len(),
        texts.len()
    );
    assert!(differing.is_empty(), "{differing:?}");
}

#[test]
#[ignore = "needs the pinned name model: set SOMICUS_NAME_MODEL_DIR (see the README)"]
fn screen_latency_with_the_model_is_asserted_on_a_quiet_machine_only() {
    let report =
        report().expect("SOMICUS_NAME_MODEL_DIR names the folder of the pinned name model");
    let report = report
        .as_ref()
        .expect("the R-suite runs with the name model");
    // Wall-clock time means something on a quiet machine with an optimised build, not on a busy one: opt in.
    if std::env::var_os("SOMICUS_TIMING_ASSERTIONS").is_some() && !cfg!(debug_assertions) {
        let summary = report.summary("rules and name model");
        // The screen the next turn sees: the target (ADR-0042 §Measured).
        assert!(
            report.cached_latency_p95_ms <= SCREEN_P95_TARGET_MS,
            "{summary}"
        );
        // A new screen: bounded by the turn's deadline in the app; here, no slower than measured.
        assert!(
            report.latency_p95_ms <= COLD_SCREEN_P95_CEILING_MS,
            "{summary}"
        );
    }
}
