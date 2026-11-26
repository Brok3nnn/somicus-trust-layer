//! Privacy classification types for screen text and values.

use serde::{Deserialize, Serialize};

/// Privacy classification for a node's accessible text or value.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum PrivacyClass {
    /// UI chrome or static label, safe to transmit without redaction.
    #[default]
    PublicLabel,

    /// User-entered or application content value, subject to masking/pseudonymization.
    ContentValue,

    /// Personal identifiable information (PII), requiring strict redaction.
    Personal,

    /// Secret data (passwords, tokens, PINs), masked under all circumstances.
    Secret,
}

impl PrivacyClass {
    /// Returns true if this data class represents sensitive content.
    #[must_use]
    pub const fn is_sensitive(self) -> bool {
        matches!(self, Self::ContentValue | Self::Personal | Self::Secret)
    }

    /// Returns true if this data class must never be transmitted in raw form.
    #[must_use]
    pub const fn requires_redaction(self) -> bool {
        matches!(self, Self::Personal | Self::Secret)
    }
}

/// Text paired with its on-device privacy classification.
///
/// There is deliberately no `From<&str>` or `From<String>`: every piece of screen text is classified by the
/// constructor that names its class, never by a conversion that quietly calls it a public label.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ClassifiedText {
    text: String,
    classification: PrivacyClass,
}

impl ClassifiedText {
    /// Creates a new classified text container.
    #[must_use]
    pub fn new(text: impl Into<String>, classification: PrivacyClass) -> Self {
        Self {
            text: text.into(),
            classification,
        }
    }

    /// Creates public label text that needs no redaction.
    #[must_use]
    pub fn public_label(text: impl Into<String>) -> Self {
        Self::new(text, PrivacyClass::PublicLabel)
    }

    /// Creates content value text.
    #[must_use]
    pub fn content_value(text: impl Into<String>) -> Self {
        Self::new(text, PrivacyClass::ContentValue)
    }

    /// Creates personal data text.
    #[must_use]
    pub fn personal(text: impl Into<String>) -> Self {
        Self::new(text, PrivacyClass::Personal)
    }

    /// Creates secret data text.
    #[must_use]
    pub fn secret(text: impl Into<String>) -> Self {
        Self::new(text, PrivacyClass::Secret)
    }

    /// Returns a reference to the raw text.
    #[must_use]
    pub fn raw_text(&self) -> &str {
        &self.text
    }

    /// Returns the privacy classification.
    #[must_use]
    pub const fn classification(&self) -> PrivacyClass {
        self.classification
    }

    /// Formats this text safely for the LLM screen digest.
    #[must_use]
    pub fn digest_text(&self) -> String {
        match self.classification {
            PrivacyClass::Secret => "<secret>".to_string(),
            PrivacyClass::Personal => {
                // If text already contains pseudonym brackets like ⟦ORG_1⟧, preserve them
                if self.text.starts_with('⟦') && self.text.ends_with('⟧') {
                    self.text.clone()
                } else {
                    "<personal>".to_string()
                }
            }
            PrivacyClass::ContentValue => {
                if self.text.starts_with('⟦') && self.text.ends_with('⟧') {
                    self.text.clone()
                } else if self.text.is_empty() {
                    String::new()
                } else {
                    format!("⟦VALUE_{}⟧", self.text.len())
                }
            }
            PrivacyClass::PublicLabel => self.text.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::marker::PhantomData;

    use super::*;

    /// Tells at compile time whether `T: From<S>`: an inherent constant is chosen over a trait's only when its bound
    /// holds, so `CONVERTS` is `true` exactly when the conversion exists.
    struct Probe<T, S>(PhantomData<(T, S)>);

    trait NoConversion {
        const CONVERTS: bool = false;
    }

    impl<T, S> NoConversion for Probe<T, S> {}

    impl<T: From<S>, S> Probe<T, S> {
        const CONVERTS: bool = true;
    }

    #[test]
    fn screen_text_has_no_implicit_public_label_conversion() {
        // Text from the screen must say what it is; a conversion that quietly calls it a public label leaks content.
        let conversions = [
            Probe::<ClassifiedText, &'static str>::CONVERTS,
            Probe::<ClassifiedText, String>::CONVERTS,
            // The probe does see a conversion that exists.
            Probe::<String, &'static str>::CONVERTS,
        ];
        assert_eq!(conversions, [false, false, true]);
    }
}
