//! The shield with a name model (ADR-0042), through a fake recognizer that marks one synthetic name and records what
//! it was shown.

use std::ops::Range;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use somicus_uigraph::privacy::ClassifiedText;

use crate::name_model::NameRecognizer;
use crate::shield::PrivacyShield;

/// Marks every occurrence of `name`, or returns the ranges it was given, and records each text and call.
struct Fake {
    name: &'static str,
    fixed: Option<Vec<Range<usize>>>,
    seen: Mutex<Vec<String>>,
    calls: AtomicUsize,
}

impl Fake {
    fn marking(name: &'static str) -> Arc<Self> {
        Arc::new(Self {
            name,
            fixed: None,
            seen: Mutex::new(Vec::new()),
            calls: AtomicUsize::new(0),
        })
    }

    fn seen(&self) -> Vec<String> {
        self.seen
            .lock()
            .map(|seen| seen.clone())
            .unwrap_or_default()
    }
}

impl NameRecognizer for Fake {
    fn find_names(&self, texts: &[&str], deadline: Instant) -> Vec<Option<Vec<Range<usize>>>> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        texts
            .iter()
            .map(|text| {
                if let Ok(mut seen) = self.seen.lock() {
                    seen.push((*text).to_owned());
                }
                if Instant::now() >= deadline {
                    return None;
                }
                Some(self.fixed.clone().unwrap_or_else(|| {
                    text.match_indices(self.name)
                        .map(|(start, found)| start..start + found.len())
                        .collect()
                }))
            })
            .collect()
    }
}

fn shield_with(fake: &Arc<Fake>, budget: Duration) -> PrivacyShield {
    let recognizer: Arc<Fake> = Arc::clone(fake);
    PrivacyShield::with_name_model(recognizer, budget)
}

const BUDGET: Duration = Duration::from_secs(30);

#[test]
fn the_models_names_are_added_to_the_rules() {
    let fake = Fake::marking("Zed Quill");
    let mut shield = shield_with(&fake, BUDGET);
    assert!(shield.uses_name_model());
    let redacted = shield.redact_text(&ClassifiedText::content_value("Assigned to Zed Quill"));
    assert_eq!(redacted.as_inner(), "Assigned to ⟦PERSON_1⟧");
    assert_eq!(
        shield.rehydrate(redacted.as_inner()),
        "Assigned to Zed Quill"
    );
    assert_eq!(shield.names_left_to_rules(), 0);
}

#[test]
fn the_model_never_reads_identifiers_contact_details_or_secrets() {
    let fake = Fake::marking("Zed Quill");
    let mut shield = shield_with(&fake, BUDGET);
    let text = "Zed Quill, zed@quill.example, password: hunter2xyz, SSN 123-45-6789";
    let redacted = shield.redact_text(&ClassifiedText::content_value(text));
    assert_eq!(
        redacted.as_inner(),
        "⟦PERSON_1⟧, ⟦EMAIL_1⟧, ⟦SECRET_1⟧, SSN ⟦SSN_1⟧"
    );
    let seen = fake.seen();
    assert_eq!(seen.len(), 1);
    assert_eq!(seen[0].len(), text.len(), "offsets are kept");
    for hidden in ["zed@quill", "hunter2", "123-45-6789"] {
        assert!(!seen[0].contains(hidden), "the model read {hidden}");
    }
}

#[test]
fn texts_replaced_whole_never_reach_the_model() {
    let fake = Fake::marking("Zed Quill");
    let mut shield = shield_with(&fake, BUDGET);
    let _ = shield.redact_text(&ClassifiedText::secret("Zed Quill"));
    let _ = shield.redact_text(&ClassifiedText::personal("Zed Quill"));
    let _ = shield.redact_text(&ClassifiedText::content_value("⟦PERSON_1⟧"));
    let _ = shield.redact_text(&ClassifiedText::content_value("12 / 40"));
    assert!(fake.seen().is_empty(), "{:?}", fake.seen());
}

#[test]
fn a_passed_deadline_leaves_texts_to_the_rules_and_counts_them() {
    let fake = Fake::marking("Zed Quill");
    let mut shield = shield_with(&fake, Duration::ZERO);
    let redacted = shield.redact_text(&ClassifiedText::content_value(
        "Assigned to Zed Quill, zed@quill.example",
    ));
    // The model could not run, so only the rules' replacement is made, and the turn is partial.
    assert_eq!(redacted.as_inner(), "Assigned to Zed Quill, ⟦EMAIL_1⟧");
    assert_eq!(shield.names_left_to_rules(), 1);
}

#[test]
fn ranges_off_the_text_or_inside_a_letter_are_dropped() {
    let fake = Arc::new(Fake {
        name: "",
        fixed: Some(vec![0..2, 5..400, 3..3, 12..16]),
        seen: Mutex::new(Vec::new()),
        calls: AtomicUsize::new(0),
    });
    let mut shield = shield_with(&fake, BUDGET);
    // "Zoë" is four bytes: 0..2 is "Zo", a valid range; 5..400 runs past the end; 3..3 is empty.
    let redacted = shield.redact_text(&ClassifiedText::content_value("Zoë and Quill"));
    assert_eq!(redacted.as_inner(), "⟦PERSON_1⟧ë and Quill");
}

#[test]
fn a_screen_is_scanned_in_one_batch_before_its_texts_are_redacted() {
    let fake = Fake::marking("Zed Quill");
    let mut shield = shield_with(&fake, BUDGET);
    let texts = ["Owner: Zed Quill", "Reply to zed@quill.example", "Save"];
    shield.scan_names(&texts);
    assert_eq!(fake.calls.load(Ordering::SeqCst), 1);
    let seen = fake.seen();
    assert_eq!(seen.len(), 3);
    assert!(seen.iter().all(|text| !text.contains("zed@quill")));
    // Without a model, a scan does nothing.
    PrivacyShield::new().scan_names(&texts);
}
