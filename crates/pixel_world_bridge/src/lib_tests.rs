use crate::render::{
    FragmentTerrainLod, agent_visual_style, build_grid_layout, classify_fragment_lod,
    fragment_screen_size_px, fragment_visual_style, location_visual_style,
};

use super::*;

#[cfg(target_arch = "wasm32")]
use wasm_bindgen_test::wasm_bindgen_test;

pub(super) fn sample_render_state_for_camera(selection_kind: &str) -> RenderState {
    RenderState {
        display_profile: display_profile::DisplayProfile::Legacy,
        display_source_cut: None,
        overview_query_context: None,
        exploration_mode: false,
        navigation: None,
        locale: "en".to_string(),
        world_bounds: Some(WorldBounds {
            width_cm: 3_000_000.0,
            depth_cm: 2_000_000.0,
            height_cm: 500_000.0,
        }),
        locations: vec![Location {
            id: "loc-0".to_string(),
            label: "Fragment Field Anchor".to_string(),
            pos: Position {
                x_cm: 1_500_000.0,
                y_cm: 1_000_000.0,
                z_cm: 0.0,
            },
            radius_cm: 30_000.0,
            resource_summary: "-".to_string(),
            size_hint_px: Some(10.0),
            marker_role: Some("logic_anchor".to_string()),
            marker_alpha: Some(0.32),
        }],
        fragment_terrain: vec![],
        micro_depot_facilities: vec![],
        module_visual_entities: vec![],
        agents: vec![Agent {
            id: "agent-0".to_string(),
            label: "Survey Agent".to_string(),
            pos: Some(Position {
                x_cm: 1_520_000.0,
                y_cm: 1_015_000.0,
                z_cm: 0.0,
            }),
            location_id: Some("loc-0".to_string()),
            resource_summary: "-".to_string(),
            status_badges: vec!["position=location_derived".to_string()],
            position_source: AgentPositionSource::LocationDerived,
            size_hint_px: Some(16.0),
            power_state: None,
        }],
        links: vec![],
        social_links: vec![],
        visual_hotspots: vec![],
        industry_aggregates: vec![],
        world_events: vec![],
        selection: Some(Selection {
            kind: selection_kind.to_string(),
            id: if selection_kind == "location" {
                "loc-0".to_string()
            } else {
                "agent-0".to_string()
            },
        }),
        receipt_target: None,
        recommended_target: None,
        active_intent_target: None,
    }
}

fn changed_snapshot(
    mounted: bool,
    render_state: Option<RenderState>,
    render_version: u64,
) -> SharedSnapshot {
    SharedSnapshot {
        mounted,
        render: RenderSnapshot::Changed {
            version: render_version,
            state: render_state.map(Box::new),
        },
        input_events: Vec::new(),
    }
}

fn assert_grid_layout_is_stable_for_same_camera_and_size() {
    let camera = CameraState::default();
    let left = build_grid_layout(&camera, 960.0, 540.0);
    let right = build_grid_layout(&camera, 960.0, 540.0);
    assert_eq!(left, right);
}

fn assert_grid_layout_changes_when_camera_pan_changes() {
    let mut camera = CameraState::default();
    let before = build_grid_layout(&camera, 960.0, 540.0);
    camera.pan_x_px = 10.0;
    let after = build_grid_layout(&camera, 960.0, 540.0);
    assert_ne!(before, after);
}

fn assert_fallback_point_stays_within_canvas() {
    let point = fallback_point_for_entity("agent-0", 960.0, 540.0, &CameraState::default());
    assert!(point.0 >= 0.0 && point.0 <= 960.0);
    assert!(point.1 >= 0.0 && point.1 <= 540.0);
}

fn assert_fragment_lod_uses_screen_space_size() {
    let bounds = WorldBounds {
        width_cm: 1_000_000.0,
        depth_cm: 1_000_000.0,
        height_cm: 0.0,
    };
    let mut camera = CameraState::default();
    let background_size = fragment_screen_size_px(10_000.0, &bounds, 1000.0, 1000.0, &camera);
    assert_eq!(
        classify_fragment_lod(background_size),
        FragmentTerrainLod::Background
    );

    camera.zoom = 2.0;
    let detail_size = fragment_screen_size_px(10_000.0, &bounds, 1000.0, 1000.0, &camera);
    assert_eq!(
        classify_fragment_lod(detail_size),
        FragmentTerrainLod::Detail
    );
    assert_eq!(classify_fragment_lod(1.0), FragmentTerrainLod::Hidden);
}

