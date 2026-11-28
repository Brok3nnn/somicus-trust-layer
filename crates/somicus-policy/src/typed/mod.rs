//! Which values Somicus may type by itself (ADR-0020 and ADR-0038). Somicus types what the person
//! wrote, the address of a site the person named or asked for, its own search words within tight limits, and a text it
//! composed, never into a search, address, password or strict field and never with a number that neither the request,
//! the task's screens nor a checked calculation gave. The agent runtime (not part of this repository) applies the same
//! rules first, and may be stricter, never looser.

use crate::action::{ActionTarget, is_search_target};
use crate::fields::is_strict_target;
use crate::numbers::KnownNumbers;

/// Longest literal search or navigation value Somicus types by itself.
const MAX_TYPED_CHARS: usize = 200;
/// Longest composed draft, e-mail, message or form text Somicus types when the request asks to write.
const MAX_DRAFT_CHARS: usize = 2_000;
/// Shortest site name that may be matched against the request, so short names do not match by accident.
const MIN_SITE_NAME_CHARS: usize = 4;
/// Longest word ending (for example a Hungarian case suffix) allowed after a site name in the request.
const MAX_SITE_SUFFIX_CHARS: usize = 3;

fn looks_like_web_address(value: &str) -> bool {
    !value.contains(char::is_whitespace) && value.contains('.')
}

/// `text` without Privacy Shield placeholders such as `⟦PERSON_1⟧`, whose number only tells placeholders apart.
fn without_placeholders(text: &str) -> String {
    let mut kept = String::with_capacity(text.len());
    let mut inside = false;
    for character in text.chars() {
        match character {
            '⟦' => inside = true,
            '⟧' => {
                inside = false;
                kept.push(' ');
            }
            _ if !inside => kept.push(character),
            _ => {}
        }
    }
    kept
}

/// Whether the value has no control character other than a line break, and no line break unless `multiline`.
fn well_formed(value: &str, multiline: bool) -> bool {
    !value.is_empty()
        && !value
            .chars()
            .any(|character| character.is_control() && (character != '\n' || !multiline))
}

/// Whether Somicus may type `value` by itself for `request`: the value appears in the person's own request (up to
/// spacing, case and one corrected letter), or it is a bare web address (no path or query) whose site name the request
/// names, for example `youtube.com` for a request that mentions `YouTube`, or that the request asks for without
/// naming it (`IMPLIED_SITES`: `youtube.com` for a video). Anything else the person types themselves, unless
/// [`typed_draft_allowed`] lets Somicus compose it.
#[must_use]
pub fn typed_value_allowed(value: &str, request: &str) -> bool {
    let value = value.trim();
    if !well_formed(value, false) || value.chars().count() > MAX_TYPED_CHARS {
        return false;
    }
    let request = request.to_lowercase();
    let value = value.to_lowercase();
    if request.contains(&value) {
        return true;
    }
    request_contains_corrected_value(&value, &request)
        || site_name(&value).is_some_and(|name| request_names_site(&request, name))
        || request_implies_site(&request, &value)
}

/// Whether Somicus may type `value`, a text it composed itself, into `target` for `request` (ADR-0038):
/// the field is not a search, address, password or strict field (whose values must come from the person,
/// see [`typed_value_allowed`] and [`crate::fields`]), the text has at most `MAX_DRAFT_CHARS` (2,000) characters, may
/// span lines, is not a web address, and every number in it is in the request or in `known` (the numbers of the task's
/// screens and of checked calculations). Typing it never sends it: Return after it is classified like Return.
#[must_use]
pub fn typed_draft_allowed(
    value: &str,
    request: &str,
    target: &ActionTarget<'_>,
    known: &KnownNumbers,
) -> bool {
    let value = value.trim();
    well_formed(value, true)
        && value.chars().count() <= MAX_DRAFT_CHARS
        && !is_search_target(target)
        && target.role != "password_field"
        && !is_strict_target(target)
        && !looks_like_web_address(value)
        && numbers_known(value, request, known)
}

/// Whether every number of `value` is in `request` or in `known`. Privacy Shield placeholders are not numbers.
pub(crate) fn numbers_known(value: &str, request: &str, known: &KnownNumbers) -> bool {
    let mut allowed = KnownNumbers::from_text(&without_placeholders(request));
    allowed.extend(known);
    allowed.allow(&without_placeholders(value))
}

/// Sites a request asks for without naming them, by the beginnings of the words that ask: a video or a clip is on
/// `YouTube`. Only the site's own bare address may be typed for them, a constant of this table, never a page of it or
/// another host. The agent runtime uses the same table.
const IMPLIED_SITES: [(&str, &[&str]); 1] = [("youtube.com", &["video", "clip"])];

/// Whether `value` is the bare address of a site that `request` (in lower case) asks for without naming it.
fn request_implies_site(request: &str, value: &str) -> bool {
    let Some(host) = bare_host(value) else {
        return false;
    };
    IMPLIED_SITES.iter().any(|(site, asks)| {
        *site == host
            && request
                .split(|c: char| !c.is_alphanumeric())
                .any(|word| asks.iter().any(|ask| word.starts_with(ask)))
    })
}

