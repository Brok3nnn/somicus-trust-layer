//! Scoring: a labelled part counts as found when every letter and digit of it lies inside something the recognizers
//! replace; a replacement counts as correct when it overlaps a labelled part, whatever kind the recognizer named it.

use std::collections::BTreeMap;
use std::ops::Range;
use std::time::{Duration, Instant};

use somicus_privacy::{
    EntityType, NameRecognizer, PrivacyError, RecognizedEntity, detect_all_entities,
    detect_entities_with_names,
};

use super::dataset::{Kind, Sample};

/// Found parts out of labelled parts.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Score {
    /// Labelled parts.
    pub expected: u32,
    /// Labelled parts the recognizers replaced completely.
    pub found: u32,
}

impl Score {
    /// The share of labelled parts found, from 0 to 1 (1 when nothing is labelled).
    #[must_use]
    pub fn recall(self) -> f64 {
        ratio(self.found, self.expected)
    }

    const fn add(&mut self, other: Self) {
        self.expected = self.expected.saturating_add(other.expected);
        self.found = self.found.saturating_add(other.found);
    }
}

/// Recall and precision for one kind of personal data.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct KindScore {
    /// Found parts out of labelled parts.
    pub recall: Score,
    /// Replacements the recognizers made for this kind, on every sample.
    pub replaced: u32,
    /// Of those, the ones that overlap a labelled part.
    pub correct: u32,
}

impl KindScore {
    /// The share of this kind's replacements that covered labelled personal data, from 0 to 1.
    #[must_use]
    pub fn precision(self) -> f64 {
        ratio(self.correct, self.replaced)
    }
}

/// What R-suite v1 measured.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct RSuiteV1Report {
    /// Samples scored.
    pub samples: u32,
    /// Person names.
    pub names: KindScore,
    /// Checksum-backed and national identifiers.
    pub identifiers: KindScore,
    /// E-mail addresses and phone numbers.
    pub contacts: KindScore,
    /// Replacements of every kind, and how many of them overlap a labelled part.
    pub all: KindScore,
    /// Negative samples, which must leave the device unchanged.
    pub negatives: u32,
    /// Negative samples in which something was replaced.
    pub negatives_changed: u32,
    /// Letters and digits replaced, on every sample.
    pub replaced_letters: u32,
    /// Of those, the ones inside a labelled part: the rest were replaced although nothing personal was there.
    pub replaced_letters_labelled: u32,
    /// Texts on which a recognizer could not run; the shield then replaces the whole text.
    pub recognizer_errors: u32,
    /// Texts the name model did not scan (it failed on them); 0 without a model.
    pub model_skipped: u32,
    /// Recall per form, such as `initials` or `in_file_name`.
    pub by_form: BTreeMap<&'static str, Score>,
    /// Median redaction time per English screen, in milliseconds.
    pub latency_p50_ms: f64,
    /// 95th-percentile redaction time per English screen, in milliseconds.
    pub latency_p95_ms: f64,
    /// With a name model: the 95th-percentile time per screen when the screen's texts were scanned before (the model's
    /// cache answers, as on the next turn over the same window), in milliseconds; 0 without a model.
    pub cached_latency_p95_ms: f64,
}

impl RSuiteV1Report {
    /// The share of replaced letters and digits that were inside a labelled part, from 0 to 1: a replacement that runs
    /// on past a name into the rest of the sentence lowers it, although it overlaps the name.
    #[must_use]
    pub fn letter_precision(&self) -> f64 {
        ratio(self.replaced_letters_labelled, self.replaced_letters)
    }

