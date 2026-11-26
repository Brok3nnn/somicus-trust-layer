//! The model's own tokenizer: BERT's cased `WordPiece`, as `tokenizer.json` describes it (`BertNormalizer` without
//! lower-casing or accent stripping, `BertPreTokenizer`, `WordPiece` with `##` continuations and words of at most 100
//! characters), with each word piece's place in the original text.
//!
//! It is written here rather than taken from the `tokenizers` crate, whose dependency tree is about 25 crates (proc
//! macros, a parser combinator library, a second regex engine) for the few hundred lines this model needs. Its output
//! was compared with `tokenizers` 0.23.2 on every R-suite sample (ADR-0042). One difference is deliberate: special
//! tokens written in screen text (`[CLS]`, `[MASK]`) are read as ordinary text, so a screen cannot steer the model with
//! them.

use std::collections::HashMap;
use std::sync::LazyLock;

use regex::Regex;
use serde::Deserialize;

use crate::error::NerError;

/// The id of `[CLS]`, which starts every sequence.
pub(crate) const CLS: &str = "[CLS]";
/// The id of `[SEP]`, which ends every sequence.
pub(crate) const SEP: &str = "[SEP]";
const UNKNOWN: &str = "[UNK]";
const CONTINUATION: &str = "##";
const MAX_WORD_CHARACTERS: usize = 100;

/// A word piece of a text: its vocabulary id and the bytes of the text it covers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Piece {
    /// The vocabulary id.
    pub id: u32,
    /// Where it starts in the text, in bytes.
    pub start: usize,
    /// Where it ends in the text, in bytes.
    pub end: usize,
    /// Which word of the text it belongs to, counted from 0: the pieces of one word share it.
    pub word: usize,
}

/// The parts of `tokenizer.json` this tokenizer reads; everything else must match what it implements.
#[derive(Deserialize)]
struct TokenizerFile {
    normalizer: Normalizer,
    pre_tokenizer: PreTokenizer,
    model: Model,
}

#[derive(Deserialize)]
struct Normalizer {
    #[serde(rename = "type")]
    kind: String,
    clean_text: bool,
    handle_chinese_chars: bool,
    strip_accents: Option<bool>,
    lowercase: bool,
}

#[derive(Deserialize)]
struct PreTokenizer {
    #[serde(rename = "type")]
    kind: String,
}

#[derive(Deserialize)]
struct Model {
    #[serde(rename = "type")]
    kind: String,
    unk_token: String,
    continuing_subword_prefix: String,
    max_input_chars_per_word: usize,
    vocab: HashMap<String, u32>,
}

/// BERT's cased `WordPiece` tokenizer.
pub(crate) struct WordPiece {
    vocabulary: HashMap<String, u32>,
    unknown: u32,
    cls: u32,
    sep: u32,
}

/// Unicode punctuation (`P*`), which BERT splits off as words of their own.
static PUNCTUATION: LazyLock<Option<Regex>> = LazyLock::new(|| Regex::new(r"^\p{P}$").ok());
/// Unicode "other" characters (`C*`: controls, formats, unassigned, private use), which BERT's normalizer removes.
static OTHER: LazyLock<Option<Regex>> = LazyLock::new(|| Regex::new(r"^\p{C}$").ok());

fn in_class(class: &LazyLock<Option<Regex>>, character: char) -> bool {
    let mut buffer = [0u8; 4];
    LazyLock::force(class)
        .as_ref()
        .is_some_and(|regex| regex.is_match(character.encode_utf8(&mut buffer)))
}

/// Whether BERT treats `character` as whitespace: tab, line feed, carriage return and Unicode white space.
fn is_whitespace(character: char) -> bool {
    character.is_whitespace()
}

