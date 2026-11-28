//! Risk floors and value rules for the steps Somicus performs itself in Do mode.
//!
//! The floor depends only on the kind of action and the target's role in the local UI Graph, never on model output,
//! so a wrong or hostile model answer cannot lower it. The class of a performed action is the maximum of this floor,
//! the local lexicon and the model's estimate. Which values Somicus may type is in [`crate::typed`].

use crate::risk::{ActionContext, RiskClass, RiskClassifier};

/// What Somicus would do to the target element.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActionKind {
    /// Click the element.
    Click,
    /// Double-click the element.
    DoubleClick,
    /// Open the target's context menu; choosing a menu item is a separate action.
    ContextMenu,
    /// Copy the current selection into the task-owned clipboard.
    Copy,
    /// Paste the task-owned clipboard into a checked target.
    Paste,
    /// Move an item between independently checked endpoints.
    Drag,
    /// Focus the element and type a value into it.
    Type,
    /// Focus the element and press Return.
    PressEnter,
    /// Press an allowed key in the app in front; its own floor comes from [`crate::key_floor`].
    PressKey,
    /// Scroll the content under the element.
    Scroll,
    /// Move the pointer over the element without clicking.
    Hover,
    /// Open an installed app by name.
    OpenApp,
    /// Wait for the screen to load.
    Wait,
}

impl ActionKind {
    /// Whether the action only changes what is visible: scrolling, hovering or waiting never changes data.
    #[must_use]
    pub const fn only_observes(self) -> bool {
        matches!(self, Self::Scroll | Self::Hover | Self::Wait)
    }
}

/// The element an action targets, described only by local UI Graph data.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ActionTarget<'a> {
    /// The element's accessible name.
    pub label: &'a str,
    /// The UI Graph role name, for example `button`, `text_field`, `search_field` or `password_field`.
    pub role: &'a str,
}

impl<'a> ActionTarget<'a> {
    /// Describes a target by its accessible name and UI Graph role name.
    #[must_use]
    pub const fn new(label: &'a str, role: &'a str) -> Self {
        Self { label, role }
    }
}

/// Beginnings of the label words that mark a search field (EN, HU, DE), and the whole word `url`.
const SEARCH_WORD_STARTS: &[&str] = &["search", "keres", "suche"];
/// Label phrases that mark a browser's address bar (EN, HU, DE). A bare "address" names a recipient or a postal address,
/// where Return submits.
const ADDRESS_BAR_PHRASES: &[&str] = &[
    "address bar",
    "address and search",
    "címsor",
    "adressleiste",
    // A spreadsheet's Name Box: a cell address or range with Return only moves the selection (2026-11-10).
    "name box",
];

/// The lowest risk class an action can have, from its kind and its target alone.
///
/// Scrolling, hovering and waiting only change what is visible (R0). Anything else in a password field is R5 and never
/// performed. A click, a double-click or opening an app navigates (R1) and typing edits a draft (R2). Return in a
/// search or address field navigates (R1); anywhere else it may submit a form, so it counts as a commit (R3). A key
/// press starts at R1; [`crate::key_floor`] raises it per key.
#[must_use]
pub fn action_floor(kind: ActionKind, target: &ActionTarget<'_>) -> RiskClass {
    match kind {
        ActionKind::Scroll | ActionKind::Hover | ActionKind::Wait => RiskClass::R0,
        _ if target.role == "password_field" => RiskClass::R5,
        ActionKind::Click
        | ActionKind::DoubleClick
        | ActionKind::ContextMenu
        | ActionKind::Copy
        | ActionKind::OpenApp
        | ActionKind::PressKey => RiskClass::R1,
        ActionKind::Type | ActionKind::Paste | ActionKind::Drag => RiskClass::R2,
        ActionKind::PressEnter if is_search_target(target) => RiskClass::R1,
        ActionKind::PressEnter => RiskClass::R3,
    }
}

/// Whether `target` is a search or address field, where Return navigates rather than submits.
pub(crate) fn is_search_target(target: &ActionTarget<'_>) -> bool {
    if target.role == "search_field" {
        return true;
    }
    let label = target.label.to_lowercase();
    ADDRESS_BAR_PHRASES
        .iter()
        .any(|phrase| label.contains(phrase))
        || label.split(|c: char| !c.is_alphanumeric()).any(|word| {
            word == "url"
                || SEARCH_WORD_STARTS
                    .iter()
                    .any(|start| word.starts_with(start))
        })
}

/// The role names the lexicon classifier understands.
fn lexicon_role(role: &str) -> &str {
    match role {
        "text_field" | "text_area" | "search_field" | "password_field" => "textbox",
        "combo_box" => "combobox",
        "check_box" => "checkbox",
        "radio_button" => "radio",
        "menu_item" => "menuitem",
        "tree_item" => "treeitem",
        other => other,
    }
}

