//! [`Span`]: an operation's span, carrying only allow-listed fields, closed and timed when its last handle drops.

use std::future::Future;

use tracing::Instrument as _;

use crate::field::{Label, Outcome};

/// A span made by [`crate::span!`]. Its fields can be set only through [`crate::record!`] and
/// [`Span::record_result`], which take the allow-list's names and value types.
#[derive(Debug, Clone)]
#[must_use = "a span closes, and is written, when its last handle is dropped"]
pub struct Span(tracing::Span);

impl Span {
    /// Wraps a span the macro declared with every allow-listed field. Used by [`crate::span!`]; call it instead.
    #[doc(hidden)]
    pub const fn __new(inner: tracing::Span) -> Self {
        Self(inner)
    }

    /// Records one field the macros checked. Used by [`crate::record!`]; call it instead.
    #[doc(hidden)]
    pub fn __record(&self, field: &'static str, value: impl tracing::field::Value) {
        self.0.record(field, value);
    }

    /// The `tracing` span, for this crate's tests of what the macro declares.
    #[cfg(test)]
    pub(crate) const fn inner(&self) -> &tracing::Span {
        &self.0
    }

    /// Runs `run` inside the span.
    pub fn in_scope<T>(&self, run: impl FnOnce() -> T) -> T {
        self.0.in_scope(run)
    }

    /// Polls `future` inside the span.
    pub fn instrument<F: Future>(&self, future: F) -> impl Future<Output = F::Output> + use<F> {
        future.instrument(self.0.clone())
    }

    /// Records `outcome` as `ok`, or as `error` with the error's fixed [`Label`] as `reason`.
    pub fn record_result<T, E: Label>(&self, result: &Result<T, E>) {
        match result {
            Ok(_) => self.__record("outcome", Outcome::Ok.label()),
            Err(error) => {
                self.__record("outcome", Outcome::Error.label());
                self.__record("reason", error.label());
            }
        }
    }
}
