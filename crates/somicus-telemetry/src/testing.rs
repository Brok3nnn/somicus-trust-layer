//! Captures what the allow-list layer writes, so a crate's tests can prove its spans carry only allowed fields.
//!
//! Available with the `testing` feature, which only dev-dependencies enable.

use std::future::Future;
use std::sync::{Arc, Mutex, Once, PoisonError};

use tracing::Level;
use tracing_subscriber::layer::SubscriberExt as _;

use crate::layer::{AllowList, Record, Sink};

/// The records of one capture.
#[derive(Clone, Default)]
struct Memory(Arc<Mutex<Vec<Record>>>);

impl Memory {
    fn take(&self) -> Vec<Record> {
        std::mem::take(&mut *self.0.lock().unwrap_or_else(PoisonError::into_inner))
    }
}

impl Sink for Memory {
    fn write(&self, record: &Record) {
        self.0
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push(record.clone());
    }
}

fn subscriber(memory: &Memory, max_level: Level) -> impl tracing::Subscriber + Send + Sync + use<> {
    register_global_dispatcher();
    tracing_subscriber::registry().with(AllowList::new(memory.clone(), max_level))
}

/// Registers a global no-op subscriber once, before the first capture's own.
///
/// `tracing-core` caches each callsite's interest. While a single dispatcher is registered, a callsite seen for the
/// first time on another thread takes its interest from that thread's default alone, which is the no-op default when
/// it captures nothing: the callsite is then disabled for a capture running at the same time on another thread of the
/// same test binary (`cargo test` runs a crate's tests on several threads). With a global dispatcher registered too,
/// every first sight asks all live dispatchers, and a capture's spans are recorded whatever other tests do.
fn register_global_dispatcher() {
    static GLOBAL: Once = Once::new();
    GLOBAL.call_once(|| {
        // An error means a global subscriber is already set, which is registered and serves as well.
        let _ =
            tracing::subscriber::set_global_default(tracing::subscriber::NoSubscriber::default());
    });
}

/// Runs `run` with the allow-list layer as this thread's subscriber at `max_level`, and returns what it recorded.
pub fn capture_at<T>(max_level: Level, run: impl FnOnce() -> T) -> (T, Vec<Record>) {
    let memory = Memory::default();
    let value = tracing::subscriber::with_default(subscriber(&memory, max_level), run);
    (value, memory.take())
}

/// Runs `run` with the allow-list layer as this thread's subscriber at every level, and returns what it recorded.
pub fn capture<T>(run: impl FnOnce() -> T) -> (T, Vec<Record>) {
    capture_at(Level::TRACE, run)
}

/// Polls `future` with the allow-list layer as this thread's subscriber at every level, and returns what it recorded.
/// Tasks the future starts on other threads are not captured; a current-thread runtime, such as `#[tokio::test]`'s,
/// keeps everything on this thread.
pub async fn capture_async<F: Future>(future: F) -> (F::Output, Vec<Record>) {
    let memory = Memory::default();
    let guard = tracing::subscriber::set_default(subscriber(&memory, Level::TRACE));
    let value = future.await;
    drop(guard);
    (value, memory.take())
}
