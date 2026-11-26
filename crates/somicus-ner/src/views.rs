//! A second reading of a text for the model, when its form hides names from a cased model trained on news: file names
//! and handles (`olivia_bennett_cv.pdf`, `HannahClarke_Passport.jpg`, `@marcus.reyes`) and text in one case
//! (`OWNER: LIAM HARPER`, `signed by chloe turner`). The model reads both the text and this view; a name found in either
//! is replaced, so the view can only add names.
//!
//! The view separates the parts of a file name or handle with spaces (underscores, a full stop or hyphen between
//! letters, a leading `@`, a change from a small to a capital letter) and writes in title case a
//! text without capitals, or a run of two or more words in capitals. Every character of the view knows the
//! characters of the original it came from, so a name found in the view is a range of the original.

/// A text as the model reads it the second time, with each character's origin.
pub(crate) struct View {
    /// The view's text.
    pub text: String,
    /// For each character of `text`: where it starts in `text`, and the original bytes it came from (empty for a space
    /// the view inserted).
    origin: Vec<(usize, usize, usize)>,
}

impl View {
    /// The original bytes that `text[start..end]` came from.
    pub(crate) fn original(&self, start: usize, end: usize) -> Option<(usize, usize)> {
        let first = self.origin.iter().find(|place| place.0 == start)?;
        let last = self.origin.iter().rev().find(|place| place.0 < end)?;
        Some((first.1, last.2))
    }
}

/// A character of the view being built: the character and the original bytes it came from.
type Placed = (char, usize, usize);

/// Whether the character at `index` lies between two letters.
fn between_letters(characters: &[Placed], index: usize) -> bool {
    index > 0
        && characters
            .get(index - 1)
            .is_some_and(|(character, ..)| character.is_alphabetic())
        && characters
            .get(index + 1)
            .is_some_and(|(character, ..)| character.is_alphabetic())
}

/// The text with the parts of file names and handles separated by spaces, camel case split, and as many characters
/// as before except the inserted spaces.
fn separated(text: &str) -> Vec<Placed> {
    let characters: Vec<Placed> = text
        .char_indices()
        .map(|(start, character)| (character, start, start + character.len_utf8()))
        .collect();
    let mut view: Vec<Placed> = Vec::with_capacity(characters.len() + 4);
    for (index, &(character, start, end)) in characters.iter().enumerate() {
        let after_space = index == 0
            || characters
                .get(index - 1)
                .is_some_and(|(previous, ..)| previous.is_whitespace());
        let separator = character == '_'
            || (matches!(character, '.' | '-') && between_letters(&characters, index))
            || (character == '@' && after_space);
        if separator {
            view.push((' ', start, end));
            continue;
        }
        let camel = character.is_uppercase()
            && view
                .last()
                .is_some_and(|(previous, ..)| previous.is_lowercase());
        if camel {
            view.push((' ', start, start));
        }
        view.push((character, start, end));
    }
    view
}

/// The words of `view` (runs of letters), as index ranges.
fn words(view: &[Placed]) -> Vec<std::ops::Range<usize>> {
    let mut words = Vec::new();
    let mut start = None;
    for (index, (character, ..)) in view.iter().enumerate() {
        match (character.is_alphabetic(), start) {
            (true, None) => start = Some(index),
            (false, Some(first)) => {
                words.push(first..index);
                start = None;
            }
            _ => {}
        }
    }
    if let Some(first) = start {
        words.push(first..view.len());
    }
    words
}

/// Whether the word is two or more letters, all capitals.
fn shouted(view: &[Placed], word: &std::ops::Range<usize>) -> bool {
    word.len() >= 2
        && view[word.clone()]
            .iter()
            .all(|(character, ..)| character.is_uppercase())
}

/// The second reading of `text`, or `None` when it would read the same.
pub(crate) fn normalised(text: &str) -> Option<View> {
    let view = separated(text);
    let words = words(&view);
    let no_capitals = !view.iter().any(|(character, ..)| character.is_uppercase());
    let title_case: Vec<bool> = (0..words.len())
        .map(|index| {
            no_capitals
                || (shouted(&view, &words[index])
                    && ((index > 0 && shouted(&view, &words[index - 1]))
                        || words
                            .get(index + 1)
                            .is_some_and(|next| shouted(&view, next))))
        })
        .collect();
    let mut cased: Vec<Placed> = Vec::with_capacity(view.len());
    let mut next_word = 0;
    for (index, &(character, start, end)) in view.iter().enumerate() {
        while words.get(next_word).is_some_and(|word| word.end <= index) {
            next_word += 1;
        }
        let in_titled = words
            .get(next_word)
            .filter(|word| word.contains(&index))
            .is_some_and(|_| title_case[next_word]);
        if in_titled {
            let first = words[next_word].start == index;
            let mapped: Vec<char> = if first {
                character.to_uppercase().collect()
            } else {
                character.to_lowercase().collect()
            };
            cased.extend(mapped.into_iter().map(|mapped| (mapped, start, end)));
        } else {
            cased.push((character, start, end));
        }
    }
    let mut result = View {
        text: String::with_capacity(text.len() + 4),
        origin: Vec::with_capacity(cased.len()),
    };
    for (character, start, end) in cased {
        result.origin.push((result.text.len(), start, end));
        result.text.push(character);
    }
    (result.text != text).then_some(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn view(text: &str) -> Option<String> {
        normalised(text).map(|view| view.text)
    }

    #[test]
    fn file_names_and_handles_are_separated() {
        assert_eq!(
            view("Olivia_Bennett_CV.pdf").as_deref(),
            Some("Olivia Bennett CV pdf")
        );
        assert_eq!(
            view("HannahClarke_Passport.jpg").as_deref(),
            Some("Hannah Clarke Passport jpg")
        );
        assert_eq!(
            view("Ryan.Mitchell.Resume.pdf").as_deref(),
            Some("Ryan Mitchell Resume pdf")
        );
        assert_eq!(view("@marcus.reyes").as_deref(), Some(" Marcus Reyes"));
        assert_eq!(view("interview notes (Emily Carter).txt"), None);
    }

    #[test]
    fn text_in_one_case_is_written_in_title_case() {
        assert_eq!(
            view("signed by chloe turner").as_deref(),
            Some("Signed By Chloe Turner")
        );
        assert_eq!(
            view("Owner: LIAM HARPER").as_deref(),
            Some("Owner: Liam Harper")
        );
        // A lone word in capitals is an abbreviation, not shouting.
        assert_eq!(view("Ethan Morris contract v2 FINAL"), None);
        assert_eq!(view("Mark as Read"), None);
    }

    #[test]
    fn every_character_of_the_view_knows_its_original_bytes() -> Result<(), &'static str> {
        let text = "TomHale-expenses";
        let view = normalised(text).ok_or("a view")?;
        assert_eq!(view.text, "Tom Hale expenses");
        let hale = view.text.find("Hale").ok_or("Hale")?;
        let (start, end) = view.original(hale, hale + 4).ok_or("an origin")?;
        assert_eq!(&text[start..end], "Hale");
        let both = view.original(0, hale + 4).ok_or("an origin")?;
        assert_eq!(&text[both.0..both.1], "TomHale");
        Ok(())
    }
}
