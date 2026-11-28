//! Screen Digest generation for LLM serialization.
//!
//! Compact, privacy-safe, token-budgeted text representation of the UI Graph.

use crate::error::UiGraphError;
use crate::graph::UiGraph;
use crate::node::{NodeId, UiNode};
use crate::role::Role;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::fmt::Write;

/// Configuration options for Screen Digest serialization.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ScreenDigestConfig {
    /// Maximum token budget for the serialized digest (default: 2,500 tokens).
    pub max_token_budget: usize,
    /// BCP-47 locale tag (e.g. "en-US", "de-DE").
    pub locale: Option<String>,
    /// Matched knowledge pack screen name and confidence score.
    pub pack_screen: Option<(String, f32)>,
    /// Summary of recent user action for contextual grounding.
    pub recent_action: Option<String>,
}

impl Default for ScreenDigestConfig {
    fn default() -> Self {
        Self {
            max_token_budget: 2500,
            locale: Some("en-US".to_string()),
            pack_screen: None,
            recent_action: None,
        }
    }
}

/// A serialized screen digest ready to be included in agent prompt payloads.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScreenDigest {
    /// The serialized text content.
    pub text: String,
    /// Approximate token count (~4 characters per token heuristic).
    pub estimated_tokens: usize,
    /// Total nodes serialized in the digest.
    pub included_nodes: usize,
    /// Number of nodes pruned to satisfy the token budget.
    pub pruned_nodes: usize,
}

/// Estimates token count using standard GPT heuristic (~4 characters per token, minimum 1 per word).
#[must_use]
pub fn estimate_tokens(text: &str) -> usize {
    if text.is_empty() {
        return 0;
    }
    let chars = text.chars().count();
    let words = text.split_whitespace().count();
    chars.div_ceil(4).max(words)
}

/// Generates a privacy-safe, token-budgeted Screen Digest from a UI Graph.
///
/// # Errors
/// Returns `UiGraphError::InvalidTokenBudget` if the budget is 0.
pub fn generate_screen_digest(
    graph: &UiGraph,
    config: &ScreenDigestConfig,
) -> Result<ScreenDigest, UiGraphError> {
    if config.max_token_budget == 0 {
        return Err(UiGraphError::InvalidTokenBudget(0));
    }

    let mut out = String::with_capacity(4096);
    render_header(graph, config, &mut out);

    let focused_id = graph.focus.node_id.as_ref();
    let mut node_scores = score_nodes_for_budget(graph, focused_id);
    let mut included_ids: HashSet<NodeId> = graph.nodes.iter().map(|n| n.id.clone()).collect();
    let total_nodes = graph.nodes.len();

    let rendered_tree = render_tree(graph, &included_ids, 1);
    let mut candidate_text = format!("{out}{rendered_tree}");
    append_footer(&mut candidate_text, graph, config);

    let mut pruned_count = 0;
    while estimate_tokens(&candidate_text) > config.max_token_budget && !node_scores.is_empty() {
        if let Some((prune_id, _)) = node_scores.pop()
            && Some(&prune_id) != focused_id
        {
            included_ids.remove(&prune_id);
            pruned_count += 1;
        }

        let trimmed_tree = render_tree(graph, &included_ids, 1);
        candidate_text = format!("{out}{trimmed_tree}");
        if pruned_count > 0 {
            let _ = writeln!(candidate_text, "  ... ({pruned_count} elements collapsed)");
        }
        append_footer(&mut candidate_text, graph, config);
    }

    let estimated = estimate_tokens(&candidate_text);
    Ok(ScreenDigest {
        text: candidate_text,
        estimated_tokens: estimated,
        included_nodes: total_nodes.saturating_sub(pruned_count),
        pruned_nodes: pruned_count,
    })
}

