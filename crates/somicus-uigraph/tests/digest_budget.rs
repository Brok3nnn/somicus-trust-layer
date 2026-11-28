//! Screen Digest token budgeting and serialization tests.

use somicus_uigraph::{
    AppInfo, ClassifiedText, DisplayInfo, FocusInfo, NodeActions, NodeId, NodeRelations,
    NodeSource, NodeStates, PhysicalRect, PrivacyClass, Role, ScreenDigestConfig, SelectorSet,
    UiGraph, UiNode, WindowInfo, generate_screen_digest,
};

#[allow(
    clippy::too_many_lines,
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    reason = "a synthetic graph of a few hundred nodes, whose index fits in i32"
)]
fn create_synthetic_graph(node_count: usize) -> UiGraph {
    let mut nodes = Vec::with_capacity(node_count);

    // Root window container
    nodes.push(UiNode {
        id: NodeId::new("e1"),
        fp: somicus_uigraph::ElementFingerprint::default(),
        parent: None,
        children: Vec::new(),
        role: Role::Window,
        name: ClassifiedText::public_label("Order Processing"),
        description: None,
        value: None,
        states: NodeStates {
            enabled: true,
            ..Default::default()
        },
        bounds: Some(PhysicalRect::new(0, 0, 1920, 1080)),
        actions: NodeActions::default(),
        shortcut: None,
        relations: NodeRelations::default(),
        source: NodeSource::Uia,
        selectors: SelectorSet::default(),
        confidence: 0.95,
        classification: PrivacyClass::PublicLabel,
    });

    for i in 2..=node_count {
        let (role, name, val, priv_class) = match i % 5 {
            0 => (
                Role::Button,
                ClassifiedText::public_label(format!("Action Button {i}")),
                None,
                PrivacyClass::PublicLabel,
            ),
            1 => (
                Role::TextField,
                ClassifiedText::public_label(format!("Customer Name {i}")),
                Some(ClassifiedText::personal("John Doe")),
                PrivacyClass::Personal,
            ),
            2 => (
                Role::PasswordField,
                ClassifiedText::public_label("API Secret"),
                Some(ClassifiedText::secret("supersecret")),
                PrivacyClass::Secret,
            ),
            3 => (
                Role::Tab,
                ClassifiedText::public_label(format!("Tab Page {i}")),
                None,
                PrivacyClass::PublicLabel,
            ),
            _ => (
                Role::Label,
                ClassifiedText::public_label(format!("Static Label {i}")),
                None,
                PrivacyClass::PublicLabel,
            ),
        };

        nodes.push(UiNode {
            id: NodeId::from_index(i),
            fp: somicus_uigraph::ElementFingerprint::default(),
            parent: Some(NodeId::new("e1")),
            children: Vec::new(),
            role,
            name,
            description: None,
            value: val,
            states: NodeStates {
                enabled: true,
                focused: i == 5,
                ..Default::default()
            },
            bounds: Some(PhysicalRect::new(10, (i as i32) * 30, 200, 25)),
            actions: NodeActions {
                invoke: role.is_interactive(),
                ..Default::default()
            },
            shortcut: None,
            relations: NodeRelations::default(),
            source: NodeSource::Uia,
            selectors: SelectorSet::default(),
            confidence: 0.90,
            classification: priv_class,
        });
    }

    // Attach children to e1
    let child_ids: Vec<NodeId> = (2..=node_count).map(NodeId::from_index).collect();
    nodes[0].children = child_ids;

    UiGraph {
        snapshot_id: "snap_1001".to_string(),
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
            app_id: Some("business_central".to_string()),
            process_name: "BusinessCentral.exe".to_string(),
            bundle_id: None,
            exe_path: None,
            version: Some("24.0".to_string()),
            pid: 1234,
        }],
        windows: vec![WindowInfo {
            window_ref: "w1".to_string(),
            app_ref: "app_1".to_string(),
            title: ClassifiedText::public_label("Order Processing Card"),
            bounds_px: PhysicalRect::new(0, 0, 1920, 1080),
            display_id: 1,
            z_order: 1,
            is_foreground: true,
            is_modal: false,
            url: None,
        }],
        focus: FocusInfo {
            node_id: Some(NodeId::new("e5")),
            window_ref: Some("w1".to_string()),
        },
        nodes,
    }
}

#[test]
fn the_digest_prunes_nodes_to_stay_within_its_token_budget() {
    let graph = create_synthetic_graph(80);

    // Test with small budget (100 tokens)
    let config_small = ScreenDigestConfig {
        max_token_budget: 100,
        locale: Some("en-US".to_string()),
        pack_screen: Some(("order_card".to_string(), 0.95)),
        recent_action: None,
    };

    let digest_small = generate_screen_digest(&graph, &config_small).expect("generation succeeds");
    assert!(
        digest_small.estimated_tokens <= 100,
        "estimated tokens {} must be <= 100",
        digest_small.estimated_tokens
    );
    assert!(
        digest_small.pruned_nodes > 0,
        "should prune nodes to fit budget"
    );

    // Test with default budget (2500 tokens)
    let config_large = ScreenDigestConfig {
        max_token_budget: 2500,
        ..Default::default()
    };
    let digest_large = generate_screen_digest(&graph, &config_large).expect("generation succeeds");
    assert!(digest_large.estimated_tokens <= 2500);
    assert_eq!(digest_large.pruned_nodes, 0);
}

#[test]
fn the_digest_masks_secret_and_personal_values() {
    let graph = create_synthetic_graph(10);
    let config = ScreenDigestConfig::default();
    let digest = generate_screen_digest(&graph, &config).expect("digest generation");

    // Secret must be masked as <secret>
    assert!(digest.text.contains("value=<secret>"));
    assert!(!digest.text.contains("supersecret"));

    // Personal value must be masked as <personal>
    assert!(digest.text.contains("value=<personal>"));
    assert!(!digest.text.contains("John Doe"));
}
