//! Steps the person must ask for in words, checked on the device (ADR-0028 and ADR-0040).
//!
//! Sending, deleting, paying, posting and approving cannot be taken back. While the person has confirmations switched
//! off, the device performs such a step without the Confirmation sheet only when the words of the current task ask for
//! that kind of thing; otherwise the step waits for a confirmation after all. This mirrors the agent runtime's own
//! check (the agent runtime is not part of this repository), so a compromised or mistaken
//! model service cannot propose an unasked send that the device then performs without asking.
//!
//! The words are compared in lower case without accents, as whole words; a word's apostrophe is dropped, so "don't"
//! reads as "dont". The person's asks are English. The names of steps also know the Hungarian and German stems the
//! runtime knows, because they classify the labels of third-party screens, and past forms of them ("Elküldött elemek")
//! name a folder, not a step. An English negation takes the asks after it back until its sentence ends: a sentence
//! mark before a space or the end, a new line or "but" ("don't delete it but send it" asks to send). A missed phrasing
//! only asks the person, which is the safe way to be wrong.

use crate::action::ActionKind;
use crate::risk::RiskClass;

/// A kind of step that cannot be taken back; `Critical` is an R4 step whose label names no other kind.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Consequence {
    /// Sending or submitting, also Return that commits outside a search or address field.
    Send,
    /// Deleting or removing.
    Delete,
    /// Paying, buying or ordering.
    Pay,
    /// Posting or publishing.
    Post,
    /// Approving.
    Approve,
    /// An R4 step that names none of the kinds above.
    Critical,
}

/// How a kind is named on a step (any language) and asked for by the person (English).
struct Kind {
    /// The kind.
    consequence: Consequence,
    /// Word beginnings, each with the continuations that do not count (`küld` but not `küldött`).
    stems: &'static [(&'static str, &'static [&'static str])],
    /// Whole words or phrases that name the kind on a step.
    names: &'static [&'static str],
    /// Whole words or phrases with which the person asks for it.
    asks: &'static [&'static str],
}

const KINDS: [Kind; 5] = [
    Kind {
        consequence: Consequence::Send,
        stems: &[
            ("kuld", &["ott", "emeny"]),
            ("elkuld", &["ott", "emeny"]),
            ("bekuld", &["ott", "emeny"]),
            ("senden", &[]),
            ("absend", &[]),
            ("abschick", &[]),
            ("schick", &[]),
        ],
        names: &["send", "submit"],
        asks: &[
            "send",
            "sends",
            "sending",
            "submit",
            "forward",
            "mail it",
            "mail this",
            "mail them",
            "mail the",
            "email it",
            "email this",
            "email them",
            "email the",
            // Registering, signing up or subscribing is sending that form (ADR-0056).
            "register",
            "sign up",
            "sign me up",
            "subscribe",
        ],
    },
    Kind {
        consequence: Consequence::Delete,
        stems: &[
            ("torol", &["t", "ve"]),
            ("kitorol", &["t", "ve"]),
            ("torles", &[]),
            ("losch", &[]),
            ("loesch", &[]),
            ("entfern", &[]),
        ],
        names: &["delete"],
        asks: &["delete", "deleting", "remove"],
    },
    Kind {
        consequence: Consequence::Pay,
        stems: &[
            ("fizet", &["ett", "ve"]),
            ("vasarl", &[]),
            ("megvasarol", &[]),
            ("bezahl", &[]),
            ("kauf", &[]),
            ("bestell", &[]),
        ],
        names: &[
            "pay",
            "buy",
            "buy now",
            "pay now",
            "place order",
            "checkout",
            "check out",
            "confirm purchase",
        ],
        asks: &["pay", "buy", "purchase", "order"],
    },
    Kind {
        consequence: Consequence::Post,
        stems: &[("kozzete", &[]), ("posztol", &[]), ("veroffentlich", &[])],
        names: &["post", "publish"],
        asks: &["post", "publish"],
    },
    Kind {
        consequence: Consequence::Approve,
        stems: &[("jovahagy", &[]), ("genehmig", &[])],
        names: &["approve"],
        asks: &["approve"],
    },
];

/// English negations as people type them, their apostrophe dropped, and "without", "neither" and "nor".
const NEGATIONS: [&str; 23] = [
    "not", "never", "cannot", "dont", "doesnt", "didnt", "isnt", "arent", "wasnt", "werent",
    "cant", "couldnt", "wont", "wouldnt", "shouldnt", "mustnt", "havent", "hasnt", "hadnt",
    "neednt", "without", "neither", "nor",
];

