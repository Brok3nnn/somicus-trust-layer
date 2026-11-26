//! The allow-list holds: fields off the list and values that do not fit their kind never reach a sink.

use crate::field::{ALLOWED, Kind, MAX_LABEL_CHARS, is_label, kind_of};
use crate::testing::{capture, capture_at};
use crate::{FieldValue, Label, Level, Record, RecordKind};

/// Synthetic content of the kinds that must never be recorded: a name, an e-mail address, a URL with a path and a
/// query, a home-folder path and a token.
const CONTENT: &[&str] = &[
    "Anna Kovacs",
    "anna@example.test",
    "https://example.test/inbox/42?token=synthetic",
    "/home/anna/Documents/plan.txt",
    "synthetic-access-token",
];

/// A fixed error reason, as the crates' error types give one.
struct Refused;

impl Label for Refused {
    fn label(&self) -> &'static str {
        "refused"
    }
}

fn only(records: &[Record]) -> &Record {
    assert_eq!(records.len(), 1, "{records:?}");
    &records[0]
}

fn assert_no_content(records: &[Record]) {
    for record in records {
        let line = record.to_string();
        for content in CONTENT {
            assert!(!line.contains(content), "{line}");
        }
    }
}

#[test]
fn a_field_outside_the_allow_list_is_dropped() {
    let ((), records) = capture(|| {
        tracing::info!(
            url = CONTENT[2],
            user = CONTENT[0],
            path = CONTENT[3],
            status = 200_u64,
            "message text with {}",
            CONTENT[1]
        );
    });
    let record = only(&records);
    assert_eq!(record.fields, vec![("status", FieldValue::Count(200))]);
    // `url`, `user`, `path` and the message.
    assert_eq!(record.dropped, 4);
    assert_no_content(&records);
}

#[test]
fn a_string_cannot_carry_content_through_an_allowed_name() {
    let too_long = "a".repeat(MAX_LABEL_CHARS + 1);
    let ((), records) = capture(|| {
        tracing::info!(
            kind = CONTENT[0],
            reason = CONTENT[2],
            outcome = CONTENT[3],
            command = CONTENT[1],
            transport = CONTENT[4].to_uppercase().as_str(),
            decision = too_long.as_str(),
            status = "200",
            nodes = -1_i64,
            risk = true,
            redactions = 1.5_f64,
            seq = tracing::field::debug(CONTENT[4]),
            bytes = tracing::field::display(CONTENT[1]),
        );
    });
    let record = only(&records);
    assert!(record.fields.is_empty(), "{record:?}");
    assert_eq!(record.dropped, 12);
    assert_no_content(&records);
}

#[test]
fn labels_must_be_short_lower_case_identifiers() {
    for label in [
        "ok",
        "get",
        "r3",
        "submit_guidance_query",
        "perception_no_foreground_window",
    ] {
        assert!(is_label(label), "{label}");
    }
    let too_long = "a".repeat(MAX_LABEL_CHARS + 1);
    for content in CONTENT.iter().copied().chain([
        "",
        "Ok",
        "3d",
        "_private",
        "two words",
        "a.b",
        "a-b",
        too_long.as_str(),
    ]) {
        assert!(!is_label(content), "{content}");
    }
}

#[test]
fn the_macros_keep_labels_and_counts() {
    let ((), records) = capture(|| {
        crate::event!(
            INFO,
            "probe",
            kind = "get",
            status = 200_u16,
            nodes = 3_usize,
            reason = Refused
        );
    });
    let record = only(&records);
    assert_eq!(record.kind, RecordKind::Event);
    assert_eq!(record.name, "probe");
    assert_eq!(record.target, module_path!());
    assert_eq!(
        record.fields,
        vec![
            ("kind", FieldValue::Label("get".to_owned())),
            ("status", FieldValue::Count(200)),
            ("nodes", FieldValue::Count(3)),
            ("reason", FieldValue::Label("refused".to_owned())),
        ]
    );
    assert_eq!(record.dropped, 0);
}

