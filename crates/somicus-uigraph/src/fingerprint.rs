//! Stable element fingerprinting for UI nodes (Perception §6).
//!
//! An `ElementFingerprint` is a 64-bit hash (`xxh3_64`) over a canonical tuple:
//! `(app_id, role, normalized_name, stable_attrs, ancestor_signature, sibling_bucket)`.
//! Values are strictly excluded (privacy), and names are normalized (case-folded,
//! whitespace collapsed, digits masked for dynamic labels).

use crate::role::Role;
use serde::{Deserialize, Serialize};
use std::fmt;
use std::str::FromStr;
use twox_hash::XxHash3_64;

/// 64-bit element fingerprint stable across snapshots and sessions.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize, Default,
)]
pub struct ElementFingerprint(pub u64);

impl ElementFingerprint {
    /// Creates a fingerprint from a raw 64-bit integer.
    #[must_use]
    pub const fn new(hash: u64) -> Self {
        Self(hash)
    }

    /// Returns the raw 64-bit integer.
    #[must_use]
    pub const fn as_u64(self) -> u64 {
        self.0
    }

    /// Returns the 16-character hexadecimal representation.
    #[must_use]
    pub fn to_hex(self) -> String {
        format!("{:016x}", self.0)
    }

    /// Parses a hexadecimal string into an `ElementFingerprint`.
    ///
    /// # Errors
    /// Returns an error if the string is not valid hex.
    pub fn from_hex(s: &str) -> Result<Self, std::num::ParseIntError> {
        let cleaned = s.strip_prefix("0x").unwrap_or(s);
        let val = u64::from_str_radix(cleaned, 16)?;
        Ok(Self(val))
    }
}

impl fmt::Display for ElementFingerprint {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:016x}", self.0)
    }
}

impl FromStr for ElementFingerprint {
    type Err = std::num::ParseIntError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::from_hex(s)
    }
}

/// Normalizes accessible text for fingerprinting.
///
/// 1. Lowercases the string.
/// 2. Collapses consecutive whitespace to a single space and trims.
/// 3. Replaces all digits [0-9] with '#' to avoid thrashing on dynamic IDs or dates.
#[must_use]
pub fn normalize_name_for_fingerprint(text: &str) -> String {
    let lower = text.trim().to_lowercase();
    let mut normalized = String::with_capacity(lower.len());
    let mut last_was_whitespace = false;

    for ch in lower.chars() {
        if ch.is_whitespace() {
            if !last_was_whitespace {
                normalized.push(' ');
                last_was_whitespace = true;
            }
        } else if ch.is_ascii_digit() {
            normalized.push('#');
            last_was_whitespace = false;
        } else {
            normalized.push(ch);
            last_was_whitespace = false;
        }
    }

    normalized
}

/// Buckets a sibling index to make fingerprints resilient to small insertions/deletions.
#[must_use]
pub const fn bucket_sibling_index(index: usize) -> u8 {
    match index {
        0 => 0,
        1 => 1,
        2 => 2,
        3..=5 => 3,
        6..=10 => 6,
        _ => 11,
    }
}

/// Canonical input tuple for computing an element fingerprint.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FingerprintInput<'a> {
    /// Normalized application process identifier.
    pub app_id: &'a str,
    /// Normalized element role.
    pub role: Role,
    /// Accessible name (unredacted label or placeholder).
    pub accessible_name: &'a str,
    /// Stable native attribute identifiers (e.g. `automation_id`, `data-testid`).
    pub stable_attrs: &'a [&'a str],
    /// Up to 4 landmark ancestors (role name and accessible name).
    pub ancestor_landmarks: &'a [(&'a str, &'a str)],
    /// Index among siblings of the same parent.
    pub sibling_index: usize,
}

/// Computes the stable 64-bit element fingerprint from the canonical tuple components.
#[must_use]
pub fn compute_element_fingerprint(input: &FingerprintInput<'_>) -> ElementFingerprint {
    let norm_name = normalize_name_for_fingerprint(input.accessible_name);
    let sibling_bucket = bucket_sibling_index(input.sibling_index);

    // Format stable attributes: filter non-empty and join
    let mut stable_joined = String::new();
    for (i, attr) in input
        .stable_attrs
        .iter()
        .filter(|s| !s.trim().is_empty())
        .enumerate()
    {
        if i > 0 {
            stable_joined.push('|');
        }
        stable_joined.push_str(attr.trim());
    }

    // Format ancestor signature: up to 4 ancestors
    let mut ancestor_joined = String::new();
    for (i, (anc_role, anc_name)) in input.ancestor_landmarks.iter().take(4).enumerate() {
        if i > 0 {
            ancestor_joined.push(';');
        }
        ancestor_joined.push_str(anc_role);
        ancestor_joined.push(':');
        ancestor_joined.push_str(&normalize_name_for_fingerprint(anc_name));
    }

    // Build canonical UTF-8 tuple with ASCII Unit Separator (\x1f)
    let canonical = format!(
        "{}\x1f{}\x1f{}\x1f{}\x1f{}\x1f{}",
        input.app_id.trim().to_lowercase(),
        input.role.as_str(),
        norm_name,
        stable_joined,
        ancestor_joined,
        sibling_bucket
    );

    let hash = XxHash3_64::oneshot(canonical.as_bytes());
    ElementFingerprint(hash)
}
