//! Core UI Node representations (Perception §2).

use crate::fingerprint::ElementFingerprint;
use crate::geometry::PhysicalRect;
use crate::privacy::{ClassifiedText, PrivacyClass};
use crate::role::Role;
use serde::{Deserialize, Serialize};
use std::fmt;

/// Short, session-scoped element identifier (e.g. "e42").
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct NodeId(pub String);

impl NodeId {
    /// Creates a new `NodeId` from a string slice.
    #[must_use]
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }

    /// Creates a numeric indexed `NodeId` ("e1", "e2", etc.).
    #[must_use]
    pub fn from_index(index: usize) -> Self {
        Self(format!("e{index}"))
    }

    /// The special choice returned when the intended target is not visible on screen.
    #[must_use]
    pub fn none_visible() -> Self {
        Self("none_visible".to_string())
    }

    /// Returns a string slice of the ID.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for NodeId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Bitset of state flags for a UI node.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[allow(
    clippy::struct_excessive_bools,
    reason = "independent accessibility states, serialized as named flags"
)]
pub struct NodeStates {
    /// Whether the element is enabled for interaction.
    pub enabled: bool,
    /// Whether the element currently has input focus.
    pub focused: bool,
    /// Whether the element can accept keyboard/input focus.
    pub focusable: bool,
    /// Whether the element is currently selected.
    pub selected: bool,
    /// Whether the element is checked (for checkbox/radio).
    pub checked: bool,
    /// Whether the element is in an indeterminate/mixed state.
    pub mixed: bool,
    /// Whether the expandable container is currently expanded.
    pub expanded: bool,
    /// Whether the expandable container is currently collapsed.
    pub collapsed: bool,
    /// Whether input in this field is required.
    pub required: bool,
    /// Whether this field is read-only.
    pub readonly: bool,
    /// Whether the field contents are marked invalid.
    pub invalid: bool,
    /// Whether the element is scrolled or clipped offscreen.
    pub offscreen: bool,
    /// Whether the element or its subtree is currently busy.
    pub busy: bool,
    /// Whether the window or dialog is modal.
    pub modal: bool,
    /// Whether this is the default action button (e.g. Enter triggers it).
    pub default_action: bool,
}

/// Bitset of supported actions on a UI node.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[allow(
    clippy::struct_excessive_bools,
    reason = "independent supported actions, serialized as named flags"
)]
pub struct NodeActions {
    /// Click/invoke the primary action.
    pub invoke: bool,
    /// Toggle state (checkbox, switch).
    pub toggle: bool,
    /// Expand container.
    pub expand: bool,
    /// Collapse container.
    pub collapse: bool,
    /// Select this item.
    pub select: bool,
    /// Set text/value.
    pub set_value: bool,
    /// Scroll into viewport.
    pub scroll_into_view: bool,
    /// Request focus.
    pub focus: bool,
    /// Open context menu.
    pub show_menu: bool,
}

/// Provenance adapter that discovered the node.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum NodeSource {
    /// Windows UI Automation.
    #[default]
    Uia,
    /// macOS Accessibility API (AX).
    Ax,
    /// Linux AT-SPI.
    Atspi,
    /// Browser DOM (via extension).
    Dom,
    /// SAP GUI Scripting API.
    Sap,
    /// Java Access Bridge.
    Jab,
    /// On-device OCR detection.
    Ocr,
    /// On-device icon/element detector.
    Detector,
    /// Cloud Vision-Language Model.
    Vlm,
}

impl NodeSource {
    /// Returns the initial baseline confidence for elements from this source (Perception §5).
    #[must_use]
    pub const fn prior_confidence(self) -> f32 {
        match self {
            Self::Dom => 0.95,
            Self::Uia | Self::Ax | Self::Sap | Self::Jab => 0.90,
            Self::Atspi => 0.85,
            Self::Vlm => 0.75,
            Self::Detector | Self::Ocr => 0.60,
        }
    }
}

/// Semantic relations to other nodes in the same graph.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub struct NodeRelations {
    /// Node providing the accessible label for this element.
    pub labelled_by: Option<NodeId>,
    /// Target node controlled by this element.
    pub controls: Option<NodeId>,
    /// Node providing supplementary help or description.
    pub described_by: Option<NodeId>,
    /// Logical next node in linear reading flow.
    pub flows_to: Option<NodeId>,
}

/// Selector strategies for element re-resolution (Perception §6).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub struct SelectorSet {
    /// Native platform automation ID (UIA `AutomationId`, `AXIdentifier`).
    pub automation_id: Option<String>,
    /// Toolkit class name.
    pub class_name: Option<String>,
    /// HTML `id` attribute.
    pub dom_id: Option<String>,
    /// HTML `data-testid` attribute.
    pub data_testid: Option<String>,
    /// HTML `name` attribute.
    pub dom_name: Option<String>,
    /// Explicit ARIA role.
    pub aria_role: Option<String>,
    /// Structural accessibility path.
    pub a11y_path: Option<String>,
    /// Relative anchor descriptions ("in group 'Payment'").
    pub relative_anchors: Vec<String>,
    /// Visual anchor text or offset.
    pub visual_anchor: Option<String>,
}

/// A normalized, fused node in the UI Graph (Perception §2).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct UiNode {
    /// Short session-scoped identifier.
    pub id: NodeId,
    /// Stable 64-bit element fingerprint.
    pub fp: ElementFingerprint,
    /// Parent node ID, if not a root element.
    pub parent: Option<NodeId>,
    /// Child node IDs in presentation order.
    pub children: Vec<NodeId>,
    /// Normalized element role.
    pub role: Role,
    /// Accessible name (with on-device privacy classification).
    pub name: ClassifiedText,
    /// Optional description / tooltip.
    pub description: Option<ClassifiedText>,
    /// Current input value (masked by default).
    pub value: Option<ClassifiedText>,
    /// Element states bitset.
    pub states: NodeStates,
    /// Global physical pixel bounding box (None if offscreen/hidden).
    pub bounds: Option<PhysicalRect>,
    /// Permitted user / agent actions.
    pub actions: NodeActions,
    /// Keyboard shortcut / accelerator.
    pub shortcut: Option<String>,
    /// Semantic relations to other nodes.
    pub relations: NodeRelations,
    /// Provenance source adapter.
    pub source: NodeSource,
    /// Set of selector strategies.
    pub selectors: SelectorSet,
    /// Confidence score between 0.0 and 1.0.
    pub confidence: f32,
    /// Overall node privacy classification.
    pub classification: PrivacyClass,
}
