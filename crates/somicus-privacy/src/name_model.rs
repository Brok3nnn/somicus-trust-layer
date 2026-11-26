//! The seam for the on-device name model (ADR-0042): the Privacy Shield asks a [`NameRecognizer`] for person names in
//! what its rules left, and adds them to the rules' matches.
//!
//! The model lives in `somicus-ner`, which implements the trait, so this crate gains no machine-learning dependency
//! and no file or network access. A recognizer can only add replacements: the identifiers, secrets, contact details and
//! addresses the rules found are blanked out before the model sees a text, texts the shield replaces whole never reach
//! it, and its ranges are merged with the rules' matches as one more recognizer.

use std::ops::Range;
use std::time::Instant;

/// Finds person names with a model.
pub trait NameRecognizer: Send + Sync {
    /// For each of `texts`, the byte ranges of the person names in it, in the same order, or `None` for a text it did
    /// not scan: because `deadline` passed before it reached the text, or because the model failed on it.
    ///
    /// Implementations may cache their results per text, so that a text scanned once (for example by
    /// [`PrivacyShield::scan_names`](crate::PrivacyShield::scan_names), which scans a whole screen in one batch) is
    /// answered again without the model. The shield drops a range that is empty, runs past its text or does not fall
    /// on character boundaries.
    fn find_names(&self, texts: &[&str], deadline: Instant) -> Vec<Option<Vec<Range<usize>>>>;
}

/// How the Privacy Shield protects person names (ADR-0042 decision 6). The account hub shows it; a turn's trace records
/// its label, never content.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum NameProtection {
    /// The name model and the rules.
    Model,
    /// The name model and the rules, except for texts the model did not reach before the turn's deadline, which went
    /// with the rules only.
    Partial,
    /// The rules only, and why.
    RulesOnly(RulesOnlyReason),
}

impl NameProtection {
    /// Whether the name model is ready: `false` while the shield works with the rules only.
    #[must_use]
    pub const fn model_ready(self) -> bool {
        !matches!(self, Self::RulesOnly(_))
    }
}

/// Why the Privacy Shield works with the rules only.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RulesOnlyReason {
    /// The model files are not on this device yet.
    NotDownloaded,
    /// The model files are being downloaded.
    Downloading,
    /// A downloaded or stored file did not have the pinned length and SHA-256, so it was not used.
    VerificationFailed,
    /// The verified files could not be read as the model.
    LoadFailed,
    /// The verified model is being loaded (after the app started or after it was unloaded while idle).
    Loading,
}

/// The state's name, for spans and traces.
impl somicus_telemetry::Label for NameProtection {
    fn label(&self) -> &'static str {
        match self {
            Self::Model => "model",
            Self::Partial => "partial",
            Self::RulesOnly(_) => "rules_only",
        }
    }
}

/// The reason's name, for spans, traces and the webviews.
impl somicus_telemetry::Label for RulesOnlyReason {
    fn label(&self) -> &'static str {
        match self {
            Self::NotDownloaded => "not_downloaded",
            Self::Downloading => "downloading",
            Self::VerificationFailed => "verification_failed",
            Self::LoadFailed => "load_failed",
            Self::Loading => "loading",
        }
    }
}

/// `text` with every byte inside `ranges` replaced by a space, so a model reads what the rules left without the parts
/// they replace. Lengths and offsets do not change: a range found in the result is a range of `text`.
#[must_use]
pub fn blank_out(text: &str, ranges: &[Range<usize>]) -> String {
    let mut blanked = String::with_capacity(text.len());
    for (offset, character) in text.char_indices() {
        if ranges.iter().any(|range| range.contains(&offset)) {
            for _ in 0..character.len_utf8() {
                blanked.push(' ');
            }
        } else {
            blanked.push(character);
        }
    }
    blanked
}

/// The ranges of `found` that are a non-empty part of `text` on character boundaries.
pub(crate) fn valid_ranges(text: &str, found: Vec<Range<usize>>) -> Vec<Range<usize>> {
    found
        .into_iter()
        .filter(|range| {
            range.start < range.end
                && range.end <= text.len()
                && text.is_char_boundary(range.start)
                && text.is_char_boundary(range.end)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use somicus_telemetry::Label as _;

    use super::*;

    #[test]
    fn blanking_out_keeps_every_offset() {
        let text = "Pay Zoë at zoe@example.test now";
        let email = 11..27;
        let blanked = blank_out(text, std::slice::from_ref(&email));
        assert_eq!(blanked.len(), text.len());
        assert_eq!(&blanked[..11], &text[..11]);
        assert!(blanked[email.clone()].bytes().all(|byte| byte == b' '));
        assert_eq!(&blanked[email.end..], &text[email.end..]);
        // A multi-byte letter inside a range becomes as many spaces as it had bytes.
        let e_with_diaeresis = 2..4;
        assert_eq!(
            blank_out("Zoë", std::slice::from_ref(&e_with_diaeresis)),
            "Zo  "
        );
    }

    #[test]
    fn a_range_off_its_text_or_off_a_character_boundary_is_dropped() {
        let text = "Zoë Hale";
        assert_eq!(
            valid_ranges(text, vec![0..4, 5..9, 0..3, 4..4, 6..20]),
            vec![0..4, 5..9]
        );
    }

    #[test]
    fn only_the_model_states_are_ready_and_every_label_is_fixed() {
        assert!(NameProtection::Model.model_ready());
        assert!(NameProtection::Partial.model_ready());
        assert!(!NameProtection::RulesOnly(RulesOnlyReason::NotDownloaded).model_ready());
        for reason in [
            RulesOnlyReason::NotDownloaded,
            RulesOnlyReason::Downloading,
            RulesOnlyReason::VerificationFailed,
            RulesOnlyReason::LoadFailed,
            RulesOnlyReason::Loading,
        ] {
            assert!(somicus_telemetry::field::is_label(reason.label()));
            assert_eq!(NameProtection::RulesOnly(reason).label(), "rules_only");
        }
    }
}