/// Marks that end a sentence when a space or the end follows them.
const SENTENCE_MARKS: [char; 6] = ['.', '!', '?', ';', ':', '…'];

/// `character` in lower case without the accents the lexicons meet (Hungarian, German and the common Latin ones).
fn fold_char(character: char) -> char {
    match character {
        'á' | 'à' | 'â' | 'ä' | 'ã' | 'å' | 'ā' => 'a',
        'é' | 'è' | 'ê' | 'ë' | 'ē' | 'ė' | 'ę' => 'e',
        'í' | 'ì' | 'î' | 'ï' | 'ī' => 'i',
        'ó' | 'ò' | 'ô' | 'ö' | 'õ' | 'ő' | 'ō' => 'o',
        'ú' | 'ù' | 'û' | 'ü' | 'ű' | 'ū' => 'u',
        'ç' | 'ć' | 'č' => 'c',
        'ñ' | 'ń' => 'n',
        'ý' | 'ÿ' => 'y',
        'š' | 'ś' => 's',
        'ž' | 'ź' | 'ż' => 'z',
        other => other,
    }
}

/// One piece of folded text: a word, or the end of a sentence.
#[derive(Debug, PartialEq, Eq)]
enum Piece {
    Word(String),
    End,
}

/// `text` in lower case without accents, as words and sentence ends. Letters and digits make words; an apostrophe
/// inside a word is dropped.
fn pieces(text: &str) -> Vec<Piece> {
    let folded: Vec<char> = text
        .chars()
        .flat_map(char::to_lowercase)
        .map(fold_char)
        .collect();
    let mut pieces = Vec::new();
    let mut word = String::new();
    let mut index = 0;
    while let Some(&character) = folded.get(index) {
        let next = folded.get(index + 1).copied();
        if character.is_alphanumeric() {
            word.push(character);
        } else if matches!(character, '\'' | '’')
            && !word.is_empty()
            && next.is_some_and(char::is_alphanumeric)
        {
            // "don't" is "dont".
        } else {
            if !word.is_empty() {
                pieces.push(Piece::Word(std::mem::take(&mut word)));
            }
            if character == '\n' {
                pieces.push(Piece::End);
            } else if SENTENCE_MARKS.contains(&character) {
                while folded
                    .get(index + 1)
                    .is_some_and(|mark| SENTENCE_MARKS.contains(mark))
                {
                    index += 1;
                }
                if folded
                    .get(index + 1)
                    .is_none_or(|after| after.is_whitespace())
                {
                    pieces.push(Piece::End);
                }
            }
        }
        index += 1;
    }
    if !word.is_empty() {
        pieces.push(Piece::Word(word));
    }
    pieces
        .into_iter()
        .map(|piece| match piece {
            Piece::Word(word) if word == "but" => Piece::End,
            other => other,
        })
        .collect()
}

/// The words of `text`, ignoring sentence ends.
fn words(text: &str) -> Vec<String> {
    pieces(text)
        .into_iter()
        .filter_map(|piece| match piece {
            Piece::Word(word) => Some(word),
            Piece::End => None,
        })
        .collect()
}

/// Whether `phrase` (words separated by single spaces) occurs in `words` as consecutive whole words.
fn has_phrase(words: &[String], phrase: &str) -> bool {
    let wanted: Vec<&str> = phrase.split(' ').collect();
    words
        .windows(wanted.len())
        .any(|window| window.iter().zip(&wanted).all(|(word, want)| word == want))
}

/// Whether one of `words` begins with a stem and goes on with none of its excluded continuations.
fn has_stem(words: &[String], stems: &[(&str, &[&str])]) -> bool {
    words.iter().any(|word| {
        stems.iter().any(|(stem, excluded)| {
            word.strip_prefix(stem).is_some_and(|rest| {
                !excluded
                    .iter()
                    .any(|continuation| rest.starts_with(continuation))
            })
        })
    })
}

