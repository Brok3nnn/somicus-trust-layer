//! Multilingual recognizer suite combining structured patterns and personal name extraction.

pub mod names;
pub mod patterns;

use std::sync::LazyLock;
use std::time::Instant;

use regex::Regex;

use crate::error::PrivacyError;
use crate::name_model::{NameRecognizer, blank_out, valid_ranges};

pub use names::{NameMatch, detect_names};
pub use patterns::{
    PatternCategory, PatternMatch, find_pattern_matches, passes_de_steuer_id_checksum,
    passes_hu_taj_checksum, passes_hu_tax_id_checksum, passes_iban_checksum, passes_luhn_checksum,
};

/// Type of entity recognized by Privacy Shield.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum EntityType {
    /// Email address.
    Email,
    /// Telephone number.
    Phone,
    /// Credit/debit card number passing Luhn.
    PaymentCard,
    /// IBAN passing mod-97.
    Iban,
    /// National identity or tax number.
    NationalId,
    /// Secret or credential.
    Secret,
    /// Personal name in DE/EN/HU.
    PersonName,
    /// Postal address.
    Address,
}

impl EntityType {
    /// Returns the canonical token prefix (e.g. `EMAIL`, `PERSON`, `IBAN`).
    #[must_use]
    pub const fn token_prefix(self) -> &'static str {
        match self {
            Self::Email => "EMAIL",
            Self::Phone => "PHONE",
            Self::PaymentCard => "CARD",
            Self::Iban => "IBAN",
            Self::NationalId => "SSN",
            Self::Secret => "SECRET",
            Self::PersonName => "PERSON",
            Self::Address => "ADDR",
        }
    }
}

/// A recognized entity with byte range in text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecognizedEntity {
    /// Entity classification.
    pub entity_type: EntityType,
    /// Raw text matched in the input.
    pub raw_text: String,
    /// Byte offset start in the containing text.
    pub start: usize,
    /// Byte offset end in the containing text.
    pub end: usize,
}

/// Compiles a recognizer's static pattern. `None` is a defect in the pattern, which [`recognizer`] reports.
pub(crate) fn compile_pattern(pattern: &str) -> Option<Regex> {
    Regex::new(pattern).ok()
}

/// The compiled pattern of the recognizer called `name`.
///
/// # Errors
/// Returns [`PrivacyError::RecognitionError`] when the pattern does not compile, so a broken recognizer is reported
/// instead of silently matching nothing and letting its entities through.
pub(crate) fn recognizer(
    compiled: &'static LazyLock<Option<Regex>>,
    name: &str,
) -> Result<&'static Regex, PrivacyError> {
    LazyLock::force(compiled).as_ref().ok_or_else(|| {
        PrivacyError::RecognitionError(format!("the {name} recognizer's pattern does not compile"))
    })
}

/// Scans text and extracts all sensitive entities (patterns + personal names).
///
/// # Errors
/// Returns [`PrivacyError::RecognitionError`] when a recognizer cannot run; callers must then treat the whole text as
/// sensitive.
pub fn detect_all_entities(text: &str) -> Result<Vec<RecognizedEntity>, PrivacyError> {
    if text.len() < 3 || (text.starts_with('⟦') && text.ends_with('⟧')) {
        return Ok(Vec::new());
    }

    let mut entities = Vec::new();

    // 1. Pattern matches (checksum identifiers, emails, phones, secrets)
    for p in find_pattern_matches(text)? {
        let entity_type = match p.category {
            PatternCategory::Email => EntityType::Email,
            PatternCategory::Phone => EntityType::Phone,
            PatternCategory::PaymentCard => EntityType::PaymentCard,
            PatternCategory::Iban => EntityType::Iban,
            PatternCategory::UsSsn
            | PatternCategory::DeSteuerId
            | PatternCategory::HuTaxId
            | PatternCategory::HuTaj
            | PatternCategory::UkNino => EntityType::NationalId,
            PatternCategory::Secret => EntityType::Secret,
            PatternCategory::Address => EntityType::Address,
        };

        entities.push(RecognizedEntity {
            entity_type,
            raw_text: p.matched_text,
            start: p.start,
            end: p.end,
        });
    }

    // 2. Personal names across DE/EN/HU
    for n in detect_names(text)? {
        let overlaps = entities
            .iter()
            .any(|e| !(n.end <= e.start || n.start >= e.end));
        if !overlaps {
            entities.push(RecognizedEntity {
                entity_type: EntityType::PersonName,
                raw_text: n.matched_name,
                start: n.start,
                end: n.end,
            });
        }
    }

    entities.sort_by_key(|e| e.start);
    Ok(entities)
}

