//! Pattern recognizers and checksum validators for structured identifiers.
//!
//! Implements Privacy Shield §4:
//! - Email, phone (DE/EN/HU), IBAN (mod-97), payment cards (Luhn),
//!   national IDs (US SSN, DE Steuer-ID, HU adóazonosító jel, HU TAJ),
//!   API secrets and access tokens.

use regex::Regex;
use std::sync::LazyLock;

use super::{compile_pattern, recognizer};
use crate::error::PrivacyError;

/// Structured identifier categories recognized by deterministic rules.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PatternCategory {
    /// Email address.
    Email,
    /// Phone number.
    Phone,
    /// Credit/debit payment card passing Luhn.
    PaymentCard,
    /// IBAN passing mod-97.
    Iban,
    /// US Social Security Number.
    UsSsn,
    /// German Steuer-ID (11 digits).
    DeSteuerId,
    /// Hungarian tax identification number (adóazonosító jel, 10 digits).
    HuTaxId,
    /// Hungarian social security number (TAJ szám, 9 digits).
    HuTaj,
    /// UK National Insurance number (two letters, six digits, a letter from A to D).
    UkNino,
    /// API secret, bearer token, or password.
    Secret,
    /// Postal address.
    Address,
}

static EMAIL_RE: LazyLock<Option<Regex>> =
    LazyLock::new(|| compile_pattern(r"(?i)\b[A-Za-z0-9._%+-]+@[A-Za-z0-9.-]+\.[A-Za-z]{2,}\b"));

/// An international number, or a national one that starts with its area code in brackets or with a trunk `0`. Only the
/// trunk branch starts at a word boundary: a bracket after a space is no word boundary, so `\b(` never matched
/// `(312) 555-0147` in running text.
static PHONE_RE: LazyLock<Option<Regex>> = LazyLock::new(|| {
    compile_pattern(
        r"(?:(?:\+|00)\d{1,3}[\s.-]?(?:\(\d{1,4}\)[\s.-]?)?\d{1,4}[\s.-]?\d{3,4}[\s.-]?\d{3,4}\b|(?:\(\d{2,4}\)[\s.-]?|\b0\d{1,4}[\s.-]?)\d{3,4}[\s.-]?\d{3,4}\b)",
    )
});

static CARD_RE: LazyLock<Option<Regex>> =
    LazyLock::new(|| compile_pattern(r"\b(?:\d[ -]?){13,19}\b"));

/// An IBAN in one block, or in the printed form: groups of four separated by single spaces, the last group shorter
/// ("GB82 WEST 1234 5698 7654 32").
static IBAN_RE: LazyLock<Option<Regex>> = LazyLock::new(|| {
    compile_pattern(
        r"\b[A-Z]{2}\d{2}(?:[A-Za-z0-9]{11,30}|(?: [A-Z0-9]{4}){2,7}(?: [A-Z0-9]{1,4})?)\b",
    )
});

/// A US social security number with hyphens or with spaces ("078 05 1120").
static US_SSN_RE: LazyLock<Option<Regex>> =
    LazyLock::new(|| compile_pattern(r"\b(?:\d{3}-\d{2}-\d{4}|\d{3} \d{2} \d{4})\b"));

/// A UK National Insurance number with or without spaces. Letters HMRC never uses in a prefix are refused (D, F, I, U
/// and V in either place, O second); Q is allowed, so that HMRC's example prefix QQ is replaced too.
static UK_NINO_RE: LazyLock<Option<Regex>> = LazyLock::new(|| {
    compile_pattern(r"\b[A-CEGHJ-TW-Z][A-CEGHJ-NP-TW-Z] ?\d{2} ?\d{2} ?\d{2} ?[A-D]\b")
});

/// The National Insurance prefixes that are never issued.
const UK_NINO_UNUSED_PREFIXES: [&str; 7] = ["BG", "GB", "KN", "NK", "NT", "TN", "ZZ"];

static DE_STEUER_ID_RE: LazyLock<Option<Regex>> =
    LazyLock::new(|| compile_pattern(r"\b[1-9]\d{2}[\s]?\d{3}[\s]?\d{3}[\s]?\d{2}\b"));

static HU_TAX_ID_RE: LazyLock<Option<Regex>> = LazyLock::new(|| compile_pattern(r"\b8\d{9}\b"));

static HU_TAJ_RE: LazyLock<Option<Regex>> =
    LazyLock::new(|| compile_pattern(r"\b\d{3}[\s-]?\d{3}[\s-]?\d{3}\b"));