    /// One line with every measure, for test output and the README: `setup` says what was measured (`rules only`,
    /// `rules and name model`).
    #[must_use]
    pub fn summary(&self, setup: &str) -> String {
        let percent = |value: f64| format!("{:.1} %", value * 100.0);
        format!(
            "R-suite v1, {setup}, {} samples: names recall {} ({}/{}) precision {}; identifiers recall {} ({}/{}) \
             precision {}; contacts recall {} precision {}; letters replaced that were personal {}; negatives changed \
             {} of {}; per English screen p50 {:.2} ms p95 {:.2} ms, cached p95 {:.2} ms",
            self.samples,
            percent(self.names.recall.recall()),
            self.names.recall.found,
            self.names.recall.expected,
            percent(self.names.precision()),
            percent(self.identifiers.recall.recall()),
            self.identifiers.recall.found,
            self.identifiers.recall.expected,
            percent(self.identifiers.precision()),
            percent(self.contacts.recall.recall()),
            percent(self.contacts.precision()),
            percent(self.letter_precision()),
            self.negatives_changed,
            self.negatives,
            self.latency_p50_ms,
            self.latency_p95_ms,
            self.cached_latency_p95_ms,
        )
    }
}

/// How long the name model may take per sample while recall is measured: long enough never to be the limit.
const SCORING_BUDGET: Duration = Duration::from_secs(60);

/// The entities of `text`: the rules', and with `model` the names it finds in what they left. The flag says whether
/// the model scanned the text.
fn entities(
    text: &str,
    model: Option<&dyn NameRecognizer>,
) -> Result<(Vec<RecognizedEntity>, bool), PrivacyError> {
    match model {
        Some(model) => detect_entities_with_names(text, model, Instant::now() + SCORING_BUDGET),
        None => detect_all_entities(text).map(|entities| (entities, true)),
    }
}

/// Scores every sample, with the rules alone or with `model` too. The latency fields stay zero.
pub(super) fn score(samples: &[Sample], model: Option<&dyn NameRecognizer>) -> RSuiteV1Report {
    let mut report = RSuiteV1Report::default();
    for sample in samples {
        report.samples = report.samples.saturating_add(1);
        let replaced = if let Ok((entities, scanned)) = entities(sample.text, model) {
            report.model_skipped = report.model_skipped.saturating_add(u32::from(!scanned));
            entities
                .into_iter()
                .map(|entity| (kind_of(entity.entity_type), entity.start..entity.end))
                .collect()
        } else {
            // The shield replaces the whole text when a recognizer cannot run.
            report.recognizer_errors = report.recognizer_errors.saturating_add(1);
            vec![(sample.kind, 0..sample.text.len())]
        };
        score_sample(&mut report, sample, &replaced);
    }
    report
}

fn score_sample(report: &mut RSuiteV1Report, sample: &Sample, replaced: &[(Kind, Range<usize>)]) {
    let labelled: Vec<Range<usize>> = sample
        .parts
        .iter()
        .filter_map(|part| {
            sample
                .text
                .find(part)
                .map(|start| start..start + part.len())
        })
        .collect();
    if sample.kind == Kind::Negative {
        report.negatives = report.negatives.saturating_add(1);
        if !replaced.is_empty() {
            report.negatives_changed = report.negatives_changed.saturating_add(1);
        }
    }
    let tally = labelled.iter().fold(Score::default(), |tally, part| Score {
        expected: tally.expected.saturating_add(1),
        found: tally
            .found
            .saturating_add(u32::from(covers(sample.text, part, replaced))),
    });
    report.by_form.entry(sample.form).or_default().add(tally);
    if let Some(kind) = kind_score(report, sample.kind) {
        kind.recall.add(tally);
    }
    report.all.recall.add(tally);
    for (offset, character) in sample.text.char_indices() {
        let is_replaced = replaced.iter().any(|(_, range)| range.contains(&offset));
        if character.is_alphanumeric() && is_replaced {
            let is_labelled = labelled.iter().any(|range| range.contains(&offset));
            report.replaced_letters = report.replaced_letters.saturating_add(1);
            report.replaced_letters_labelled = report
                .replaced_letters_labelled
                .saturating_add(u32::from(is_labelled));
        }
    }
    for (kind, range) in replaced {
        let correct = labelled.iter().any(|part| overlaps(part, range));
        report.all.replaced = report.all.replaced.saturating_add(1);
        report.all.correct = report.all.correct.saturating_add(u32::from(correct));
        if let Some(score) = kind_score(report, *kind) {
            score.replaced = score.replaced.saturating_add(1);
            score.correct = score.correct.saturating_add(u32::from(correct));
        }
    }
}

