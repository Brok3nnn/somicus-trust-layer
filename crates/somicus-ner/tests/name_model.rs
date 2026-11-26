//! The name model itself, on synthetic sentences. The model files are never part of the repository, so these tests
//! are ignored by default. Run them with `cargo test -p somicus-ner -- --ignored` once `SOMICUS_NAME_MODEL_DIR` names a
//! folder with the pinned `model.safetensors` (16-bit) and `tokenizer.json`; without it they fail.

use std::path::PathBuf;
use std::sync::OnceLock;
use std::time::{Duration, Instant};

use somicus_ner::{NameModel, NerError};
use somicus_privacy::NameRecognizer as _;

/// The verified model, loaded once for every test, or `None` when `SOMICUS_NAME_MODEL_DIR` is not set.
fn model() -> Option<&'static Result<NameModel, NerError>> {
    static MODEL: OnceLock<Option<Result<NameModel, NerError>>> = OnceLock::new();
    MODEL
        .get_or_init(|| {
            let folder = std::env::var_os("SOMICUS_NAME_MODEL_DIR").map(PathBuf::from)?;
            Some(NameModel::load(&folder))
        })
        .as_ref()
}

/// The names found in `text`, or `None` when the model did not scan it.
fn names(model: &NameModel, text: &str) -> Option<Vec<String>> {
    let deadline = Instant::now() + Duration::from_secs(30);
    let ranges = model.find_names(&[text], deadline).pop()??;
    Some(
        ranges
            .into_iter()
            .map(|range| text[range].to_owned())
            .collect(),
    )
}

#[test]
#[ignore = "needs the pinned name model: set SOMICUS_NAME_MODEL_DIR (see the README)"]
fn only_people_are_marked_not_organisations_places_or_events() {
    let model = model().expect("SOMICUS_NAME_MODEL_DIR names the folder of the pinned name model");
    let model = model.as_ref().expect("the pinned model files load");
    // The classifier's rows are O, then B- and I- of PER, ORG, LOC and MISC: with another order, the organisation,
    // the place or the event would come out as people.
    assert_eq!(
        names(
            model,
            "Olivia Bennett joined Hale Trading Ltd in Leeds for the Winter Olympics"
        ),
        Some(vec!["Olivia Bennett".to_owned()])
    );
    assert_eq!(
        names(
            model,
            "Invoice from Northwind Traders, London, approved by Marcus Reyes"
        ),
        Some(vec!["Marcus Reyes".to_owned()])
    );
    assert_eq!(
        names(model, "Open the quarterly report in Microsoft Excel"),
        Some(Vec::new())
    );
}

#[test]
#[ignore = "needs the pinned name model: set SOMICUS_NAME_MODEL_DIR (see the README)"]
fn a_name_is_marked_as_whole_words_and_joined_across_hyphens_and_apostrophes() {
    let model = model().expect("SOMICUS_NAME_MODEL_DIR names the folder of the pinned name model");
    let model = model.as_ref().expect("the pinned model files load");
    assert_eq!(
        names(model, "Reviewed by Mary-Jane O'Neil yesterday"),
        Some(vec!["Mary-Jane O'Neil".to_owned()])
    );
}

#[test]
#[ignore = "needs the pinned name model: set SOMICUS_NAME_MODEL_DIR (see the README)"]
fn file_names_and_text_in_one_case_are_read_a_second_time() {
    let model = model().expect("SOMICUS_NAME_MODEL_DIR names the folder of the pinned name model");
    let model = model.as_ref().expect("the pinned model files load");
    // The model may take the next word of a file name for part of the name; the name itself must be covered.
    let found = names(model, "HannahClarke_Passport.jpg").unwrap_or_default();
    assert!(
        found.iter().any(|name| name.contains("HannahClarke")),
        "{found:?}"
    );
    assert_eq!(
        names(model, "APPROVED BY DANIEL FOSTER"),
        Some(vec!["DANIEL FOSTER".to_owned()])
    );
}

#[test]
#[ignore = "needs the pinned name model: set SOMICUS_NAME_MODEL_DIR (see the README)"]
fn a_long_text_is_read_in_overlapping_windows() {
    let model = model().expect("SOMICUS_NAME_MODEL_DIR names the folder of the pinned name model");
    let model = model.as_ref().expect("the pinned model files load");
    let filler = "The budget table was updated and the totals were checked again. ".repeat(30);
    let text = format!("{filler}Please send it to Hannah Clarke before Friday.");
    assert_eq!(names(model, &text), Some(vec!["Hannah Clarke".to_owned()]));
}

#[test]
#[ignore = "needs the pinned name model: set SOMICUS_NAME_MODEL_DIR (see the README)"]
fn a_passed_deadline_scans_nothing_new_but_the_cache_still_answers() {
    let model = model().expect("SOMICUS_NAME_MODEL_DIR names the folder of the pinned name model");
    let model = model.as_ref().expect("the pinned model files load");
    let text = "Ticket assigned to Liam Harper";
    let past = Instant::now();
    std::thread::sleep(Duration::from_millis(1));
    model.clear_cache();
    assert_eq!(model.find_names(&[text], past), vec![None]);
    assert_eq!(names(model, text), Some(vec!["Liam Harper".to_owned()]));
    assert!(model.find_names(&[text], past)[0].is_some());
}
