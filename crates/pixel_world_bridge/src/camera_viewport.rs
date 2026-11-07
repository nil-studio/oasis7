use super::*;

// Inverse of to_canvas_point, using the actual Bevy logical viewport dimensions.
// Floor/ceil expand outward so a clipped boundary entity remains queryable.
#[cfg(any(target_arch = "wasm32", test))]
pub(super) fn camera_payload(runtime: &BevyRuntimeState) -> Value {
    let camera = &runtime.camera;
    let mut payload = json!({
        "zoom": camera.zoom,
        "pan_x_px": camera.pan_x_px,
        "pan_y_px": camera.pan_y_px,
    });
    let Some((width, height)) = runtime.last_canvas_size else {
        return payload;
    };
    let Some(bounds) = runtime
        .render_state
        .as_ref()
        .and_then(|state| state.world_bounds.as_ref())
    else {
        return payload;
    };
    let values = [
        bounds.width_cm,
        bounds.depth_cm,
        camera.zoom,
        camera.pan_x_px,
        camera.pan_y_px,
    ];
    if width <= 40
        || height <= 40
        || values.iter().any(|v| !v.is_finite())
        || bounds.width_cm <= 0.0
        || bounds.depth_cm <= 0.0
        || bounds.width_cm > 9_007_199_254_740_991.0
        || bounds.depth_cm > 9_007_199_254_740_991.0
    {
        return payload;
    }
    let layout = pixel_pipeline::canvas_layout(width as f64, height as f64);
    let pixel = layout.map(|layout| layout.pixel_css()).unwrap_or(Vec2::ONE);
    let axis = |size: f64, extent: f64, pan: f64, unit: f64| {
        let pan = ((pan / unit) + 0.5).floor() * unit;
        let inverse = |pixel: f64| {
            (((pixel - size / 2.0 - pan) / camera.zoom.max(0.5) + size / 2.0 - 20.0)
                / (size - 40.0)
                * extent)
                .clamp(0.0, extent)
        };
        (
            format!("{:.0}", inverse(-unit / 2.0).floor()),
            format!("{:.0}", inverse(size + unit / 2.0).ceil().min(extent)),
        )
    };
    let (min_x, max_x) = axis(
        width as f64,
        bounds.width_cm,
        camera.pan_x_px,
        pixel.x as f64,
    );
    let (min_y, max_y) = axis(
        height as f64,
        bounds.depth_cm,
        camera.pan_y_px,
        pixel.y as f64,
    );
    payload["viewport_bounds"] =
        json!({"min_x_cm":min_x,"max_x_cm":max_x,"min_y_cm":min_y,"max_y_cm":max_y});
    payload["viewport_size_px"] = json!({"width":width,"height":height});
    payload
}

#[cfg(target_arch = "wasm32")]
pub(super) fn emit(runtime: &BevyRuntimeState) -> Result<(), JsValue> {
    emit_event_value(&json!({"type":"camera_state_changed","camera":camera_payload(runtime)}))
}

// Native CPU/raster tests have no JavaScript callback surface.
#[cfg(not(target_arch = "wasm32"))]
pub(super) fn emit(_runtime: &BevyRuntimeState) -> Result<(), JsValue> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn viewport_is_inverse_of_world_projection_and_expands_outward() {
        let mut runtime = BevyRuntimeState::default();
        runtime.render_state = Some(crate::lib_tests::sample_render_state_for_camera("agent"));
        runtime.last_canvas_size = Some((1000, 600));
        runtime.camera.zoom = 2.0;
        let payload = camera_payload(&runtime);
        let bounds = &payload["viewport_bounds"];
        // Half a logical pixel is included on each side of the visible query.
        assert_eq!(bounds["min_x_cm"], "717968");
        assert_eq!(bounds["max_x_cm"], "2282032");
        let state = runtime.render_state.as_ref().unwrap();
        let point = Position {
            x_cm: 718750.0,
            y_cm: 1000000.0,
            z_cm: 0.0,
        };
        let (x, _) = to_canvas_point(
            &point,
            state.world_bounds.as_ref().unwrap(),
            1000.0,
            600.0,
            &runtime.camera,
        )
        .unwrap();
        assert!(x.abs() < 0.0001);
        runtime.camera.pan_x_px = 100.0;
        assert!(
            camera_payload(&runtime)["viewport_bounds"]["min_x_cm"]
                .as_str()
                .unwrap()
                .parse::<i64>()
                .unwrap()
                < 718750
        );
    }
    #[test]
    fn camera_payload_preserves_fractional_inputs_for_shared_snap() {
        let mut runtime = BevyRuntimeState::default();
        runtime.camera.pan_x_px = 0.6;
        runtime.camera.pan_y_px = -0.6;
        runtime.camera.zoom = 1.23456789;
        let payload = camera_payload(&runtime);
        assert_eq!(payload["pan_x_px"], json!(0.6));
        assert_eq!(payload["pan_y_px"], json!(-0.6));
        assert_eq!(payload["zoom"], json!(1.23456789));
    }
    #[test]
    fn no_world_or_invalid_viewport_omits_query_bounds() {
        let mut runtime = BevyRuntimeState::default();
        assert!(camera_payload(&runtime).get("viewport_bounds").is_none());
        runtime.render_state = Some(crate::lib_tests::sample_render_state_for_camera("agent"));
        runtime.last_canvas_size = Some((30, 30));
        assert!(camera_payload(&runtime).get("viewport_bounds").is_none());
    }
}
