//! Minimal delta patching and change detection for UI Graphs.
//!
//! Emits minimal patches containing only added nodes, removed node IDs,
//! and modified fields for existing nodes.

use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

use crate::error::UiGraphError;
use crate::geometry::PhysicalRect;
use crate::graph::UiGraph;
use crate::node::{NodeActions, NodeId, NodeStates, UiNode};
use crate::privacy::ClassifiedText;

/// Granular changes to a single UI node's mutable properties.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NodeChange {
    /// ID of the node being modified.
    pub id: NodeId,
    /// Bounding rectangle update, if changed.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bounds: Option<Option<PhysicalRect>>,
    /// State bitset update, if changed.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub states: Option<NodeStates>,
    /// Accessible name update, if changed.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<ClassifiedText>,
    /// Value update, if changed.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub value: Option<Option<ClassifiedText>>,
    /// Actions update, if changed.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub actions: Option<NodeActions>,
    /// Confidence update, if changed.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub confidence: Option<f32>,
}

impl NodeChange {
    /// Returns true if this change record contains at least one modified field.
    #[must_use]
    pub const fn has_changes(&self) -> bool {
        self.bounds.is_some()
            || self.states.is_some()
            || self.name.is_some()
            || self.value.is_some()
            || self.actions.is_some()
            || self.confidence.is_some()
    }
}

/// A compact structural and property diff between two UI Graph snapshots.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct UiGraphDelta {
    /// Identifier of the base snapshot this delta applies to.
    pub base_snapshot_id: String,
    /// Identifier of the resulting snapshot after applying this delta.
    pub snapshot_id: String,
    /// Capture timestamp of the new snapshot in unix epoch ms.
    pub captured_at_unix_ms: u64,
    /// Newly appeared UI nodes.
    pub added: Vec<UiNode>,
    /// IDs of UI nodes that were destroyed or disappeared.
    pub removed: Vec<NodeId>,
    /// Nodes that persisted but had property or bounds changes.
    pub changed: Vec<NodeChange>,
}

impl UiGraphDelta {
    /// Returns true if this delta represents no modifications (empty patch).
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.added.is_empty() && self.removed.is_empty() && self.changed.is_empty()
    }

    /// Returns the total number of element patch operations in this delta.
    #[must_use]
    pub fn patch_count(&self) -> usize {
        self.added.len() + self.removed.len() + self.changed.len()
    }
}

impl UiGraph {
    /// Computes a minimal `UiGraphDelta` from `self` to `next`.
    ///
    /// Preserves existing node identities, computes added and removed nodes,
    /// and emits field-level changes only where values actually differ.
    #[must_use]
    pub fn diff(&self, next: &Self) -> UiGraphDelta {
        let prev_map: HashMap<&NodeId, &UiNode> = self.nodes.iter().map(|n| (&n.id, n)).collect();
        let next_ids: HashSet<&NodeId> = next.nodes.iter().map(|n| &n.id).collect();

        let mut added = Vec::new();
        let mut changed = Vec::new();

        for next_node in &next.nodes {
            if let Some(prev_node) = prev_map.get(&next_node.id) {
                if let Some(change) = Self::compute_node_change(prev_node, next_node) {
                    changed.push(change);
                }
            } else {
                added.push(next_node.clone());
            }
        }

        let mut removed = Vec::new();
        for prev_node in &self.nodes {
            if !next_ids.contains(&prev_node.id) {
                removed.push(prev_node.id.clone());
            }
        }

        UiGraphDelta {
            base_snapshot_id: self.snapshot_id.clone(),
            snapshot_id: next.snapshot_id.clone(),
            captured_at_unix_ms: next.captured_at_unix_ms,
            added,
            removed,
            changed,
        }
    }

    fn compute_node_change(prev: &UiNode, next: &UiNode) -> Option<NodeChange> {
        let bounds = if prev.bounds == next.bounds {
            None
        } else {
            Some(next.bounds)
        };

        let states = if prev.states == next.states {
            None
        } else {
            Some(next.states)
        };

        let name = if prev.name == next.name {
            None
        } else {
            Some(next.name.clone())
        };

        let value = if prev.value == next.value {
            None
        } else {
            Some(next.value.clone())
        };

        let actions = if prev.actions == next.actions {
            None
        } else {
            Some(next.actions)
        };

        let confidence = if (prev.confidence - next.confidence).abs() < f32::EPSILON {
            None
        } else {
            Some(next.confidence)
        };

        let change = NodeChange {
            id: next.id.clone(),
            bounds,
            states,
            name,
            value,
            actions,
            confidence,
        };

        if change.has_changes() {
            Some(change)
        } else {
            None
        }
    }

    /// Applies a delta update to this graph to produce the next graph.
    ///
    /// # Errors
    /// Returns `UiGraphError::Serialization` if the delta's base snapshot ID does not match.
    pub fn apply_delta(&self, delta: &UiGraphDelta) -> Result<Self, UiGraphError> {
        if delta.base_snapshot_id != self.snapshot_id {
            return Err(UiGraphError::Serialization(format!(
                "mismatched delta base: expected {}, got {}",
                self.snapshot_id, delta.base_snapshot_id
            )));
        }

        let removed_set: HashSet<&NodeId> = delta.removed.iter().collect();
        let change_map: HashMap<&NodeId, &NodeChange> =
            delta.changed.iter().map(|c| (&c.id, c)).collect();

        let mut new_nodes = Vec::with_capacity(self.nodes.len() + delta.added.len());

        for mut node in self.nodes.clone() {
            if removed_set.contains(&node.id) {
                continue;
            }

            if let Some(change) = change_map.get(&node.id) {
                if let Some(b) = change.bounds {
                    node.bounds = b;
                }
                if let Some(s) = change.states {
                    node.states = s;
                }
                if let Some(ref n) = change.name {
                    node.name.clone_from(n);
                }
                if let Some(ref v) = change.value {
                    node.value.clone_from(v);
                }
                if let Some(a) = change.actions {
                    node.actions = a;
                }
                if let Some(c) = change.confidence {
                    node.confidence = c;
                }
            }

            new_nodes.push(node);
        }

        new_nodes.extend(delta.added.clone());

        let mut new_graph = Self {
            snapshot_id: delta.snapshot_id.clone(),
            captured_at_unix_ms: delta.captured_at_unix_ms,
            base_snapshot_id: Some(delta.base_snapshot_id.clone()),
            displays: self.displays.clone(),
            apps: self.apps.clone(),
            windows: self.windows.clone(),
            focus: self.focus.clone(),
            nodes: new_nodes,
        };

        new_graph.recompute_fingerprints();
        new_graph.validate()?;
        Ok(new_graph)
    }
}
