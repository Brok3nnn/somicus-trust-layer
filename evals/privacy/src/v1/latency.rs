//! Redaction time per English screen: the samples' texts in groups of a dense screen's static texts, each redacted as
//! content by one Privacy Shield per screen, the way the core redacts a capture before a turn. With the name model, the
//! shield first scans the screen in one batch (`PrivacyShield::scan_names`), as the turn does.

use std::sync::Arc;
use std::time::{Duration, Instant};

use somicus_ner::NameModel;
use somicus_privacy::PrivacyShield;
use somicus_uigraph::privacy::ClassifiedText;

use super::dataset::Sample;

/// The name model's budget per screen while the time is measured: long enough never to cut a scan short, so the
/// measurement is the model's whole cost.
const MEASURING_BUDGET: Duration = Duration::from_secs(60);

/// How the name model's cache stands when a screen is redacted.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Cache {
    /// Emptied before every screen: each screen is new to the model.
    Cold,
    /// Kept: every text was scanned on an earlier round, as on the next turn over the same window.
    Warm,
}

/// The median and 95th-percentile time, in milliseconds, to redact one screen of `texts_per_screen` texts, over
/// `rounds` rounds of every screen. `(0, 0)` when there is nothing to measure.
pub(super) fn per_screen_ms(
    samples: &[Sample],
    texts_per_screen: usize,
    rounds: usize,
    model: Option<(&Arc<NameModel>, Cache)>,
) -> (f64, f64) {
    let screens: Vec<Vec<ClassifiedText>> = samples
        .chunks(texts_per_screen.max(1))
        .map(|chunk| {
            chunk
                .iter()
                .map(|sample| ClassifiedText::content_value(sample.text))
                .collect()
        })
        .collect();
    // Warm up the compiled recognizers (and the model's cache, when it is kept), as a running app has.
    for screen in &screens {
        redact(screen, model.map(|(model, _)| model));
    }
    let mut durations: Vec<f64> = Vec::with_capacity(screens.len() * rounds);
    for _ in 0..rounds {
        for screen in &screens {
            if let Some((model, Cache::Cold)) = model {
                model.clear_cache();
            }
            let start = Instant::now();
            redact(screen, model.map(|(model, _)| model));
            durations.push(start.elapsed().as_secs_f64() * 1_000.0);
        }
    }
    durations.sort_by(f64::total_cmp);
    (percentile(&durations, 50), percentile(&durations, 95))
}

/// Redacts `screen` with a shield of its own, scanning it with `model` first when there is one.
fn redact(screen: &[ClassifiedText], model: Option<&Arc<NameModel>>) {
    let mut shield = match model {
        Some(model) => {
            let recognizer: Arc<NameModel> = Arc::clone(model);
            PrivacyShield::with_name_model(recognizer, MEASURING_BUDGET)
        }
        None => PrivacyShield::new(),
    };
    let texts: Vec<&str> = screen.iter().map(ClassifiedText::raw_text).collect();
    shield.scan_names(&texts);
    for text in screen {
        let _ = shield.redact_text(text);
    }
}

/// The `percent`th percentile of sorted `values`, by the nearest rank; 0 for none.
fn percentile(values: &[f64], percent: usize) -> f64 {
    if values.is_empty() {
        return 0.0;
    }
    let rank = (values.len() * percent).div_ceil(100).max(1);
    values.get(rank - 1).copied().unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::percentile;

    #[test]
    fn the_percentile_is_the_nearest_rank() {
        let values: Vec<f64> = (1..=20).map(f64::from).collect();
        assert!((percentile(&values, 50) - 10.0).abs() < f64::EPSILON);
        assert!((percentile(&values, 95) - 19.0).abs() < f64::EPSILON);
        assert!(percentile(&[], 95).abs() < f64::EPSILON);
    }
}