fn assert_fragment_lod_keeps_blocks_background_at_agent_readable_scale() {
    let bounds = WorldBounds {
        width_cm: 3_000_000.0,
        depth_cm: 2_000_000.0,
        height_cm: 0.0,
    };
    let camera = CameraState {
        zoom: 3.0,
        ..Default::default()
    };

    let screen_size = fragment_screen_size_px(12_000.0, &bounds, 960.0, 540.0, &camera);
    assert!(screen_size < 10.0);
    assert_eq!(
        classify_fragment_lod(screen_size),
        FragmentTerrainLod::Background
    );
}

fn assert_bevy_visual_styles_keep_fragments_background_behind_readable_agents() {
    let bounds = WorldBounds {
        width_cm: 3_000_000.0,
        depth_cm: 2_000_000.0,
        height_cm: 0.0,
    };
    let camera = CameraState {
        zoom: 3.0,
        ..Default::default()
    };
    let fragment = FragmentTerrainPatch {
        id: "fragment:loc-0:0".to_string(),
        location_id: "loc-0".to_string(),
        pos: Position {
            x_cm: 1_500_000.0,
            y_cm: 1_000_000.0,
            z_cm: 0.0,
        },
        footprint_cm: 12_000.0,
        dominant_compound: "silicate_matrix".to_string(),
        color: [141, 199, 170],
        emphasis: Some(0.58),
    };
    let location = Location {
        id: "loc-0".to_string(),
        label: "Fragment Field Anchor".to_string(),
        pos: Position {
            x_cm: 1_500_000.0,
            y_cm: 1_000_000.0,
            z_cm: 0.0,
        },
        radius_cm: 30_000.0,
        resource_summary: "-".to_string(),
        size_hint_px: Some(10.0),
        marker_role: Some("logic_anchor".to_string()),
        marker_alpha: Some(0.32),
    };
    let agent = Agent {
        id: "agent-0".to_string(),
        label: "Survey Agent".to_string(),
        pos: Some(Position {
            x_cm: 1_520_000.0,
            y_cm: 1_015_000.0,
            z_cm: 0.0,
        }),
        location_id: Some("loc-0".to_string()),
        resource_summary: "-".to_string(),
        status_badges: vec!["position=location_derived".to_string()],
        position_source: AgentPositionSource::LocationDerived,
        size_hint_px: Some(16.0),
        power_state: None,
    };

    let fragment_style = fragment_visual_style(&fragment, &bounds, 960.0, 540.0, &camera).unwrap();
    let location_style = location_visual_style(&location, 0.0);
    let agent_style = agent_visual_style(&agent, true, 0.0, 0);

    assert_eq!(fragment_style.lod, FragmentTerrainLod::Background);
    assert!(fragment_style.size_px < agent_style.size_px);
    assert!(fragment_style.alpha < location_style.alpha);
    assert!(location_style.alpha < 0.5);
    assert!(fragment_style.layer_z < location_style.layer_z);
    assert!(location_style.layer_z < agent_style.layer_z);
}

fn assert_selection_only_render_update_preserves_manual_camera_override() {
    let mut runtime = BevyRuntimeState {
        mounted: true,
        render_state: Some(sample_render_state_for_camera("agent")),
        render_version: 1,
        render_content_signature: render_content_signature(None),
        camera: CameraState {
            zoom: 2.25,
            pan_x_px: 42.0,
            pan_y_px: -18.0,
        },
        camera_fit_version: 1,
        camera_user_override: true,
        ..Default::default()
    };
    let initial_signature = render_content_signature(runtime.render_state.as_ref());
    runtime.render_content_signature = initial_signature;

    let next_state = sample_render_state_for_camera("location");
    let snapshot = changed_snapshot(true, Some(next_state), 2);
    apply_external_render_snapshot(&mut runtime, snapshot.mounted, snapshot.render);

    assert_eq!(runtime.render_version, 2);
    assert_eq!(runtime.render_content_signature, initial_signature);
    assert_eq!(runtime.camera.zoom, 2.25);
    assert_eq!(runtime.camera.pan_x_px, 42.0);
    assert_eq!(runtime.camera.pan_y_px, -18.0);
    assert!(runtime.camera_user_override);
    assert_eq!(runtime.camera_fit_version, 1);
    assert_eq!(
        runtime.pending_focus_target,
        Some(FocusTarget {
            kind: "location".to_string(),
            id: "loc-0".to_string(),
        })
    );
}

