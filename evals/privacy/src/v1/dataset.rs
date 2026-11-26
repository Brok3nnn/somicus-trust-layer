//! The labelled samples of R-suite v1, read from the data files at compile time and checked when they are parsed.

use std::fmt;

use crate::dataset::{LabeledSample, de_samples, en_samples, hu_samples};

const NAMES: &str = include_str!("../../data/names.txt");
const IDENTIFIERS: &str = include_str!("../../data/identifiers.txt");
const NEGATIVES: &str = include_str!("../../data/negatives.txt");

/// Separates the columns of a sample line.
const COLUMN: &str = " | ";
/// Separates the labelled parts of a sample.
const PART: &str = " ; ";

/// What a sample's labelled parts are.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Kind {
    /// A person's name, in any form.
    Name,
    /// A checksum-backed or national identifier: a payment card, an IBAN, a social security or tax number.
    Identifier,
    /// An e-mail address or a phone number.
    Contact,
    /// Nothing: the text must leave the device unchanged.
    Negative,
}

/// The forms a sample may take, and what each one labels.
const FORMS: &[(&str, Kind)] = &[
    ("first_last", Kind::Name),
    ("title", Kind::Name),
    ("initials", Kind::Name),
    ("last_first", Kind::Name),
    ("first_only", Kind::Name),
    ("in_email", Kind::Name),
    ("in_file_name", Kind::Name),
    ("non_english", Kind::Name),
    ("also_a_word", Kind::Name),
    ("letter_case", Kind::Name),
    ("ui_chrome", Kind::Name),
    ("hyphen_apostrophe", Kind::Name),
    ("several", Kind::Name),
    ("middle_name", Kind::Name),
    ("card", Kind::Identifier),
    ("iban", Kind::Identifier),
    ("national_id", Kind::Identifier),
    ("v0_identifier", Kind::Identifier),
    ("email", Kind::Contact),
    ("phone", Kind::Contact),
    ("v0_contact", Kind::Contact),
    ("ui_label", Kind::Negative),
    ("sentence", Kind::Negative),
    ("organisation", Kind::Negative),
    ("place", Kind::Negative),
    ("number", Kind::Negative),
    ("one_case", Kind::Negative),
    ("file_name", Kind::Negative),
];

/// One labelled screen text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Sample {
    /// The form the labelled parts take, such as `initials` or `in_file_name`.
    pub form: &'static str,
    /// What the labelled parts are.
    pub kind: Kind,
    /// The synthetic screen text.
    pub text: &'static str,
    /// The parts of `text` that must not leave the device; each occurs in it exactly once. Empty for a negative.
    pub parts: Vec<&'static str>,
}

/// A data file line that cannot be read as a sample.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DatasetError {
    /// The data file.
    pub file: &'static str,
    /// The line, counted from 1.
    pub line: usize,
    /// What is wrong with it.
    pub reason: &'static str,
}

impl fmt::Display for DatasetError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "{} line {}: {}",
            self.file, self.line, self.reason
        )
    }
}

impl std::error::Error for DatasetError {}

/// Every v1 sample: the names, the identifiers and contact details, the negatives, and R-suite v0's checksum cases.
///
/// # Errors
/// Returns [`DatasetError`] for the first malformed line.
pub fn samples() -> Result<Vec<Sample>, DatasetError> {
    let mut all = parse("names.txt", NAMES)?;
    all.extend(parse("identifiers.txt", IDENTIFIERS)?);
    all.extend(parse("negatives.txt", NEGATIVES)?);
    all.extend(
        [de_samples(), en_samples(), hu_samples()]
            .into_iter()
            .flatten()
            .filter(|sample| sample.is_checksum_id)
            .map(|sample| from_v0(&sample)),
    );
    Ok(all)
}

/// An R-suite v0 checksum case: its e-mail addresses and phone numbers are contact details, the rest identifiers.
fn from_v0(sample: &LabeledSample) -> Sample {
    let is_contact = sample.expected_raw.contains('@') || sample.expected_raw.starts_with('+');
    let (form, kind) = if is_contact {
        ("v0_contact", Kind::Contact)
    } else {
        ("v0_identifier", Kind::Identifier)
    };
    Sample {
        form,
        kind,
        text: sample.text,
        parts: vec![sample.expected_raw],
    }
}

fn parse(file: &'static str, contents: &'static str) -> Result<Vec<Sample>, DatasetError> {
    contents
        .lines()
        .enumerate()
        .filter(|(_, line)| !line.is_empty() && !line.starts_with('#'))
        .map(|(index, line)| {
            parse_line(line).map_err(|reason| DatasetError {
                file,
                line: index + 1,
                reason,
            })
        })
        .collect()
}

fn parse_line(line: &'static str) -> Result<Sample, &'static str> {
    let mut columns = line.split(COLUMN);
    let form = columns.next().ok_or("no form")?;
    let text = columns.next().ok_or("no text")?;
    let parts: Vec<&'static str> = columns
        .next()
        .map(|parts| parts.split(PART).collect())
        .unwrap_or_default();
    if columns.next().is_some() {
        return Err("more than three columns");
    }
    let kind = FORMS
        .iter()
        .find(|(name, _)| *name == form)
        .map(|(_, kind)| *kind)
        .ok_or("unknown form")?;
    match (kind, parts.is_empty()) {
        (Kind::Negative, false) => return Err("a negative labels nothing"),
        (Kind::Name | Kind::Identifier | Kind::Contact, true) => return Err("no labelled part"),
        _ => {}
    }
    if parts
        .iter()
        .any(|part| part.is_empty() || text.matches(part).count() != 1)
    {
        return Err("a labelled part does not occur exactly once in the text");
    }
    Ok(Sample {
        form,
        kind,
        text,
        parts,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_line_is_read_with_its_parts() {
        let sample = parse_line("several | From Ann Lee to Bo Kim | Ann Lee ; Bo Kim");
        assert_eq!(
            sample,
            Ok(Sample {
                form: "several",
                kind: Kind::Name,
                text: "From Ann Lee to Bo Kim",
                parts: vec!["Ann Lee", "Bo Kim"],
            })
        );
    }

    #[test]
    fn a_malformed_line_is_refused() {
        assert_eq!(parse_line("nickname | Ann | Ann"), Err("unknown form"));
        assert_eq!(parse_line("first_last | Ann Lee"), Err("no labelled part"));
        assert_eq!(
            parse_line("ui_label | Save | Save"),
            Err("a negative labels nothing")
        );
        assert_eq!(
            parse_line("first_last | Ann Lee | Bo Kim"),
            Err("a labelled part does not occur exactly once in the text")
        );
        assert_eq!(
            parse_line("first_last | Ann and Ann | Ann"),
            Err("a labelled part does not occur exactly once in the text")
        );
        assert_eq!(
            parse_line("first_last | Ann | Ann | extra"),
            Err("more than three columns")
        );
    }
}
