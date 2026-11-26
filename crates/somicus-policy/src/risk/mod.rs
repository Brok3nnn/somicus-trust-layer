//! Action risk classification, multilingual lexicon, and consequence notes.
//!
//! Implements local risk classification per:
//! - ADR-0013 (`docs/adr/ADR-0013-autonomy-levels-and-risk-classes.md`).
//! - Conservative bias: model scores can raise risk, never lower it below local heuristics.
//! - Multilingual label lexicon: EN, DE, HU, FR, ES, kept in `lexicon`.

mod lexicon;

use serde::{Deserialize, Serialize};

use lexicon::{is_commit_keyword, is_critical_keyword, is_edit_keyword, is_navigate_keyword};

/// Action risk classes per Somicus Interaction Model §3.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RiskClass {
    /// R0: Observe (read-only, inspect, highlight, no side effects).
    R0 = 0,
    /// R1: Navigate (switch tab, scroll, expand section, cursor positioning).
    R1 = 1,
    /// R2: Edit (type text, select dropdown, check box, reversible edits).
    R2 = 2,
    /// R3: Commit (send email, delete item, post transaction, submit form).
    R3 = 3,
    /// R4: Critical (delete account, bulk drop, money transfer above limit).
    R4 = 4,
    /// R5: Prohibited (hard invariant: security bypass, unauthorized access).
    R5 = 5,
}

impl RiskClass {
    /// Returns the human-readable identifier for this risk class.
    #[must_use]
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::R0 => "R0",
            Self::R1 => "R1",
            Self::R2 => "R2",
            Self::R3 => "R3",
            Self::R4 => "R4",
            Self::R5 => "R5",
        }
    }

    /// Whether this risk class requires physical user confirmation. A commit (R3) and an irreversible step (R4) are
    /// confirmed at every autonomy level; R5 never runs at all.
    #[must_use]
    pub const fn requires_physical_confirmation(&self) -> bool {
        matches!(self, Self::R3 | Self::R4 | Self::R5)
    }
}

/// Autonomy levels per Somicus Interaction Model §2.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AutonomyLevel {
    /// L0: Answer only (coaching in chat, no pointer).
    L0 = 0,
    /// L1: Show (ghost cursor points, Step Card explains, user acts).
    L1 = 1,
    /// L2: Assist (user requests "Do this step", Somicus executes).
    L2 = 2,
    /// L3: Autopilot (Somicus executes multi-step plans with checkpoints).
    L3 = 3,
}

impl AutonomyLevel {
    /// Returns human-readable label.
    #[must_use]
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::L0 => "L0",
            Self::L1 => "L1",
            Self::L2 => "L2",
            Self::L3 => "L3",
        }
    }
}

/// Context for classifying an action candidate.
#[derive(Debug, Clone, Copy)]
pub struct ActionContext<'a> {
    /// Label or accessible name of the target UI element.
    pub label: &'a str,
    /// Semantic accessibility role (e.g. "button", "link", "textbox").
    pub role: &'a str,
    /// Whether the action is flagged as destructive (e.g. danger button styling).
    pub is_destructive: bool,
    /// Whether the action applies in bulk (e.g. "all items", multiple selection).
    pub is_bulk: bool,
}

impl<'a> ActionContext<'a> {
    /// Constructs a new `ActionContext`.
    #[must_use]
    pub const fn new(label: &'a str, role: &'a str) -> Self {
        Self {
            label,
            role,
            is_destructive: false,
            is_bulk: false,
        }
    }

    /// Sets whether the action is destructive.
    #[must_use]
    pub const fn with_destructive(mut self, destructive: bool) -> Self {
        self.is_destructive = destructive;
        self
    }

    /// Sets whether the action applies in bulk.
    #[must_use]
    pub const fn with_bulk(mut self, bulk: bool) -> Self {
        self.is_bulk = bulk;
        self
    }
}

/// Local risk classifier implementing heuristic evaluation and multilingual lexicon.
#[derive(Debug, Default, Clone)]
pub struct RiskClassifier;

impl RiskClassifier {
    /// Constructs a new `RiskClassifier`.
    #[must_use]
    pub fn new() -> Self {
        Self
    }

    /// Classifies an action candidate based on element label, role, and destructive hints.
    #[must_use]
    pub fn classify_action(&self, context: &ActionContext<'_>) -> RiskClass {
        let normalized = context.label.trim().to_lowercase();

        // 1. Critical actions (R4)
        if is_critical_keyword(&normalized) || (context.is_destructive && context.is_bulk) {
            return RiskClass::R4;
        }

        // 2. Commit / Irreversible actions (R3)
        if context.is_destructive || is_commit_keyword(&normalized) {
            return if context.is_bulk {
                RiskClass::R4
            } else {
                RiskClass::R3
            };
        }

        // 3. Edit actions (R2)
        if is_edit_keyword(&normalized)
            || matches!(context.role, "textbox" | "combobox" | "checkbox" | "radio")
        {
            return RiskClass::R2;
        }

        // 4. Navigation actions (R1)
        if is_navigate_keyword(&normalized)
            || matches!(context.role, "link" | "tab" | "menuitem" | "treeitem")
        {
            return RiskClass::R1;
        }

        // Default for interactive buttons without commit terms
        if context.role == "button" {
            RiskClass::R1
        } else {
            RiskClass::R0
        }
    }

    /// Combines local heuristic classification with a model-proposed risk class.
    ///
    /// Per Interaction Model §3.4:
    /// "Conservative bias: when uncertain, the next higher class. The model can
    /// only raise the class, never lower it below local heuristics."
    #[must_use]
    pub fn combine_with_model(&self, local: RiskClass, model: RiskClass) -> RiskClass {
        local.max(model)
    }
}
