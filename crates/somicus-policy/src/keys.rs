//! The keys Somicus may press by itself in a `press_key` step (ADR-0021 §3).
//!
//! The list mirrors `ALLOWED_KEYS` in the agent runtime (not part of this repository); a key outside
//! it is never pressed, whatever the model asks for. It holds navigation keys and a few browser and media shortcuts.
//! Keys that quit, close, save, send, print or delete files are not on it, and neither are letters that would type
//! text: typed text must come from the person's request (see [`crate::typed_value_allowed`]).

use crate::action::{ActionTarget, is_search_target};
use crate::risk::RiskClass;

/// Every key a step may press, in canonical form: modifiers in the order ctrl, option, shift, cmd, then the key.
pub const ALLOWED_KEYS: [&str; 30] = [
    "up",
    "down",
    "left",
    "right",
    "page_up",
    "page_down",
    "home",
    "end",
    "space",
    "tab",
    "shift+tab",
    "escape",
    "return",
    "delete",
    "cmd+a",
    "cmd+s",
    "cmd+l",
    "cmd+f",
    "cmd+t",
    "cmd+r",
    "cmd+k",
    "cmd+up",
    "cmd+down",
    "cmd+left",
    "cmd+right",
    "k",
    "m",
    "f",
    "j",
    "l",
];

/// Whether `key` is one Somicus may press.
#[must_use]
pub fn allowed_key(key: &str) -> bool {
    ALLOWED_KEYS.contains(&key)
}

/// The lowest risk class of pressing `key` with `target` focused, from the key and the target alone.
///
/// A key that is not allowed is R5 and never pressed. Return is a commit (R3) unless it is pressed in a search or
/// address field, where it navigates (R1). Delete edits text in a field (R2); anywhere else it removes what is selected,
/// such as a file, a message or a row, so it is a commit (R3). Select All edits a draft (R2). Save (⌘S) is a
/// commit (R3), like the Save button. Everything else on
/// the list only moves, scrolls or switches the view (R1). Nothing is pressed into a password field.
#[must_use]
pub fn key_floor(key: &str, target: &ActionTarget<'_>) -> RiskClass {
    if !allowed_key(key) || target.role == "password_field" {
        return RiskClass::R5;
    }
    match key {
        "return" if is_search_target(target) => RiskClass::R1,
        // Saving (cmd+s) persists the document, like the Save button (2026-11-10, agent-sim `excel-grades`).
        "return" | "cmd+s" => RiskClass::R3,
        "delete" if !is_text_target(target) => RiskClass::R3,
        "delete" | "cmd+a" => RiskClass::R2,
        _ => RiskClass::R1,
    }
}

/// Whether `target` holds editable text, where Delete removes characters.
fn is_text_target(target: &ActionTarget<'_>) -> bool {
    matches!(
        target.role,
        "text_field" | "text_area" | "search_field" | "combo_box"
    )
}

/// Whether pressing `key` on its own would type a character into a focused text field: a bare letter or the space bar.
/// Such keys are YouTube-style media shortcuts on a page, but text in a field, which must come from the person.
#[must_use]
pub fn types_text(key: &str) -> bool {
    key == "space" || (key.chars().count() == 1 && key.chars().all(char::is_alphabetic))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_listed_keys_are_allowed() {
        assert!(allowed_key("cmd+l"));
        assert!(allowed_key("page_down"));
        // Saving like the Save button (ADR-0055: a save no longer asks).
        assert!(allowed_key("cmd+s"));
        for key in [
            "cmd+q",
            "cmd+w",
            "cmd+delete",
            "cmd+return",
            "a",
            "1",
            "cmd+v",
            "Cmd+L",
        ] {
            assert!(!allowed_key(key), "{key}");
        }
    }

    #[test]
    fn keys_are_classified_by_what_they_can_change() {
        let page = ActionTarget::new("YouTube", "web_area");
        let search = ActionTarget::new("Search", "search_field");
        let password = ActionTarget::new("Password", "password_field");
        assert_eq!(key_floor("page_down", &page), RiskClass::R1);
        assert_eq!(key_floor("return", &search), RiskClass::R1);
        assert_eq!(key_floor("return", &page), RiskClass::R3);
        assert_eq!(key_floor("delete", &search), RiskClass::R2);
        assert_eq!(key_floor("cmd+q", &page), RiskClass::R5);
        assert_eq!(key_floor("up", &password), RiskClass::R5);
    }

    #[test]
    fn only_bare_letters_and_space_type_text() {
        for key in ["k", "m", "f", "j", "l", "space"] {
            assert!(types_text(key), "{key}");
        }
        for key in [
            "up",
            "page_down",
            "return",
            "delete",
            "tab",
            "cmd+l",
            "shift+tab",
        ] {
            assert!(!types_text(key), "{key}");
        }
    }
}