fn assert_content_render_update_clears_manual_camera_override() {
    let mut runtime = BevyRuntimeState {
        mounted: true,
        render_state: Some(sample_render_state_for_camera("agent")),
        render_version: 1,
        render_content_signature: render_content_signature(None),
        camera: CameraState {
            zoom: 2.25,
            pan_x_px: 42.0,
            pan_y_px: -18.0,
        },
        camera_fit_version: 1,
        camera_user_override: true,
        ..Default::default()
    };
    let initial_signature = render_content_signature(runtime.render_state.as_ref());
    runtime.render_content_signature = initial_signature;

    let mut next_state = sample_render_state_for_camera("agent");
    next_state.agents[0].pos = Some(Position {
        x_cm: 2_500_000.0,
        y_cm: 1_800_000.0,
        z_cm: 0.0,
    });
    let next_signature = render_content_signature(Some(&next_state));
    assert_ne!(next_signature, initial_signature);

    let snapshot = changed_snapshot(true, Some(next_state), 2);
    apply_external_render_snapshot(&mut runtime, snapshot.mounted, snapshot.render);

    assert_eq!(runtime.render_version, 2);
    assert_eq!(runtime.render_content_signature, next_signature);
    assert!(!runtime.camera_user_override);
    assert_eq!(runtime.camera_fit_version, 0);
    assert_eq!(runtime.pending_focus_target, None);
}

#[test]
fn agent_label_changes_the_render_content_signature() {
    let original = sample_render_state_for_camera("agent");
    let mut renamed = original.clone();
    renamed.agents[0].label = "Renamed Survey Agent".to_string();

    assert_ne!(
        render_content_signature(Some(&original)),
        render_content_signature(Some(&renamed)),
        "a player-visible Agent label must trigger reconciliation"
    );
}

fn sample_render_state_with_presentation_metadata() -> RenderState {
    let mut state = sample_render_state_for_camera("agent");
    state.module_visual_entities = vec![ModuleVisualEntity {
        factory_kind: None,
        id: "module-visual-0".to_string(),
        module_id: "module-0".to_string(),
        kind: "relay".to_string(),
        label: Some("Relay marker".to_string()),
        pos: Position {
            x_cm: 1_700_000.0,
            y_cm: 1_200_000.0,
            z_cm: 0.0,
        },
    }];
    state.links = vec![Link {
        id: "route-0".to_string(),
        kind: "logistics_route".to_string(),
        label: Some("North supply route".to_string()),
        from: Position {
            x_cm: 1_000_000.0,
            y_cm: 700_000.0,
            z_cm: 0.0,
        },
        to: Position {
            x_cm: 2_000_000.0,
            y_cm: 1_400_000.0,
            z_cm: 0.0,
        },
        emphasis: Some(0.72),
        status: Some("active".to_string()),
        source_class: Some("runtime_projection".to_string()),
        freshness: Some("current".to_string()),
    }];
    state.visual_hotspots = vec![VisualHotspot {
        id: "hotspot-0".to_string(),
        label: "Current objective".to_string(),
        kind: "goal".to_string(),
        pos: Position {
            x_cm: 1_900_000.0,
            y_cm: 1_500_000.0,
            z_cm: 0.0,
        },
        emphasis: Some(0.8),
        size_hint_px: Some(12.0),
    }];
    state
}

fn assert_presentation_metadata_update_reconciles(
    field: &str,
    mutate: impl FnOnce(&mut RenderState),
) {
    let original = sample_render_state_with_presentation_metadata();
    let initial_signature = render_content_signature(Some(&original));
    let mut runtime = BevyRuntimeState {
        mounted: true,
        render_state: Some(original.clone()),
        render_version: 1,
        render_content_signature: initial_signature,
        reactive_scheduling: true,
        camera: CameraState {
            zoom: 2.25,
            pan_x_px: 42.0,
            pan_y_px: -18.0,
        },
        camera_fit_version: 7,
        last_canvas_size: Some((960, 540)),
        camera_user_override: true,
        hit_regions_dirty: false,
        ..Default::default()
    };

    let mut changed = original;
    mutate(&mut changed);
    let next_signature = render_content_signature(Some(&changed));
    assert_ne!(
        next_signature, initial_signature,
        "{field} metadata must invalidate the render content signature"
    );

    apply_external_render_snapshot(
        &mut runtime,
        true,
        RenderSnapshot::Changed {
            version: 2,
            state: Some(Box::new(changed)),
        },
    );

    assert_eq!(runtime.render_content_signature, next_signature);
    assert!(
        runtime.needs_reconcile,
        "{field} metadata must trigger reactive reconciliation"
    );
    assert!(
        runtime.camera_user_override,
        "{field} metadata must preserve the manual camera"
    );
    assert_eq!(runtime.camera_fit_version, 7);
    assert_eq!(runtime.last_canvas_size, Some((960, 540)));
    assert!(!runtime.hit_regions_dirty);
}

