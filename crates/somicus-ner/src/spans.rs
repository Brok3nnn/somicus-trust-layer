//! From labelled word pieces to the byte ranges the shield replaces: whole words, names joined across hyphens and
//! apostrophes, and nothing that reads as interface wording rather than a name.

use std::ops::Range;

use crate::tokenizer::Piece;

/// Lower-case words that belong inside a name ("Ludwig van Beethoven", "Ana de la Cruz"). Any other lower-case word in
/// a span ("Mark as Read") says the span is a phrase, not a name.
const NAME_PARTICLES: &[&str] = &[
    "al", "bin", "binti", "da", "das", "de", "del", "della", "den", "der", "di", "do", "dos", "du",
    "el", "la", "le", "ten", "ter", "van", "von", "y", "zu",
];

/// Words of application interfaces that are not names, so a span holding one ("Max Width", "Victor Mode", "Account
/// Settings") is a label the model took for a name. Words that are also common names or surnames ("Bill", "Mark",
/// "Grace", "Will") are deliberately absent.
const INTERFACE_WORDS: &[&str] = &[
    "account",
    "accounts",
    "admin",
    "all",
    "archive",
    "attachment",
    "attachments",
    "calendar",
    "cells",
    "chart",
    "comment",
    "comments",
    "dashboard",
    "delete",
    "discussion",
    "document",
    "documents",
    "download",
    "draft",
    "drafts",
    "edit",
    "error",
    "folder",
    "folders",
    "font",
    "format",
    "formatting",
    "height",
    "inbox",
    "layout",
    "mode",
    "notes",
    "notification",
    "notifications",
    "options",
    "outbox",
    "playlist",
    "preferences",
    "preview",
    "print",
    "profile",
    "read",
    "scheduling",
    "search",
    "settings",
    "setup",
    "sheet",
    "slide",
    "slides",
    "spam",
    "table",
    "template",
    "theme",
    "toolbar",
    "trash",
    "unread",
    "upload",
    "view",
    "warning",
    "width",
    "window",
    "workspace",
    "zoom",
];

/// The byte ranges of the person names in `text`: a word is a name when any of its pieces is labelled a person (a part
/// of a name left out would still identify someone), and names separated only by spaces, hyphens, apostrophes or full
/// stops become one range ("Mary-Jane O'Neil", "J. Smith").
pub(crate) fn name_ranges(text: &str, pieces: &[Piece], person: &[bool]) -> Vec<Range<usize>> {
    let mut words: Vec<(usize, Range<usize>, bool)> = Vec::new();
    for (piece, is_person) in pieces.iter().zip(person) {
        match words.last_mut() {
            Some((word, range, flag)) if *word == piece.word => {
                range.end = piece.end;
                *flag |= *is_person;
            }
            _ => words.push((piece.word, piece.start..piece.end, *is_person)),
        }
    }
    let names = words
        .into_iter()
        .filter(|(_, _, is_person)| *is_person)
        .map(|(_, range, _)| range)
        .collect();
    joined(text, names)
}

/// `ranges` sorted, with those that overlap or are separated only by joining characters merged.
pub(crate) fn joined(text: &str, mut ranges: Vec<Range<usize>>) -> Vec<Range<usize>> {
    ranges.sort_by_key(|range| (range.start, range.end));
    let joins = |gap: &str| {
        gap.chars()
            .all(|character| matches!(character, ' ' | '-' | '\'' | '\u{2019}' | '.'))
    };
    let mut merged: Vec<Range<usize>> = Vec::with_capacity(ranges.len());
    for range in ranges {
        match merged.last_mut() {
            Some(last) if range.start <= last.end => last.end = last.end.max(range.end),
            Some(last) if text.get(last.end..range.start).is_some_and(joins) => {
                last.end = range.end;
            }
            _ => merged.push(range),
        }
    }
    merged
}

/// Whether the span `text[range]` can be a name: it holds no interface word and no lower-case word other than a name
/// particle.
pub(crate) fn plausible(text: &str, range: &Range<usize>) -> bool {
    let Some(span) = text.get(range.clone()) else {
        return false;
    };
    span.split(|character: char| !character.is_alphabetic())
        .filter(|word| !word.is_empty())
        .all(|word| {
            let lower = word.to_lowercase();
            let starts_small = word.chars().next().is_some_and(char::is_lowercase);
            !INTERFACE_WORDS.contains(&lower.as_str())
                && (!starts_small || NAME_PARTICLES.contains(&word))
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn piece(word: usize, start: usize, end: usize) -> Piece {
        Piece {
            id: 0,
            start,
            end,
            word,
        }
    }

    #[test]
    fn a_word_is_a_name_when_any_of_its_pieces_is() {
        let text = "From Okonkwo today";
        // "Okonkwo" in three pieces, only the middle one labelled.
        let pieces = [
            piece(0, 0, 4),
            piece(1, 5, 7),
            piece(1, 7, 10),
            piece(1, 10, 12),
            piece(2, 13, 18),
        ];
        let person = [false, false, true, false, false];
        assert_eq!(name_ranges(text, &pieces, &person), vec![5..12]);
    }

    #[test]
    fn names_join_across_hyphens_apostrophes_and_spaces_but_not_commas() {
        let text = "Mary-Jane O'Neil, Bo Kim";
        assert_eq!(
            joined(text, vec![18..20, 0..4, 5..9, 10..11, 12..16, 21..24]),
            vec![0..16, 18..24]
        );
        // Overlapping ranges, as from two readings of a text, become one.
        assert_eq!(joined("Ann Lee", vec![0..3, 0..7]), vec![0..7]);
    }

    #[test]
    fn interface_wording_is_not_a_name() {
        let plausible_in = |text: &str| plausible(text, &(0..text.len()));
        assert!(plausible_in("Olivia Bennett"));
        assert!(plausible_in("Ludwig van Beethoven"));
        assert!(plausible_in("Mark Stone"));
        assert!(!plausible_in("Mark as Read"));
        assert!(!plausible_in("Max Width"));
        assert!(!plausible_in("Account Settings"));
    }
}
