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