/// What a name model reads of `text`: the text with the identifiers, secrets, contact details and addresses the rules
/// found blanked out, so the model never reads them, or `None` when nothing that could be a name is left (no letter,
/// or a whole token). The rules' own name matches stay, so the model reads a name whole ("Dr Sarah Lin", where a rule
/// matched "Dr Sarah"); its ranges and theirs are merged.
pub(crate) fn name_model_input(text: &str, entities: &[RecognizedEntity]) -> Option<String> {
    if text.len() < 2 || (text.starts_with('⟦') && text.ends_with('⟧')) {
        return None;
    }
    let found: Vec<std::ops::Range<usize>> = entities
        .iter()
        .filter(|e| e.entity_type != EntityType::PersonName)
        .map(|e| e.start..e.end)
        .collect();
    let blanked = blank_out(text, &found);
    blanked.chars().any(char::is_alphabetic).then_some(blanked)
}

/// The person names `recognizer` finds in `blanked` (see [`name_model_input`]), as entities of `text`, or `None` when
/// it did not scan the text before `deadline`.
pub(crate) fn model_names(
    text: &str,
    blanked: &str,
    recognizer: &dyn NameRecognizer,
    deadline: Instant,
) -> Option<Vec<RecognizedEntity>> {
    let found = recognizer
        .find_names(&[blanked], deadline)
        .into_iter()
        .next()??;
    Some(
        valid_ranges(text, found)
            .into_iter()
            .map(|range| RecognizedEntity {
                entity_type: EntityType::PersonName,
                raw_text: text[range.clone()].to_owned(),
                start: range.start,
                end: range.end,
            })
            .collect(),
    )
}

/// Scans `text` with the rules and then with `recognizer` (ADR-0042): the identifiers, secrets, contact details and
/// addresses the rules found are blanked out first, so the model never reads them, and the names it finds are added to
/// the rules' matches. The flag is `false` when the recognizer did not scan the text (its deadline passed or it
/// failed); the entities are then the rules' alone.
///
/// # Errors
/// Returns [`PrivacyError::RecognitionError`] when a rule-based recognizer cannot run; callers must then treat the
/// whole text as sensitive.
pub fn detect_entities_with_names(
    text: &str,
    recognizer: &dyn NameRecognizer,
    deadline: Instant,
) -> Result<(Vec<RecognizedEntity>, bool), PrivacyError> {
    let mut entities = detect_all_entities(text)?;
    let Some(blanked) = name_model_input(text, &entities) else {
        return Ok((entities, true));
    };
    let Some(names) = model_names(text, &blanked, recognizer, deadline) else {
        return Ok((entities, false));
    };
    entities.extend(names);
    entities.sort_by_key(|e| e.start);
    Ok((entities, true))
}

#[cfg(test)]
mod tests {
    use std::sync::LazyLock;

    use regex::Regex;

    use super::*;

    #[test]
    fn a_pattern_that_does_not_compile_is_reported_not_silently_empty() {
        static BROKEN: LazyLock<Option<Regex>> = LazyLock::new(|| compile_pattern("("));
        assert!(matches!(
            recognizer(&BROKEN, "broken"),
            Err(crate::PrivacyError::RecognitionError(_))
        ));
    }

    #[test]
    fn todays_api_key_formats_are_secrets() {
        // Synthetic keys in each provider's shape, none real, joined from a prefix and a body when the test runs, so the
        // repository's own secret scan does not take the source for a leak.
        for (context, prefix, body) in [
            ("key ", "sk-", "proj-AbCdEfGh_ijklMNop-qrstUVwx12"),
            ("", "sk-", "ant-api03-AbCdEfGhIjKlMnOpQrSt"),
            ("My API key is ", "sk-", "live-4f3c9a7b2e1d8c6a5f4e"),
            ("stripe ", "sk_", "live_51HxAbCdEfGhIjKlMn"),
            ("", "github_", "pat_11ABCDEFG0123456789abcdefghij"),
            ("", "gh", "p_abcdefghijklmnopqrstuvwxyz0123456789"),
            ("slack ", "xox", "b-1234567890-abcdefghij"),
            ("", "AI", "zaSyA1b2C3d4E5f6G7h8I9j0K1l2M3n4O5p6Q"),
            // A labelled key with any value; a low-entropy one, so the repository scan stays quiet.
            ("", "api_", "key: abcabcabcabc"),
            ("", "Pass", "word = hunter2"),
            ("", "-----BEGIN RSA ", "PRIVATE KEY-----"),
            (
                "token ",
                "eyJ",
                "hbGciOiJIUzI1.eyJzdWIiOiIxMjM0.SflKxwRJSMeKKF2QT4",
            ),
        ] {
            let secret = format!("{prefix}{body}");
            let text = format!("{context}{secret}");
            let found = find_pattern_matches(&text).unwrap_or_default();
            assert!(
                found
                    .iter()
                    .any(|m| m.category == PatternCategory::Secret && m.matched_text == secret),
                "{text}: {found:?}"
            );
        }
    }