/// Secrets and credentials: bearer tokens; today's API key formats, whose bodies carry hyphens and underscores
/// (`sk-proj-…`, `sk-ant-…`, Stripe's `sk_live_…`, GitHub's `ghp_…` and `github_pat_…`, Slack's `xox…`, Google's
/// `AIza…`, AWS's `AKIA…`); JSON Web Tokens; a labelled password, API key, secret or token; and a private key's armour.
/// Found on 2026-11-08 by the memory tests of ADR-0043: `sk-` with a hyphen in its body, such as a model provider's
/// project key, was not a secret before.
static SECRET_RE: LazyLock<Option<Regex>> = LazyLock::new(|| {
    compile_pattern(concat!(
        r"(?i)(?:\b(?:",
        r"Bearer\s+[A-Za-z0-9_\-\.]{16,}",
        r"|sk-[A-Za-z0-9][A-Za-z0-9_\-]{19,}",
        r"|[sr]k_(?:live|test)_[A-Za-z0-9]{16,}",
        r"|gh[pousr]_[A-Za-z0-9]{36}",
        r"|github_pat_[A-Za-z0-9_]{22,}",
        r"|xox[abposr]-[A-Za-z0-9\-]{10,}",
        r"|AIza[0-9A-Za-z_\-]{35}",
        r"|AKIA[0-9A-Z]{16}",
        r"|eyJ[A-Za-z0-9_\-]{10,}\.[A-Za-z0-9_\-]{10,}\.[A-Za-z0-9_\-]{10,}",
        r"|(?:password|passwd|passcode|api[_\- ]?key|secret|access[_\- ]?token)\s*[:=]\s*\S+",
        r")\b|-----BEGIN [A-Z ]*PRIVATE KEY-----)",
    ))
});

static ADDRESS_RE: LazyLock<Option<Regex>> = LazyLock::new(|| {
    compile_pattern(
        r"(?i)\b\d{1,5}\s+[A-Za-z0-9\s.,-]+(?:Street|St\b|Avenue|Ave\b|Road|Rd\b|Boulevard|Blvd\b|Lane|Ln\b|Drive|Dr\b|Weg\b|Straße|Str\.\b|Gasse\b|utca\b|út\b|tér\b)\b",
    )
});

/// Validates a payment card candidate using the standard Luhn algorithm (mod 10).
#[must_use]
pub fn passes_luhn_checksum(candidate: &str) -> bool {
    let digits: Vec<u32> = candidate
        .chars()
        .filter(char::is_ascii_digit)
        .filter_map(|c| c.to_digit(10))
        .collect();

    if digits.len() < 13 || digits.len() > 19 {
        return false;
    }

    let mut sum = 0;
    let len = digits.len();
    for (i, &digit) in digits.iter().enumerate() {
        let is_even_from_right = (len - 1 - i) % 2 == 1;
        if is_even_from_right {
            let doubled = digit * 2;
            sum += if doubled > 9 { doubled - 9 } else { doubled };
        } else {
            sum += digit;
        }
    }

    sum % 10 == 0
}

/// Validates an IBAN candidate using the ISO 7064 mod-97 check.
#[must_use]
pub fn passes_iban_checksum(candidate: &str) -> bool {
    let cleaned: String = candidate
        .chars()
        .filter(char::is_ascii_alphanumeric)
        .collect();
    if cleaned.len() < 15 || cleaned.len() > 34 {
        return false;
    }

    let rearranged = format!("{}{}", &cleaned[4..], &cleaned[..4]);
    let mut remainder = 0u64;

    for ch in rearranged.chars() {
        if ch.is_ascii_digit() {
            let digit = u64::from(ch.to_digit(10).unwrap_or(0));
            remainder = (remainder * 10 + digit) % 97;
        } else if ch.is_ascii_uppercase() {
            let val = u64::from(ch as u8 - b'A' + 10);
            remainder = (remainder * 100 + val) % 97;
        } else {
            return false;
        }
    }

    remainder == 1
}

/// How much of an IBAN `candidate` passes the mod-97 check: all of it, or, for the grouped form, the longest part that
/// ends at a group (the pattern may take a following word of four capitals for one more group). `None` when no part
/// passes.
fn checked_iban_length(candidate: &str) -> Option<usize> {
    if passes_iban_checksum(candidate) {
        return Some(candidate.len());
    }
    candidate
        .match_indices(' ')
        .map(|(space, _)| space)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .find(|space| passes_iban_checksum(&candidate[..*space]))
}