/// Classifies an action Somicus would perform: the maximum of [`action_floor`], the local lexicon and `model`.
/// Classification only ever escalates, so the model can raise the class but never lower it. The lexicon reads the
/// target's label, so it does not apply to scrolling, hovering or waiting, which never use the element, nor to
/// opening an app, whose target is only where the cursor points.
#[must_use]
pub fn classify_performed_action(
    classifier: &RiskClassifier,
    kind: ActionKind,
    target: &ActionTarget<'_>,
    model: RiskClass,
) -> RiskClass {
    let floor = action_floor(kind, target);
    if kind.only_observes() || kind == ActionKind::OpenApp {
        return floor.max(model);
    }
    let lexicon =
        classifier.classify_action(&ActionContext::new(target.label, lexicon_role(target.role)));
    floor.max(classifier.combine_with_model(lexicon, model))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nothing_is_ever_typed_into_a_password_field() {
        let field = ActionTarget::new("Password", "password_field");
        assert_eq!(action_floor(ActionKind::Type, &field), RiskClass::R5);
        assert_eq!(action_floor(ActionKind::Click, &field), RiskClass::R5);
        assert_eq!(
            classify_performed_action(
                &RiskClassifier::new(),
                ActionKind::Type,
                &field,
                RiskClass::R0
            ),
            RiskClass::R5
        );
    }

    #[test]
    fn observing_is_r0_and_never_reads_the_label() {
        let classifier = RiskClassifier::new();
        let delete = ActionTarget::new("Delete account", "button");
        for kind in [ActionKind::Scroll, ActionKind::Hover, ActionKind::Wait] {
            assert_eq!(action_floor(kind, &delete), RiskClass::R0);
            assert_eq!(
                classify_performed_action(&classifier, kind, &delete, RiskClass::R0),
                RiskClass::R0
            );
            let password = ActionTarget::new("Password", "password_field");
            assert_eq!(action_floor(kind, &password), RiskClass::R0);
        }
        assert_eq!(
            classify_performed_action(&classifier, ActionKind::Hover, &delete, RiskClass::R2),
            RiskClass::R2
        );
        assert!(
            classify_performed_action(&classifier, ActionKind::DoubleClick, &delete, RiskClass::R0)
                >= RiskClass::R3
        );
        let spotify = ActionTarget::new("Spotify", "window");
        assert_eq!(
            classify_performed_action(&classifier, ActionKind::OpenApp, &spotify, RiskClass::R0),
            RiskClass::R1
        );
        assert_eq!(action_floor(ActionKind::PressKey, &spotify), RiskClass::R1);
        assert_eq!(
            action_floor(ActionKind::ContextMenu, &spotify),
            RiskClass::R1
        );
    }

    #[test]
    fn return_navigates_only_in_search_and_address_fields() {
        let search = ActionTarget::new("Search", "text_field");
        let address = ActionTarget::new("Address and search bar", "text_field");
        // A Hungarian search label stays: defensive data for third-party screen content.
        let hungarian = ActionTarget::new("Keresés", "combo_box");
        let form = ActionTarget::new("Comment", "text_area");
        // A spreadsheet's Name Box only moves the selection (2026-11-10).
        let name_box = ActionTarget::new("Name Box", "text_field");
        assert_eq!(
            action_floor(ActionKind::PressEnter, &name_box),
            RiskClass::R1
        );
        assert_eq!(action_floor(ActionKind::PressEnter, &search), RiskClass::R1);
        assert_eq!(
            action_floor(ActionKind::PressEnter, &address),
            RiskClass::R1
        );
        assert_eq!(
            action_floor(ActionKind::PressEnter, &hungarian),
            RiskClass::R1
        );
        assert_eq!(
            action_floor(
                ActionKind::PressEnter,
                &ActionTarget::new("", "search_field")
            ),
            RiskClass::R1
        );
        assert_eq!(action_floor(ActionKind::PressEnter, &form), RiskClass::R3);
    }

    #[test]
    fn the_model_can_raise_the_class_but_never_lower_it() {
        let classifier = RiskClassifier::new();
        let safari = ActionTarget::new("Safari", "button");
        assert_eq!(
            classify_performed_action(&classifier, ActionKind::Click, &safari, RiskClass::R0),
            RiskClass::R1
        );
        assert_eq!(
            classify_performed_action(&classifier, ActionKind::Click, &safari, RiskClass::R4),
            RiskClass::R4
        );
        let send = ActionTarget::new("Send", "button");
        assert!(
            classify_performed_action(&classifier, ActionKind::Click, &send, RiskClass::R0)
                >= RiskClass::R3
        );
        let field = ActionTarget::new("Search YouTube", "text_field");
        assert_eq!(
            classify_performed_action(&classifier, ActionKind::Type, &field, RiskClass::R0),
            RiskClass::R2
        );
    }
}
