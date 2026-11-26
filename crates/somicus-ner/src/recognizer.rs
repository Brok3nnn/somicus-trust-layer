//! The name recognizer the Privacy Shield asks. Each text is read as it is and, when its form hides names (file names,
//! one case), a second time as a [`View`]; its word pieces go in windows of at most [`MAX_PIECES`], packed into rows,
//! a few rows per pass of the model until the deadline; the `PER` labels become byte ranges of whole words, and the
//! results are cached per text.

use std::collections::HashMap;
use std::hash::{BuildHasher as _, RandomState};
use std::ops::Range;
use std::sync::Mutex;
use std::time::Instant;

use somicus_privacy::NameRecognizer;

use crate::distilbert::{DistilBert, LABELS, Row};
use crate::spans::{joined, name_ranges, plausible};
use crate::tokenizer::{Piece, WordPiece};
use crate::views::{View, normalised};

/// Word pieces per model input, `[CLS]` and `[SEP]` included (ADR-0042 decision 7).
pub const MAX_PIECES: usize = 128;
/// Word pieces of a text in one window: [`MAX_PIECES`] without `[CLS]` and `[SEP]`.
const WINDOW: usize = MAX_PIECES - 2;
/// How far the next window of a long text starts: windows overlap by a quarter, so every piece is read with context.
const STRIDE: usize = WINDOW * 3 / 4;
/// Rows per pass of the model: a dense screen in one or two passes, and the deadline checked between passes of a few
/// tens of milliseconds.
const ROWS_PER_PASS: usize = 6;
/// Texts whose results the cache keeps before it starts again.
const CACHE_TEXTS: usize = 8_192;

/// The rows of the classifier: `O`, then `B-` and `I-` of `PER`, `ORG`, `LOC` and `MISC`. The model's `config.json`,
/// which names them, was not part of the approved download; this order was read from the model's answers on synthetic
/// sentences and is held by `tests/name_model.rs`.
const PERSON_ROWS: [usize; 2] = [1, 2];

/// Whether the highest of a word piece's label `scores` is `B-PER` or `I-PER`.
fn is_person(scores: &[f32]) -> bool {
    scores.len() == LABELS
        && scores
            .iter()
            .enumerate()
            .max_by(|left, right| left.1.total_cmp(right.1))
            .is_some_and(|(index, _)| PERSON_ROWS.contains(&index))
}

/// One reading of a text: the string the model reads, its pieces, and which of them are labelled a person.
struct Reading {
    text: usize,
    view: Option<View>,
    pieces: Vec<Piece>,
    person: Vec<bool>,
}

/// One window of one reading, as the model reads it.
struct Window {
    reading: usize,
    /// The index in the reading's pieces of this window's first piece.
    first: usize,
    ids: Vec<u32>,
}

/// The name model, loaded and verified, with its cache of results per text.
pub struct NameModel {
    tokenizer: WordPiece,
    model: DistilBert,
    /// Name ranges per text, keyed by a hash with a per-process random key: it holds offsets, never text.
    cache: Mutex<HashMap<u64, Vec<Range<usize>>>>,
    hasher: RandomState,
}

impl std::fmt::Debug for NameModel {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.debug_struct("NameModel").finish_non_exhaustive()
    }
}

impl NameModel {
    pub(crate) fn new(tokenizer: WordPiece, model: DistilBert) -> Self {
        Self {
            tokenizer,
            model,
            cache: Mutex::new(HashMap::new()),
            hasher: RandomState::new(),
        }
    }

    /// Forgets every cached result, as when the token vault is cleared at the end of a session.
    pub fn clear_cache(&self) {
        if let Ok(mut cache) = self.cache.lock() {
            cache.clear();
        }
    }

    fn cached(&self, key: u64) -> Option<Vec<Range<usize>>> {
        self.cache.lock().ok()?.get(&key).cloned()
    }

    fn remember(&self, key: u64, names: &[Range<usize>]) {
        if let Ok(mut cache) = self.cache.lock() {
            if cache.len() >= CACHE_TEXTS {
                cache.clear();
            }
            cache.insert(key, names.to_vec());
        }
    }

