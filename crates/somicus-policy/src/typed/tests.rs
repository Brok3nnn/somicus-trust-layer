use super::*;

#[test]
fn values_from_the_request_may_be_typed() {
    let request = "Open YouTube and search for Northwind";
    assert!(typed_value_allowed("Northwind", request));
    assert!(typed_value_allowed("  northwind ", request));
    assert!(!typed_value_allowed("Marta Vale", request));
    assert!(!typed_value_allowed("", request));
    assert!(!typed_value_allowed("North\twind", "North\twind"));
    assert!(!typed_value_allowed(&"a".repeat(201), &"a".repeat(201)));
    assert!(typed_value_allowed(
        "Marta Vale",
        "play a random Mrata Vale song"
    ));
    assert!(!typed_value_allowed(
        "Victor Hale",
        "play a random Mrata Vale song"
    ));
}

#[test]
fn one_corrected_letter_is_accepted_but_numbers_never_change() {
    let request = "put on a random northwiind video on youtube for me";
    // Spacing and case, an extra letter, a missing letter, a wrong letter and a swapped pair.
    assert!(typed_value_allowed("Northwind", "play north wind"));
    assert!(typed_value_allowed("Northwind", request));
    assert!(typed_value_allowed("North Wiind", "play northwind"));
    assert!(typed_value_allowed("Marta Vale", "play marta vane"));
    assert!(typed_value_allowed(
        "Marta Vale",
        "open spotify and put me a random mart avale song."
    ));
    // Two edits, a short value, an invented word, and any change to a number.
    assert!(!typed_value_allowed("Northwind", "play nortthwiind"));
    assert!(!typed_value_allowed("Vale", "play Vaale"));
    assert!(!typed_value_allowed("Northwind videos", request));
    assert!(!typed_value_allowed("1000000", "transfer 100000 dollars"));
    assert!(!typed_value_allowed("100001", "transfer 100000 dollars"));
    assert!(!typed_value_allowed("account 4321", "account 4312"));
}

#[test]
fn the_address_of_a_named_site_may_be_typed() {
    assert!(typed_value_allowed("youtube.com", "Open YouTube"));
    assert!(typed_value_allowed(
        "https://www.youtube.com/",
        "watch Northwind on youtube"
    ));
    // A short ending after the name, or the name without its last letter.
    assert!(typed_value_allowed("youtube.com", "search for youtubers"));
    assert!(typed_value_allowed("youtube.com", "open youtub for me"));
    // One corrected letter in a long name, also before an ending.
    assert!(typed_value_allowed(
        "youtube.com",
        "turn the volume up on youtbers"
    ));
    assert!(typed_value_allowed("youtube.com", "open yuotube"));
    assert!(typed_value_allowed("spotify.com", "open spotfy"));
    assert!(!typed_value_allowed("news.example", "open the nws page"));
    assert!(!typed_value_allowed(
        "amazon.com",
        "show me something amazing"
    ));
    // Not named, too short to match safely, a longer word, or carrying a path the person never wrote.
    assert!(!typed_value_allowed("evil.com", "open YouTube"));
    assert!(!typed_value_allowed("you.com", "can you open youtube"));
    assert!(!typed_value_allowed("tube.com", "open youtube"));
    assert!(!typed_value_allowed(
        "youtube.com/watch?v=abc",
        "open YouTube"
    ));
    assert!(!typed_value_allowed("you tube.com", "open you tube"));
}

