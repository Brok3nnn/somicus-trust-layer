//! The subscriber layer that enforces the allow-list: the only path from a span or event to any output.
//!
//! It admits only callsites in Somicus crates, keeps only the fields [`crate::field::ALLOWED`] names with values of
//! their kind, and hands a [`Record`] of those values to a [`Sink`]. A sink therefore never sees a field that is not on
//! the list, a free-text message, a `Debug` rendering or a label that does not have the shape of one, whether the call
//! site used this crate's macros or called `tracing` directly.

use std::fmt;
use std::time::Instant;

use tracing::field::{Field, Visit};
use tracing::span::{Attributes, Id};
use tracing::subscriber::Interest;
use tracing::{Event, Level, Metadata, Subscriber};
use tracing_subscriber::layer::{Context, Layer};
use tracing_subscriber::registry::LookupSpan;

use crate::field::{Kind, is_label, kind_of};

/// A kept field's value.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FieldValue {
    /// A label that has the shape [`is_label`] accepts.
    Label(String),
    /// A count.
    Count(u64),
}

impl fmt::Display for FieldValue {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Label(label) => formatter.write_str(label),
            Self::Count(count) => write!(formatter, "{count}"),
        }
    }
}

/// Whether a record is an event or a closed span.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecordKind {
    /// Something that happened at one moment.
    Event,
    /// An operation that ended; it carries its duration.
    Span,
}

/// What reaches a [`Sink`]: allow-listed fields only.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Record {
    /// An event or a closed span.
    pub kind: RecordKind,
    /// Its level.
    pub level: Level,
    /// The module that emitted it, such as `somicus_net::service`.
    pub target: &'static str,
    /// Its name from the macro's literal, or `unnamed` when that name is not a label.
    pub name: &'static str,
    /// The kept fields, in the order they were recorded.
    pub fields: Vec<(&'static str, FieldValue)>,
    /// For a span, how long it was open, in microseconds.
    pub duration_us: Option<u64>,
    /// How many fields were dropped because they were not on the list or did not fit their kind.
    pub dropped: u32,
}

impl Record {
    /// The kept value of the field `name`.
    #[must_use]
    pub fn field(&self, name: &str) -> Option<&FieldValue> {
        self.fields
            .iter()
            .find(|(field, _)| *field == name)
            .map(|(_, value)| value)
    }

    /// The kept label of the field `name`.
    #[must_use]
    pub fn label(&self, name: &str) -> Option<&str> {
        match self.field(name) {
            Some(FieldValue::Label(label)) => Some(label),
            _ => None,
        }
    }

    /// The kept count of the field `name`.
    #[must_use]
    pub fn count(&self, name: &str) -> Option<u64> {
        match self.field(name) {
            Some(FieldValue::Count(count)) => Some(*count),
            _ => None,
        }
    }

    /// The names of the kept fields, in order.
    #[must_use]
    pub fn field_names(&self) -> Vec<&'static str> {
        self.fields.iter().map(|(name, _)| *name).collect()
    }
}

impl fmt::Display for Record {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{} {} {}", self.level, self.target, self.name)?;
        for (name, value) in &self.fields {
            write!(formatter, " {name}={value}")?;
        }
        if let Some(duration) = self.duration_us {
            write!(formatter, " duration_us={duration}")?;
        }
        if self.dropped > 0 {
            write!(formatter, " dropped={}", self.dropped)?;
        }
        Ok(())
    }
}

/// Where records go: standard error in debug builds, a test's memory, and later an exporter. It receives only allow-listed values.
pub trait Sink: Send + Sync + 'static {
    /// Takes one record.
    fn write(&self, record: &Record);
}

/// The allow-listed fields of one span or event, collected from what `tracing` visits.
#[derive(Default)]
struct Kept {
    fields: Vec<(&'static str, FieldValue)>,
    dropped: u32,
}

impl Kept {
    fn keep(&mut self, field: &Field, value: Option<FieldValue>) {
        match value {
            Some(value) => {
                if let Some(slot) = self
                    .fields
                    .iter_mut()
                    .find(|(name, _)| *name == field.name())
                {
                    slot.1 = value;
                } else {
                    self.fields.push((field.name(), value));
                }
            }
            None => self.dropped = self.dropped.saturating_add(1),
        }
    }
}

impl Visit for Kept {
    fn record_u64(&mut self, field: &Field, value: u64) {
        let kept = (kind_of(field.name()) == Some(Kind::Count)).then_some(FieldValue::Count(value));
        self.keep(field, kept);
    }