    /// The readings of the texts not cached: each text as it is, and its [`View`] when it has one.
    fn readings(&self, texts: &[&str], results: &[Option<Vec<Range<usize>>>]) -> Vec<Reading> {
        let mut readings = Vec::new();
        for (index, text) in texts.iter().enumerate() {
            if results[index].is_some() {
                continue;
            }
            let views = std::iter::once(None).chain(normalised(text).map(Some));
            for view in views {
                let pieces = self
                    .tokenizer
                    .pieces(view.as_ref().map_or(*text, |view| view.text.as_str()));
                readings.push(Reading {
                    text: index,
                    person: vec![false; pieces.len()],
                    view,
                    pieces,
                });
            }
        }
        readings
    }

    /// The model inputs of `readings`: each reading's pieces in windows of at most [`WINDOW`], each overlapping the
    /// one before by a quarter, between `[CLS]` and `[SEP]`.
    fn windows(&self, readings: &[Reading]) -> Vec<Window> {
        let mut windows = Vec::new();
        for (index, reading) in readings.iter().enumerate() {
            let mut first = 0;
            while first < reading.pieces.len() {
                let last = (first + WINDOW).min(reading.pieces.len());
                let mut ids = Vec::with_capacity(last - first + 2);
                ids.push(self.tokenizer.cls());
                ids.extend(reading.pieces[first..last].iter().map(|piece| piece.id));
                ids.push(self.tokenizer.sep());
                windows.push(Window {
                    reading: index,
                    first,
                    ids,
                });
                if last == reading.pieces.len() {
                    break;
                }
                first += STRIDE;
            }
        }
        windows
    }

    /// The person names in each of `texts`, scanning those not cached until `deadline`.
    fn scan(&self, texts: &[&str], deadline: Instant) -> Vec<Option<Vec<Range<usize>>>> {
        let keys: Vec<u64> = texts
            .iter()
            .map(|text| self.hasher.hash_one(text))
            .collect();
        let mut results: Vec<Option<Vec<Range<usize>>>> =
            keys.iter().map(|key| self.cached(*key)).collect();
        let mut readings = self.readings(texts, &results);
        let windows = self.windows(&readings);
        let unfinished = self.label_until(&windows, &mut readings, deadline);
        let mut found: Vec<Vec<Range<usize>>> = vec![Vec::new(); texts.len()];
        for reading in &readings {
            let text = reading
                .view
                .as_ref()
                .map_or(texts[reading.text], |view| view.text.as_str());
            for range in name_ranges(text, &reading.pieces, &reading.person) {
                if !plausible(text, &range) {
                    continue;
                }
                let original = match &reading.view {
                    Some(view) => view.original(range.start, range.end),
                    None => Some((range.start, range.end)),
                };
                if let Some((start, end)) = original {
                    found[reading.text].push(start..end);
                }
            }
        }
        for (index, result) in results.iter_mut().enumerate() {
            if result.is_some() || unfinished.contains(&index) {
                continue;
            }
            let names = joined(texts[index], std::mem::take(&mut found[index]));
            self.remember(keys[index], &names);
            *result = Some(names);
        }
        results
    }

