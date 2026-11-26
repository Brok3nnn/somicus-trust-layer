//! The process-wide subscriber: standard error in debug builds, nothing in release builds.

use std::io::Write as _;

use tracing::Level;
use tracing_subscriber::layer::SubscriberExt as _;

use crate::layer::{AllowList, Record, Sink};

/// Writes one line per record to standard error.
struct StandardError;

impl Sink for StandardError {
    fn write(&self, record: &Record) {
        // A diagnostic that cannot be written is dropped; it must never stop the app.
        let _ = writeln!(std::io::stderr().lock(), "somicus {record}");
    }
}

/// Installs the process-wide subscriber, once, and says whether this call installed it.
///
/// In debug builds, the allow-list layer writes one line per event and per closed span at `INFO` and above to
/// standard error. Release builds install nothing: no exporter exists yet, so a span
/// or event costs one level check and goes nowhere.
#[must_use = "the caller decides whether a second call is a mistake"]
pub fn init() -> bool {
    if !cfg!(debug_assertions) {
        return false;
    }
    let subscriber =
        tracing_subscriber::registry().with(AllowList::new(StandardError, Level::INFO));
    tracing::subscriber::set_global_default(subscriber).is_ok()
}
