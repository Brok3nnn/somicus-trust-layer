//! Fields with rules of their own (ADR-0020 and ADR-0038). A strict field (payment, card, bank account,
//! amount, tax, official identity) takes only the person's own words. A search or address field takes Somicus's own
//! search words, but only short ones that cannot carry data out: no unseen number, no Privacy Shield placeholder, no
//! e-mail address, no web address (a site's address is [`crate::typed_value_allowed`]'s to allow).

use crate::action::{ActionTarget, is_search_target};
use crate::numbers::KnownNumbers;
use crate::typed::numbers_known;

/// Longest search Somicus words by itself, in characters and in words.
const MAX_SEARCH_CHARS: usize = 80;
const MAX_SEARCH_WORDS: usize = 8;

/// Beginnings of the label words of strict fields (EN, HU, DE).
const STRICT_STEMS: &[&str] = &[
    "card",
    "kártya",
    "karte",
    "kreditkart",
    "iban",
    "bic",
    "swift",
    "cvc",
    "cvv",
    "expiry",
    "lejárat",
    "ablauf",
    "amount",
    "összeg",
    "betrag",
    "tax",
    "adó",
    "steuer",
    "passport",
    "útlevél",
    "reisepass",
    "ausweis",
    "személyi",
    "taj",
    "ssn",
    "routing",
    "számlaszám",
    "bankszámla",
    "kontonummer",
    "bankkonto",
    "payment",
    "fizetés",
    "zahlung",
    "überweis",
    "átutal",
];
/// Phrases of strict field labels.
const STRICT_PHRASES: &[&str] = &["account number", "sort code", "social security"];

/// Whether `target` is a strict field: its label names a payment, card, bank account, amount, tax or official identity.
#[must_use]
pub fn is_strict_target(target: &ActionTarget<'_>) -> bool {
    let label = target.label.to_lowercase();
    STRICT_PHRASES.iter().any(|phrase| label.contains(phrase))
        || label
            .split(|c: char| !c.is_alphanumeric())
            .any(|word| STRICT_STEMS.iter().any(|stem| word.starts_with(stem)))
}

/// Whether Somicus may type `value`, search words of its own, into the search or address field `target` for
/// `request`: at most [`MAX_SEARCH_CHARS`] characters and [`MAX_SEARCH_WORDS`] words on one line, no web or e-mail
/// address, no placeholder, and no number that neither the request nor `known` has.
#[must_use]
pub fn search_words_allowed(
    value: &str,
    request: &str,
    target: &ActionTarget<'_>,
    known: &KnownNumbers,
) -> bool {
    let value = value.trim();
    let address = !value.contains(char::is_whitespace) && value.contains('.');
    is_search_target(target)
        && !value.is_empty()
        && value.chars().count() <= MAX_SEARCH_CHARS
        && value.split_whitespace().count() <= MAX_SEARCH_WORDS
        && !value.chars().any(char::is_control)
        && !value.contains(['⟦', '⟧', '@'])
        && !value.contains("://")
        && !address
        && numbers_known(value, request, known)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn payment_bank_tax_and_identity_fields_are_strict() {
        for label in [
            "Kártyaszám",
            "Card number",
            "IBAN",
            "Összeg (Ft)",
            "Adószám",
            "Account number",
            "Betrag",
        ] {
            assert!(
                is_strict_target(&ActionTarget::new(label, "text_field")),
                "{label}"
            );
        }
        for label in ["Cím", "Tárgy", "Üzenet törzse", "Kiadó", "Tartalom", "B2"] {
            assert!(
                !is_strict_target(&ActionTarget::new(label, "text_field")),
                "{label}"
            );
        }
    }

    #[test]
    fn own_search_words_are_short_and_carry_no_data_out() {
        let search = ActionTarget::new("Keresés", "search_field");
        let none = KnownNumbers::default();
        let request = "készíts egy ppt-t a kutyákról";
        assert!(search_words_allowed("kutya", request, &search, &none));
        assert!(search_words_allowed(
            "golden retriever kölyök",
            request,
            &search,
            &none
        ));
        assert!(!search_words_allowed(
            "kutya",
            request,
            &ActionTarget::new("Tartalom", "text_area"),
            &none
        ));
        assert!(!search_words_allowed("kutya 1990", request, &search, &none));
        assert!(!search_words_allowed(
            "anna@pelda.hu",
            request,
            &search,
            &none
        ));
        assert!(!search_words_allowed(
            "⟦PERSON_1⟧ address",
            request,
            &search,
            &none
        ));
        assert!(!search_words_allowed("evil.com", request, &search, &none));
        assert!(!search_words_allowed(
            &"szó ".repeat(9),
            request,
            &search,
            &none
        ));
        let seen = KnownNumbers::from_text("1990");
        assert!(search_words_allowed("kutya 1990", request, &search, &seen));
    }
}
