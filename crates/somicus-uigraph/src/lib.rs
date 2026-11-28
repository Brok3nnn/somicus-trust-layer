//! Platform-neutral UI Graph types and algorithms for Somicus.
//!
//! This crate is the foundational representation of screen perception,
//! element fingerprinting, screen digest LLM serialization, and target candidate sets.

#![forbid(unsafe_code)]

pub mod candidates;
pub mod coordinates;
pub mod delta;
pub mod digest;
pub mod error;
pub mod fingerprint;
pub mod geometry;
pub mod graph;
pub mod node;
pub mod privacy;
pub mod role;

pub use candidates::{Candidate, CandidateRankingOptions, CandidateSet, extract_candidate_set};
pub use coordinates::{CoordinateTransformer, LogicalPoint, LogicalRect};
pub use delta::{NodeChange, UiGraphDelta};
pub use digest::{ScreenDigest, ScreenDigestConfig, estimate_tokens, generate_screen_digest};
pub use error::UiGraphError;
pub use fingerprint::{
    ElementFingerprint, FingerprintInput, bucket_sibling_index, compute_element_fingerprint,
    normalize_name_for_fingerprint,
};
pub use geometry::{PhysicalPoint, PhysicalRect};
pub use graph::{AppInfo, DisplayInfo, FocusInfo, UiGraph, WindowInfo};
pub use node::{NodeActions, NodeId, NodeRelations, NodeSource, NodeStates, SelectorSet, UiNode};
pub use privacy::{ClassifiedText, PrivacyClass};
pub use role::Role;
