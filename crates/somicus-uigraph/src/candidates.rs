//! Actionable candidate ranking for `decide.target` decision routes.
//!
//! Emits top-K (default 32, max 64) deterministic candidates plus the mandatory `none_visible` option.

use crate::graph::UiGraph;
use crate::node::{NodeId, UiNode};
use crate::role::Role;
use serde::{Deserialize, Serialize};

/// An actionable UI target candidate.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Candidate {
    /// Target node ID ("e42" or "`none_visible`").
    pub id: NodeId,
    /// Normalized element role.
    pub role: Role,
    /// Accessible name (public label or masked).
    pub name: String,
    /// Hierarchical container breadcrumbs.
    pub container_path: String,
    /// Human-readable one-line description for model context.
    pub description: String,
    /// Deterministic relevance score.
    pub score: f32,
}

/// The set of actionable candidates formatted for `decide.target`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CandidateSet {
    /// Ordered list of top actionable candidates (highest score first).
    pub candidates: Vec<Candidate>,
    /// Fallback choice when the user's intent cannot be fulfilled on the current screen.
    pub none_visible: Candidate,
}

impl CandidateSet {
    /// Returns all options including `none_visible` as the final choice.
    #[must_use]
    pub fn all_choices(&self) -> Vec<&Candidate> {
        let mut choices: Vec<&Candidate> = self.candidates.iter().collect();
        choices.push(&self.none_visible);
        choices
    }

    /// Looks up a candidate by its string ID.
    #[must_use]
    pub fn find_candidate(&self, id: &str) -> Option<&Candidate> {
        if id == "none_visible" {
            Some(&self.none_visible)
        } else {
            self.candidates.iter().find(|c| c.id.as_str() == id)
        }
    }
}

/// Options to guide candidate ranking.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CandidateRankingOptions {
    /// User's natural language goal or instruction.
    pub goal_text: Option<String>,
    /// Maximum candidates to return (default: 32, capped at 64).
    pub max_candidates: usize,
}

impl Default for CandidateRankingOptions {
    fn default() -> Self {
        Self {
            goal_text: None,
            max_candidates: 32,
        }
    }
}

/// Ranks and produces the candidate set for `decide.target`.
#[must_use]
pub fn extract_candidate_set(graph: &UiGraph, options: &CandidateRankingOptions) -> CandidateSet {
    let limit = options.max_candidates.clamp(1, 64);
    let goal_words: Vec<String> = options
        .goal_text
        .as_deref()
        .unwrap_or("")
        .split_whitespace()
        .map(|w| w.trim().to_lowercase())
        .filter(|w| w.len() >= 2)
        .collect();

    let mut scored: Vec<Candidate> = graph
        .nodes
        .iter()
        .filter(|node| is_actionable(node))
        .map(|node| {
            let container = graph.container_path(&node.id);
            let score = score_candidate(node, &container, &goal_words, graph);
            let desc = format_description(node);

            Candidate {
                id: node.id.clone(),
                role: node.role,
                name: node.name.digest_text(),
                container_path: container,
                description: desc,
                score,
            }
        })
        .collect();

    // Deterministic sort: score descending, then container_path asc, role asc, name asc, id asc
    scored.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.container_path.cmp(&b.container_path))
            .then_with(|| a.role.as_str().cmp(b.role.as_str()))
            .then_with(|| a.name.cmp(&b.name))
            .then_with(|| a.id.0.cmp(&b.id.0))
    });

    scored.truncate(limit);

    let none_visible = Candidate {
        id: NodeId::none_visible(),
        role: Role::Unknown,
        name: "none_visible".to_string(),
        container_path: String::new(),
        description: "Target element is not visible or not present on screen".to_string(),
        score: 0.0,
    };

    CandidateSet {
        candidates: scored,
        none_visible,
    }
}

fn is_actionable(node: &UiNode) -> bool {
    node.role.is_interactive()
        || node.actions.invoke
        || node.actions.select
        || node.actions.toggle
        || node.actions.set_value
        || node.actions.expand
        || node.actions.collapse
}

fn score_candidate(node: &UiNode, container: &str, goal_words: &[String], graph: &UiGraph) -> f32 {
    let mut score = match node.role {
        Role::Button | Role::SplitButton => 60.0,
        Role::TextField | Role::TextArea | Role::SearchField | Role::ComboBox => 55.0,
        Role::Link | Role::Tab | Role::MenuItem => 50.0,
        Role::CheckBox | Role::RadioButton | Role::Switch => 45.0,
        Role::Option | Role::TreeItem | Role::ListItem | Role::Cell => 35.0,
        _ => 20.0,
    };

    // Keyword matching bonuses
    let name_lower = node.name.raw_text().to_lowercase();
    let container_lower = container.to_lowercase();
    for word in goal_words {
        if name_lower.contains(word) {
            score += 100.0;
        } else if container_lower.contains(word) {
            score += 30.0;
        }
    }

    // State adjustments
    if node.states.enabled {
        score += 20.0;
    } else {
        score -= 50.0;
    }

    if node.states.focused {
        score += 40.0;
    }

    if node.states.offscreen {
        score -= 60.0;
    }

    // Proximity to focused window
    if let Some(fg) = graph.foreground_window()
        && node
            .bounds
            .is_some_and(|b| fg.bounds_px.intersection(&b).is_some())
    {
        score += 25.0;
    }

    // Apply confidence weight
    (score * node.confidence.clamp(0.1, 1.0)).max(0.0)
}

fn format_description(node: &UiNode) -> String {
    let mut parts = Vec::new();
    parts.push(node.role.as_str());

    let name = node.name.digest_text();
    if !name.is_empty() {
        parts.push(&name);
    }

    let mut state_tags = Vec::new();
    if node.states.focused {
        state_tags.push("focused");
    }
    if node.states.selected {
        state_tags.push("selected");
    }
    if !node.states.enabled {
        state_tags.push("disabled");
    }

    if state_tags.is_empty() {
        parts.join(" ")
    } else {
        format!("{} [{}]", parts.join(" "), state_tags.join(", "))
    }
}
