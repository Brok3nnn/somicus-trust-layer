//! Multilingual personal name recognizer for German (DE), English (EN), and Hungarian (HU).
//!
//! Recognizes personal names in UI contexts from titles, cue words and a short name list. Its recall is measured by
//! the R-suite in `evals/privacy`.

use regex::Regex;
use std::collections::HashSet;
use std::sync::LazyLock;

use super::{compile_pattern, recognizer};
use crate::error::PrivacyError;

/// Represents a detected personal name match within text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NameMatch {
    /// Detected personal name string.
    pub matched_name: String,
    /// Byte offset start in text.
    pub start: usize,
    /// Byte offset end in text.
    pub end: usize,
}

/// A title (with or without its full stop, as British English writes "Dr Sarah Lin"), then one or more capitalised
/// words: the name. Only the title ignores case, so the name's words must start with a capital and "Mr. Tom Hale has
/// checked in" ends at "Hale" instead of running on into the sentence.
static TITLE_PREFIX_RE: LazyLock<Option<Regex>> = LazyLock::new(|| {
    compile_pattern(
        r"\b(?i:Herr|Frau|Dr\.?|Prof\.?|Mr\.?|Mrs\.?|Ms\.?|Miss|ifj\.|id\.|özv\.)\s+([A-ZÁÉÍÓÖŐÚÜŰ][a-záéíóöőúüű]+(?:\s+[A-ZÁÉÍÓÖŐÚÜŰ][a-záéíóöőúüű]+)*)",
    )
});

/// A cue word and a colon, then two or more capitalised words: the name. As with titles, only the cue ignores case.
static CONTEXT_PREFIX_RE: LazyLock<Option<Regex>> = LazyLock::new(|| {
    compile_pattern(
        r"\b(?i:Customer|Contact|Employee|Assigned to|Ansprechpartner|Kunde|Mitarbeiter|Ügyfél|Kapcsolattartó|Munkatárs|Felelős)\s*[:=]\s*([A-ZÁÉÍÓÖŐÚÜŰ][a-záéíóöőúüű]+(?:\s+[A-ZÁÉÍÓÖŐÚÜŰ][a-záéíóöőúüű]+)+)",
    )
});

static NAME_PAIR_RE: LazyLock<Option<Regex>> = LazyLock::new(|| {
    compile_pattern(r"\b([A-ZÁÉÍÓÖŐÚÜŰ][a-záéíóöőúüű]+)\s+([A-ZÁÉÍÓÖŐÚÜŰ][a-záéíóöőúüű]+)\b")
});

static KNOWN_NAMES: LazyLock<HashSet<&'static str>> = LazyLock::new(|| {
    let mut s = HashSet::new();
    // German (DE) surnames & first names
    for name in &[
        "müller",
        "schmidt",
        "schneider",
        "fischer",
        "weber",
        "meyer",
        "wagner",
        "becker",
        "schulz",
        "hoffmann",
        "schäfer",
        "koch",
        "bauer",
        "richter",
        "klein",
        "wolf",
        "schröder",
        "neumann",
        "schwarz",
        "zimmermann",
        "braun",
        "krüger",
        "hofbauer",
        "alexander",
        "thomas",
        "michael",
        "andreas",
        "stefan",
        "christian",
        "frank",
        "klaus",
        "martin",
        "hans",
        "anna",
        "maria",
        "julia",
        "laura",
        "sarah",
        "sandra",
    ] {
        s.insert(*name);
    }
    // English (EN) surnames & first names
    for name in &[
        "smith",
        "johnson",
        "williams",
        "brown",
        "jones",
        "garcia",
        "miller",
        "davis",
        "rodriguez",
        "martinez",
        "hernandez",
        "lopez",
        "gonzalez",
        "wilson",
        "anderson",
        "thomas",
        "taylor",
        "moore",
        "jackson",
        "john",
        "william",
        "james",
        "robert",
        "david",
        "richard",
        "charles",
        "joseph",
        "mary",
        "patricia",
        "jennifer",
        "linda",
        "elizabeth",
        "barbara",
        "susan",
        "jessica",
        "karen",
        "alice",
    ] {
        s.insert(*name);
    }
    // Hungarian (HU) surnames & first names
    for name in &[
        "kovács",
        "nagy",
        "tóth",
        "szabó",
        "kiss",
        "varga",
        "molnár",
        "németh",
        "farkas",
        "balogh",
        "papp",
        "takács",
        "juhász",
        "lakatos",
        "mészáros",
        "oláh",
        "simon",
        "rácz",
        "fekete",
        "szilágyi",
        "török",
        "fehér",
        "balázs",
        "gál",
        "horváth",
        "bálint",
        "lászló",
        "istván",
        "józsef",
        "jános",
        "zoltán",
        "sándor",
        "ferenc",
        "gábor",
        "attila",
        "péter",
        "tamás",
        "katalin",
        "éva",
        "ilona",
        "erzsébet",
        "zsuzsanna",
        "judit",
        "ágnes",
        "andrea",
        "ildikó",
    ] {
        s.insert(*name);
    }
    s
});