/// Whether `candidate` (nine digits with separators) can be a US social security or taxpayer number: no part is all
/// zeros and the area is not 666.
fn is_possible_us_ssn(candidate: &str) -> bool {
    let digits: String = candidate.chars().filter(char::is_ascii_digit).collect();
    digits.len() == 9
        && &digits[..3] != "000"
        && &digits[..3] != "666"
        && &digits[3..5] != "00"
        && &digits[5..] != "0000"
}

/// Validates a Hungarian tax identification number (adóazonosító jel, 10 digits starting with 8).
#[must_use]
pub fn passes_hu_tax_id_checksum(candidate: &str) -> bool {
    let digits: Vec<u32> = candidate
        .chars()
        .filter(char::is_ascii_digit)
        .filter_map(|c| c.to_digit(10))
        .collect();

    if digits.len() != 10 || digits[0] != 8 {
        return false;
    }

    let mut sum = 0u32;
    for (i, &d) in digits.iter().take(9).enumerate() {
        sum += d * (u32::try_from(i).unwrap_or(0) + 1);
    }

    let check_digit = sum % 11;
    check_digit == digits[9]
}

/// Validates a Hungarian social security number (TAJ szám, 9 digits).
#[must_use]
pub fn passes_hu_taj_checksum(candidate: &str) -> bool {
    let digits: Vec<u32> = candidate
        .chars()
        .filter(char::is_ascii_digit)
        .filter_map(|c| c.to_digit(10))
        .collect();

    if digits.len() != 9 {
        return false;
    }

    let weights = [3u32, 7, 3, 7, 3, 7, 3, 7];
    let mut sum = 0u32;
    for (i, &weight) in weights.iter().enumerate() {
        sum += digits[i] * weight;
    }

    let check_digit = sum % 10;
    check_digit == digits[8]
}

/// Validates a German Steuer-ID (11 digits, first digit non-zero).
#[must_use]
pub fn passes_de_steuer_id_checksum(candidate: &str) -> bool {
    let digits: Vec<u32> = candidate
        .chars()
        .filter(char::is_ascii_digit)
        .filter_map(|c| c.to_digit(10))
        .collect();

    if digits.len() != 11 || digits[0] == 0 {
        return false;
    }

    // Check digit algorithm: ISO 7064 Mod 11, 10
    let mut remainder = 10u32;
    for &digit in digits.iter().take(10) {
        let sum = (digit + remainder) % 10;
        let factor = if sum == 0 { 10 } else { sum };
        remainder = (factor * 2) % 11;
    }

    let check_digit = (11 - remainder) % 10;
    check_digit == digits[10]
}

/// Represents a pattern match found in text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PatternMatch {
    /// Category of pattern matched.
    pub category: PatternCategory,
    /// Substring matched.
    pub matched_text: String,
    /// Byte offset start in text.
    pub start: usize,
    /// Byte offset end in text.
    pub end: usize,
}

fn match_secrets(text: &str, matches: &mut Vec<PatternMatch>) -> Result<(), PrivacyError> {
    for m in recognizer(&SECRET_RE, "secret")?.find_iter(text) {
        matches.push(PatternMatch {
            category: PatternCategory::Secret,
            matched_text: m.as_str().to_string(),
            start: m.start(),
            end: m.end(),
        });
    }
    Ok(())
}

fn match_emails(text: &str, matches: &mut Vec<PatternMatch>) -> Result<(), PrivacyError> {
    for m in recognizer(&EMAIL_RE, "email")?.find_iter(text) {
        matches.push(PatternMatch {
            category: PatternCategory::Email,
            matched_text: m.as_str().to_string(),
            start: m.start(),
            end: m.end(),
        });
    }
    Ok(())
}

