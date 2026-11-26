//! The banner buttons of each choice, read word by word as the runtime reads them, the bare verbs that need a banner
//! without terms, and the words that name cookies and terms (ADR-0056).

use super::{
    Banner, BannerChoice, ButtonChoice, answers_banner, button_choice, names_cookies, names_terms,
};
use crate::{ActionKind, RiskClass, consequences_of};

const COOKIES: Banner = Banner { names_terms: false };
const WITH_TERMS: Banner = Banner { names_terms: true };

/// The labels both the device and the runtime must read alike.
const SHARED: &str = include_str!("labels.json");

#[derive(serde::Deserialize)]
struct Shared {
    labels: Vec<Labelled>,
}

#[derive(serde::Deserialize)]
struct Labelled {
    label: String,
    choice: Option<String>,
    bare: bool,
}

fn expected(label: &Labelled) -> Option<ButtonChoice> {
    let choice = match label.choice.as_deref()? {
        "necessary" => BannerChoice::NecessaryOnly,
        _ => BannerChoice::AcceptAll,
    };
    Some(ButtonChoice {
        choice,
        bare: label.bare,
    })
}

#[test]
fn reads_every_label_of_the_shared_list_as_the_runtime_does() {
    let shared: Shared = serde_json::from_str(SHARED).expect("the shared labels are JSON");
    assert!(shared.labels.len() > 40);
    for label in &shared.labels {
        assert_eq!(
            button_choice(&label.label),
            expected(label),
            "{}",
            label.label
        );
    }
}

#[test]
fn answers_with_the_buttons_of_the_persons_choice_only() {
    for label in [
        "Reject all",
        "Only necessary cookies",
        "Alle ablehnen",
        "Tout refuser",
        "Decline",
    ] {
        assert!(
            answers_banner(label, BannerChoice::NecessaryOnly, COOKIES),
            "{label}"
        );
        assert!(
            !answers_banner(label, BannerChoice::AcceptAll, COOKIES),
            "{label}"
        );
    }
    for label in [
        "Accept all",
        "Allow all cookies",
        "Alle akzeptieren",
        "Tout accepter",
        "I agree",
    ] {
        assert!(
            answers_banner(label, BannerChoice::AcceptAll, COOKIES),
            "{label}"
        );
        assert!(
            !answers_banner(label, BannerChoice::NecessaryOnly, COOKIES),
            "{label}"
        );
    }
}

#[test]
fn a_bare_verb_needs_a_banner_that_names_no_terms() {
    for (label, choice) in [
        ("I agree", BannerChoice::AcceptAll),
        ("Agree and continue", BannerChoice::AcceptAll),
        ("Elfogadom", BannerChoice::AcceptAll),
        ("Decline", BannerChoice::NecessaryOnly),
    ] {
        assert!(answers_banner(label, choice, COOKIES), "{label}");
        assert!(!answers_banner(label, choice, WITH_TERMS), "{label}");
    }
    assert!(answers_banner(
        "Accept all cookies",
        BannerChoice::AcceptAll,
        WITH_TERMS
    ));
    assert!(answers_banner(
        "Csak az alapvető cookie-k elfogadása",
        BannerChoice::NecessaryOnly,
        WITH_TERMS
    ));
}

#[test]
fn no_banner_choice_sends_deletes_pays_posts_approves_or_is_critical() {
    let shared: Shared = serde_json::from_str(SHARED).expect("the shared labels are JSON");
    for label in shared.labels.iter().filter(|label| label.choice.is_some()) {
        assert!(
            consequences_of(ActionKind::Click, &label.label, false, RiskClass::R1).is_empty(),
            "{}",
            label.label
        );
    }
}

#[test]
fn finds_cookies_and_terms_in_a_banners_words() {
    assert!(names_cookies("We use cookies to measure traffic."));
    assert!(names_cookies("Wir verwenden Cookies"));
    assert!(names_cookies("Honlapunk sütiket használ."));
    assert!(names_cookies("Your consent choices"));
    assert!(!names_cookies(
        "Team meeting at 10:00, please accept or decline"
    ));
    assert!(names_terms("By continuing you accept our Terms of Service"));
    assert!(names_terms("Allgemeine Geschäftsbedingungen"));
    assert!(names_terms("Felhasználási feltételek"));
    assert!(!names_terms(
        "We use cookies to measure traffic and for ads."
    ));
    assert!(!names_terms("Termin am Montag"));
}