/// Finds personal names across DE, EN, and HU in text.
///
/// # Errors
/// Returns [`PrivacyError::RecognitionError`] when a name pattern does not compile: a recognizer that cannot run must
/// not look like one that found nothing.
pub fn detect_names(text: &str) -> Result<Vec<NameMatch>, PrivacyError> {
    if text.len() < 5
        || !text.contains(char::is_whitespace)
        || (text.starts_with('⟦') && text.ends_with('⟧'))
    {
        return Ok(Vec::new());
    }

    let mut matches = Vec::new();

    // 1. Match title prefixes (e.g. "Herr Müller", "Dr. Jane Smith", "özv. Kovács István"): the name, not the title
    for cap in recognizer(&TITLE_PREFIX_RE, "title name")?.captures_iter(text) {
        if let Some(m) = cap.get(1) {
            matches.push(NameMatch {
                matched_name: m.as_str().trim().to_string(),
                start: m.start(),
                end: m.end(),
            });
        }
    }

    // 2. Match context prefixes (e.g. "Customer: John Doe", "Ansprechpartner: Müller")
    for cap in recognizer(&CONTEXT_PREFIX_RE, "labelled name")?.captures_iter(text) {
        if let Some(m) = cap.get(1) {
            let already_covered = matches
                .iter()
                .any(|existing| existing.start <= m.start() && m.end() <= existing.end);
            if !already_covered {
                matches.push(NameMatch {
                    matched_name: m.as_str().trim().to_string(),
                    start: m.start(),
                    end: m.end(),
                });
            }
        }
    }

    // 3. Match known name pairs (e.g. "Alexander Müller", "John Smith", "Kovács István")
    for cap in recognizer(&NAME_PAIR_RE, "name pair")?.captures_iter(text) {
        if let (Some(first), Some(second), Some(full)) = (cap.get(1), cap.get(2), cap.get(0)) {
            let first_lower = first.as_str().to_lowercase();
            let second_lower = second.as_str().to_lowercase();

            let is_first_known = KNOWN_NAMES.contains(first_lower.as_str());
            let is_second_known = KNOWN_NAMES.contains(second_lower.as_str());

            if is_first_known || is_second_known {
                // A pair that overlaps a match already made ("Herr Müller" after the title matched "Müller") adds
                // nothing but the title.
                let overlaps = matches
                    .iter()
                    .any(|existing| existing.start < full.end() && full.start() < existing.end);
                if !overlaps {
                    matches.push(NameMatch {
                        matched_name: full.as_str().trim().to_string(),
                        start: full.start(),
                        end: full.end(),
                    });
                }
            }
        }
    }

    Ok(matches)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names(text: &str) -> Vec<String> {
        detect_names(text)
            .unwrap_or_default()
            .into_iter()
            .map(|m| m.matched_name)
            .collect()
    }

    #[test]
    fn a_title_marks_the_name_after_it_and_not_the_rest_of_the_sentence() {
        assert_eq!(names("Mr. Tom Hale has checked in"), vec!["Tom Hale"]);
        assert_eq!(names("Sehr geehrter Herr Müller, danke"), vec!["Müller"]);
        assert_eq!(
            names("DR. Julia Schneider wurde hinzugefügt"),
            vec!["Julia Schneider"]
        );
        // A title before a word in lower case marks nothing.
        assert_eq!(names("Ms. please call back"), Vec::<String>::new());
    }

    #[test]
    fn a_cue_marks_the_name_after_it_and_not_the_rest_of_the_sentence() {
        assert_eq!(
            names("Customer: John Johnson placed an order"),
            vec!["John Johnson"]
        );
        assert_eq!(
            names("CONTACT: Ann Lee Brown called"),
            vec!["Ann Lee Brown"]
        );
    }
}