fn render_header(graph: &UiGraph, config: &ScreenDigestConfig, out: &mut String) {
    let app_name = graph
        .apps
        .first()
        .map_or("Unknown App", |a| a.process_name.as_str());
    let locale_str = config.locale.as_deref().unwrap_or("en-US");

    let screen_str = match &config.pack_screen {
        Some((name, score)) => format!(" · SCREEN {name} (pack match {score:.2})"),
        None => String::new(),
    };
    let _ = writeln!(out, "APP {app_name}{screen_str} · LOCALE {locale_str}");

    for win in &graph.windows {
        let fg_tag = if win.is_foreground {
            " [foreground]"
        } else {
            ""
        };
        let modal_tag = if win.is_modal { " [modal]" } else { "" };
        let _ = writeln!(
            out,
            "WINDOW {} \"{}\"{fg_tag}{modal_tag}",
            win.window_ref,
            win.title.digest_text()
        );
    }
}

fn score_nodes_for_budget(graph: &UiGraph, focused_id: Option<&NodeId>) -> Vec<(NodeId, f32)> {
    let mut node_scores: Vec<(NodeId, f32)> = graph
        .nodes
        .iter()
        .map(|node| {
            let mut score = 10.0;
            if node.role.is_interactive() {
                score += 50.0;
            }
            if node.role.is_landmark() {
                score += 30.0;
            }
            if !node.name.raw_text().trim().is_empty() {
                score += 20.0;
            }
            if Some(&node.id) == focused_id {
                score += 100.0;
            }
            if node.states.focused || node.states.selected {
                score += 25.0;
            }
            if node.states.offscreen {
                score -= 40.0;
            }
            (node.id.clone(), score)
        })
        .collect();

    node_scores.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    node_scores
}

fn append_footer(out: &mut String, graph: &UiGraph, config: &ScreenDigestConfig) {
    if let Some(ref foc) = graph.focus.node_id {
        let _ = writeln!(out, "FOCUS {}", foc.0);
    }
    if let Some(ref act) = config.recent_action {
        let _ = writeln!(out, "RECENT {act}");
    }
}

struct RenderContext<'a> {
    graph: &'a UiGraph,
    included: &'a HashSet<NodeId>,
    out: &'a mut String,
}

fn render_tree(graph: &UiGraph, included: &HashSet<NodeId>, indent_level: usize) -> String {
    let mut res = String::new();
    let roots: Vec<&UiNode> = graph
        .nodes
        .iter()
        .filter(|n| n.parent.as_ref().is_none_or(|p| !included.contains(p)))
        .collect();

    let mut ctx = RenderContext {
        graph,
        included,
        out: &mut res,
    };

    for root in roots {
        render_node_dfs(&mut ctx, root, indent_level);
    }
    res
}

fn render_node_dfs(ctx: &mut RenderContext<'_>, node: &UiNode, indent: usize) {
    if !ctx.included.contains(&node.id) {
        return;
    }

    let pad = "  ".repeat(indent);
    let role_str = node.role.as_str();
    let name_str = node.name.digest_text();

    let mut line = format!("{pad}[{}] {role_str}", node.id.0);
    if !name_str.is_empty() {
        let _ = write!(line, " \"{name_str}\"");
    }

    if node.states.selected {
        line.push_str(" (selected)");
    }
    if node.states.checked {
        line.push_str(" (checked)");
    }
    if node.states.disabled() {
        line.push_str(" (disabled)");
    }

    if let Some(ref val) = node.value {
        let val_digest = val.digest_text();
        if !val_digest.is_empty() {
            let _ = write!(line, " value={val_digest}");
        }
    }

    if (node.role == Role::Table || node.role == Role::Grid) && !node.children.is_empty() {
        let _ = write!(line, " rows={}", node.children.len());
    }

    line.push('\n');
    ctx.out.push_str(&line);

    for child_id in &node.children {
        if let Some(child_node) = ctx.graph.node(child_id) {
            render_node_dfs(ctx, child_node, indent + 1);
        }
    }
}

impl crate::node::NodeStates {
    const fn disabled(&self) -> bool {
        !self.enabled
    }
}
