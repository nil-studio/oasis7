use super::*;

pub(super) fn maybe_auto_fit_camera(runtime: &mut BevyRuntimeState, width: f64, height: f64) {
    if runtime.camera_fit_version == runtime.render_version || runtime.camera_user_override {
        return;
    }
    let Some(render_state) = runtime.render_state.as_ref() else {
        return;
    };
    if render_state.navigation.as_ref().is_some_and(|nav| {
        nav.mode == "overview"
            && !matches!(nav.overview_query, overview_query::Declaration::Legacy)
            && (!overview_query::permits(&nav.overview_query, render_state)
                || runtime.overview.pending_result.as_deref() != Some(nav.request_id.as_str()))
    }) {
        runtime.camera_fit_version = runtime.render_version;
        return;
    }
    let Some(world_bounds) = render_state.world_bounds.as_ref() else {
        navigation::finish_overview(runtime, "unavailable");
        return;
    };
    if !world_bounds.width_cm.is_finite()
        || !world_bounds.depth_cm.is_finite()
        || world_bounds.width_cm <= 0.
        || world_bounds.depth_cm <= 0.
    {
        navigation::finish_overview(runtime, "unavailable");
        return;
    }
    let modern = render_state.navigation.as_ref().is_some_and(|n| {
        n.mode == "overview" && !matches!(n.overview_query, overview_query::Declaration::Legacy)
    });
    if modern
        && render_state
            .overview_query_context
            .as_ref()
            .is_none_or(|c| c["display_limited"].as_bool() != Some(false))
    {
        runtime.camera_fit_version = runtime.render_version;
        navigation::finish_overview(runtime, "unavailable");
        return;
    }
    let mut points = Vec::new();
    let base_camera = CameraState::default();
    for location in &render_state.locations {
        if let Some(point) =
            to_canvas_point_unsnapped(&location.pos, world_bounds, width, height, &base_camera)
        {
            points.push(point);
        }
    }
    for fragment in render_state.fragment_terrain.iter().filter(|_| !modern) {
        if let Some(point) =
            to_canvas_point_unsnapped(&fragment.pos, world_bounds, width, height, &base_camera)
        {
            points.push(point);
        }
    }
    for facility in render_state
        .micro_depot_facilities
        .iter()
        .filter(|_| !modern)
    {
        if let Some(point) =
            to_canvas_point_unsnapped(&facility.pos, world_bounds, width, height, &base_camera)
        {
            points.push(point);
        }
    }
    for entity in &render_state.module_visual_entities {
        if let Some(point) =
            to_canvas_point_unsnapped(&entity.pos, world_bounds, width, height, &base_camera)
        {
            points.push(point);
        }
    }
    for agent in &render_state.agents {
        if let Some(pos) = agent.pos.as_ref()
            && let Some(point) =
                to_canvas_point_unsnapped(pos, world_bounds, width, height, &base_camera)
        {
            points.push(point);
        }
    }
    for hotspot in render_state.visual_hotspots.iter().filter(|_| !modern) {
        if let Some(point) =
            to_canvas_point_unsnapped(&hotspot.pos, world_bounds, width, height, &base_camera)
        {
            points.push(point);
        }
    }
    if modern {
        // These positions already passed the authorized overlay projector.
        // A historical event anchor remains historical; this does not follow it.
        for visual in render_state
            .industry_aggregates
            .iter()
            .chain(&render_state.world_events)
        {
            if let Some(point) =
                to_canvas_point_unsnapped(&visual.pos, world_bounds, width, height, &base_camera)
            {
                points.push(point);
            }
        }
    }
    if points.is_empty() {
        runtime.camera_fit_version = runtime.render_version;
        navigation::finish_overview(runtime, "unavailable");
        return;
    }
    let mut min_x = f64::INFINITY;
    let mut max_x = f64::NEG_INFINITY;
    let mut min_y = f64::INFINITY;
    let mut max_y = f64::NEG_INFINITY;
    for (x, y) in points {
        min_x = min_x.min(x);
        max_x = max_x.max(x);
        min_y = min_y.min(y);
        max_y = max_y.max(y);
    }
    let content_width = (max_x - min_x).max(40.0);
    let content_height = (max_y - min_y).max(40.0);
    let target_zoom_x = ((width - 180.0).max(120.0) / content_width).clamp(0.6, 3.5);
    let target_zoom_y = ((height - 180.0).max(120.0) / content_height).clamp(0.6, 3.5);
    let target_zoom = target_zoom_x.min(target_zoom_y);
    let content_center_x = (min_x + max_x) / 2.0;
    let content_center_y = (min_y + max_y) / 2.0;
    let centered_x = content_center_x - (width / 2.0);
    let centered_y = content_center_y - (height / 2.0);
    runtime.camera.zoom = target_zoom;
    runtime.camera.pan_x_px = -(centered_x * target_zoom);
    runtime.camera.pan_y_px = -(centered_y * target_zoom);
    runtime.camera_fit_version = runtime.render_version;
    navigation::finish_overview(runtime, "applied");
    let _ = camera_viewport::emit(runtime);
}

