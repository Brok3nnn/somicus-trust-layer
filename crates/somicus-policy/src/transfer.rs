//! Local content-transfer rules for guidance (ADR-0037). Clipboard content never enters these decisions:
//! the core owns it and supplies only whether this task copied it. Unknown page sensitivity is treated as sensitive.

use crate::action::ActionTarget;
use crate::fields::is_strict_target;

/// The Privacy Shield's local classification of the page a copy would read.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SourceSensitivity {
    /// The page is not classified as sensitive.
    Ordinary,
    /// The page contains sensitive customer data.
    Sensitive,
    /// No reliable classification was available.
    Unknown,
}

/// What the local transfer rules allow before the normal Policy Guard evaluates risk and autonomy.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransferDecision {
    /// The action must not execute, even with a confirmation.
    Deny,
    /// A locally verified Step Card confirmation is required.
    Confirm,
    /// Continue to normal risk classification and Policy Guard evaluation.
    Continue,
}

/// Whether a selection may be copied from `source`. Copying a password field is always forbidden. Unknown page
/// sensitivity is conservative: it requires the same confirmation as a sensitive page.
#[must_use]
pub fn copy_decision(
    source: &ActionTarget<'_>,
    sensitivity: SourceSensitivity,
) -> TransferDecision {
    if source.role == "password_field" {
        TransferDecision::Deny
    } else if sensitivity == SourceSensitivity::Ordinary {
        TransferDecision::Continue
    } else {
        TransferDecision::Confirm
    }
}

/// Whether the task-owned clipboard may be pasted into `destination`. Strict fields need the person's own words,
/// which an opaque clipboard item cannot prove, so they and password fields are denied.
#[must_use]
pub fn paste_decision(
    destination: &ActionTarget<'_>,
    task_owns_clipboard: bool,
) -> TransferDecision {
    if !task_owns_clipboard || destination.role == "password_field" || is_strict_target(destination)
    {
        TransferDecision::Deny
    } else {
        TransferDecision::Continue
    }
}

/// Whether a drag may cross `source` and `destination`. Both endpoints are checked; a strict destination is denied
/// because a drop can fill it without the value provenance that typing requires.
#[must_use]
pub fn drag_decision(
    source: &ActionTarget<'_>,
    destination: &ActionTarget<'_>,
) -> TransferDecision {
    if source.role == "password_field"
        || destination.role == "password_field"
        || is_strict_target(destination)
    {
        TransferDecision::Deny
    } else {
        TransferDecision::Continue
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn copy_denies_passwords_and_confirms_sensitive_or_unknown_pages() {
        let selection = ActionTarget::new("Selected image", "image");
        let password = ActionTarget::new("Password", "password_field");
        assert_eq!(
            copy_decision(&password, SourceSensitivity::Ordinary),
            TransferDecision::Deny
        );
        assert_eq!(
            copy_decision(&selection, SourceSensitivity::Ordinary),
            TransferDecision::Continue
        );
        assert_eq!(
            copy_decision(&selection, SourceSensitivity::Sensitive),
            TransferDecision::Confirm
        );
        assert_eq!(
            copy_decision(&selection, SourceSensitivity::Unknown),
            TransferDecision::Confirm
        );
    }

    #[test]
    fn paste_needs_this_tasks_copy_and_never_fills_strict_fields() {
        let document = ActionTarget::new("Document body", "text_area");
        let payment = ActionTarget::new("Payment amount", "text_field");
        let password = ActionTarget::new("Password", "password_field");
        assert_eq!(paste_decision(&document, false), TransferDecision::Deny);
        assert_eq!(paste_decision(&document, true), TransferDecision::Continue);
        assert_eq!(paste_decision(&payment, true), TransferDecision::Deny);
        assert_eq!(paste_decision(&password, true), TransferDecision::Deny);
    }

    #[test]
    fn drag_checks_both_ends() {
        let picture = ActionTarget::new("Dog picture", "image");
        let slide = ActionTarget::new("Slide", "canvas");
        let password = ActionTarget::new("Password", "password_field");
        let tax = ActionTarget::new("Tax number", "text_field");
        assert_eq!(drag_decision(&picture, &slide), TransferDecision::Continue);
        assert_eq!(drag_decision(&password, &slide), TransferDecision::Deny);
        assert_eq!(drag_decision(&picture, &password), TransferDecision::Deny);
        assert_eq!(drag_decision(&picture, &tax), TransferDecision::Deny);
    }
}