/// Whether BERT's normalizer removes `character`: NUL, the replacement character and Unicode "other" characters except
/// tab, line feed and carriage return.
fn is_removed(character: char) -> bool {
    match character {
        '\t' | '\n' | '\r' => false,
        '\0' | '\u{fffd}' => true,
        _ if character.is_ascii() => character.is_ascii_control(),
        _ => in_class(&OTHER, character),
    }
}

/// Whether BERT splits `character` off as punctuation: ASCII punctuation (which includes `$`, `+`, `<`, `^` and `|`)
/// or Unicode punctuation.
fn is_punctuation(character: char) -> bool {
    if character.is_ascii() {
        character.is_ascii_punctuation()
    } else {
        in_class(&PUNCTUATION, character)
    }
}

/// Whether `character` is a CJK ideograph, which BERT splits off as a word of its own.
fn is_cjk_ideograph(character: char) -> bool {
    matches!(
        u32::from(character),
        0x4E00..=0x9FFF
            | 0x3400..=0x4DBF
            | 0x2_0000..=0x2_A6DF
            | 0x2_A700..=0x2_B73F
            | 0x2_B740..=0x2_B81F
            | 0x2_B920..=0x2_CEAF
            | 0xF900..=0xFAFF
            | 0x2_F800..=0x2_FA1F
    )
}

/// The characters of one word, each with its place in the original text.
#[derive(Default)]
struct Word {
    text: String,
    /// For each character of `text`: where it starts in `text`, and where it starts and ends in the original text.
    places: Vec<(usize, usize, usize)>,
}

impl Word {
    fn push(&mut self, character: char, start: usize) {
        self.places
            .push((self.text.len(), start, start + character.len_utf8()));
        self.text.push(character);
    }

    /// The original text's bytes that `text[start..end]` came from.
    fn original(&self, start: usize, end: usize) -> Option<(usize, usize)> {
        let first = self.places.iter().find(|place| place.0 == start)?;
        let last = self.places.iter().rev().find(|place| place.0 < end)?;
        Some((first.1, last.2))
    }
}

/// Splits `text` into BERT's words: whitespace separates words, punctuation and CJK ideographs are words of their own,
/// and the characters the normalizer removes are dropped without separating anything.
fn words(text: &str) -> Vec<Word> {
    let mut words = Vec::new();
    let mut current = Word::default();
    for (start, character) in text.char_indices() {
        if is_removed(character) {
            continue;
        }
        if is_whitespace(character) {
            if !current.text.is_empty() {
                words.push(std::mem::take(&mut current));
            }
        } else if is_punctuation(character) || is_cjk_ideograph(character) {
            if !current.text.is_empty() {
                words.push(std::mem::take(&mut current));
            }
            let mut alone = Word::default();
            alone.push(character, start);
            words.push(alone);
        } else {
            current.push(character, start);
        }
    }
    if !current.text.is_empty() {
        words.push(current);
    }
    words
}

impl WordPiece {
    /// Reads the tokenizer from the bytes of `tokenizer.json`, refusing one that is not BERT's cased `WordPiece`.
    ///
    /// # Errors
    /// Returns [`NerError::Tokenizer`] when the file is not JSON, describes another normalizer, pre-tokenizer or model,
    /// or lacks a special token.
    pub(crate) fn from_json(bytes: &[u8]) -> Result<Self, NerError> {
        let file: TokenizerFile = serde_json::from_slice(bytes)
            .map_err(|_| NerError::Tokenizer("not a tokenizer.json file"))?;
        let normalizer = &file.normalizer;
        if normalizer.kind != "BertNormalizer"
            || !normalizer.clean_text
            || !normalizer.handle_chinese_chars
            || normalizer.strip_accents.unwrap_or(normalizer.lowercase)
            || normalizer.lowercase
        {
            return Err(NerError::Tokenizer(
                "the normalizer is not BERT's cased one",
            ));
        }
        if file.pre_tokenizer.kind != "BertPreTokenizer" {
            return Err(NerError::Tokenizer("the pre-tokenizer is not BERT's"));
        }
        let model = file.model;
        if model.kind != "WordPiece"
            || model.unk_token != UNKNOWN
            || model.continuing_subword_prefix != CONTINUATION
            || model.max_input_chars_per_word != MAX_WORD_CHARACTERS
        {
            return Err(NerError::Tokenizer("the model is not BERT's WordPiece"));
        }
        let id = |token: &str| {
            model
                .vocab
                .get(token)
                .copied()
                .ok_or(NerError::Tokenizer("a special token is missing"))
        };
        Ok(Self {
            unknown: id(UNKNOWN)?,
            cls: id(CLS)?,
            sep: id(SEP)?,
            vocabulary: model.vocab,
        })
    }