    #[test]
    fn ordinary_words_that_resemble_keys_are_not_secrets() {
        for text in [
            "Ask the desk about the risk-management plan",
            "The secret garden opens at noon",
            "Our token economy talk is on Friday",
            "sk-short",
        ] {
            let found = find_pattern_matches(text).unwrap_or_default();
            assert!(
                !found.iter().any(|m| m.category == PatternCategory::Secret),
                "{text}: {found:?}"
            );
        }
    }

    #[test]
    fn a_number_with_its_area_code_in_brackets_is_a_phone() {
        for (text, number) in [
            ("Call (312) 555-0147 today", "(312) 555-0147"),
            ("Phone: (020) 7946 0958", "(020) 7946 0958"),
            ("Mobile +1 (312) 555-0147", "(312) 555-0147"),
        ] {
            let found = find_pattern_matches(text).unwrap_or_default();
            assert!(
                found
                    .iter()
                    .any(|m| m.category == PatternCategory::Phone && m.matched_text == number),
                "{text}: {found:?}"
            );
        }
    }

    fn found(text: &str, category: PatternCategory) -> Vec<String> {
        find_pattern_matches(text)
            .unwrap_or_default()
            .into_iter()
            .filter(|m| m.category == category)
            .map(|m| m.matched_text)
            .collect()
    }

    #[test]
    fn an_iban_printed_in_groups_is_found_without_the_next_word() {
        for (text, iban) in [
            (
                "Supplier IBAN GB82 WEST 1234 5698 7654 32",
                "GB82 WEST 1234 5698 7654 32",
            ),
            (
                "IBAN: DE89 3704 0044 0532 0130 00 EUR",
                "DE89 3704 0044 0532 0130 00",
            ),
            ("Pay BE68 5390 0754 7034 TODAY", "BE68 5390 0754 7034"),
            ("Account GB33BUKB20201555555555", "GB33BUKB20201555555555"),
        ] {
            assert_eq!(found(text, PatternCategory::Iban), vec![iban], "{text}");
        }
        // A grouped number that fails the check is not an IBAN.
        assert_eq!(
            found("Ref GB82 WEST 1234 5698 7654 33", PatternCategory::Iban),
            Vec::<String>::new()
        );
    }

    #[test]
    fn a_social_security_number_with_spaces_is_found_and_impossible_ones_are_not() {
        assert_eq!(
            found("Employee SSN 078 05 1120", PatternCategory::UsSsn),
            vec!["078 05 1120"]
        );
        assert_eq!(
            found("SSN 123-45-6789", PatternCategory::UsSsn),
            vec!["123-45-6789"]
        );
        for impossible in ["000-12-3456", "666 12 3456", "123 00 4567", "123-45-0000"] {
            assert!(
                found(impossible, PatternCategory::UsSsn).is_empty(),
                "{impossible}"
            );
        }
        // Mixed separators and other groupings are not social security numbers.
        assert_eq!(
            found("Code 123-45 6789", PatternCategory::UsSsn),
            Vec::<String>::new()
        );
        assert_eq!(
            found("Meeting ID 845 2913 7766", PatternCategory::UsSsn),
            Vec::<String>::new()
        );
    }

    #[test]
    fn a_national_insurance_number_is_found_with_or_without_spaces() {
        assert_eq!(
            found(
                "National Insurance number QQ 12 34 56 C",
                PatternCategory::UkNino
            ),
            vec!["QQ 12 34 56 C"]
        );
        assert_eq!(
            found("NI number: AB123456C", PatternCategory::UkNino),
            vec!["AB123456C"]
        );
        // Prefixes that are never issued, a suffix past D and lower case are refused.
        for refused in [
            "GB123456A",
            "DA123456A",
            "AO123456A",
            "AB123456E",
            "ab123456c",
        ] {
            assert!(
                found(refused, PatternCategory::UkNino).is_empty(),
                "{refused}"
            );
        }
    }

    #[test]
    fn every_recognizer_pattern_compiles() {
        // Reaches every recognizer: a title, a context prefix, a known name, '@', '-', '8', digits and an address.
        let probe = "Customer: Jane Smith, Herr Alexander Müller, 8123456789 a@b.example 123-45-6789 \
                     password: x, 12 Main Street";
        assert!(detect_all_entities(probe).is_ok());
    }
}
