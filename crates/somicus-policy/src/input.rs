//! Input event representations and source classifications.

use std::fmt;
use std::time::Instant;

use serde::{Deserialize, Serialize};
use somicus_uigraph::geometry::PhysicalPoint;

use crate::error::IntentError;

/// Canonical SHA-256 action hash binding an exact user action to confirmation.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ActionHash(String);

impl ActionHash {
    /// Constructs and validates a new action hash.
    ///
    /// # Errors
    /// Returns `IntentError::InvalidActionHash` if the string is empty or exceeds 128 characters.
    pub fn new(hash: impl Into<String>) -> Result<Self, IntentError> {
        let s = hash.into();
        let trimmed = s.trim();
        if trimmed.is_empty() {
            return Err(IntentError::InvalidActionHash(
                "action hash cannot be empty".to_string(),
            ));
        }
        if trimmed.len() > 128 {
            return Err(IntentError::InvalidActionHash(
                "action hash exceeds maximum length of 128 characters".to_string(),
            ));
        }
        if !trimmed.chars().all(|c| c.is_ascii_hexdigit()) {
            return Err(IntentError::InvalidActionHash(
                "action hash must contain only hexadecimal digits".to_string(),
            ));
        }
        Ok(Self(trimmed.to_ascii_lowercase()))
    }

    /// Returns the action hash as a string slice.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for ActionHash {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Provenance of an observed input event.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum InputSource {
    /// Genuine hardware input originating from an OS HID device (keyboard, mouse, trackpad).
    Physical,
    /// Injected, programmatic, or synthetic event (e.g. Windows `SendInput`, macOS `CGEventPost`).
    Injected {
        /// Low-level OS injection flags or source classification identifier.
        flags: u32,
    },
}

impl InputSource {
    /// Returns `true` if the input is verified physical hardware input.
    #[must_use]
    pub const fn is_physical(&self) -> bool {
        matches!(self, Self::Physical)
    }

    /// Returns `true` if the input was synthesized or injected.
    #[must_use]
    pub const fn is_injected(&self) -> bool {
        matches!(self, Self::Injected { .. })
    }
}

/// Keys the core may observe around a confirmation. Only [`ConfirmationKey::Hotkey`] confirms; Return and Space are
/// observed so a step can move on, but they never approve an action.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ConfirmationKey {
    /// Return / Enter key: never confirms.
    Enter,
    /// Spacebar: never confirms.
    Space,
    /// Dedicated hardware confirmation hotkey chord: the only key that confirms.
    Hotkey,
    /// Escape key (cancels or dismisses confirmation).
    Escape,
    /// Any other non-confirming key code.
    Other(u32),
}

impl ConfirmationKey {
    /// Returns `true` if this key signals confirmation approval: only the dedicated hotkey does.
    #[must_use]
    pub const fn is_confirm(&self) -> bool {
        matches!(self, Self::Hotkey)
    }

    /// Returns `true` if this key signals rejection or cancellation.
    #[must_use]
    pub const fn is_cancel(&self) -> bool {
        matches!(self, Self::Escape)
    }
}

/// The specific payload of an observed user input event.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InputEventKind {
    /// Mouse click event at a specific physical screen point.
    Click(PhysicalPoint),
    /// Physical key press event.
    KeyPress(ConfirmationKey),
}

/// A low-level input event observed by the core trust anchor.
#[derive(Debug, Clone)]
pub struct ObservedInput {
    /// The event payload (click or keypress).
    pub kind: InputEventKind,
    /// Whether the event was physical or injected.
    pub source: InputSource,
    /// Exact monotonic timestamp when the event was recorded.
    pub timestamp: Instant,
}

impl ObservedInput {
    /// Constructs a physical mouse click event.
    #[must_use]
    pub fn physical_click(point: PhysicalPoint, timestamp: Instant) -> Self {
        Self {
            kind: InputEventKind::Click(point),
            source: InputSource::Physical,
            timestamp,
        }
    }

    /// Constructs an injected mouse click event.
    #[must_use]
    pub fn injected_click(point: PhysicalPoint, flags: u32, timestamp: Instant) -> Self {
        Self {
            kind: InputEventKind::Click(point),
            source: InputSource::Injected { flags },
            timestamp,
        }
    }

    /// Constructs a physical key press event.
    #[must_use]
    pub fn physical_key(key: ConfirmationKey, timestamp: Instant) -> Self {
        Self {
            kind: InputEventKind::KeyPress(key),
            source: InputSource::Physical,
            timestamp,
        }
    }

    /// Constructs an injected key press event.
    #[must_use]
    pub fn injected_key(key: ConfirmationKey, flags: u32, timestamp: Instant) -> Self {
        Self {
            kind: InputEventKind::KeyPress(key),
            source: InputSource::Injected { flags },
            timestamp,
        }
    }
}