#[test]
fn locale_changes_trigger_reconcile_without_camera_refit() {
    assert_presentation_metadata_update_reconciles("locale", |state| {
        state.locale = "zh-CN".to_string();
    });
}

#[test]
fn module_kind_changes_trigger_reconcile_without_camera_refit() {
    assert_presentation_metadata_update_reconciles("module kind", |state| {
        state.module_visual_entities[0].kind = "beacon".to_string();
    });
}

#[test]
fn module_id_changes_trigger_reconcile_without_camera_refit() {
    assert_presentation_metadata_update_reconciles("module id", |state| {
        state.module_visual_entities[0].module_id = "module-1".to_string();
    });
}

#[test]
fn module_label_changes_trigger_reconcile_without_camera_refit() {
    assert_presentation_metadata_update_reconciles("module label", |state| {
        state.module_visual_entities[0].label = Some("Beacon marker".to_string());
    });
}

#[test]
fn link_label_changes_trigger_reconcile_without_camera_refit() {
    assert_presentation_metadata_update_reconciles("link label", |state| {
        state.links[0].label = Some("South supply route".to_string());
    });
}

#[test]
fn hotspot_kind_changes_trigger_reconcile_without_camera_refit() {
    assert_presentation_metadata_update_reconciles("hotspot kind", |state| {
        state.visual_hotspots[0].kind = "blocker".to_string();
    });
}

#[test]
fn hotspot_label_changes_trigger_reconcile_without_camera_refit() {
    assert_presentation_metadata_update_reconciles("hotspot label", |state| {
        state.visual_hotspots[0].label = "Blocked route".to_string();
    });
}

#[test]
fn location_label_changes_trigger_reactive_reconcile_without_camera_reset() {
    let original = sample_render_state_for_camera("location");
    let initial_signature = render_content_signature(Some(&original));
    let mut runtime = BevyRuntimeState {
        mounted: true,
        render_state: Some(original.clone()),
        render_version: 1,
        render_content_signature: initial_signature,
        reactive_scheduling: true,
        camera: CameraState {
            zoom: 2.25,
            pan_x_px: 42.0,
            pan_y_px: -18.0,
        },
        camera_fit_version: 7,
        last_canvas_size: Some((960, 540)),
        camera_user_override: true,
        hit_regions_dirty: false,
        ..Default::default()
    };

    let mut renamed = original;
    renamed.locations[0].label = "Renamed Fragment Field Anchor".to_string();
    let next_signature = render_content_signature(Some(&renamed));
    assert_ne!(
        next_signature, initial_signature,
        "a player-visible Location label must invalidate reactive rendering"
    );

    apply_external_render_snapshot(
        &mut runtime,
        true,
        RenderSnapshot::Changed {
            version: 2,
            state: Some(Box::new(renamed)),
        },
    );

    assert_eq!(runtime.render_version, 2);
    assert_eq!(runtime.render_content_signature, next_signature);
    assert!(runtime.needs_reconcile);
    assert!(runtime.camera_user_override);
    assert_eq!(runtime.camera_fit_version, 7);
    assert_eq!(runtime.last_canvas_size, Some((960, 540)));
    assert!(!runtime.hit_regions_dirty);
    assert_eq!(runtime.pending_focus_target, None);
}

#[test]
fn agent_position_source_changes_trigger_reactive_reconcile_without_camera_reset() {
    let mut missing = sample_render_state_for_camera("agent");
    missing.agents[0].position_source = AgentPositionSource::Missing;
    let mut snapshot = missing.clone();
    snapshot.agents[0].position_source = AgentPositionSource::Snapshot;
    let mut derived = missing.clone();
    derived.agents[0].position_source = AgentPositionSource::LocationDerived;

    let missing_signature = render_content_signature(Some(&missing));
    assert_ne!(missing_signature, render_content_signature(Some(&snapshot)));
    assert_ne!(missing_signature, render_content_signature(Some(&derived)));
    assert_ne!(
        render_content_signature(Some(&snapshot)),
        render_content_signature(Some(&derived))
    );

    let mut runtime = BevyRuntimeState {
        mounted: true,
        render_state: Some(missing),
        render_content_signature: missing_signature,
        reactive_scheduling: true,
        camera: CameraState {
            zoom: 2.0,
            pan_x_px: 12.0,
            pan_y_px: -8.0,
        },
        camera_fit_version: 7,
        camera_user_override: true,
        hit_regions_dirty: false,
        ..Default::default()
    };
    apply_external_render_snapshot(
        &mut runtime,
        true,
        RenderSnapshot::Changed {
            version: 2,
            state: Some(Box::new(snapshot)),
        },
    );
    assert!(runtime.needs_reconcile);
    assert!(runtime.camera_user_override);
    assert_eq!(runtime.camera_fit_version, 7);
}

