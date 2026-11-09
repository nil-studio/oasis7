use super::*;

fn state(mode: &str, request_id: &str) -> RenderState {
    let mut state = crate::lib_tests::sample_render_state_for_camera("agent");
    state.exploration_mode = true;
    state.navigation = Some(Navigation {
        mode: mode.into(),
        request_id: request_id.into(),
        bounds: None,
        overview_query: overview_query::Declaration::Legacy,
        target: Some(Selection {
            kind: "agent".into(),
            id: "agent-0".into(),
        }),
    });
    state
}

fn update(runtime: &mut BevyRuntimeState, state: RenderState, version: u64) {
    apply_external_render_snapshot(
        runtime,
        true,
        RenderSnapshot::Changed {
            version,
            state: Some(Box::new(state)),
        },
    );
}

#[test]
fn selection_in_exploration_does_not_move_camera() {
    let mut runtime = BevyRuntimeState::default();
    let mut state = state("stop", "0");
    state.navigation = None;
    runtime.camera_user_override = true;
    update(&mut runtime, state, 1);
    assert!(runtime.pending_focus_target.is_none());
    assert!(runtime.active_follow_target.is_none());
    assert!(runtime.camera_user_override);
}

#[test]
fn same_target_can_be_located_followed_stopped_and_located_again() {
    let mut runtime = BevyRuntimeState::default();
    update(&mut runtime, state("locate", "1"), 1);
    assert!(runtime.pending_focus_target.is_some());
    assert!(runtime.active_follow_target.is_none());
    update(&mut runtime, state("follow", "2"), 2);
    assert!(runtime.active_follow_target.is_some());
    update(&mut runtime, state("stop", "3"), 3);
    assert!(runtime.pending_focus_target.is_none());
    assert!(runtime.active_follow_target.is_none());
    update(&mut runtime, state("locate", "4"), 4);
    assert!(runtime.pending_focus_target.is_some());
}

#[test]
fn missing_trusted_position_pauses_follow_without_automatic_resume() {
    let mut runtime = BevyRuntimeState::default();
    update(&mut runtime, state("follow", "1"), 1);
    let mut missing = state("follow", "1");
    missing.agents[0].pos = None;
    update(&mut runtime, missing, 2);
    assert!(runtime.active_follow_target.is_none());
    update(&mut runtime, state("follow", "1"), 3);
    assert!(runtime.active_follow_target.is_none());
    assert!(runtime.pending_focus_target.is_none());
    update(&mut runtime, state("follow", "2"), 4);
    assert!(runtime.active_follow_target.is_some());
}

#[test]
fn manual_camera_override_does_not_replay_same_navigation_request() {
    let mut runtime = BevyRuntimeState::default();
    update(&mut runtime, state("follow", "1"), 1);
    runtime.active_follow_target = None;
    runtime.pending_focus_target = None;
    update(&mut runtime, state("follow", "1"), 2);
    assert!(runtime.active_follow_target.is_none());
    assert!(runtime.pending_focus_target.is_none());
}

#[test]
fn changing_selection_keeps_camera_fit_and_overview_explicitly_resets_it() {
    let mut runtime = BevyRuntimeState::default();
    update(&mut runtime, state("stop", "1"), 1);
    runtime.camera_fit_version = 1;
    let mut next = state("stop", "1");
    next.selection = Some(Selection {
        kind: "location".into(),
        id: "loc-0".into(),
    });
    update(&mut runtime, next, 2);
    assert_eq!(runtime.camera_fit_version, 2);
    assert!(runtime.pending_focus_target.is_none());
    update(&mut runtime, state("overview", "2"), 3);
    assert_eq!(runtime.camera_fit_version, 0);
    assert!(runtime.needs_reconcile);
}

#[test]
fn fit_bounds_uses_world_transform_and_does_not_create_follow_or_business_subject() {
    let mut runtime = BevyRuntimeState::default();
    let mut next = state("fit_bounds", "fit-1");
    next.navigation.as_mut().unwrap().bounds = Some(
        json!({"min_x_cm":"1200000","max_x_cm":"1800000","min_y_cm":"800000","max_y_cm":"1200000"}),
    );
    update(&mut runtime, next, 1);
    super::apply_pending_fit_bounds(&mut runtime, 1000.0, 600.0);
    assert!(runtime.camera.zoom > 3.5);
    let zoom = runtime.camera.zoom;
    process_input_event(&mut runtime, InputEvent::Wheel { delta_y: -1.0 });
    assert!(runtime.camera.zoom > zoom);
    process_input_event(&mut runtime, InputEvent::Wheel { delta_y: 1.0 });
    let b = runtime
        .render_state
        .as_ref()
        .unwrap()
        .world_bounds
        .as_ref()
        .unwrap();
    let (x, y) = to_canvas_point(
        &Position {
            x_cm: 1500000.0,
            y_cm: 1000000.0,
            z_cm: 0.0,
        },
        b,
        1000.0,
        600.0,
        &runtime.camera,
    )
    .unwrap();
    assert!((x - 500.0).abs() < 0.001 && (y - 300.0).abs() < 0.001);
    assert!(runtime.active_follow_target.is_none());
    assert!(runtime.pending_focus_target.is_none());
    assert!(runtime.pending_fit_bounds.is_none());
}

#[test]
fn event_can_be_located_at_its_original_point_but_never_becomes_a_follow_target() {
    let mut runtime = BevyRuntimeState::default();
    let mut next = state("locate", "event-1");
    next.world_events
        .push(crate::spatial_display::SpatialVisual {
            id: "event".into(),
            label: "Published".into(),
            pos: Position {
                x_cm: 100.0,
                y_cm: 200.0,
                z_cm: 0.0,
            },
            historical: true,
            member_count: None,
        });
    next.navigation.as_mut().unwrap().target = Some(Selection {
        kind: "world_event".into(),
        id: "event".into(),
    });
    update(&mut runtime, next.clone(), 1);
    assert_eq!(
        runtime.pending_focus_target.as_ref().unwrap().kind,
        "world_event"
    );
    next.navigation.as_mut().unwrap().request_id = "event-2".into();
    next.navigation.as_mut().unwrap().mode = "follow".into();
    update(&mut runtime, next, 2);
    assert!(runtime.active_follow_target.is_none());
    assert!(runtime.pending_focus_target.is_none());
}

#[test]
fn fit_subpixel_bounds_preserves_center_before_final_pixel_snap() {
    let mut runtime = BevyRuntimeState::default();
    runtime.render_state = Some(state("stop", "0"));
    runtime.pending_fit_bounds =
        Some(json!({"min_x_cm":1501400,"max_x_cm":1501600,"min_y_cm":999900,"max_y_cm":1000100}));
    super::apply_pending_fit_bounds(&mut runtime, 1280., 720.);
    // 200cm / 2,000,000cm * 680 CSS px = .068px; usable height is 656px.
    assert!((runtime.camera.zoom - 9647.058823529412).abs() < 1e-6);
    assert!(
        (runtime.camera.pan_x_px + 5981.176470588235).abs() < 1e-6,
        "subpixel bounds must not collapse through display snapping"
    );
}
