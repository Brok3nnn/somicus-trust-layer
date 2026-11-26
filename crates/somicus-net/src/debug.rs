//! `Debug` helpers that show a value is there without showing it.

use std::fmt;

/// Prints `<redacted>` in place of a credential.
pub(crate) struct Hidden;

impl fmt::Debug for Hidden {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("<redacted>")
    }
}

/// Prints how many bytes a body has in place of the body.
pub(crate) struct ByteCount(pub(crate) usize);

impl fmt::Debug for ByteCount {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "<{} bytes>", self.0)
    }
}
