//! The Privacy Shield's token vault, text redaction and whole-graph redaction.
//!
//! The recall and latency evaluation (B3.05) runs in `evals/privacy`.

use somicus_privacy::recognizers::EntityType;
use somicus_privacy::shield::PrivacyShield;
use somicus_privacy::vault::TokenVault;
use somicus_uigraph::fingerprint::ElementFingerprint;
use somicus_uigraph::geometry::PhysicalRect;
use somicus_uigraph::graph::{AppInfo, DisplayInfo, FocusInfo, UiGraph, WindowInfo};
use somicus_uigraph::node::{
    NodeActions, NodeId, NodeRelations, NodeSource, NodeStates, SelectorSet, UiNode,
};
use somicus_uigraph::privacy::{ClassifiedText, PrivacyClass};
use somicus_uigraph::role::Role;

#[test]
fn the_vault_keeps_one_token_per_value_for_the_session_and_rehydrates_it() {
    let mut vault = TokenVault::new();

    let email = "alexander.mueller@example.de";
    let token1 = vault.tokenize(email, EntityType::Email);
    let token2 = vault.tokenize(email, EntityType::Email);

    // Session consistency: same raw value maps to same token
    assert_eq!(token1, token2);
    assert_eq!(token1, "⟦EMAIL_1⟧");

    // Different value gets next token
    let email2 = "contact@acme.corp";
    let token_other = vault.tokenize(email2, EntityType::Email);
    assert_eq!(token_other, "⟦EMAIL_2⟧");
    assert_ne!(token1, token_other);

    // Rehydration
    let model_response = format!("Opening customer record for {token1} at {token_other}");
    let rehydrated = vault.rehydrate(&model_response);
    assert_eq!(
        rehydrated,
        format!("Opening customer record for {email} at {email2}")
    );

    // Zeroization / clear
    vault.clear();
    assert_eq!(vault.len(), 0);
    assert!(vault.is_empty());
}

#[test]
fn the_shield_redacts_secret_and_personal_texts() {
    let mut shield = PrivacyShield::new();

    let secret_input = ClassifiedText::secret("SuperSecretToken12345");
    let redacted_secret = shield.redact_text(&secret_input);
    assert_eq!(redacted_secret.as_inner(), "⟦SECRET_1⟧");

    let personal_input = ClassifiedText::personal("Kovács István");
    let redacted_personal = shield.redact_text(&personal_input);
    assert_eq!(redacted_personal.as_inner(), "⟦PERSON_1⟧");

    let rehydrated = shield.rehydrate("User ⟦PERSON_1⟧ authenticated with ⟦SECRET_1⟧");
    assert_eq!(
        rehydrated,
        "User Kovács István authenticated with SuperSecretToken12345"
    );
}

fn create_test_uigraph() -> UiGraph {
    UiGraph {
        snapshot_id: "snap-test".to_string(),
        captured_at_unix_ms: 1_700_000_000_000,
        base_snapshot_id: None,
        displays: vec![DisplayInfo {
            display_id: 1,
            bounds_px: PhysicalRect::new(0, 0, 1920, 1080),
            scale_factor: 1.0,
            work_area_px: PhysicalRect::new(0, 0, 1920, 1040),
            is_primary: true,
        }],
        apps: vec![AppInfo {
            app_ref: "app_1".to_string(),
            app_id: Some("portal".to_string()),
            process_name: "portal.exe".to_string(),
            bundle_id: None,
            exe_path: None,
            version: None,
            pid: 100,
        }],
        windows: vec![WindowInfo {
            window_ref: "win_1".to_string(),
            app_ref: "app_1".to_string(),
            title: ClassifiedText::public_label("Account: alice@company.com"),
            bounds_px: PhysicalRect::new(0, 0, 1920, 1080),
            display_id: 1,
            z_order: 0,
            is_foreground: true,
            is_modal: false,
            url: None,
        }],
        focus: FocusInfo::default(),
        nodes: vec![UiNode {
            id: NodeId::new("node_1"),
            fp: ElementFingerprint::new(1),
            parent: None,
            children: Vec::new(),
            role: Role::Label,
            name: ClassifiedText::public_label("Customer Name: Alexander Müller"),
            description: None,
            value: Some(ClassifiedText::content_value("IBAN DE89370400440532013000")),
            states: NodeStates::default(),
            bounds: Some(PhysicalRect::new(0, 0, 100, 20)),
            actions: NodeActions::default(),
            shortcut: None,
            relations: NodeRelations::default(),
            source: NodeSource::Uia,
            selectors: SelectorSet::default(),
            confidence: 0.90,
            classification: PrivacyClass::PublicLabel,
        }],
    }
}

#[test]
fn the_shield_redacts_every_text_of_a_ui_graph() {
    let mut shield = PrivacyShield::new();
    let graph = create_test_uigraph();

    let redacted_graph = shield.redact_uigraph(&graph);
    let inner_graph = redacted_graph.into_inner();

    // Window title was redacted
    assert!(
        !inner_graph.windows[0]
            .title
            .raw_text()
            .contains("alice@company.com")
    );
    assert!(
        inner_graph.windows[0]
            .title
            .raw_text()
            .contains("⟦EMAIL_1⟧")
    );

    // Node name was redacted
    assert!(
        !inner_graph.nodes[0]
            .name
            .raw_text()
            .contains("Alexander Müller")
    );
    assert!(inner_graph.nodes[0].name.raw_text().contains("⟦PERSON_1⟧"));

    // Node value was redacted
    assert!(
        !inner_graph.nodes[0]
            .value
            .as_ref()
            .unwrap()
            .raw_text()
            .contains("DE89370400440532013000")
    );
    assert!(
        inner_graph.nodes[0]
            .value
            .as_ref()
            .unwrap()
            .raw_text()
            .contains("⟦IBAN_1⟧")
    );
}