    /// The id of `[CLS]`.
    pub(crate) const fn cls(&self) -> u32 {
        self.cls
    }

    /// The id of `[SEP]`.
    pub(crate) const fn sep(&self) -> u32 {
        self.sep
    }

    /// How many entries the vocabulary has.
    pub(crate) fn vocabulary_size(&self) -> usize {
        self.vocabulary.len()
    }

    /// The word pieces of `text`, without `[CLS]` and `[SEP]`.
    pub(crate) fn pieces(&self, text: &str) -> Vec<Piece> {
        let mut pieces = Vec::new();
        for (index, word) in words(text).iter().enumerate() {
            self.word_pieces(word, index, &mut pieces);
        }
        pieces
    }

    /// The word pieces of one word: the longest vocabulary entries from its start, continuations marked `##`, or one
    /// `[UNK]` for the whole word when it is too long or some part of it is in no entry.
    fn word_pieces(&self, word: &Word, index: usize, pieces: &mut Vec<Piece>) {
        let unknown = || {
            let start = word.places.first().map_or(0, |place| place.1);
            let end = word.places.last().map_or(start, |place| place.2);
            Piece {
                id: self.unknown,
                start,
                end,
                word: index,
            }
        };
        if word.places.len() > MAX_WORD_CHARACTERS {
            pieces.push(unknown());
            return;
        }
        let text = word.text.as_str();
        let mut found = Vec::new();
        let mut candidate = String::with_capacity(text.len() + CONTINUATION.len());
        let mut start = 0;
        while start < text.len() {
            let mut end = text.len();
            let mut matched = None;
            while start < end {
                candidate.clear();
                if start > 0 {
                    candidate.push_str(CONTINUATION);
                }
                candidate.push_str(&text[start..end]);
                if let Some(id) = self.vocabulary.get(&candidate) {
                    matched = Some(*id);
                    break;
                }
                end -= text[start..end].chars().last().map_or(1, char::len_utf8);
            }
            let Some(id) = matched else {
                pieces.push(unknown());
                return;
            };
            let Some((original_start, original_end)) = word.original(start, end) else {
                pieces.push(unknown());
                return;
            };
            found.push(Piece {
                id,
                start: original_start,
                end: original_end,
                word: index,
            });
            start = end;
        }
        pieces.extend(found);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A tokenizer with a tiny vocabulary, in the shape of the real `tokenizer.json`.
    fn tiny() -> WordPiece {
        let vocabulary = [
            "[PAD]", "[UNK]", "[CLS]", "[SEP]", "Ann", "Lee", "##e", "Zo", "##\u{eb}", "-", ",",
            "'", "O", "Neil", "\u{65e5}", "un", "##aff", "##able", "$",
        ];
        let entries: Vec<String> = vocabulary
            .iter()
            .enumerate()
            .map(|(id, token)| format!("{}:{id}", serde_json::to_string(token).unwrap_or_default()))
            .collect();
        let json = format!(
            r###"{{"normalizer":{{"type":"BertNormalizer","clean_text":true,"handle_chinese_chars":true,"strip_accents":null,"lowercase":false}},
            "pre_tokenizer":{{"type":"BertPreTokenizer"}},
            "model":{{"type":"WordPiece","unk_token":"[UNK]","continuing_subword_prefix":"##","max_input_chars_per_word":100,"vocab":{{{}}}}}}}"###,
            entries.join(",")
        );
        WordPiece::from_json(json.as_bytes()).expect("a valid synthetic fixture")
    }

    fn spans(tokenizer: &WordPiece, text: &str) -> Vec<(u32, String, usize)> {
        tokenizer
            .pieces(text)
            .into_iter()
            .map(|piece| {
                (
                    piece.id,
                    text[piece.start..piece.end].to_owned(),
                    piece.word,
                )
            })
            .collect()
    }

    #[test]
    fn words_split_on_whitespace_and_punctuation_and_keep_their_places() {
        let tokenizer = tiny();
        assert_eq!(
            spans(&tokenizer, "Ann  Lee, O'Neil"),
            vec![
                (4, "Ann".to_owned(), 0),
                (5, "Lee".to_owned(), 1),
                (10, ",".to_owned(), 2),
                (12, "O".to_owned(), 3),
                (11, "'".to_owned(), 4),
                (13, "Neil".to_owned(), 5),
            ]
        );
    }

    #[test]
    fn continuations_and_multi_byte_letters_map_back_to_the_original_bytes() {
        let tokenizer = tiny();
        assert_eq!(
            spans(&tokenizer, "Zo\u{eb} unaffable"),
            vec![
                (7, "Zo".to_owned(), 0),
                (8, "\u{eb}".to_owned(), 0),
                (15, "un".to_owned(), 1),
                (16, "aff".to_owned(), 1),
                (17, "able".to_owned(), 1),
            ]
        );
    }

    #[test]
    fn a_word_with_an_unknown_part_is_one_unknown_piece() {
        let tokenizer = tiny();
        assert_eq!(spans(&tokenizer, "Annx"), vec![(1, "Annx".to_owned(), 0)]);
        let long = "a".repeat(101);
        assert_eq!(spans(&tokenizer, &long), vec![(1, long.clone(), 0)]);
    }

    #[test]
    fn removed_characters_join_their_neighbours_and_ideographs_stand_alone() {
        let tokenizer = tiny();
        // A zero-width space (a format character) is removed, not a separator.
        assert_eq!(
            spans(&tokenizer, "An\u{200b}n \u{65e5}Lee"),
            vec![
                (4, "An\u{200b}n".to_owned(), 0),
                (14, "\u{65e5}".to_owned(), 1),
                (5, "Lee".to_owned(), 2),
            ]
        );
        // ASCII symbols count as punctuation; a special token in the text is ordinary text.
        assert_eq!(spans(&tokenizer, "$Ann")[0], (18, "$".to_owned(), 0));
        assert!(spans(&tokenizer, "[CLS]").iter().all(|piece| piece.0 != 2));
    }

    #[test]
    fn a_tokenizer_that_is_not_berts_cased_wordpiece_is_refused() {
        let base = r###"{"normalizer":{"type":"BertNormalizer","clean_text":true,"handle_chinese_chars":true,"strip_accents":null,"lowercase":LOWER},
            "pre_tokenizer":{"type":"BertPreTokenizer"},
            "model":{"type":"WordPiece","unk_token":"[UNK]","continuing_subword_prefix":"##","max_input_chars_per_word":100,"vocab":{"[UNK]":0,"[CLS]":1,"[SEP]":2}}}"###;
        assert!(WordPiece::from_json(base.replace("LOWER", "false").as_bytes()).is_ok());
        assert!(matches!(
            WordPiece::from_json(base.replace("LOWER", "true").as_bytes()),
            Err(NerError::Tokenizer(_))
        ));
        assert!(WordPiece::from_json(b"not json").is_err());
        let without_sep = base.replace("LOWER", "false").replace(",\"[SEP]\":2", "");
        assert!(WordPiece::from_json(without_sep.as_bytes()).is_err());
    }
}