#[test]
fn location_resource_report_changes_trigger_reactive_reconcile_without_camera_or_hits() {
    let mut empty_report = sample_render_state_for_camera("location");
    empty_report.locations[0].resource_summary = "-".to_string();
    let mut published_report = empty_report.clone();
    published_report.locations[0].resource_summary = "water:12".to_string();

    let initial_signature = render_content_signature(Some(&empty_report));
    assert_ne!(
        render_content_signature(Some(&published_report)),
        initial_signature,
        "a published Location resource report must invalidate reactive render scheduling"
    );

    let mut runtime = BevyRuntimeState {
        mounted: true,
        render_state: Some(empty_report),
        render_content_signature: initial_signature,
        reactive_scheduling: true,
        needs_reconcile: false,
        hit_regions_dirty: false,
        ..Default::default()
    };
    let snapshot = changed_snapshot(true, Some(published_report), 2);
    apply_external_render_snapshot(&mut runtime, snapshot.mounted, snapshot.render);

    assert!(runtime.needs_reconcile);
    assert!(!runtime.hit_regions_dirty);
    assert_eq!(runtime.render_version, 2);
}

#[test]
fn social_link_only_render_updates_trigger_reconcile_without_camera_reset() {
    let social_link = SocialLink {
        id: "social_edge:7".to_string(),
        from: Position {
            x_cm: 1_200_000.0,
            y_cm: 700_000.0,
            z_cm: 0.0,
        },
        to: Position {
            x_cm: 1_800_000.0,
            y_cm: 1_300_000.0,
            z_cm: 0.0,
        },
        relation_kind: "ally".to_string(),
        lifecycle: "active".to_string(),
    };
    let cases = [
        ("add", Vec::new(), vec![social_link.clone()]),
        ("remove", vec![social_link.clone()], Vec::new()),
        (
            "endpoint",
            vec![social_link.clone()],
            vec![SocialLink {
                to: Position {
                    x_cm: 1_900_000.0,
                    ..social_link.to.clone()
                },
                ..social_link.clone()
            }],
        ),
        (
            "metadata",
            vec![social_link.clone()],
            vec![SocialLink {
                relation_kind: "supports".to_string(),
                ..social_link.clone()
            }],
        ),
    ];

    for (case, initial_links, next_links) in cases {
        let mut initial = sample_render_state_for_camera("agent");
        initial.social_links = initial_links;
        let initial_signature = render_content_signature(Some(&initial));
        let mut runtime = BevyRuntimeState {
            mounted: true,
            render_state: Some(initial),
            render_content_signature: initial_signature,
            reactive_scheduling: true,
            camera: CameraState {
                zoom: 2.0,
                pan_x_px: 12.0,
                pan_y_px: -8.0,
            },
            camera_fit_version: 7,
            camera_user_override: true,
            hit_regions_dirty: false,
            ..Default::default()
        };
        let mut next = sample_render_state_for_camera("agent");
        next.social_links = next_links;

        apply_external_render_snapshot(
            &mut runtime,
            true,
            RenderSnapshot::Changed {
                version: 2,
                state: Some(Box::new(next)),
            },
        );

        assert!(runtime.needs_reconcile, "social-only {case} must reconcile");
        assert!(
            runtime.camera_user_override,
            "social-only {case} must not reset camera"
        );
        assert_eq!(runtime.camera_fit_version, 7);
        assert!(!runtime.hit_regions_dirty);
    }
}