    fn record_i64(&mut self, field: &Field, value: i64) {
        match u64::try_from(value) {
            Ok(value) => self.record_u64(field, value),
            Err(_) => self.keep(field, None),
        }
    }

    fn record_str(&mut self, field: &Field, value: &str) {
        let kept = (kind_of(field.name()) == Some(Kind::Label) && is_label(value))
            .then(|| FieldValue::Label(value.to_owned()));
        self.keep(field, kept);
    }

    /// Everything else (`Debug` and `Display` values, errors, booleans, floats, bytes, the message) is dropped unread.
    fn record_debug(&mut self, field: &Field, _value: &dyn fmt::Debug) {
        self.keep(field, None);
    }
}

/// A span's fields while it is open, kept in the registry's extensions.
struct Open {
    started: Instant,
    kept: Kept,
}

/// The allow-list layer. Put it on a `tracing_subscriber` registry with the sink records should go to.
pub struct AllowList<S> {
    sink: S,
    max_level: Level,
}

impl<S: Sink> AllowList<S> {
    /// A layer that writes to `sink` the spans and events of Somicus crates at `max_level` and below.
    #[must_use]
    pub const fn new(sink: S, max_level: Level) -> Self {
        Self { sink, max_level }
    }

    fn admits(&self, metadata: &Metadata<'_>) -> bool {
        is_somicus(metadata.target()) && *metadata.level() <= self.max_level
    }
}

/// Whether `target` is a module of a Somicus crate. Other crates' spans and events are never recorded: their fields
/// and messages were not written against the allow-list.
fn is_somicus(target: &str) -> bool {
    let crate_name = target.split("::").next().unwrap_or_default();
    crate_name == "somicus" || crate_name.starts_with("somicus_")
}

/// The name of a span or event when it is a label; anything else (such as `tracing`'s default `event file:line`)
/// becomes `unnamed`.
fn name_of(metadata: &'static Metadata<'static>) -> &'static str {
    let name = metadata.name();
    if is_label(name) { name } else { "unnamed" }
}

fn micros(since: Instant) -> u64 {
    u64::try_from(since.elapsed().as_micros()).unwrap_or(u64::MAX)
}

impl<S, K> Layer<S> for AllowList<K>
where
    S: Subscriber + for<'lookup> LookupSpan<'lookup>,
    K: Sink,
{
    fn register_callsite(&self, metadata: &'static Metadata<'static>) -> Interest {
        if self.admits(metadata) {
            Interest::always()
        } else {
            Interest::never()
        }
    }

    fn enabled(&self, metadata: &Metadata<'_>, _context: Context<'_, S>) -> bool {
        self.admits(metadata)
    }

    fn on_new_span(&self, attributes: &Attributes<'_>, id: &Id, context: Context<'_, S>) {
        let Some(span) = context.span(id) else {
            return;
        };
        let mut kept = Kept::default();
        attributes.record(&mut kept);
        span.extensions_mut().insert(Open {
            started: Instant::now(),
            kept,
        });
    }

    fn on_record(&self, id: &Id, values: &tracing::span::Record<'_>, context: Context<'_, S>) {
        if let Some(span) = context.span(id)
            && let Some(open) = span.extensions_mut().get_mut::<Open>()
        {
            values.record(&mut open.kept);
        }
    }

    fn on_event(&self, event: &Event<'_>, _context: Context<'_, S>) {
        let metadata = event.metadata();
        let mut kept = Kept::default();
        event.record(&mut kept);
        self.sink.write(&Record {
            kind: RecordKind::Event,
            level: *metadata.level(),
            target: metadata.target(),
            name: name_of(metadata),
            fields: kept.fields,
            duration_us: None,
            dropped: kept.dropped,
        });
    }

    fn on_close(&self, id: Id, context: Context<'_, S>) {
        let Some(span) = context.span(&id) else {
            return;
        };
        let Some(open) = span.extensions_mut().remove::<Open>() else {
            return;
        };
        let metadata = span.metadata();
        self.sink.write(&Record {
            kind: RecordKind::Span,
            level: *metadata.level(),
            target: metadata.target(),
            name: name_of(metadata),
            fields: open.kept.fields,
            duration_us: Some(micros(open.started)),
            dropped: open.kept.dropped,
        });
    }
}