#[test]
fn a_span_records_its_fields_outcome_and_duration_when_it_closes() {
    let ((), records) = capture(|| {
        let span = crate::span!(INFO, "probe_span", command = "close_dock");
        let result: Result<(), Refused> = span.in_scope(|| Err(Refused));
        span.record_result(&result);
        crate::record!(span, bytes = 12_u32);
    });
    let record = only(&records);
    assert_eq!(record.kind, RecordKind::Span);
    assert_eq!(record.name, "probe_span");
    assert_eq!(
        record.field_names(),
        ["command", "outcome", "reason", "bytes"]
    );
    assert_eq!(record.label("outcome"), Some("error"));
    assert_eq!(record.label("reason"), Some("refused"));
    assert!(record.duration_us.is_some());
    assert_eq!(record.dropped, 0);
}

#[test]
fn a_raw_record_on_a_macro_span_cannot_add_a_field() {
    let ((), records) = capture(|| {
        let span = crate::span!(INFO, "probe_span");
        span.inner().record("url", CONTENT[2]);
        span.inner().record("kind", CONTENT[0]);
    });
    let record = only(&records);
    assert!(record.fields.is_empty(), "{record:?}");
    // `url` is not declared on the span, so `tracing` ignores it; `kind` is declared and its value is refused.
    assert_eq!(record.dropped, 1);
    assert_no_content(&records);
}

#[test]
fn the_span_macro_declares_exactly_the_allow_list() {
    let (names, _) = capture(|| {
        let span = crate::span!(INFO, "probe_span");
        span.inner().metadata().map(|metadata| {
            metadata
                .fields()
                .iter()
                .map(|field| field.name())
                .collect::<Vec<_>>()
        })
    });
    let allowed: Vec<&str> = ALLOWED.iter().map(|(name, _)| *name).collect();
    assert_eq!(names, Some(allowed));
}

#[test]
fn every_allowed_name_takes_its_kind_through_the_macros() {
    let ((), records) = capture(|| {
        crate::event!(
            INFO,
            "probe",
            command = "a",
            kind = "a",
            outcome = "a",
            reason = "a",
            transport = "a",
            decision = "a",
            risk = "a",
            autonomy = "a",
            status = 1_u8,
            bytes = 1_u8,
            seq = 1_u8,
            nodes = 1_u8,
            redactions = 1_u8,
        );
    });
    let record = only(&records);
    assert_eq!(record.dropped, 0);
    assert_eq!(record.fields.len(), ALLOWED.len());
    for (name, value) in &record.fields {
        let expected = match value {
            FieldValue::Label(_) => Kind::Label,
            FieldValue::Count(_) => Kind::Count,
        };
        assert_eq!(kind_of(name), Some(expected), "{name}");
    }
}

#[test]
fn other_crates_are_not_recorded() {
    let ((), records) = capture(|| {
        tracing::info!(target: "tauri::manager", status = 200_u64);
        tracing::info!(target: "somicusx", status = 200_u64);
        let _span = tracing::info_span!(target: "hyper::client", "request", status = 200_u64);
    });
    assert!(records.is_empty(), "{records:?}");
}

#[test]
fn levels_above_the_maximum_are_not_recorded() {
    let ((), records) = capture_at(Level::INFO, || {
        crate::event!(DEBUG, "probe", nodes = 1_u8);
        crate::event!(WARN, "probe", nodes = 2_u8);
    });
    assert_eq!(only(&records).count("nodes"), Some(2));
}

#[test]
fn an_unnamed_raw_event_is_named_unnamed() {
    let ((), records) = capture(|| tracing::warn!(nodes = 1_u64));
    assert_eq!(only(&records).name, "unnamed");
}

#[test]
fn a_record_renders_only_kept_values() {
    let ((), records) = capture(|| {
        tracing::info!(name: "probe", kind = "get", url = CONTENT[2], status = 404_u64);
    });
    let line = only(&records).to_string();
    assert_eq!(
        line,
        format!(
            "INFO {} probe kind=get status=404 dropped=1",
            module_path!()
        )
    );
}
