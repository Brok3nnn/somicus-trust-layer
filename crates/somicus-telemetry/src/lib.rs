//! Content-free diagnostics for the Somicus desktop core and its crates.
//!
//! This crate is the only way Somicus Rust code emits spans and events. Two gates keep content out:
//!
//! 1. **The macros** ([`span!`], [`record!`], [`event!`]) accept only the field names on [`field::ALLOWED`], each with
//!    its kind of value: a [`Label`] (a `&'static str` or a type with fixed names) or a [`Count`] (an unsigned
//!    integer). Any other name, a `String`, a borrowed `&str` or a `Debug` value does not compile.
//! 2. **The layer** ([`AllowList`]) admits only spans and events of Somicus crates and drops every field that is not
//!    on the list or whose value does not fit its kind, including a label that is not a short lower-case identifier.
//!    A raw `tracing` call therefore cannot add content either.
//!
//! What can be recorded: ids and counts, durations (measured by the layer), enum names, error kinds and command names.
//! What cannot: screen text, field values, prompts, model output, URLs, file paths, tokens, keys, e-mail addresses or
//! any message text. [`init`] writes to standard error in debug builds and installs nothing in release builds.

#![forbid(unsafe_code)]

pub mod field;
mod init;
mod layer;
mod macros;
mod scope;
#[cfg(any(test, feature = "testing"))]
pub mod testing;

pub use field::{Count, Label, Outcome};
pub use init::init;
pub use layer::{AllowList, FieldValue, Record, RecordKind, Sink};
pub use scope::Span;
pub use tracing::Level;

/// What the macros expand to. Not part of the API.
#[doc(hidden)]
pub mod __private {
    pub use tracing;
}

#[cfg(test)]
mod tests;