const fn kind_score(report: &mut RSuiteV1Report, kind: Kind) -> Option<&mut KindScore> {
    match kind {
        Kind::Name => Some(&mut report.names),
        Kind::Identifier => Some(&mut report.identifiers),
        Kind::Contact => Some(&mut report.contacts),
        Kind::Negative => None,
    }
}

/// The kind of personal data a recognized entity is. Secrets and postal addresses count with no kind.
const fn kind_of(entity_type: EntityType) -> Kind {
    match entity_type {
        EntityType::PersonName => Kind::Name,
        EntityType::PaymentCard | EntityType::Iban | EntityType::NationalId => Kind::Identifier,
        EntityType::Email | EntityType::Phone => Kind::Contact,
        EntityType::Secret | EntityType::Address => Kind::Negative,
    }
}

/// Whether every letter and digit of `part` lies inside a replaced range: punctuation left between tokens carries
/// nothing personal.
fn covers(text: &str, part: &Range<usize>, replaced: &[(Kind, Range<usize>)]) -> bool {
    text.get(part.clone()).is_some_and(|slice| {
        slice
            .char_indices()
            .filter(|(_, character)| character.is_alphanumeric())
            .all(|(offset, _)| {
                let at = part.start + offset;
                replaced.iter().any(|(_, range)| range.contains(&at))
            })
    })
}

const fn overlaps(first: &Range<usize>, second: &Range<usize>) -> bool {
    first.start < second.end && second.start < first.end
}

fn ratio(numerator: u32, denominator: u32) -> f64 {
    if denominator == 0 {
        1.0
    } else {
        f64::from(numerator) / f64::from(denominator)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample(kind: Kind, text: &'static str, parts: Vec<&'static str>) -> Sample {
        Sample {
            form: "test",
            kind,
            text,
            parts,
        }
    }

    #[test]
    fn a_part_is_found_only_when_all_its_letters_are_replaced() {
        let text = "From Ann Lee";
        assert!(covers(text, &(5..12), &[(Kind::Name, 5..12)]));
        assert!(!covers(text, &(5..12), &[(Kind::Name, 9..12)]));
        // Punctuation outside the replacement leaks nothing.
        assert!(covers(
            "J. Smith",
            &(0..8),
            &[(Kind::Name, 0..1), (Kind::Name, 3..8)]
        ));
    }

    #[test]
    fn a_replacement_on_a_negative_lowers_precision_and_counts_the_negative() {
        let mut report = RSuiteV1Report::default();
        let negative = sample(Kind::Negative, "Mark as Read", Vec::new());
        score_sample(&mut report, &negative, &[(Kind::Name, 0..4)]);
        let name = sample(Kind::Name, "Owner: Ann Lee", vec!["Ann Lee"]);
        score_sample(&mut report, &name, &[(Kind::Name, 7..14)]);
        assert_eq!((report.negatives, report.negatives_changed), (1, 1));
        assert_eq!(
            report.names.recall,
            Score {
                expected: 1,
                found: 1
            }
        );
        assert_eq!((report.names.replaced, report.names.correct), (2, 1));
        assert!((report.names.precision() - 0.5).abs() < f64::EPSILON);
        // "Mark" on the negative and "Ann Lee" on the name: 4 of 10 replaced letters were not personal.
        assert_eq!(
            (report.replaced_letters, report.replaced_letters_labelled),
            (10, 6)
        );
    }
}
