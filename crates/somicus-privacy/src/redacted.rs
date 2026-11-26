//! The egress brand: only Privacy Shield output, or reviewed non-content framing, may leave the device.
//!
//! See ADR-0014. [`Redacted<T>`] has no public constructor and no `From<T>`: the
//! [`PrivacyShield`](crate::PrivacyShield) creates it, and brand-preserving combinators ([`Redacted::map`],
//! [`Redacted::zip`], `collect`) carry it through serialization. [`Redacted::without_content`] is the only other way
//! in, for protocol framing, identifiers and credentials that carry no screen or user content; [`Redacted::account_data`]
//! brands what the person types into Somicus's own sign-in and registration forms for the Somicus identity service
//! (ADR-0032). Both are explicit, named escape hatches: every call site is an audit point, easy to find with grep. The brand cannot be deserialized from untrusted input, and `Debug` never prints the value.

use std::fmt;
use std::ops::Deref;

use serde::{Serialize, Serializer};

/// A value that may cross the network egress boundary.
///
/// Only the [`PrivacyShield`](crate::PrivacyShield) brands screen or user text:
///
/// ```
/// use somicus_privacy::{PrivacyShield, Redacted};
/// use somicus_uigraph::ClassifiedText;
///
/// let mut shield = PrivacyShield::new();
/// let text = ClassifiedText::content_value("Write to jane.doe@example.com");
/// let branded: Redacted<String> = shield.redact_text(&text);
/// assert!(!branded.as_inner().contains("jane.doe@example.com"));
/// ```
///
/// Code outside this crate cannot brand a raw value by construction, conversion or deserialization:
///
/// ```compile_fail
/// let leaked = somicus_privacy::Redacted::new(String::from("jane.doe@example.com"));
/// ```
///
/// ```compile_fail
/// let leaked: somicus_privacy::Redacted<String> = String::from("jane.doe@example.com").into();
/// ```
///
/// ```compile_fail
/// let leaked: somicus_privacy::Redacted<String> = serde_json::from_str("\"jane.doe@example.com\"").unwrap();
/// ```
#[derive(Clone, PartialEq, Eq)]
pub struct Redacted<T> {
    inner: T,
}

impl<T> Redacted<T> {
    /// Brands Privacy Shield output. Only this crate can call it.
    pub(crate) const fn new(inner: T) -> Self {
        Self { inner }
    }

    /// Brands protocol framing, identifiers or credentials that carry no screen or user content.
    ///
    /// Never use it for text that came from the screen or from the person; run that through the Privacy Shield.
    /// Every call site is an audit point.
    #[must_use]
    pub const fn without_content(value: T) -> Self {
        Self { inner: value }
    }

    /// Brands account data the person typed into Somicus's own sign-in or registration form, to send it to the Somicus
    /// identity service and nowhere else: their e-mail address, the sign-in code and the name they chose (ADR-0032).
    ///
    /// Never use it for anything read from the screen or said to the assistant; run that through the Privacy Shield.
    /// Every call site is an audit point.
    #[must_use]
    pub const fn account_data(value: T) -> Self {
        Self { inner: value }
    }

    /// Consumes this wrapper and returns the inner value, for the egress implementation that sends it.
    #[must_use]
    pub fn into_inner(self) -> T {
        self.inner
    }

    /// Returns a reference to the inner value.
    #[must_use]
    pub const fn as_inner(&self) -> &T {
        &self.inner
    }

    /// Transforms the value while keeping the brand, for example to serialize it.
    #[must_use]
    pub fn map<U>(self, transform: impl FnOnce(T) -> U) -> Redacted<U> {
        Redacted::new(transform(self.inner))
    }

    /// Combines two branded values into one.
    #[must_use]
    pub fn zip<U>(self, other: Redacted<U>) -> Redacted<(T, U)> {
        Redacted::new((self.inner, other.inner))
    }
}

impl<T> Deref for Redacted<T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

impl<T> FromIterator<Redacted<T>> for Redacted<Vec<T>> {
    fn from_iter<I: IntoIterator<Item = Redacted<T>>>(items: I) -> Self {
        Self::new(items.into_iter().map(Redacted::into_inner).collect())
    }
}

impl<T> fmt::Debug for Redacted<T> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("Redacted(…)")
    }
}

impl<T: Serialize> Serialize for Redacted<T> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        self.inner.serialize(serializer)
    }
}

impl<T: Serialize> Redacted<T> {
    /// Serializes an already-branded value without losing its egress brand.
    ///
    /// # Errors
    /// Returns the JSON serializer's error if the value cannot be encoded.
    pub fn to_json_bytes(self) -> Result<Redacted<Vec<u8>>, serde_json::Error> {
        serde_json::to_vec(&self.inner).map(Redacted::new)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn debug_output_never_shows_the_value() {
        assert_eq!(
            format!("{:?}", Redacted::new("jane@example.com")),
            "Redacted(…)"
        );
    }

    #[test]
    fn serializes_transparently() {
        let value = Redacted::new(vec!["a", "b"]);
        assert_eq!(
            serde_json::to_string(&value).unwrap_or_default(),
            r#"["a","b"]"#
        );
    }

    #[test]
    fn account_data_is_branded_and_never_printed() {
        let email = Redacted::account_data("ann@example.test".to_owned());
        assert_eq!(format!("{email:?}"), "Redacted(…)");
        assert_eq!(email.into_inner(), "ann@example.test");
    }

    #[test]
    fn combinators_keep_the_brand() {
        let joined: Redacted<Vec<&str>> = [Redacted::new("a"), Redacted::new("b")]
            .into_iter()
            .collect();
        let mapped = joined.map(|parts| parts.join("+"));
        let (left, right) = mapped.zip(Redacted::new(1)).into_inner();
        assert_eq!((left.as_str(), right), ("a+b", 1));
    }

    #[test]
    fn json_serialization_keeps_the_brand() {
        let value = Redacted::new(vec!["masked"]);
        let bytes: Redacted<Vec<u8>> = value.to_json_bytes().unwrap();
        assert_eq!(bytes.as_inner(), br#"["masked"]"#);
    }
}
