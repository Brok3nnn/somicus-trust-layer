//! Candidate set ranking determinism and snapshot tests.

use somicus_uigraph::{
    AppInfo, CandidateRankingOptions, ClassifiedText, DisplayInfo, FocusInfo, NodeActions, NodeId,
    NodeRelations, NodeSource, NodeStates, PhysicalRect, PrivacyClass, Role, SelectorSet, UiGraph,
    UiNode, WindowInfo, extract_candidate_set,
};

#[allow(
    clippy::too_many_lines,
    clippy::vec_init_then_push,
    reason = "the graph is written node by node, each with a comment on its part in the ranking"
)]
fn create_test_graph() -> UiGraph {
    let mut nodes = Vec::new();

    // Window
    nodes.push(UiNode {
        id: NodeId::new("e1"),
        fp: somicus_uigraph::ElementFingerprint::default(),
        parent: None,
        children: vec![
            NodeId::new("e2"),
            NodeId::new("e3"),
            NodeId::new("e4"),
            NodeId::new("e5"),
        ],
        role: Role::Window,
        name: ClassifiedText::public_label("Sales Order Card"),
        description: None,
        value: None,
        states: NodeStates {
            enabled: true,
            ..Default::default()
        },
        bounds: Some(PhysicalRect::new(0, 0, 1200, 800)),
        actions: NodeActions::default(),
        shortcut: None,
        relations: NodeRelations::default(),
        source: NodeSource::Uia,
        selectors: SelectorSet::default(),
        confidence: 0.95,
        classification: PrivacyClass::PublicLabel,
    });

    // Button: "Post..."
    nodes.push(UiNode {
        id: NodeId::new("e2"),
        fp: somicus_uigraph::ElementFingerprint::default(),
        parent: Some(NodeId::new("e1")),
        children: Vec::new(),
        role: Role::Button,
        name: ClassifiedText::public_label("Post..."),
        description: Some(ClassifiedText::public_label("Post and finalize invoice")),
        value: None,
        states: NodeStates {
            enabled: true,
            ..Default::default()
        },
        bounds: Some(PhysicalRect::new(100, 50, 80, 30)),
        actions: NodeActions {
            invoke: true,
            ..Default::default()
        },
        shortcut: Some("F9".to_string()),
        relations: NodeRelations::default(),
        source: NodeSource::Uia,
        selectors: SelectorSet::default(),
        confidence: 0.95,
        classification: PrivacyClass::PublicLabel,
    });

    // Button: "Release"
    nodes.push(UiNode {
        id: NodeId::new("e3"),
        fp: somicus_uigraph::ElementFingerprint::default(),
        parent: Some(NodeId::new("e1")),
        children: Vec::new(),
        role: Role::Button,
        name: ClassifiedText::public_label("Release"),
        description: None,
        value: None,
        states: NodeStates {
            enabled: true,
            ..Default::default()
        },
        bounds: Some(PhysicalRect::new(200, 50, 80, 30)),
        actions: NodeActions {
            invoke: true,
            ..Default::default()
        },
        shortcut: None,
        relations: NodeRelations::default(),
        source: NodeSource::Uia,
        selectors: SelectorSet::default(),
        confidence: 0.90,
        classification: PrivacyClass::PublicLabel,
    });

    // Text field: "Customer No."
    nodes.push(UiNode {
        id: NodeId::new("e4"),
        fp: somicus_uigraph::ElementFingerprint::default(),
        parent: Some(NodeId::new("e1")),
        children: Vec::new(),
        role: Role::TextField,
        name: ClassifiedText::public_label("Customer No."),
        description: None,
        value: Some(ClassifiedText::content_value("⟦CUST_1001⟧")),
        states: NodeStates {
            enabled: true,
            focused: true,
            focusable: true,
            ..Default::default()
        },
        bounds: Some(PhysicalRect::new(100, 120, 200, 30)),
        actions: NodeActions {
            set_value: true,
            focus: true,
            ..Default::default()
        },
        shortcut: None,
        relations: NodeRelations::default(),
        source: NodeSource::Uia,
        selectors: SelectorSet::default(),
        confidence: 0.95,
        classification: PrivacyClass::ContentValue,
    });

    // Disabled Button: "Reopen"
    nodes.push(UiNode {
        id: NodeId::new("e5"),
        fp: somicus_uigraph::ElementFingerprint::default(),
        parent: Some(NodeId::new("e1")),
        children: Vec::new(),
        role: Role::Button,
        name: ClassifiedText::public_label("Reopen"),
        description: None,
        value: None,
        states: NodeStates {
            enabled: false,
            ..Default::default()
        },
        bounds: Some(PhysicalRect::new(300, 50, 80, 30)),
        actions: NodeActions {
            invoke: true,
            ..Default::default()
        },
        shortcut: None,
        relations: NodeRelations::default(),
        source: NodeSource::Uia,
        selectors: SelectorSet::default(),
        confidence: 0.90,
        classification: PrivacyClass::PublicLabel,
    });

    UiGraph {
        snapshot_id: "snap_card_01".to_string(),
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
            app_ref: "app_bc".to_string(),
            app_id: Some("business_central".to_string()),
            process_name: "BusinessCentral".to_string(),
            bundle_id: None,
            exe_path: None,
            version: None,
            pid: 4321,
        }],
        windows: vec![WindowInfo {
            window_ref: "w1".to_string(),
            app_ref: "app_bc".to_string(),
            title: ClassifiedText::public_label("Sales Order Card"),
            bounds_px: PhysicalRect::new(0, 0, 1200, 800),
            display_id: 1,
            z_order: 1,
            is_foreground: true,
            is_modal: false,
            url: None,
        }],
        focus: FocusInfo {
            node_id: Some(NodeId::new("e4")),
            window_ref: Some("w1".to_string()),
        },
        nodes,
    }
}

