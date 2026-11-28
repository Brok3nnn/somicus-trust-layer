use somicus_telemetry::testing::capture;
use somicus_uigraph::fingerprint::ElementFingerprint;
use somicus_uigraph::geometry::PhysicalRect;
use somicus_uigraph::graph::{AppInfo, FocusInfo, WindowInfo};
use somicus_uigraph::node::{
    NodeActions, NodeId, NodeRelations, NodeSource, NodeStates, SelectorSet, UiNode,
};
use somicus_uigraph::role::Role;

use super::*;

#[test]
fn a_match_inside_another_becomes_one_token() {
    let mut shield = PrivacyShield::new();
    let redacted = shield.redact_text(&ClassifiedText::content_value(
        "password: jane.doe@example.com and more text here",
    ));
    assert_eq!(redacted.as_inner(), "⟦SECRET_1⟧ and more text here");
    assert_eq!(
        shield.rehydrate(redacted.as_inner()),
        "password: jane.doe@example.com and more text here"
    );
}

#[test]
fn overlapping_matches_before_multibyte_text_do_not_panic() {
    let mut shield = PrivacyShield::new();
    let redacted = shield.redact_text(&ClassifiedText::content_value(
        "password: jane.doe@example.com és még több",
    ));
    assert_eq!(redacted.as_inner(), "⟦SECRET_1⟧ és még több");
}

#[test]
fn debug_output_never_shows_a_mapping() {
    let mut shield = PrivacyShield::new();
    let _ = shield.redact_text(&ClassifiedText::content_value(
        "Write to jane.doe@example.com",
    ));
    let printed = format!("{shield:?}");
    assert!(!printed.contains("jane.doe"), "{printed}");
    assert_eq!(
        printed,
        "PrivacyShield { vault: TokenVault { entries: 1, .. }, replaced: 1, names: None }"
    );
}

#[test]
fn redaction_spans_carry_only_classes_sizes_and_counts() {
    let mut shield = PrivacyShield::new();
    let digest = ScreenDigest {
        text: "Invoice for jane.doe@example.com".to_owned(),
        estimated_tokens: 8,
        included_nodes: 1,
        pruned_nodes: 0,
    };
    let graph = graph_with(
        Some("https://mail.example.test/inbox/jane.doe?token=synthetic"),
        Some("/home/jane.doe/Applications/Portal.app/Contents/MacOS/Portal"),
    );
    let ((), records) = capture(|| {
        let _ = shield.redact_text(&ClassifiedText::content_value(
            "Write to jane.doe@example.com",
        ));
        let _ = shield.redact_digest(&digest);
        let _ = shield.redact_uigraph(&graph);
    });

    let text = &records[0];
    assert_eq!(text.name, "privacy_redact_text");
    assert_eq!(text.field_names(), ["kind", "redactions"]);
    assert_eq!(text.label("kind"), Some("content_value"));
    assert_eq!(text.count("redactions"), Some(1));
    let digest_span = &records[1];
    assert_eq!(digest_span.name, "privacy_redact_digest");
    assert_eq!(
        digest_span.count("bytes"),
        u64::try_from(digest.text.len()).ok()
    );
    assert_eq!(digest_span.count("redactions"), Some(1));
    let graph_span = records.last();
    assert_eq!(
        graph_span.map(|record| record.name),
        Some("privacy_redact_graph")
    );
    assert_eq!(graph_span.and_then(|record| record.count("nodes")), Some(1));
    for record in &records {
        assert_eq!(record.dropped, 0, "{record:?}");
        let line = record.to_string();
        for content in [
            "jane",
            "Invoice",
            "example",
            "Portal",
            "home",
            "Inbox",
            "synthetic",
        ] {
            assert!(!line.contains(content), "{line}");
        }
    }
}