#[cfg(test)]
mod overview_partial_regression {
    use super::*;

    // Same authorized geometry and pinned-first order as the controlled 1k
    // fixture. Count=200 is the first partial page, not a complete 200 scene.
    // The current RenderState loses that distinction; this is the contract gap.
    fn scene(count: usize, mode: &str, request: &str) -> RenderState {
        let mut state = crate::lib_tests::sample_render_state_for_camera("agent");
        state.exploration_mode = true;
        state.navigation = Some(
            serde_json::from_value(json!({
                "request_id":request,"mode":mode
            }))
            .unwrap(),
        );
        state.world_bounds = Some(WorldBounds {
            width_cm: 1_000_000.,
            depth_cm: 1_000_000.,
            height_cm: 100_000.,
        });
        state.locations.clear();
        state.fragment_terrain.clear();
        state.micro_depot_facilities.clear();
        state.module_visual_entities.clear();
        state.visual_hotspots.clear();
        let prototype = state.agents[0].clone();
        let cells: Vec<(usize, usize)> = (0..200)
            .map(|i| (i % 40, 6 + i / 40))
            .chain(
                (0..1000)
                    .map(|i| (i % 40, i / 40))
                    .filter(|(_, row)| *row < 6 || *row > 10),
            )
            .collect();
        state.agents = std::iter::once(499)
            .chain((0..1000).filter(|id| *id != 499))
            .take(count)
            .map(|id| {
                let mut agent = prototype.clone();
                agent.id = format!("qa-agent-{id:04}");
                let (col, row) = cells[id];
                agent.pos = Some(Position {
                    x_cm: 256_250. + col as f64 * 12_500.,
                    y_cm: 275_000. + row as f64 * 18_750.,
                    z_cm: 0.,
                });
                agent
            })
            .collect();
        state
    }

    fn consume(
        runtime: &mut BevyRuntimeState,
        count: usize,
        mode: &str,
        request: &str,
        version: u64,
    ) {
        crate::apply_external_render_snapshot(
            runtime,
            true,
            RenderSnapshot::Changed {
                version,
                state: Some(Box::new(scene(count, mode, request))),
            },
        );
        maybe_auto_fit_camera(runtime, 1440., 1000.);
    }

    fn modern_scene(count: usize, completeness: &str, request: &str) -> RenderState {
        let mut state = scene(count, "overview", request);
        let context = overview_query::context(&json!({"world_id":"W","scope_id":"S",
            "source_cut":"C","request_generation":"query-1","completeness":completeness}));
        state.navigation.as_mut().unwrap().overview_query = overview_query::Declaration::Declared(
            json!({"version":1,"binding":context["binding"]}),
        );
        state.overview_query_context = Some(context);
        state
    }

    fn consume_modern(runtime: &mut BevyRuntimeState, state: RenderState, version: u64) {
        crate::apply_external_render_snapshot(
            runtime,
            true,
            RenderSnapshot::Changed {
                version,
                state: Some(Box::new(state)),
            },
        );
        maybe_auto_fit_camera(runtime, 1440., 1000.);
    }

    #[test]
    fn records_same_navigation_consuming_first_page_then_complete_scene() {
        let mut runtime = BevyRuntimeState::default();
        consume(&mut runtime, 1000, "stop", "1", 1);
        assert!(runtime.camera.pan_y_px.abs() < 1e-9);
        consume(&mut runtime, 200, "overview", "2", 2);
        assert_eq!(runtime.render_version, 2);
        assert_eq!(runtime.render_state.as_ref().unwrap().agents.len(), 200);
        assert!((runtime.camera.pan_y_px - 99.6923076923077).abs() < 1e-9);
        consume(&mut runtime, 1000, "overview", "2", 3);
        assert_eq!(runtime.render_version, 3);
        assert_eq!(runtime.camera_fit_version, 3);
        assert_eq!(runtime.render_state.as_ref().unwrap().agents.len(), 1000);
        assert!((runtime.camera.pan_y_px - 99.6923076923077).abs() < 1e-9);
    }

