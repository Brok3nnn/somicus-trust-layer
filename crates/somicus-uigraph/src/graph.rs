//! Top-level UI Graph container and graph algorithms.

use crate::error::UiGraphError;
use crate::fingerprint::{FingerprintInput, compute_element_fingerprint};
use crate::geometry::{PhysicalPoint, PhysicalRect};
use crate::node::{NodeId, UiNode};
use crate::privacy::ClassifiedText;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

/// Information about a connected display monitor.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DisplayInfo {
    /// Native display identifier.
    pub display_id: u32,
    /// Bounding box in physical pixels.
    pub bounds_px: PhysicalRect,
    /// Backing scale factor (e.g. 2.0 for Retina, 1.25 for 125% DPI).
    pub scale_factor: f64,
    /// Usable work area excluding taskbar/dock.
    pub work_area_px: PhysicalRect,
    /// Whether this is the system primary display.
    pub is_primary: bool,
}

/// Metadata about a running application process.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AppInfo {
    /// Internal reference string within the session.
    pub app_ref: String,
    /// Normalized application ID from knowledge pack (e.g. "`business_central`").
    pub app_id: Option<String>,
    /// Native executable process name.
    pub process_name: String,
    /// macOS bundle identifier.
    pub bundle_id: Option<String>,
    /// Windows executable path.
    pub exe_path: Option<String>,
    /// Application version string.
    pub version: Option<String>,
    /// OS process identifier (PID).
    pub pid: u32,
}

/// Metadata about a top-level or child window on screen.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WindowInfo {
    /// Internal window reference string.
    pub window_ref: String,
    /// Reference to the owning application.
    pub app_ref: String,
    /// Window title with privacy classification.
    pub title: ClassifiedText,
    /// Bounding rectangle in global physical pixels.
    pub bounds_px: PhysicalRect,
    /// Display on which this window primarily resides.
    pub display_id: u32,
    /// Stacking order (lower number = closer to front).
    pub z_order: u32,
    /// Whether this window currently has foreground focus.
    pub is_foreground: bool,
    /// Whether this window is an active modal blocker.
    pub is_modal: bool,
    /// URL if this is a web browser window.
    pub url: Option<String>,
}

/// Information about current user input focus.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct FocusInfo {
    /// Focused UI node ID.
    pub node_id: Option<NodeId>,
    /// Reference of window owning current focus.
    pub window_ref: Option<String>,
}

/// The normalized, fused UI Graph model of the on-screen state.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct UiGraph {
    /// Unique snapshot identifier.
    pub snapshot_id: String,
    /// Timestamp of capture in milliseconds since Unix epoch.
    pub captured_at_unix_ms: u64,
    /// Base snapshot ID if this is a delta update.
    pub base_snapshot_id: Option<String>,
    /// Displays present at capture time.
    pub displays: Vec<DisplayInfo>,
    /// Applications present on screen.
    pub apps: Vec<AppInfo>,
    /// Windows present on screen.
    pub windows: Vec<WindowInfo>,
    /// Current input focus.
    pub focus: FocusInfo,
    /// Complete list of normalized UI nodes.
    pub nodes: Vec<UiNode>,
}

impl UiGraph {
    /// Returns a reference to the node with the given ID.
    #[must_use]
    pub fn node(&self, id: &NodeId) -> Option<&UiNode> {
        self.nodes.iter().find(|n| &n.id == id)
    }

    /// Returns a mutable reference to the node with the given ID.
    pub fn node_mut(&mut self, id: &NodeId) -> Option<&mut UiNode> {
        self.nodes.iter_mut().find(|n| &n.id == id)
    }

    /// Returns the currently focused UI node, if known.
    #[must_use]
    pub fn focused_node(&self) -> Option<&UiNode> {
        self.focus.node_id.as_ref().and_then(|id| self.node(id))
    }

    /// Returns the active foreground window, if any.
    #[must_use]
    pub fn foreground_window(&self) -> Option<&WindowInfo> {
        self.windows.iter().find(|w| w.is_foreground)
    }

    /// Returns child nodes of the given parent node.
    #[must_use]
    pub fn children(&self, parent_id: &NodeId) -> Vec<&UiNode> {
        self.nodes
            .iter()
            .filter(|n| n.parent.as_ref() == Some(parent_id))
            .collect()
    }

    /// Returns the chain of ancestor nodes up to the root.
    #[must_use]
    pub fn ancestors(&self, node_id: &NodeId) -> Vec<&UiNode> {
        let mut result = Vec::new();
        let mut current_id = self.node(node_id).and_then(|n| n.parent.as_ref());
        let mut visited = HashSet::new();

        while let Some(parent_id) = current_id {
            if !visited.insert(parent_id.clone()) {
                break; // Cycle guard
            }
            if let Some(parent_node) = self.node(parent_id) {
                result.push(parent_node);
                current_id = parent_node.parent.as_ref();
            } else {
                break;
            }
        }
        result
    }

    /// Returns a human-readable container path for the node (e.g. `Window > Group "Payment"`).
    #[must_use]
    pub fn container_path(&self, node_id: &NodeId) -> String {
        let mut segments = Vec::new();
        for anc in self.ancestors(node_id).iter().rev() {
            if anc.role.is_container() || anc.role.is_landmark() {
                let name = anc.name.raw_text().trim();
                if name.is_empty() {
                    segments.push(anc.role.as_str().to_string());
                } else {
                    segments.push(format!("{} \"{name}\"", anc.role.as_str()));
                }
            }
        }
        segments.join(" > ")
    }