    /// Labels `windows` packed into rows of at most [`MAX_PIECES`] pieces, [`ROWS_PER_PASS`] rows per pass, marking
    /// the pieces labelled a person in `readings`, until `deadline`. Returns the texts not finished: not reached in
    /// time, or in a pass the model failed on.
    fn label_until(
        &self,
        windows: &[Window],
        readings: &mut [Reading],
        deadline: Instant,
    ) -> Vec<usize> {
        let rows = pack(windows);
        let mut unfinished = Vec::new();
        for (pass, batch) in rows.chunks(ROWS_PER_PASS).enumerate() {
            if Instant::now() >= deadline {
                for row in &rows[pass * ROWS_PER_PASS..] {
                    unfinished.extend(
                        row.iter()
                            .map(|&window| readings[windows[window].reading].text),
                    );
                }
                break;
            }
            let inputs: Vec<Row> = batch.iter().map(|row| packed_row(windows, row)).collect();
            let Ok(logits) = self.model.logits(&inputs) else {
                for row in batch {
                    unfinished.extend(
                        row.iter()
                            .map(|&window| readings[windows[window].reading].text),
                    );
                }
                continue;
            };
            for (row, scores) in batch.iter().zip(&logits) {
                let mut offset = 0;
                for &index in row {
                    let window = &windows[index];
                    let pieces = window.ids.len() - 2;
                    let labels = scores.iter().skip(offset + 1).take(pieces);
                    let flags = readings[window.reading]
                        .person
                        .iter_mut()
                        .skip(window.first);
                    for (flag, label) in flags.zip(labels) {
                        *flag |= is_person(label);
                    }
                    offset += window.ids.len();
                }
            }
        }
        unfinished
    }
}

/// Packs `windows` into rows of at most [`MAX_PIECES`] pieces, longest first, each into the first row it fits: the
/// windows of a row by index.
fn pack(windows: &[Window]) -> Vec<Vec<usize>> {
    let mut order: Vec<usize> = (0..windows.len()).collect();
    order.sort_by_key(|&index| std::cmp::Reverse(windows[index].ids.len()));
    let mut rows: Vec<(usize, Vec<usize>)> = Vec::new();
    for index in order {
        let length = windows[index].ids.len();
        match rows
            .iter_mut()
            .find(|(used, _)| used + length <= MAX_PIECES)
        {
            Some((used, row)) => {
                *used += length;
                row.push(index);
            }
            None => rows.push((length, vec![index])),
        }
    }
    rows.into_iter().map(|(_, row)| row).collect()
}

/// The model input of one packed row: its windows one after another, each with positions from 0.
fn packed_row(windows: &[Window], row: &[usize]) -> Row {
    let mut packed = Row::default();
    for (segment, &index) in (0u32..).zip(row) {
        let ids = &windows[index].ids;
        packed.ids.extend_from_slice(ids);
        packed
            .positions
            .extend(0..u32::try_from(ids.len()).unwrap_or(u32::MAX));
        packed
            .segments
            .extend(std::iter::repeat_n(segment, ids.len()));
    }
    packed
}

impl NameRecognizer for NameModel {
    fn find_names(&self, texts: &[&str], deadline: Instant) -> Vec<Option<Vec<Range<usize>>>> {
        let span = somicus_telemetry::span!(DEBUG, "privacy_name_model", nodes = texts.len());
        let results = span.in_scope(|| self.scan(texts, deadline));
        let found: usize = results.iter().flatten().map(Vec::len).sum();
        let skipped = results.iter().filter(|result| result.is_none()).count();
        somicus_telemetry::record!(
            span,
            redactions = found,
            kind = if skipped == 0 { "model" } else { "partial" }
        );
        results
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn window(reading: usize, length: usize) -> Window {
        Window {
            reading,
            first: 0,
            ids: vec![7; length],
        }
    }

    #[test]
    fn windows_are_packed_longest_first_into_rows_that_fit() {
        let windows = [
            window(0, 100),
            window(1, 30),
            window(2, 28),
            window(3, 128),
            window(4, 20),
        ];
        let rows = pack(&windows);
        assert_eq!(rows, vec![vec![3], vec![0, 2], vec![1, 4]]);
        let row = packed_row(&windows, &rows[1]);
        assert_eq!(row.ids.len(), 128);
        assert_eq!(row.positions[100], 0);
        assert_eq!(row.segments[99], 0);
        assert_eq!(row.segments[100], 1);
    }

    #[test]
    fn only_the_two_person_rows_are_a_person() {
        let mut scores = [0.0f32; LABELS];
        for (index, expected) in [(0, false), (1, true), (2, true), (3, false), (8, false)] {
            scores.fill(0.0);
            scores[index] = 1.0;
            assert_eq!(is_person(&scores), expected, "row {index}");
        }
        assert!(!is_person(&[1.0, 2.0]));
    }
}