fn match_national_ids(text: &str, matches: &mut Vec<PatternMatch>) -> Result<(), PrivacyError> {
    // US SSN, with hyphens or spaces
    for m in recognizer(&US_SSN_RE, "US social security number")?.find_iter(text) {
        if is_possible_us_ssn(m.as_str()) {
            matches.push(PatternMatch {
                category: PatternCategory::UsSsn,
                matched_text: m.as_str().to_string(),
                start: m.start(),
                end: m.end(),
            });
        }
    }

    // UK National Insurance number
    for m in recognizer(&UK_NINO_RE, "UK National Insurance number")?.find_iter(text) {
        if !UK_NINO_UNUSED_PREFIXES.contains(&&m.as_str()[..2]) {
            matches.push(PatternMatch {
                category: PatternCategory::UkNino,
                matched_text: m.as_str().to_string(),
                start: m.start(),
                end: m.end(),
            });
        }
    }

    // Hungarian Tax ID (adóazonosító jel)
    if text.contains('8') {
        for m in recognizer(&HU_TAX_ID_RE, "Hungarian tax number")?.find_iter(text) {
            if passes_hu_tax_id_checksum(m.as_str()) {
                matches.push(PatternMatch {
                    category: PatternCategory::HuTaxId,
                    matched_text: m.as_str().to_string(),
                    start: m.start(),
                    end: m.end(),
                });
            }
        }
    }

    // Hungarian TAJ
    for m in recognizer(&HU_TAJ_RE, "Hungarian TAJ number")?.find_iter(text) {
        if passes_hu_taj_checksum(m.as_str()) {
            matches.push(PatternMatch {
                category: PatternCategory::HuTaj,
                matched_text: m.as_str().to_string(),
                start: m.start(),
                end: m.end(),
            });
        }
    }

    // German Steuer-ID
    for m in recognizer(&DE_STEUER_ID_RE, "German tax ID")?.find_iter(text) {
        if passes_de_steuer_id_checksum(m.as_str()) {
            matches.push(PatternMatch {
                category: PatternCategory::DeSteuerId,
                matched_text: m.as_str().to_string(),
                start: m.start(),
                end: m.end(),
            });
        }
    }
    Ok(())
}

fn match_financial_patterns(
    text: &str,
    matches: &mut Vec<PatternMatch>,
) -> Result<(), PrivacyError> {
    // Payment Cards (Luhn)
    for m in recognizer(&CARD_RE, "payment card")?.find_iter(text) {
        let candidate = m.as_str();
        if passes_luhn_checksum(candidate) {
            matches.push(PatternMatch {
                category: PatternCategory::PaymentCard,
                matched_text: candidate.to_string(),
                start: m.start(),
                end: m.end(),
            });
        }
    }

    // IBAN (mod-97), in one block or in groups
    for m in recognizer(&IBAN_RE, "IBAN")?.find_iter(text) {
        if let Some(length) = checked_iban_length(m.as_str()) {
            matches.push(PatternMatch {
                category: PatternCategory::Iban,
                matched_text: m.as_str()[..length].to_string(),
                start: m.start(),
                end: m.start() + length,
            });
        }
    }

    // Phone (with digit count filter)
    for m in recognizer(&PHONE_RE, "phone")?.find_iter(text) {
        let candidate_phone = m.as_str();
        let digit_count = candidate_phone.chars().filter(char::is_ascii_digit).count();
        if digit_count >= 7 {
            let already_covered = matches
                .iter()
                .any(|existing| existing.start <= m.start() && m.end() <= existing.end);
            if !already_covered {
                matches.push(PatternMatch {
                    category: PatternCategory::Phone,
                    matched_text: candidate_phone.to_string(),
                    start: m.start(),
                    end: m.end(),
                });
            }
        }
    }
    Ok(())
}

fn match_addresses(text: &str, matches: &mut Vec<PatternMatch>) -> Result<(), PrivacyError> {
    for m in recognizer(&ADDRESS_RE, "postal address")?.find_iter(text) {
        let already_covered = matches
            .iter()
            .any(|existing| existing.start <= m.start() && m.end() <= existing.end);
        if !already_covered {
            matches.push(PatternMatch {
                category: PatternCategory::Address,
                matched_text: m.as_str().to_string(),
                start: m.start(),
                end: m.end(),
            });
        }
    }
    Ok(())
}

/// Finds all structured pattern matches in the input text.
///
/// # Errors
/// Returns [`PrivacyError::RecognitionError`] when a recognizer's pattern does not compile: a recognizer that cannot
/// run must not look like one that found nothing.
pub fn find_pattern_matches(text: &str) -> Result<Vec<PatternMatch>, PrivacyError> {
    let mut matches = Vec::new();

    if text.len() < 3 {
        return Ok(matches);
    }

    match_secrets(text, &mut matches)?;

    if text.contains('@') {
        match_emails(text, &mut matches)?;
    }

    if text.bytes().any(|b| b.is_ascii_digit()) {
        match_national_ids(text, &mut matches)?;
        match_financial_patterns(text, &mut matches)?;
        match_addresses(text, &mut matches)?;
    }

    Ok(matches)
}
