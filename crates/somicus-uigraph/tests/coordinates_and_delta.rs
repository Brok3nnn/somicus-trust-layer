//! Property-based and unit tests for coordinate mapping and delta engine.

use proptest::prelude::*;
use somicus_uigraph::{
    ClassifiedText, CoordinateTransformer, DisplayInfo, FocusInfo, LogicalPoint, NodeActions,
    NodeId, NodeSource, NodeStates, PhysicalPoint, PhysicalRect, PrivacyClass, Role, SelectorSet,
    UiGraph, UiNode, WindowInfo,
};

#[allow(
    clippy::too_many_arguments,
    reason = "a test node is its id, name and bounds, spelled out at every call"
)]
fn make_test_node(id: &str, name: &str, x: i32, y: i32, w: u32, h: u32) -> UiNode {
    UiNode {
        id: NodeId::new(id),
        fp: somicus_uigraph::ElementFingerprint::default(),
        parent: None,
        children: Vec::new(),
        role: Role::Button,
        name: ClassifiedText::new(name, PrivacyClass::PublicLabel),
        description: None,
        value: None,
        states: NodeStates {
            enabled: true,
            focusable: true,
            ..Default::default()
        },
        bounds: Some(PhysicalRect::new(x, y, w, h)),
        actions: NodeActions {
            invoke: true,
            ..Default::default()
        },
        shortcut: None,
        relations: somicus_uigraph::NodeRelations::default(),
        source: NodeSource::Uia,
        selectors: SelectorSet::default(),
        confidence: 0.9,
        classification: PrivacyClass::PublicLabel,
    }
}

fn make_test_graph(nodes: Vec<UiNode>) -> UiGraph {
    let mut g = UiGraph {
        snapshot_id: "snap_1".to_string(),
        captured_at_unix_ms: 1_700_000_000_000,
        base_snapshot_id: None,
        displays: vec![
            DisplayInfo {
                display_id: 1,
                bounds_px: PhysicalRect::new(0, 0, 1920, 1080),
                scale_factor: 1.0,
                work_area_px: PhysicalRect::new(0, 0, 1920, 1040),
                is_primary: true,
            },
            DisplayInfo {
                display_id: 2,
                bounds_px: PhysicalRect::new(-1920, 0, 1920, 1080),
                scale_factor: 1.25,
                work_area_px: PhysicalRect::new(-1920, 0, 1920, 1040),
                is_primary: false,
            },
        ],
        apps: Vec::new(),
        windows: vec![WindowInfo {
            window_ref: "w1".to_string(),
            app_ref: "a1".to_string(),
            title: ClassifiedText::public_label("App"),
            bounds_px: PhysicalRect::new(100, 100, 800, 600),
            display_id: 1,
            z_order: 0,
            is_foreground: true,
            is_modal: false,
            url: None,
        }],
        focus: FocusInfo::default(),
        nodes,
    };
    g.recompute_fingerprints();
    g
}

#[test]
fn points_convert_across_displays_with_a_negative_origin() {
    let displays = vec![
        DisplayInfo {
            display_id: 1,
            bounds_px: PhysicalRect::new(0, 0, 2560, 1440),
            scale_factor: 1.5,
            work_area_px: PhysicalRect::new(0, 0, 2560, 1400),
            is_primary: true,
        },
        DisplayInfo {
            display_id: 2,
            bounds_px: PhysicalRect::new(-1920, 0, 1920, 1080),
            scale_factor: 1.0,
            work_area_px: PhysicalRect::new(-1920, 0, 1920, 1080),
            is_primary: false,
        },
    ];

    // Primary display point
    let p_prim = PhysicalPoint::new(1500, 600);
    let d1 = CoordinateTransformer::find_display(&displays, p_prim).expect("should find display 1");
    assert_eq!(d1.display_id, 1);
    let log_1 = CoordinateTransformer::physical_to_display_logical(d1, p_prim).unwrap();
    assert_eq!(log_1.x, 1000.0);
    assert_eq!(log_1.y, 400.0);

    let roundtrip_prim = CoordinateTransformer::display_logical_to_physical(d1, log_1).unwrap();
    assert_eq!(roundtrip_prim, p_prim);

    // Negative origin display point
    let p_neg = PhysicalPoint::new(-1000, 500);
    let d2 = CoordinateTransformer::find_display(&displays, p_neg).expect("should find display 2");
    assert_eq!(d2.display_id, 2);
    let log_2 = CoordinateTransformer::physical_to_display_logical(d2, p_neg).unwrap();
    assert_eq!(log_2.x, 920.0);
    assert_eq!(log_2.y, 500.0);

    let roundtrip_neg = CoordinateTransformer::display_logical_to_physical(d2, log_2).unwrap();
    assert_eq!(roundtrip_neg, p_neg);
}

