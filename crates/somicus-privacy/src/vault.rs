//! In-memory, reversible pseudonymization token vault (ADR-0014).
//!
//! Stores session-consistent bidirectional mappings between raw PII and typed tokens
//! (`⟦PERSON_1⟧`, `⟦EMAIL_1⟧`, `⟦IBAN_1⟧`). Memory is zeroized on drop.

use crate::recognizers::EntityType;
use std::collections::HashMap;
use std::fmt;
use zeroize::Zeroize;

/// In-memory token vault for session-consistent pseudonymization and local rehydration. `Debug` shows how many values
/// it holds, never a value or its token.
#[derive(Default)]
pub struct TokenVault {
    forward: HashMap<String, String>,
    reverse: HashMap<String, String>,
    counters: HashMap<EntityType, usize>,
}

impl TokenVault {
    /// Creates a new, empty in-memory token vault.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Replaces a raw sensitive entity string with a session-consistent token.
    ///
    /// If the raw string was already tokenized in this session, the existing token is returned.
    pub fn tokenize(&mut self, raw: &str, entity_type: EntityType) -> String {
        if let Some(existing) = self.forward.get(raw) {
            return existing.clone();
        }

        let counter = self.counters.entry(entity_type).or_insert(1);
        let token = format!("⟦{}_{counter}⟧", entity_type.token_prefix());
        *counter += 1;

        self.forward.insert(raw.to_string(), token.clone());
        self.reverse.insert(token.clone(), raw.to_string());

        token
    }

    /// Rehydrates a pseudonymized text by replacing all known `⟦TOKEN_n⟧` tokens with raw values.
    #[must_use]
    pub fn rehydrate(&self, text: &str) -> String {
        let mut result = text.to_string();
        for (token, raw) in &self.reverse {
            result = result.replace(token, raw);
        }
        result
    }

    /// Returns the number of distinct entities tracked in the vault.
    #[must_use]
    pub fn len(&self) -> usize {
        self.forward.len()
    }

    /// Returns true if the vault contains no mapped tokens.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.forward.is_empty()
    }

    /// Clears and zeroizes all stored mappings.
    pub fn clear(&mut self) {
        for (mut k, mut v) in self.forward.drain() {
            k.zeroize();
            v.zeroize();
        }
        for (mut k, mut v) in self.reverse.drain() {
            k.zeroize();
            v.zeroize();
        }
        self.counters.clear();
    }
}

impl fmt::Debug for TokenVault {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("TokenVault")
            .field("entries", &self.forward.len())
            .finish_non_exhaustive()
    }
}

impl Drop for TokenVault {
    fn drop(&mut self) {
        self.clear();
    }
}