    /// Computes and assigns stable element fingerprints across all nodes.
    pub fn recompute_fingerprints(&mut self) {
        let default_app = self
            .apps
            .first()
            .and_then(|a| a.app_id.as_deref().or(Some(a.process_name.as_str())))
            .unwrap_or("app")
            .to_string();

        let node_count = self.nodes.len();
        let mut id_to_index = HashMap::with_capacity(node_count);
        for (i, node) in self.nodes.iter().enumerate() {
            id_to_index.insert(node.id.clone(), i);
        }

        for i in 0..node_count {
            self.assign_single_fingerprint(i, &default_app, &id_to_index);
        }
    }

    fn assign_single_fingerprint(
        &mut self,
        i: usize,
        default_app: &str,
        id_to_index: &HashMap<NodeId, usize>,
    ) {
        let (role, name, stable_attrs, sibling_index) = {
            let node = &self.nodes[i];
            let mut attrs = Vec::new();
            if let Some(ref s) = node.selectors.automation_id {
                attrs.push(s.as_str());
            }
            if let Some(ref s) = node.selectors.dom_id {
                attrs.push(s.as_str());
            }
            if let Some(ref s) = node.selectors.data_testid {
                attrs.push(s.as_str());
            }
            if let Some(ref s) = node.selectors.dom_name {
                attrs.push(s.as_str());
            }

            let sib_idx = match node.parent {
                Some(ref pid) => id_to_index.get(pid).map_or(0, |&p_idx| {
                    self.nodes[p_idx]
                        .children
                        .iter()
                        .position(|cid| cid == &node.id)
                        .unwrap_or(0)
                }),
                None => i,
            };

            (node.role, node.name.raw_text().to_string(), attrs, sib_idx)
        };

        let ancestor_landmarks = self.collect_ancestor_landmarks(i, id_to_index);
        let anc_refs: Vec<(&str, &str)> = ancestor_landmarks
            .iter()
            .map(|(r, n)| (*r, n.as_str()))
            .collect();

        let input = FingerprintInput {
            app_id: default_app,
            role,
            accessible_name: &name,
            stable_attrs: &stable_attrs,
            ancestor_landmarks: &anc_refs,
            sibling_index,
        };

        self.nodes[i].fp = compute_element_fingerprint(&input);
    }

    fn collect_ancestor_landmarks(
        &self,
        node_idx: usize,
        id_to_index: &HashMap<NodeId, usize>,
    ) -> Vec<(&'static str, String)> {
        let mut ancestor_landmarks = Vec::new();
        let mut curr = self.nodes[node_idx].parent.clone();
        let mut visited = HashSet::new();

        while let Some(ref pid) = curr {
            if !visited.insert(pid.clone()) {
                break;
            }
            if let Some(&p_idx) = id_to_index.get(pid) {
                let parent_node = &self.nodes[p_idx];
                if parent_node.role.is_landmark() || parent_node.role.is_container() {
                    ancestor_landmarks.push((
                        parent_node.role.as_str(),
                        parent_node.name.raw_text().to_string(),
                    ));
                    if ancestor_landmarks.len() >= 4 {
                        break;
                    }
                }
                curr.clone_from(&parent_node.parent);
            } else {
                break;
            }
        }
        ancestor_landmarks
    }

    /// Performs hit-testing to find the topmost interactive or salient node at the point.
    #[must_use]
    pub fn hit_test(&self, point: PhysicalPoint) -> Option<&UiNode> {
        let mut candidates: Vec<&UiNode> = self
            .nodes
            .iter()
            .filter(|n| n.bounds.is_some_and(|b| b.contains_point(point)))
            .collect();

        candidates.sort_by(|a, b| {
            let a_inter = a.role.is_interactive();
            let b_inter = b.role.is_interactive();
            b_inter.cmp(&a_inter).then_with(|| {
                let a_area = a.bounds.map_or(u64::MAX, |r| r.area());
                let b_area = b.bounds.map_or(u64::MAX, |r| r.area());
                a_area.cmp(&b_area)
            })
        });

        candidates.first().copied()
    }

    /// Validates graph consistency (unique IDs, acyclic structure).
    ///
    /// # Errors
    /// Returns `UiGraphError` if duplicates, missing parents, or cycles are found.
    pub fn validate(&self) -> Result<(), UiGraphError> {
        let mut seen_ids = HashSet::new();
        for node in &self.nodes {
            if !seen_ids.insert(node.id.clone()) {
                return Err(UiGraphError::DuplicateNodeId(node.id.0.clone()));
            }
        }

        for node in &self.nodes {
            if let Some(ref pid) = node.parent
                && !seen_ids.contains(pid)
            {
                return Err(UiGraphError::NodeNotFound(pid.0.clone()));
            }

            let mut visited = HashSet::new();
            visited.insert(node.id.clone());
            let mut curr = node.parent.clone();
            while let Some(pid) = curr {
                if !visited.insert(pid.clone()) {
                    return Err(UiGraphError::CycleDetected(node.id.0.clone()));
                }
                curr = self.node(&pid).and_then(|n| n.parent.clone());
            }
        }

        Ok(())
    }
}