fn assert_label_only_render_update_reconciles_without_resetting_camera_or_follow() {
    let original = sample_render_state_for_camera("agent");
    let mut runtime = BevyRuntimeState {
        mounted: true,
        render_state: Some(original.clone()),
        render_version: 1,
        camera: CameraState {
            zoom: 2.25,
            pan_x_px: 42.0,
            pan_y_px: -18.0,
        },
        camera_fit_version: 7,
        last_canvas_size: Some((960, 540)),
        camera_user_override: true,
        active_follow_target: Some(FocusTarget {
            kind: "agent".to_string(),
            id: "agent-0".to_string(),
        }),
        hit_regions_dirty: false,
        ..Default::default()
    };
    let initial_signature = render_content_signature(Some(&original));
    runtime.render_content_signature = initial_signature;

    let mut renamed = original;
    renamed.agents[0].label = "Renamed Survey Agent".to_string();
    apply_external_render_snapshot(
        &mut runtime,
        true,
        RenderSnapshot::Changed {
            version: 2,
            state: Some(Box::new(renamed)),
        },
    );

    assert_ne!(runtime.render_content_signature, initial_signature);
    assert!(runtime.needs_reconcile);
    assert!(runtime.camera_user_override);
    assert_eq!(runtime.camera_fit_version, 7);
    assert_eq!(runtime.last_canvas_size, Some((960, 540)));
    assert!(!runtime.hit_regions_dirty);
    assert_eq!(runtime.pending_focus_target, None);
    assert_eq!(
        runtime.active_follow_target,
        Some(FocusTarget {
            kind: "agent".to_string(),
            id: "agent-0".to_string(),
        })
    );
}

fn assert_active_intent_only_update_preserves_manual_camera_and_focus() {
    let mut original = sample_render_state_for_camera("agent");
    original.active_intent_target = Some(ActiveIntentTarget {
        agent_id: "agent-0".to_string(),
        status: "accepted".to_string(),
    });
    let mut runtime = BevyRuntimeState {
        mounted: true,
        render_state: Some(original.clone()),
        render_content_signature: render_content_signature(Some(&original)),
        camera: CameraState {
            zoom: 2.0,
            pan_x_px: 12.0,
            pan_y_px: -8.0,
        },
        camera_fit_version: 7,
        camera_user_override: true,
        active_follow_target: Some(FocusTarget {
            kind: "agent".to_string(),
            id: "agent-0".to_string(),
        }),
        hit_regions_dirty: false,
        ..Default::default()
    };
    let mut next = original;
    next.active_intent_target.as_mut().unwrap().status = "blocked".to_string();

    apply_external_render_snapshot(
        &mut runtime,
        true,
        RenderSnapshot::Changed {
            version: 2,
            state: Some(Box::new(next)),
        },
    );

    assert!(runtime.needs_reconcile);
    assert!(runtime.camera_user_override);
    assert_eq!(runtime.camera_fit_version, 7);
    assert!(!runtime.hit_regions_dirty);
    assert_eq!(runtime.pending_focus_target, None);
    assert_eq!(
        runtime.active_follow_target,
        Some(FocusTarget {
            kind: "agent".to_string(),
            id: "agent-0".to_string()
        })
    );
}

fn assert_selection_change_sets_pending_focus_target() {
    let mut runtime = BevyRuntimeState {
        mounted: true,
        render_state: Some(sample_render_state_for_camera("agent")),
        render_version: 1,
        render_content_signature: render_content_signature(None),
        camera: CameraState {
            zoom: 1.5,
            pan_x_px: 12.0,
            pan_y_px: -6.0,
        },
        camera_fit_version: 1,
        camera_user_override: true,
        ..Default::default()
    };
    let initial_signature = render_content_signature(runtime.render_state.as_ref());
    runtime.render_content_signature = initial_signature;

    let next_state = sample_render_state_for_camera("location");
    let snapshot = changed_snapshot(true, Some(next_state), 2);
    apply_external_render_snapshot(&mut runtime, snapshot.mounted, snapshot.render);

    assert_eq!(
        runtime.pending_focus_target,
        Some(FocusTarget {
            kind: "location".to_string(),
            id: "loc-0".to_string(),
        })
    );
}

fn assert_followed_agent_content_update_reissues_pending_focus() {
    let mut runtime = BevyRuntimeState {
        mounted: true,
        render_state: Some(sample_render_state_for_camera("agent")),
        render_version: 1,
        render_content_signature: render_content_signature(None),
        active_follow_target: Some(FocusTarget {
            kind: "agent".to_string(),
            id: "agent-0".to_string(),
        }),
        ..Default::default()
    };
    let initial_signature = render_content_signature(runtime.render_state.as_ref());
    runtime.render_content_signature = initial_signature;

    let mut next_state = sample_render_state_for_camera("agent");
    next_state.agents[0].pos = Some(Position {
        x_cm: 2_100_000.0,
        y_cm: 1_400_000.0,
        z_cm: 0.0,
    });
    let snapshot = changed_snapshot(true, Some(next_state), 2);
    apply_external_render_snapshot(&mut runtime, snapshot.mounted, snapshot.render);

    assert_eq!(
        runtime.pending_focus_target,
        Some(FocusTarget {
            kind: "agent".to_string(),
            id: "agent-0".to_string(),
        })
    );
    assert_eq!(
        runtime.active_follow_target,
        Some(FocusTarget {
            kind: "agent".to_string(),
            id: "agent-0".to_string(),
        })
    );
}