#[test]
fn youtube_may_be_typed_for_a_video_the_request_asks_for_without_naming_the_site() {
    assert!(typed_value_allowed(
        "youtube.com",
        "Put on a Northwind video."
    ));
    assert!(typed_value_allowed(
        "youtube.com",
        "put on some northwind videos"
    ));
    assert!(typed_value_allowed(
        "https://www.youtube.com/",
        "play a Northwind video"
    ));
    assert!(typed_value_allowed("youtube.com", "put on a clip"));
    // Only the site's own bare address, and only for a video.
    assert!(!typed_value_allowed("youtube.com", "open the Daily Ledger"));
    assert!(!typed_value_allowed(
        "youtube.com/watch?v=abc",
        "put on a Northwind video"
    ));
    assert!(!typed_value_allowed(
        "youtube.evil.com",
        "put on a Northwind video"
    ));
    assert!(!typed_value_allowed("vimeo.com", "put on a video"));
    // The person's words are English: other languages' words for a clip imply no site.
    assert!(!typed_value_allowed(
        "youtube.com",
        "tegy\u{e9}l be egy klipet"
    ));
    assert!(!typed_value_allowed(
        "youtube.com",
        "egy vide\u{f3}t k\u{e9}rek"
    ));
}

#[test]
fn a_composed_text_may_be_typed_for_every_task_outside_search_password_and_strict_fields() {
    let body = ActionTarget::new("Message body", "text_area");
    let slide = ActionTarget::new("Content", "text_area");
    let none = KnownNumbers::default();
    let draft = "Dear Anna,\n\nWelcome to the team.\n\nBest regards,\nPeter";
    assert!(typed_draft_allowed(
        draft,
        "Write an email to the new user",
        &body,
        &none
    ));
    // A deck, a document or a table asks for text too.
    let bullets = "The dog is the oldest domestic animal.\nLoyal and eager to learn.";
    assert!(typed_draft_allowed(
        bullets,
        "make a deck about dogs",
        &slide,
        &none
    ));
    assert!(!typed_value_allowed(
        draft,
        "Write an email to the new user"
    ));
    let search = ActionTarget::new("Search", "search_field");
    let password = ActionTarget::new("Password", "password_field");
    let card = ActionTarget::new("Card number", "text_field");
    assert!(!typed_draft_allowed(
        "cute cats",
        "write a note about cats",
        &search,
        &none
    ));
    assert!(!typed_draft_allowed(
        "hunter2",
        "write my password",
        &password,
        &none
    ));
    assert!(!typed_draft_allowed("Some text", "pay it", &card, &none));
    assert!(!typed_draft_allowed(
        "evil.com",
        "write a note",
        &body,
        &none
    ));
    assert!(!typed_draft_allowed(
        &"a".repeat(2_001),
        "write a note",
        &body,
        &none
    ));
    assert!(!typed_draft_allowed(
        "Hi\u{7}",
        "write a note",
        &body,
        &none
    ));
}

#[test]
fn numbers_come_from_the_request_the_task_screens_or_a_checked_calculation() {
    let cell = ActionTarget::new("B2", "cell");
    let request = "make an Excel table of the five largest cities in the country";
    let mut seen = KnownNumbers::from_text("1. | Northport | 1,686,222 · 2. | Eastvale | 199,520");
    assert!(typed_draft_allowed("1686222", request, &cell, &seen));
    assert!(!typed_draft_allowed("1700000", request, &cell, &seen));
    assert!(!typed_draft_allowed("1885742", request, &cell, &seen));
    assert!(seen.add_calculation("1686222 + 199520"));
    assert!(typed_draft_allowed("1885742", request, &cell, &seen));
    let none = KnownNumbers::default();
    let form = ActionTarget::new("Note", "text_field");
    assert!(typed_draft_allowed(
        "See you at 5.",
        "tell Anna that we meet at 5",
        &form,
        &none
    ));
    assert!(!typed_draft_allowed(
        "See you at 6.",
        "tell Anna that we meet at 5",
        &form,
        &none
    ));
}

#[test]
fn a_language_subdomain_names_the_site() {
    assert!(typed_value_allowed(
        "en.wikipedia.org",
        "according to wikipedia"
    ));
    assert!(typed_value_allowed(
        "de.wikipedia.org",
        "check the German Wikipedia"
    ));
    assert!(!typed_value_allowed(
        "en.evil.org",
        "according to wikipedia"
    ));
}
