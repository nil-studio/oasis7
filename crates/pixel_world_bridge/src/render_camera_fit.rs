use super::*;

pub(super) fn maybe_auto_fit_camera(runtime: &mut BevyRuntimeState, width: f64, height: f64) {
    if runtime.camera_fit_version == runtime.render_version || runtime.camera_user_override {
        return;
    }
    let Some(render_state) = runtime.render_state.as_ref() else {
        return;
    };
    let Some(world_bounds) = render_state.world_bounds.as_ref() else {
        return;
    };
    let mut points = Vec::new();
    let base_camera = CameraState::default();
    for location in &render_state.locations {
        if let Some(point) =
            to_canvas_point_unsnapped(&location.pos, world_bounds, width, height, &base_camera)
        {
            points.push(point);
        }
    }
    for fragment in &render_state.fragment_terrain {
        if let Some(point) =
            to_canvas_point_unsnapped(&fragment.pos, world_bounds, width, height, &base_camera)
        {
            points.push(point);
        }
    }
    for facility in &render_state.micro_depot_facilities {
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
    for hotspot in &render_state.visual_hotspots {
        if let Some(point) =
            to_canvas_point_unsnapped(&hotspot.pos, world_bounds, width, height, &base_camera)
        {
            points.push(point);
        }
    }
    if points.is_empty() {
        runtime.camera_fit_version = runtime.render_version;
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

    // Candidate desired behavior for explicit overview: a progressive page
    // must not consume the request as if it described the complete extent.
    // Intentionally red until the missing completeness/navigation contract is
    // agreed and carried into RenderState; not a production-ready change.
    #[test]
    fn overview_must_not_finalize_the_partial_extent_as_complete() {
        let mut runtime = BevyRuntimeState::default();
        consume(&mut runtime, 1000, "stop", "1", 1);
        consume(&mut runtime, 200, "overview", "2", 2);
        consume(&mut runtime, 1000, "overview", "2", 3);
        assert!(
            runtime.camera.pan_y_px.abs() < 1e-9,
            "same overview intent consumed partial version 2 and never fit complete version 3: pan_y={}",
            runtime.camera.pan_y_px
        );
    }
}