fn assert_selection_change_to_location_clears_follow_target() {
    let mut runtime = BevyRuntimeState {
        mounted: true,
        render_state: Some(sample_render_state_for_camera("agent")),
        render_version: 1,
        render_content_signature: render_content_signature(None),
        active_follow_target: Some(FocusTarget {
            kind: "agent".to_string(),
            id: "agent-0".to_string(),
        }),
        ..Default::default()
    };
    let initial_signature = render_content_signature(runtime.render_state.as_ref());
    runtime.render_content_signature = initial_signature;

    let snapshot = changed_snapshot(true, Some(sample_render_state_for_camera("location")), 2);
    apply_external_render_snapshot(&mut runtime, snapshot.mounted, snapshot.render);

    assert_eq!(runtime.active_follow_target, None);
    assert_eq!(
        runtime.pending_focus_target,
        Some(FocusTarget {
            kind: "location".to_string(),
            id: "loc-0".to_string(),
        })
    );
}

fn assert_unchanged_render_snapshot_preserves_render_state_and_processes_input() {
    let original_state = sample_render_state_for_camera("agent");
    let mut runtime = BevyRuntimeState {
        mounted: true,
        render_state: Some(original_state),
        render_version: 7,
        camera: CameraState {
            pan_x_px: 12.0,
            pan_y_px: -8.0,
            ..Default::default()
        },
        ..Default::default()
    };
    let original_signature = render_content_signature(runtime.render_state.as_ref());
    runtime.render_content_signature = original_signature;

    let snapshot = SharedSnapshot {
        mounted: false,
        render: RenderSnapshot::Unchanged,
        input_events: vec![InputEvent::PointerDown {
            x: 100.0,
            y: 200.0,
            pointer_id: 3,
        }],
    };
    let SharedSnapshot {
        mounted,
        render,
        input_events,
    } = snapshot;
    apply_external_render_snapshot(&mut runtime, mounted, render);
    for event in input_events {
        process_input_event(&mut runtime, event);
    }

    assert!(!runtime.mounted);
    assert_eq!(runtime.render_version, 7);
    let selection = runtime
        .render_state
        .as_ref()
        .unwrap()
        .selection
        .as_ref()
        .unwrap();
    assert_eq!(selection.kind, "agent");
    assert_eq!(selection.id, "agent-0");
    assert_eq!(runtime.render_content_signature, original_signature);
    assert_eq!(runtime.drag_state.as_ref().unwrap().pointer_id, 3);
    assert_eq!(runtime.drag_state.as_ref().unwrap().start_pan_x, 12.0);
    assert_eq!(runtime.drag_state.as_ref().unwrap().start_pan_y, -8.0);
}

