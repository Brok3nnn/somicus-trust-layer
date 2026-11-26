//! R-suite v1: how much of the personal data on labelled synthetic English screens the
//! Privacy Shield finds (recall), how much of what it replaces was personal (precision), and how long it takes per
//! screen, with the rules alone and with the name model of ADR-0042.
//!
//! The samples live in `data/*.txt`, one per line, so that they can be read and reviewed without the code. The
//! checksum-identifier cases of R-suite v0 are scored with them. Every text is scored as static screen text, which the
//! core classifies as content.

mod dataset;
mod latency;
mod score;

use std::path::PathBuf;
use std::sync::Arc;

use somicus_ner::NameModel;

pub use dataset::{DatasetError, Kind, Sample, samples};
pub use score::{KindScore, RSuiteV1Report, Score};

/// The recall target for checksum-backed and national identifiers.
pub const IDENTIFIER_RECALL_TARGET: f64 = 0.99;
/// The recall target for person names.
pub const NAME_RECALL_TARGET: f64 = 0.90;
/// The latency target per English screen, in milliseconds (an optimised build on a recent Mac).
pub const SCREEN_P95_TARGET_MS: f64 = 40.0;
/// Static texts per synthetic screen in the latency measurement: a dense window with a document or a message list.
pub const TEXTS_PER_SCREEN: usize = 40;
/// How often each screen is redacted for the latency measurement.
pub const LATENCY_ROUNDS: usize = 20;
/// The variable naming the folder with the pinned name model (`model.safetensors` and `tokenizer.json`).
pub const MODEL_FOLDER_VARIABLE: &str = "SOMICUS_NAME_MODEL_DIR";

/// Scores the Privacy Shield's rules on every v1 sample and measures their latency per English screen.
///
/// # Errors
/// Returns [`DatasetError`] when a data file is malformed, for example when a labelled part is not in its text.
pub fn run_r_suite_v1() -> Result<RSuiteV1Report, DatasetError> {
    let samples = samples()?;
    let mut report = score::score(&samples, None);
    let (p50, p95) = latency::per_screen_ms(&samples, TEXTS_PER_SCREEN, LATENCY_ROUNDS, None);
    report.latency_p50_ms = p50;
    report.latency_p95_ms = p95;
    Ok(report)
}

/// Scores the rules and the name model together on every v1 sample, and measures their latency per English screen:
/// cold (every screen new to the model, the `latency_*` fields) and with the model's cache warm
/// (`cached_latency_p95_ms`).
///
/// # Errors
/// Returns [`DatasetError`] when a data file is malformed.
pub fn run_r_suite_v1_with_model(model: &Arc<NameModel>) -> Result<RSuiteV1Report, DatasetError> {
    let samples = samples()?;
    model.clear_cache();
    let mut report = score::score(&samples, Some(model.as_ref()));
    let cold = Some((model, latency::Cache::Cold));
    let (p50, p95) = latency::per_screen_ms(&samples, TEXTS_PER_SCREEN, LATENCY_ROUNDS, cold);
    report.latency_p50_ms = p50;
    report.latency_p95_ms = p95;
    let warm = Some((model, latency::Cache::Warm));
    let (_, cached_p95) = latency::per_screen_ms(&samples, TEXTS_PER_SCREEN, LATENCY_ROUNDS, warm);
    report.cached_latency_p95_ms = cached_p95;
    Ok(report)
}

/// The folder named by [`MODEL_FOLDER_VARIABLE`], if it is set: the model files are never part of the repository.
#[must_use]
pub fn model_folder() -> Option<PathBuf> {
    std::env::var_os(MODEL_FOLDER_VARIABLE).map(PathBuf::from)
}