#[test]
fn ranking_the_same_graph_gives_the_same_candidates_in_the_same_order() {
    let graph = create_test_graph();
    let options = CandidateRankingOptions {
        goal_text: Some("post the invoice".to_string()),
        max_candidates: 32,
    };

    let first_run = extract_candidate_set(&graph, &options);

    // Verify 100 consecutive runs produce the exact same candidates in identical order
    for _ in 0..100 {
        let run = extract_candidate_set(&graph, &options);
        assert_eq!(first_run, run);
    }
}

#[test]
fn none_visible_is_always_the_last_candidate() {
    let graph = create_test_graph();
    let options = CandidateRankingOptions::default();
    let candidate_set = extract_candidate_set(&graph, &options);

    assert_eq!(candidate_set.none_visible.id.as_str(), "none_visible");
    let all = candidate_set.all_choices();
    assert_eq!(all.last().map(|c| c.id.as_str()), Some("none_visible"));
}

#[test]
fn the_control_named_in_the_goal_ranks_first_and_a_disabled_one_last() {
    let graph = create_test_graph();

    // With goal "post", the "Post..." button must be ranked #1
    let options = CandidateRankingOptions {
        goal_text: Some("post".to_string()),
        max_candidates: 32,
    };

    let set = extract_candidate_set(&graph, &options);
    assert_ne!(set.candidates.len(), 0);
    assert_eq!(set.candidates[0].id.as_str(), "e2");
    assert_eq!(set.candidates[0].name, "Post...");

    // Disabled button "Reopen" (e5) should be ranked below active elements
    let reopen_pos = set.candidates.iter().position(|c| c.id.as_str() == "e5");
    assert!(reopen_pos.is_some());
    assert_eq!(reopen_pos, Some(set.candidates.len() - 1));
}

#[test]
fn the_ranked_candidates_match_their_snapshot() {
    let graph = create_test_graph();
    let options = CandidateRankingOptions {
        goal_text: Some("post".to_string()),
        max_candidates: 4,
    };

    let set = extract_candidate_set(&graph, &options);
    let candidate_summary: Vec<(&str, &str, &str)> = set
        .candidates
        .iter()
        .map(|c| (c.id.as_str(), c.role.as_str(), c.name.as_str()))
        .collect();

    assert_eq!(
        candidate_summary,
        vec![
            ("e2", "button", "Post..."),
            ("e4", "text_field", "Customer No."),
            ("e3", "button", "Release"),
            ("e5", "button", "Reopen"),
        ]
    );
}