fn graph_with(url: Option<&str>, exe_path: Option<&str>) -> UiGraph {
    UiGraph {
        snapshot_id: "snap-test".to_owned(),
        captured_at_unix_ms: 1_700_000_000_000,
        base_snapshot_id: None,
        displays: Vec::new(),
        apps: vec![AppInfo {
            app_ref: "app_1".to_owned(),
            app_id: Some("portal".to_owned()),
            process_name: "Portal".to_owned(),
            bundle_id: None,
            exe_path: exe_path.map(ToOwned::to_owned),
            version: None,
            pid: 100,
        }],
        windows: vec![WindowInfo {
            window_ref: "win_1".to_owned(),
            app_ref: "app_1".to_owned(),
            title: ClassifiedText::public_label("Inbox"),
            bounds_px: PhysicalRect::new(0, 0, 1920, 1080),
            display_id: 1,
            z_order: 0,
            is_foreground: true,
            is_modal: false,
            url: url.map(ToOwned::to_owned),
        }],
        focus: FocusInfo::default(),
        nodes: vec![UiNode {
            id: NodeId::new("node_1"),
            fp: ElementFingerprint::new(1),
            parent: None,
            children: Vec::new(),
            role: Role::Button,
            name: ClassifiedText::public_label("Send"),
            description: None,
            value: None,
            states: NodeStates::default(),
            bounds: Some(PhysicalRect::new(0, 0, 100, 20)),
            actions: NodeActions::default(),
            shortcut: None,
            relations: NodeRelations::default(),
            source: NodeSource::Ax,
            selectors: SelectorSet::default(),
            confidence: 0.9,
            classification: PrivacyClass::PublicLabel,
        }],
    }
}

#[test]
fn selectors_and_shortcuts_of_a_graph_are_redacted_too() {
    let mut shield = PrivacyShield::new();
    let mut graph = graph_with(None, None);
    let email = "jane.doe@example.test";
    graph.nodes[0].shortcut = Some(format!("Mail {email}"));
    graph.nodes[0].selectors = SelectorSet {
        dom_id: Some(format!("sender-{email}")),
        a11y_path: Some(format!("list/row[{email}]")),
        relative_anchors: vec![format!("in row '{email}'")],
        visual_anchor: Some(format!("right of {email}")),
        ..SelectorSet::default()
    };
    let json = shield
        .redact_uigraph(&graph)
        .to_json_bytes()
        .map(Redacted::into_inner)
        .unwrap_or_default();
    let json = String::from_utf8_lossy(&json);
    assert!(json.contains("⟦EMAIL_1⟧"), "{json}");
    assert!(!json.contains(email), "{json}");
}

#[test]
fn window_urls_keep_only_their_origin() {
    let mut shield = PrivacyShield::new();
    for (url, kept) in [
        (
            "https://user:synthetic-secret@Mail.Example.test:8443/inbox/jane.doe?token=abc#top",
            Some("https://mail.example.test:8443"),
        ),
        ("http://127.0.0.1:3000/", Some("http://127.0.0.1:3000")),
        ("file:///home/jane.doe/Documents/salary.xlsx", None),
        ("about:blank", None),
        ("https://", None),
        ("https://bad host/x", None),
    ] {
        let graph = shield.redact_uigraph(&graph_with(Some(url), None));
        assert_eq!(graph.as_inner().windows[0].url.as_deref(), kept, "{url}");
    }
}

#[test]
fn a_recognizer_that_cannot_run_redacts_the_whole_text() {
    let mut shield = PrivacyShield::new();
    let redacted = shield.pseudonymize_detected(
        "Customer: Jane Smith, IBAN DE89370400440532013000",
        Err(PrivacyError::RecognitionError("synthetic".to_owned())),
    );
    assert_eq!(redacted, "⟦SECRET_1⟧");
}

#[test]
fn executable_paths_keep_only_their_file_name() {
    let mut shield = PrivacyShield::new();
    for (path, kept) in [
        (
            "/home/jane.doe/Applications/Portal.app/Contents/MacOS/Portal",
            Some("Portal"),
        ),
        (
            r"C:\Users\jane.doe\AppData\Local\Portal\portal.exe",
            Some("portal.exe"),
        ),
        ("portal", Some("portal")),
        ("/home/jane.doe/", None),
    ] {
        let graph = shield.redact_uigraph(&graph_with(None, Some(path)));
        assert_eq!(graph.as_inner().apps[0].exe_path.as_deref(), kept, "{path}");
    }
}