fn assert_hotspot_click_is_not_promoted_to_entity_selection() {
    assert_eq!(
        click_selection_from_hit(Some(("hotspot".to_string(), "hotspot-blocker".to_string()))),
        None,
        "hotspot hit regions are hover-only and must not emit select_entity"
    );
    assert_eq!(
        click_selection_from_hit(Some(("agent".to_string(), "agent-0".to_string()))),
        Some(("agent".to_string(), "agent-0".to_string())),
        "agent selection remains unchanged"
    );
    assert_eq!(
        click_selection_from_hit(Some(("location".to_string(), "loc-0".to_string()))),
        Some(("location".to_string(), "loc-0".to_string())),
        "location selection remains unchanged"
    );
}

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn grid_layout_is_stable_for_same_camera_and_size() {
    assert_grid_layout_is_stable_for_same_camera_and_size();
}

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn grid_layout_changes_when_camera_pan_changes() {
    assert_grid_layout_changes_when_camera_pan_changes();
}

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn fallback_point_stays_within_canvas() {
    assert_fallback_point_stays_within_canvas();
}

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn fragment_lod_uses_screen_space_size() {
    assert_fragment_lod_uses_screen_space_size();
}

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn fragment_lod_keeps_blocks_background_at_agent_readable_scale() {
    assert_fragment_lod_keeps_blocks_background_at_agent_readable_scale();
}

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn bevy_visual_styles_keep_fragments_background_behind_readable_agents() {
    assert_bevy_visual_styles_keep_fragments_background_behind_readable_agents();
}

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn selection_only_render_update_preserves_manual_camera_override() {
    assert_selection_only_render_update_preserves_manual_camera_override();
}

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn content_render_update_clears_manual_camera_override() {
    assert_content_render_update_clears_manual_camera_override();
}

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn label_only_render_update_reconciles_without_resetting_camera_or_follow() {
    assert_label_only_render_update_reconciles_without_resetting_camera_or_follow();
}

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn active_intent_only_update_preserves_manual_camera_and_focus() {
    assert_active_intent_only_update_preserves_manual_camera_and_focus();
}

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn selection_change_sets_pending_focus_target() {
    assert_selection_change_sets_pending_focus_target();
}

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn followed_agent_content_update_reissues_pending_focus() {
    assert_followed_agent_content_update_reissues_pending_focus();
}

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn selection_change_to_location_clears_follow_target() {
    assert_selection_change_to_location_clears_follow_target();
}

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn unchanged_render_snapshot_preserves_render_state_and_processes_input() {
    assert_unchanged_render_snapshot_preserves_render_state_and_processes_input();
}

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn hotspot_click_is_not_promoted_to_entity_selection() {
    assert_hotspot_click_is_not_promoted_to_entity_selection();
}

#[cfg(target_arch = "wasm32")]
#[wasm_bindgen_test]
fn wasm_grid_layout_is_stable_for_same_camera_and_size() {
    assert_grid_layout_is_stable_for_same_camera_and_size();
}

#[cfg(target_arch = "wasm32")]
#[wasm_bindgen_test]
fn wasm_grid_layout_changes_when_camera_pan_changes() {
    assert_grid_layout_changes_when_camera_pan_changes();
}

#[cfg(target_arch = "wasm32")]
#[wasm_bindgen_test]
fn wasm_fallback_point_stays_within_canvas() {
    assert_fallback_point_stays_within_canvas();
}

#[cfg(target_arch = "wasm32")]
#[wasm_bindgen_test]
fn wasm_fragment_lod_uses_screen_space_size() {
    assert_fragment_lod_uses_screen_space_size();
}

#[cfg(target_arch = "wasm32")]
#[wasm_bindgen_test]
fn wasm_fragment_lod_keeps_blocks_background_at_agent_readable_scale() {
    assert_fragment_lod_keeps_blocks_background_at_agent_readable_scale();
}

#[cfg(target_arch = "wasm32")]
#[wasm_bindgen_test]
fn wasm_bevy_visual_styles_keep_fragments_background_behind_readable_agents() {
    assert_bevy_visual_styles_keep_fragments_background_behind_readable_agents();
}

#[cfg(target_arch = "wasm32")]
#[wasm_bindgen_test]
fn wasm_selection_only_render_update_preserves_manual_camera_override() {
    assert_selection_only_render_update_preserves_manual_camera_override();
}

#[cfg(target_arch = "wasm32")]
#[wasm_bindgen_test]
fn wasm_content_render_update_clears_manual_camera_override() {
    assert_content_render_update_clears_manual_camera_override();
}

#[cfg(target_arch = "wasm32")]
#[wasm_bindgen_test]
fn wasm_label_only_render_update_reconciles_without_resetting_camera_or_follow() {
    assert_label_only_render_update_reconciles_without_resetting_camera_or_follow();
}

#[cfg(target_arch = "wasm32")]
#[wasm_bindgen_test]
fn wasm_selection_change_sets_pending_focus_target() {
    assert_selection_change_sets_pending_focus_target();
}

#[cfg(target_arch = "wasm32")]
#[wasm_bindgen_test]
fn wasm_followed_agent_content_update_reissues_pending_focus() {
    assert_followed_agent_content_update_reissues_pending_focus();
}

#[cfg(target_arch = "wasm32")]
#[wasm_bindgen_test]
fn wasm_selection_change_to_location_clears_follow_target() {
    assert_selection_change_to_location_clears_follow_target();
}

#[cfg(target_arch = "wasm32")]
#[wasm_bindgen_test]
fn wasm_unchanged_render_snapshot_preserves_render_state_and_processes_input() {
    assert_unchanged_render_snapshot_preserves_render_state_and_processes_input();
}
