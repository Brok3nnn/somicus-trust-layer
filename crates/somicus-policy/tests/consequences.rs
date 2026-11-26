//! The device's check that the person asked for a step that cannot be taken back (ADR-0040 requirement 7), on the
//! cases of the agent runtime's own tests (the runtime is not part of this repository), so the device and the runtime
//! agree. All words are synthetic.

use somicus_policy::{
    ActionKind, Consequence, RiskClass, asks_for, asks_to_rename, consequences_of, sends_on_return,
};

fn named(label: &str) -> Vec<Consequence> {
    consequences_of(ActionKind::Click, label, false, RiskClass::R1)
}

#[test]
fn a_step_is_named_by_its_label_by_return_that_commits_or_by_a_critical_class() {
    assert_eq!(named("Send"), [Consequence::Send]);
    assert_eq!(named("Delete"), [Consequence::Delete]);
    assert_eq!(named("Buy now"), [Consequence::Pay]);
    assert_eq!(named("Place order"), [Consequence::Pay]);
    assert_eq!(named("Post"), [Consequence::Post]);
    assert_eq!(named("Approve"), [Consequence::Approve]);
    assert_eq!(
        consequences_of(ActionKind::PressEnter, "Message", true, RiskClass::R3),
        [Consequence::Send]
    );
    assert_eq!(
        consequences_of(ActionKind::Click, "OK", false, RiskClass::R4),
        [Consequence::Critical]
    );
    // Third-party screens in Hungarian and German still name the step.
    assert_eq!(named("Küldés"), [Consequence::Send]);
    assert_eq!(named("Jóváhagyás"), [Consequence::Approve]);
    assert_eq!(named("Abschicken"), [Consequence::Send]);
    assert_eq!(named("Löschen"), [Consequence::Delete]);
    assert_eq!(
        consequences_of(ActionKind::Click, "Törlés", false, RiskClass::R4),
        [Consequence::Delete]
    );
}

#[test]
fn navigation_folders_of_past_steps_sorting_and_pointing_name_nothing() {
    for label in [
        "Posts",
        "Sort order",
        "PayPal",
        "Play",
        "Elküldött elemek",
        "Törölt elemek",
        "Küldemény követése",
    ] {
        assert_eq!(named(label), [], "{label}");
    }
    assert_eq!(
        consequences_of(ActionKind::Scroll, "Delete", true, RiskClass::R4),
        []
    );
}

#[test]
fn the_person_asks_for_each_kind_in_english() {
    assert!(asks_for(
        &[Consequence::Send],
        "write a letter to Peter and send it"
    ));
    assert!(asks_for(&[Consequence::Delete], "delete the old messages"));
    assert!(asks_for(&[Consequence::Pay], "buy the book in the cart"));
    assert!(asks_for(&[Consequence::Send], "forward it to Anna"));
    assert!(asks_for(&[Consequence::Post], "publish the post"));
    assert!(asks_for(&[Consequence::Approve], "approve the request"));
    assert!(asks_for(&[Consequence::Critical], "send the letter"));
}

#[test]
fn registering_signing_up_or_subscribing_asks_to_send_that_form() {
    // The runtime's own cases (ADR-0056).
    for words in [
        "register me for this webinar",
        "sign me up for the newsletter",
        "subscribe to the course",
    ] {
        assert!(asks_for(&[Consequence::Send], words), "{words}");
    }
    assert!(!asks_for(&[Consequence::Send], "do not register me"));
    assert!(!asks_for(
        &[Consequence::Send],
        "fill in the registration form"
    ));
}

#[test]
fn emailing_something_asks_to_send_it_but_writing_an_email_does_not() {
    // The runtime's own cases (agent-sim `mail-invoice`, 2026-11-10).
    for words in [
        "rename it, then email it to anna@example.com",
        "email this report to Ben",
        "mail them the invoice",
    ] {
        assert!(asks_for(&[Consequence::Send], words), "{words}");
    }
    for words in [
        "write an email to Anna about the meeting",
        "don't email it yet",
    ] {
        assert!(!asks_for(&[Consequence::Send], words), "{words}");
    }
}

