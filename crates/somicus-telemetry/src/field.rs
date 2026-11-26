//! The field allow-list: every name a span or event may carry, and what each may hold.
//!
//! Two kinds of value exist. A [`Kind::Label`] is a fixed, lower-case name from a closed vocabulary (an enum variant,
//! an error kind, a command's name); the macros accept it only through [`Label`], whose value is a `&'static str`, so
//! text read at runtime (screen content, field values, prompts, URLs, paths, tokens) cannot become one. A
//! [`Kind::Count`] is a non-negative number. Nothing else is recordable: no free text, no `Debug` output, no floats.

/// What a field on the allow-list may hold.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// A fixed name of at most [`MAX_LABEL_CHARS`] lower-case ASCII letters, digits and underscores, starting with a
    /// letter.
    Label,
    /// A non-negative whole number: a count, a size in bytes, a status code or a sequence number.
    Count,
}

/// Every field a span or event may carry. The layer drops any other field, and any value that does not fit its kind.
pub const ALLOWED: &[(&str, Kind)] = &[
    // A desktop command's name, as the webviews invoke it.
    ("command", Kind::Label),
    // What sort of operation it was: a turn's kind, a request's method, a protocol frame's body, a capture's scope.
    ("kind", Kind::Label),
    // `ok` or `error`.
    ("outcome", Kind::Label),
    // The fixed reason of an error or a refusal: an error variant's name, never its message.
    ("reason", Kind::Label),
    // `http` or `https`.
    ("transport", Kind::Label),
    // The Policy Guard's decision.
    ("decision", Kind::Label),
    // A risk class, `r0` to `r5`.
    ("risk", Kind::Label),
    // An autonomy level, `l0` to `l3`.
    ("autonomy", Kind::Label),
    // An HTTP status code.
    ("status", Kind::Count),
    // The size of a payload in bytes.
    ("bytes", Kind::Count),
    // A session protocol sequence number.
    ("seq", Kind::Count),
    // How many UI nodes.
    ("nodes", Kind::Count),
    // How many parts of a text the Privacy Shield replaced with tokens.
    ("redactions", Kind::Count),
];

/// The longest label the layer keeps.
pub const MAX_LABEL_CHARS: usize = 48;

/// What the field called `name` may hold, or `None` when it is not on the allow-list.
#[must_use]
pub fn kind_of(name: &str) -> Option<Kind> {
    ALLOWED
        .iter()
        .find(|(allowed, _)| *allowed == name)
        .map(|(_, kind)| *kind)
}

/// Whether `value` has the shape of a label: one to [`MAX_LABEL_CHARS`] lower-case ASCII letters, digits and
/// underscores, starting with a letter. Spaces, capitals, punctuation, `/`, `@`, `:` and `?` are refused, so a URL, a
/// path, an e-mail address, a sentence or a token never passes.
#[must_use]
pub fn is_label(value: &str) -> bool {
    let mut bytes = value.bytes();
    value.len() <= MAX_LABEL_CHARS
        && bytes.next().is_some_and(|first| first.is_ascii_lowercase())
        && bytes.all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'_')
}

/// A value with a fixed, content-free name, such as an enum variant or an error kind.
///
/// Implement it with a `match` that returns string literals. The name must have the shape [`is_label`] accepts, or the
/// layer drops the field.
pub trait Label {
    /// The fixed name of this value.
    fn label(&self) -> &'static str;
}

/// A string literal, such as a command's name. Only `&'static str` qualifies, so text read at runtime cannot.
impl Label for &'static str {
    fn label(&self) -> &'static str {
        self
    }
}

impl<L: Label + ?Sized> Label for &L {
    fn label(&self) -> &'static str {
        (**self).label()
    }
}

/// A non-negative whole number a field may count with.
pub trait Count {
    /// The value as a `u64`, saturating.
    fn to_count(self) -> u64;
}

impl Count for u8 {
    fn to_count(self) -> u64 {
        u64::from(self)
    }
}

impl Count for u16 {
    fn to_count(self) -> u64 {
        u64::from(self)
    }
}

impl Count for u32 {
    fn to_count(self) -> u64 {
        u64::from(self)
    }
}

impl Count for u64 {
    fn to_count(self) -> u64 {
        self
    }
}

impl Count for usize {
    fn to_count(self) -> u64 {
        u64::try_from(self).unwrap_or(u64::MAX)
    }
}

/// Whether an operation succeeded.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    /// It succeeded.
    Ok,
    /// It failed; the `reason` field says why.
    Error,
}

impl Label for Outcome {
    fn label(&self) -> &'static str {
        match self {
            Self::Ok => "ok",
            Self::Error => "error",
        }
    }
}

/// The value of a label field. Used by the macros; call them instead.
#[doc(hidden)]
#[must_use]
pub fn label<L: Label + ?Sized>(value: &L) -> &'static str {
    value.label()
}

/// The value of a count field. Used by the macros; call them instead.
#[doc(hidden)]
#[must_use]
pub fn count<C: Count>(value: C) -> u64 {
    value.to_count()
}