/// Shortest value, counting letters and digits, that may differ from the request by one corrected letter.
const MIN_CORRECTED_CHARS: usize = 6;

/// The letters and digits of `text` in lower case; spaces, punctuation and case are not part of a value's meaning.
fn compact(text: &str) -> Vec<char> {
    text.chars()
        .filter(|character| character.is_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect()
}

/// Whether the request contains the value up to spacing, punctuation, case and one corrected letter: two adjacent
/// letters swapped, or one missing, extra or wrong letter, as in `Marta Vael` → `Marta Vale` or
/// `north wnid` → `Northwind`. Digits are never corrected, so a number or an amount cannot change. This accepts an
/// obvious typo without giving a model freedom to invent other text.
fn request_contains_corrected_value(value: &str, request: &str) -> bool {
    let value = compact(value);
    let length = value.len();
    if !(MIN_CORRECTED_CHARS..=MAX_TYPED_CHARS).contains(&length) {
        return false;
    }
    let request = compact(request);
    [length - 1, length, length + 1].into_iter().any(|width| {
        request
            .windows(width)
            .any(|written| one_letter_apart(&value, written))
    })
}

/// Whether `a` and `b` are equal or differ by one letter edit (see [`request_contains_corrected_value`]).
fn one_letter_apart(a: &[char], b: &[char]) -> bool {
    let same = a.iter().zip(b).take_while(|(x, y)| x == y).count();
    let (a, b) = (&a[same..], &b[same..]);
    match (a.split_first(), b.split_first()) {
        (None, None) => true,
        (None, Some((&extra, rest))) | (Some((&extra, rest)), None) => {
            rest.is_empty() && extra.is_alphabetic()
        }
        (Some((&x, a_rest)), Some((&y, b_rest))) => {
            let letters = x.is_alphabetic() && y.is_alphabetic();
            let wrong = letters && a_rest == b_rest;
            let swapped = letters
                && a_rest.first() == Some(&y)
                && b_rest.first() == Some(&x)
                && a_rest.get(1..) == b_rest.get(1..);
            let missing_in_a = y.is_alphabetic() && a == b_rest;
            let missing_in_b = x.is_alphabetic() && a_rest == b;
            wrong || swapped || missing_in_a || missing_in_b
        }
    }
}

/// The host of a bare web address without `www.` (`youtube.com` for `https://www.youtube.com/`), or `None` for anything
/// else, such as an address with a path or query.
fn bare_host(value: &str) -> Option<&str> {
    let rest = value
        .strip_prefix("https://")
        .or_else(|| value.strip_prefix("http://"))
        .unwrap_or(value);
    let host = rest.strip_suffix('/').unwrap_or(rest);
    let valid = host.contains('.')
        && host
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '-');
    valid.then(|| host.strip_prefix("www.").unwrap_or(host))
}

/// The site name of a bare web address (`youtube` for `https://www.youtube.com/`), or `None` for anything else. A
/// language or country subdomain of two letters, or `m` for mobile, is not the name: `hu.wikipedia.org` is `wikipedia`.
fn site_name(value: &str) -> Option<&str> {
    let host = bare_host(value)?;
    let labels: Vec<&str> = host.split('.').collect();
    let sub = labels.first().copied().unwrap_or_default();
    let named = if labels.len() >= 3 && (sub.len() == 2 || sub == "m") {
        labels.get(1).copied()
    } else {
        labels.first().copied()
    };
    named.filter(|name| !name.is_empty())
}

/// Whether a word of `request` is `name`, possibly followed by a short ending (`youtubers`). A name of at least six
/// letters may also lose or change its last letter before the ending (`youtubing`), and may carry one corrected letter
/// before that, as in the typo `youtbing`; shorter names must match in full so they do not match other words by
/// accident.
fn request_names_site(request: &str, name: &str) -> bool {
    let name: Vec<char> = name.chars().collect();
    if name.len() < MIN_SITE_NAME_CHARS {
        return false;
    }
    let corrected = name.len() >= MIN_CORRECTED_CHARS;
    let stem = if corrected {
        &name[..name.len() - 1]
    } else {
        &name[..]
    };
    let longest_ending = MAX_SITE_SUFFIX_CHARS + (name.len() - stem.len());
    request.split(|c: char| !c.is_alphanumeric()).any(|word| {
        let word: Vec<char> = word.chars().collect();
        begins_with_stem(&word, stem, longest_ending, corrected)
    })
}

/// Whether `word` begins with `stem`, or with `corrected` up to one corrected letter (see [`one_letter_apart`]) that
/// keeps the stem's last letter, and ends at most `longest_ending` letters later. Keeping the last letter stops a
/// shortened stem from matching another word, such as `amazing` for `amazon`.
fn begins_with_stem(word: &[char], stem: &[char], longest_ending: usize, corrected: bool) -> bool {
    let widths = if corrected {
        stem.len().saturating_sub(1)..=stem.len() + 1
    } else {
        stem.len()..=stem.len()
    };
    widths.into_iter().any(|width| {
        width <= word.len()
            && word.len() - width <= longest_ending
            && if corrected {
                word[..width].last() == stem.last() && one_letter_apart(&word[..width], stem)
            } else {
                word[..width] == *stem
            }
    })
}

#[cfg(test)]
mod tests;