/// The parts of `text` no English negation reaches: each sentence up to its first negation ("Don't delete it. Send
/// it." keeps "send it").
fn unnegated_parts(text: &str) -> Vec<Vec<String>> {
    let mut parts = vec![Vec::new()];
    let mut negated = false;
    for piece in pieces(text) {
        match piece {
            Piece::End => {
                parts.push(Vec::new());
                negated = false;
            }
            Piece::Word(word) if NEGATIONS.contains(&word.as_str()) => negated = true,
            Piece::Word(word) if !negated => {
                if let Some(part) = parts.last_mut() {
                    part.push(word);
                }
            }
            Piece::Word(_) => {}
        }
    }
    parts
}

/// What a step of `kind` on an element named `named` would do that cannot be taken back: the kinds its name names,
/// [`Consequence::Send`] when it `commits` with Return outside a search or address field, and
/// [`Consequence::Critical`] when its class is R4 but it names none. Scrolling, hovering and waiting change nothing.
#[must_use]
pub fn consequences_of(
    kind: ActionKind,
    named: &str,
    commits: bool,
    risk: RiskClass,
) -> Vec<Consequence> {
    if kind.only_observes() {
        return Vec::new();
    }
    let words = words(named);
    let mut kinds: Vec<Consequence> = KINDS
        .iter()
        .filter(|entry| {
            has_stem(&words, entry.stems) || entry.names.iter().any(|name| has_phrase(&words, name))
        })
        .map(|entry| entry.consequence)
        .collect();
    if commits && !kinds.contains(&Consequence::Send) {
        kinds.push(Consequence::Send);
    }
    if kinds.is_empty() && risk == RiskClass::R4 {
        kinds.push(Consequence::Critical);
    }
    kinds
}

/// Whether the task's `words` ask for every one of `kinds` where no English negation takes the ask back ("do not send
/// it" asks for nothing). A [`Consequence::Critical`] step names no kind, so words that ask for any kind are enough.
#[must_use]
pub fn asks_for(kinds: &[Consequence], words: &str) -> bool {
    let parts = unnegated_parts(words);
    let asked: Vec<Consequence> = KINDS
        .iter()
        .filter(|entry| {
            parts
                .iter()
                .any(|part| entry.asks.iter().any(|ask| has_phrase(part, ask)))
        })
        .map(|entry| entry.consequence)
        .collect();
    kinds.iter().all(|kind| match kind {
        Consequence::Critical => !asked.is_empty(),
        other => asked.contains(other),
    })
}

/// Apps whose text boxes send a message when the person presses Return, by whole words of their name or window.
const MESSENGERS: [&str; 10] = [
    "slack",
    "whatsapp",
    "messages",
    "teams",
    "discord",
    "telegram",
    "signal",
    "messenger",
    "skype",
    "viber",
];

/// Words in a text box's name that make it a message box, where Return sends.
const MESSAGE_BOXES: [&str; 6] = [
    "message",
    "messages",
    "reply",
    "chat",
    "comment",
    "write a message",
];

/// Whether Return in the box named `label` of the app or window `app` sends a message (ADR-0055): a box whose name
/// names a message, a reply, a chat or a comment, any box of a messenger, or a box Somicus cannot name. Return in
/// another form field only completes the form, which the person's mode decides about like Save.
#[must_use]
pub fn sends_on_return(app: &str, label: &str) -> bool {
    let named = words(label);
    let app_words = words(app);
    named.is_empty()
        || MESSAGE_BOXES.iter().any(|name| has_phrase(&named, name))
        || MESSENGERS.iter().any(|name| has_phrase(&app_words, name))
}

/// Whether the task's `words` ask to rename something, where no English negation takes it back: Return in a name field
/// then confirms the asked name rather than sending anything.
#[must_use]
pub fn asks_to_rename(words: &str) -> bool {
    unnegated_parts(words)
        .iter()
        .any(|part| has_phrase(part, "rename") || has_phrase(part, "renaming"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn words_fold_case_accents_and_apostrophes_and_split_sentences() {
        assert_eq!(
            pieces("Don't SEND it. Küldd el!\nBut töröld"),
            vec![
                Piece::Word("dont".to_owned()),
                Piece::Word("send".to_owned()),
                Piece::Word("it".to_owned()),
                Piece::End,
                Piece::Word("kuldd".to_owned()),
                Piece::Word("el".to_owned()),
                Piece::End,
                Piece::End,
                Piece::End,
                Piece::Word("torold".to_owned()),
            ]
        );
        // A mark inside a word or a number does not end the sentence.
        assert_eq!(words("e.g. 3.5"), ["e", "g", "3", "5"]);
        assert!(!pieces("version 3.5 now").contains(&Piece::End));
    }
}