#[test]
fn dom_viewport_points_map_to_physical_pixels() {
    let web_area = PhysicalRect::new(200, 150, 1200, 800);
    let dom_pt = LogicalPoint::new(100.0, 50.0);
    let dpr = 1.25;

    let phys_pt = CoordinateTransformer::dom_viewport_to_physical(web_area, dom_pt, dpr);
    assert_eq!(phys_pt.x, 200 + 125);
    assert_eq!(phys_pt.y, 150 + 63);

    let mapped_back =
        CoordinateTransformer::physical_to_dom_viewport(web_area, phys_pt, dpr).unwrap();
    assert!((mapped_back.x - dom_pt.x).abs() <= 1.0);
    assert!((mapped_back.y - dom_pt.y).abs() <= 1.0);
}

#[test]
fn identical_graphs_produce_an_empty_delta() {
    let g1 = make_test_graph(vec![
        make_test_node("e1", "Save", 10, 10, 80, 30),
        make_test_node("e2", "Cancel", 100, 10, 80, 30),
    ]);
    let mut g2 = g1.clone();
    g2.snapshot_id = "snap_2".to_string();

    let delta = g1.diff(&g2);
    assert!(delta.is_empty());
    assert_eq!(delta.patch_count(), 0);
}

#[test]
fn a_delta_lists_added_and_removed_nodes_and_only_the_changed_properties() {
    let g1 = make_test_graph(vec![
        make_test_node("e1", "Save", 10, 10, 80, 30),
        make_test_node("e2", "Cancel", 100, 10, 80, 30),
        make_test_node("e3", "Old Action", 200, 10, 80, 30),
    ]);

    let mut updated_e1 = make_test_node("e1", "Save As...", 10, 10, 90, 30);
    updated_e1.states.focused = true;

    let g2 = make_test_graph(vec![
        updated_e1,
        make_test_node("e2", "Cancel", 100, 10, 80, 30),
        make_test_node("e4", "New Action", 300, 10, 80, 30),
    ]);

    let delta = g1.diff(&g2);
    assert_eq!(delta.removed, vec![NodeId::new("e3")]);
    assert_eq!(delta.added.len(), 1);
    assert_eq!(delta.added[0].id, NodeId::new("e4"));

    assert_eq!(delta.changed.len(), 1);
    let ch = &delta.changed[0];
    assert_eq!(ch.id, NodeId::new("e1"));
    assert_eq!(
        ch.name.as_ref().map(ClassifiedText::raw_text),
        Some("Save As...")
    );
    assert!(ch.bounds.is_some());
    assert!(ch.states.is_some());
    assert!(ch.value.is_none());
    assert!(ch.actions.is_none());

    // Round-trip verification: applying delta to g1 matches g2's elements
    let applied = g1.apply_delta(&delta).expect("apply_delta should succeed");
    assert_eq!(applied.nodes.len(), g2.nodes.len());
    let e1 = applied.node(&NodeId::new("e1")).unwrap();
    assert_eq!(e1.name.raw_text(), "Save As...");
    assert!(e1.states.focused);
    assert!(applied.node(&NodeId::new("e3")).is_none());
    assert!(applied.node(&NodeId::new("e4")).is_some());
}

proptest! {
    #[test]
    fn prop_coordinate_transform_invertibility(
        px in -2000i32..4000i32,
        py in -2000i32..4000i32,
        scale in prop_oneof![Just(1.0f64), Just(1.25f64), Just(1.5f64), Just(1.75f64), Just(2.0f64)]
    ) {
        let display = DisplayInfo {
            display_id: 42,
            bounds_px: PhysicalRect::new(-3000, -3000, 10000, 10000),
            scale_factor: scale,
            work_area_px: PhysicalRect::new(-3000, -3000, 10000, 10000),
            is_primary: true,
        };

        let pt = PhysicalPoint::new(px, py);
        let logical = CoordinateTransformer::physical_to_display_logical(&display, pt).unwrap();
        let inverted = CoordinateTransformer::display_logical_to_physical(&display, logical).unwrap();

        // Rounding tolerance: at most 1 pixel difference due to float round
        prop_assert!((pt.x - inverted.x).abs() <= 1);
        prop_assert!((pt.y - inverted.y).abs() <= 1);
    }
}
