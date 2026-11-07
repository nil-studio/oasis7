//! Continuous projection for camera algorithms; display quantization is separate.
use super::*;

pub(super) fn to_canvas_point_unsnapped(
    position: &Position,
    world_bounds: &WorldBounds,
    width: f64,
    height: f64,
    camera: &CameraState,
) -> Option<(f64, f64)> {
    let safe_width = world_bounds.width_cm.max(1.0);
    let safe_depth = world_bounds.depth_cm.max(1.0);
    let normalized_x = clamp(position.x_cm / safe_width, 0.0, 1.0);
    let normalized_y = clamp(position.y_cm / safe_depth, 0.0, 1.0);
    let base_x = 20.0 + (normalized_x * (width - 40.0).max(1.0));
    let base_y = 20.0 + (normalized_y * (height - 40.0).max(1.0));
    let centered_x = base_x - (width / 2.0);
    let centered_y = base_y - (height / 2.0);
    let point = bevy::math::DVec2::new(
        (width / 2.0) + centered_x * camera.zoom.max(0.5) + camera.pan_x_px,
        (height / 2.0) + centered_y * camera.zoom.max(0.5) + camera.pan_y_px,
    );
    Some((point.x, point.y))
}