    // Modern behavior is opt-in; the legacy characterization above remains.
    #[test]
    fn overview_must_not_finalize_the_partial_extent_as_complete() {
        let mut runtime = BevyRuntimeState::default();
        consume(&mut runtime, 1000, "stop", "1", 1);
        consume_modern(&mut runtime, modern_scene(200, "partial", "2"), 2);
        assert!(runtime.camera.pan_y_px.abs() < 1e-9);
        consume_modern(&mut runtime, modern_scene(1000, "complete", "2"), 3);
        assert!(
            runtime.camera.pan_y_px.abs() < 1e-9,
            "same overview intent consumed partial version 2 and never fit complete version 3: pan_y={}",
            runtime.camera.pan_y_px
        );
        let mut background = modern_scene(1000, "complete", "2");
        for agent in &mut background.agents {
            agent.pos.as_mut().unwrap().y_cm += 10_000.;
        }
        consume_modern(&mut runtime, background, 4);
        assert!(
            runtime.camera.pan_y_px.abs() < 1e-9,
            "a background update replayed the consumed overview"
        );
    }

    #[test]
    fn complete_two_hundred_is_not_confused_with_a_partial_page() {
        let mut runtime = BevyRuntimeState::default();
        consume_modern(&mut runtime, modern_scene(200, "complete", "2"), 1);
        assert!((runtime.camera.pan_y_px - 99.6923076923077).abs() < 1e-9);
    }

    #[test]
    fn declared_invalid_or_changed_binding_cannot_fall_back_or_revive() {
        for invalid in [Value::Null, json!({"version":2}), json!({"version":1})] {
            let mut runtime = BevyRuntimeState::default();
            let mut state = modern_scene(200, "complete", "2");
            state.navigation.as_mut().unwrap().overview_query =
                overview_query::Declaration::Declared(invalid);
            consume_modern(&mut runtime, state, 1);
            assert_eq!(runtime.camera.pan_y_px, 0.);
            consume_modern(&mut runtime, modern_scene(200, "complete", "2"), 2);
            assert_eq!(
                runtime.camera.pan_y_px, 0.,
                "invalid declaration later revived"
            );
        }
        let mut runtime = BevyRuntimeState::default();
        consume_modern(&mut runtime, modern_scene(200, "partial", "2"), 1);
        let mut changed = modern_scene(200, "complete", "2");
        changed.overview_query_context.as_mut().unwrap()["binding"][1] = json!("new-cut");
        consume_modern(&mut runtime, changed, 2);
        consume_modern(&mut runtime, modern_scene(200, "complete", "2"), 3);
        assert_eq!(runtime.camera.pan_y_px, 0.);
    }

    #[test]
    fn manual_camera_or_new_navigation_retires_the_old_overview() {
        for manual in [true, false] {
            let mut runtime = BevyRuntimeState::default();
            consume_modern(&mut runtime, modern_scene(200, "partial", "2"), 1);
            if manual {
                crate::process_input_event(&mut runtime, InputEvent::Wheel { delta_y: -1. });
            } else {
                consume(&mut runtime, 200, "stop", "3", 2);
            }
            let camera = runtime.camera.clone();
            consume_modern(&mut runtime, modern_scene(200, "complete", "2"), 3);
            assert_eq!(runtime.camera.zoom, camera.zoom);
            assert_eq!(runtime.camera.pan_y_px, camera.pan_y_px);
        }
    }

    #[test]
    fn clearing_the_scene_retires_pending_and_consumed_ids_without_revival() {
        for completeness in ["partial", "complete"] {
            let mut runtime = BevyRuntimeState::default();
            consume_modern(&mut runtime, modern_scene(200, completeness, "2"), 1);
            crate::apply_external_render_snapshot(
                &mut runtime,
                false,
                RenderSnapshot::Changed {
                    version: 2,
                    state: None,
                },
            );
            let before = runtime.camera.clone();
            let mut late = modern_scene(200, "complete", "2");
            for agent in &mut late.agents {
                agent.pos.as_mut().unwrap().y_cm += 10_000.;
            }
            consume_modern(&mut runtime, late, 3);
            assert_eq!(runtime.camera.zoom, before.zoom);
            assert_eq!(runtime.camera.pan_y_px, before.pan_y_px);
            consume_modern(&mut runtime, modern_scene(200, "complete", "3"), 4);
            assert!(
                (runtime.camera.pan_y_px - 99.6923076923077).abs() < 1e-9,
                "a new explicit intent must still work after clearing"
            );
        }
    }