#[test]
fn words_that_did_not_ask_allow_nothing() {
    assert!(!asks_for(
        &[Consequence::Delete],
        "send the letter to Peter"
    ));
    assert!(!asks_for(
        &[Consequence::Send, Consequence::Delete],
        "send the letter to Peter"
    ));
    assert!(!asks_for(
        &[Consequence::Pay],
        "remove the book from the cart"
    ));
    assert!(!asks_for(&[Consequence::Critical], "open the letter"));
    assert!(!asks_for(
        &[Consequence::Send],
        "write a letter to peter@example.com"
    ));
    assert!(!asks_for(&[Consequence::Post], "open my postbox"));
    assert!(!asks_for(&[Consequence::Post], "show me the latest posts"));
    assert!(!asks_for(
        &[Consequence::Send],
        "show me the letters I sent"
    ));
    assert!(!asks_for(
        &[Consequence::Delete],
        "what was deleted yesterday?"
    ));
    // The person's asks are English; Hungarian and German words leave the step to a confirmation.
    assert!(!asks_for(
        &[Consequence::Send],
        "küldd el az ajánlatot Annának"
    ));
    assert!(!asks_for(
        &[Consequence::Send],
        "schick das Angebot an Anna"
    ));
    assert!(!asks_for(&[Consequence::Delete], "töröld a fájlt"));
}

#[test]
fn a_negated_ask_is_no_ask_until_its_sentence_ends() {
    for words in [
        "do not send it",
        "Don't send it to Alex yet",
        "Don’t send it",
        "dont send it",
        "write the draft without sending it",
        "Do not, under any circumstances, send it",
        "Don't wait, send it",
        "write the letter to alex@example.com\ndon't send it",
    ] {
        assert!(!asks_for(&[Consequence::Send], words), "{words}");
    }
    assert!(!asks_for(
        &[Consequence::Delete],
        "never delete anything in this folder"
    ));
    assert!(!asks_for(
        &[Consequence::Pay],
        "please do not pay the invoice"
    ));
    assert!(!asks_for(
        &[Consequence::Critical],
        "don't send or delete anything"
    ));

    // An ask before the negation, in another sentence or after "but" still counts.
    assert!(asks_for(&[Consequence::Send], "Don't delete it. Send it."));
    assert!(!asks_for(
        &[Consequence::Delete],
        "Don't delete it. Send it."
    ));
    assert!(asks_for(
        &[Consequence::Send],
        "don't delete it but send it"
    ));
    assert!(asks_for(&[Consequence::Send], "Send it if it isn't empty"));
    assert!(asks_for(
        &[Consequence::Send],
        "send it\ndon't delete anything"
    ));
    assert!(asks_for(
        &[Consequence::Send],
        "send the report to alex@example.com, not to Sam"
    ));
    assert!(asks_for(
        &[Consequence::Send, Consequence::Delete],
        "Send it and delete the draft; never post it"
    ));
}

#[test]
fn a_rename_is_heard_unless_negated() {
    assert!(asks_to_rename("rename report.docx to report-final.docx"));
    assert!(asks_to_rename("could you go on renaming the files?"));
    assert!(asks_to_rename("rename the report, don't send it"));
    assert!(!asks_to_rename("give the file a new name"));
    assert!(!asks_to_rename("don't rename it"));
    assert!(!asks_to_rename("Never rename files in this folder."));
    assert!(!asks_to_rename("nevezd át a fájlt"));
}

#[test]
fn return_sends_only_in_a_message_box_or_a_messenger() {
    // A message, reply, chat or comment box, and any box of a messenger, send with Return (ADR-0055).
    for (app, label) in [
        ("Safari", "Write a message"),
        ("Mail", "Reply"),
        ("Chrome", "Add a comment"),
        ("Slack", "Notes"),
        ("WhatsApp", "Type here"),
        ("Microsoft Teams", "Compose"),
    ] {
        assert!(sends_on_return(app, label), "{app}: {label}");
    }
    // An unnamed box counts as one that sends, the safe way to be wrong.
    assert!(sends_on_return("Notes", ""));
    // Other form fields only complete the form.
    for (app, label) in [
        ("Safari", "First name"),
        ("Microsoft Excel", "Cell B2"),
        ("Finder", "Name"),
        ("Chrome", "Postal code"),
    ] {
        assert!(!sends_on_return(app, label), "{app}: {label}");
    }
}
