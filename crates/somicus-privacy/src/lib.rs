//! On-device Privacy Shield: classification, entity recognition, pseudonymization, and token vault.
//!
//! Implements ADR-0014:
//! - Deterministic pattern recognizers with checksums (Email, Phone, Luhn cards, mod-97 IBAN, SSN, Tax IDs).
//! - Multilingual personal name recognizer across German (DE), English (EN), and Hungarian (HU).
//! - In-memory, reversible pseudonymization with zeroized token vault.
//! - Strict [`Redacted<T>`] branding guarantee before any network egress.
//! - The [`NameRecognizer`] seam for the on-device name model of ADR-0042, which `somicus-ner` implements.
//!
//! The R-suite v0 evaluation harness and its latency benchmark live in `evals/privacy`, outside the shipped crate.

#![forbid(unsafe_code)]

pub mod error;
pub mod name_model;
pub mod recognizers;
pub mod redacted;
pub mod shield;
pub mod vault;

pub use error::PrivacyError;
pub use name_model::{NameProtection, NameRecognizer, RulesOnlyReason};
pub use recognizers::{
    EntityType, NameMatch, PatternCategory, PatternMatch, RecognizedEntity, detect_all_entities,
    detect_entities_with_names, detect_names, find_pattern_matches, passes_de_steuer_id_checksum,
    passes_hu_taj_checksum, passes_hu_tax_id_checksum, passes_iban_checksum, passes_luhn_checksum,
};
pub use redacted::Redacted;
pub use shield::PrivacyShield;
pub use vault::TokenVault;