    #[test]
    fn complete_aggregate_or_historical_event_uses_only_its_projected_anchor() {
        for historical in [false, true] {
            let mut state = modern_scene(0, "complete", "2");
            let visual = spatial_display::SpatialVisual {
                id: "anchor".into(),
                label: "Anchor".into(),
                pos: Position {
                    x_cm: 600_000.,
                    y_cm: 400_000.,
                    z_cm: 0.,
                },
                historical,
                member_count: None,
            };
            if historical {
                state.world_events.push(visual);
            } else {
                state.industry_aggregates.push(visual);
            }
            let mut runtime = BevyRuntimeState::default();
            consume_modern(&mut runtime, state, 1);
            let (x, y) = to_canvas_point_unsnapped(
                &Position {
                    x_cm: 600_000.,
                    y_cm: 400_000.,
                    z_cm: 0.,
                },
                runtime
                    .render_state
                    .as_ref()
                    .unwrap()
                    .world_bounds
                    .as_ref()
                    .unwrap(),
                1440.,
                1000.,
                &runtime.camera,
            )
            .unwrap();
            assert!((x - 720.).abs() < 1e-9 && (y - 500.).abs() < 1e-9);
            assert!(runtime.active_follow_target.is_none());
        }
    }

    #[test]
    fn missing_bounds_or_empty_geometry_finishes_without_camera_movement() {
        for missing_bounds in [true, false] {
            let mut state = modern_scene(0, "complete", "2");
            if missing_bounds {
                state.world_bounds = None;
            }
            let mut runtime = BevyRuntimeState::default();
            consume_modern(&mut runtime, state, 1);
            assert_eq!(runtime.camera.pan_y_px, 0.);
            assert_eq!(runtime.overview.retired_generation, 2);
            assert!(runtime.overview.pending_result.is_none());
        }
    }

    #[test]
    fn invalid_request_id_or_missing_modern_context_cannot_use_initial_autofit() {
        for invalid in ["invalid-id", "0"] {
            let mut runtime = BevyRuntimeState::default();
            consume_modern(&mut runtime, modern_scene(200, "complete", invalid), 1);
            assert_eq!(runtime.camera.pan_y_px, 0.);
        }
        let mut state = modern_scene(200, "complete", "2");
        state.overview_query_context = None;
        let mut runtime = BevyRuntimeState::default();
        consume_modern(&mut runtime, state, 1);
        assert_eq!(runtime.camera.pan_y_px, 0.);
    }

    #[test]
    fn complete_read_with_output_omissions_does_not_fit_the_retained_subset() {
        let mut source = json!({"schema_version":1,"display_profile":"world_view_v1",
            "snapshot":{"time":1,"model":{},"config":{"space":{"width_cm":100,"depth_cm":100,"height_cm":10}}},
            "lists":{"agents":[],"locations":[]},"gameplay":{},
            "worldView":{"world_id":"W","scope_id":"S","source_cut":"C","request_generation":"read-1","completeness":"complete",
                "entities":([0,1].iter().map(|id|json!({"kind":"agent","entity_id":format!("A{id}"),
                    "render_record":{"name":"A","pos":{"x_cm":id,"y_cm":0,"z_cm":0}},
                    "provenance":{"geometry_source":"authoritative","freshness":"current","valid_at_source_cut":"C"}})).collect::<Vec<_>>())}});
        source["worldView"]["entities"][0]["render_record"]["kind"] = json!("k".repeat(8192));
        let context = overview_query::context(&source["worldView"]);
        source["navigation"] = json!({"mode":"overview","request_id":"2","overview_query":{"version":1,"binding":context["binding"]}});
        let output = crate::host_render_input::derive(&source.to_string()).unwrap();
        assert_eq!(output["agents"].as_array().unwrap().len(), 1);
        assert_eq!(output["overview_query_context"]["completeness"], "complete");
        assert_eq!(output["overview_query_context"]["display_limited"], true);
        let mut runtime = BevyRuntimeState::default();
        consume_modern(&mut runtime, serde_json::from_value(output).unwrap(), 1);
        assert_eq!(runtime.camera.pan_y_px, 0.);
        assert!(runtime.overview.pending_result.is_none());
    }
}
