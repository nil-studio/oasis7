use super::*;

pub(super) fn selection_focus_position(
    render_state: &RenderState,
    focus_target: &FocusTarget,
) -> Option<Position> {
    match focus_target.kind.as_str() {
        "agent" => render_state
            .agents
            .iter()
            .find(|agent| agent.id == focus_target.id)
            .and_then(|agent| agent.pos.clone()),
        "location" => render_state
            .locations
            .iter()
            .find(|location| location.id == focus_target.id)
            .map(|location| location.pos.clone()),
        "world_event" => render_state
            .world_events
            .iter()
            .find(|event| event.id == focus_target.id)
            .map(|event| event.pos.clone()),
        "module_visual" => render_state
            .module_visual_entities
            .iter()
            .find(|entity| entity.id == focus_target.id)
            .map(|entity| entity.pos.clone()),
        _ => None,
    }
}
pub(super) fn maybe_focus_selected_entity(runtime: &mut BevyRuntimeState, width: f64, height: f64) {
    let Some(focus_target) = runtime.pending_focus_target.clone() else {
        return;
    };
    let Some(render_state) = runtime.render_state.as_ref() else {
        runtime.pending_focus_target = None;
        return;
    };
    let Some(world_bounds) = render_state.world_bounds.as_ref() else {
        runtime.pending_focus_target = None;
        return;
    };
    let Some(position) = selection_focus_position(render_state, &focus_target) else {
        runtime.pending_focus_target = None;
        return;
    };
    let Some((selected_x, selected_y)) = to_canvas_point_unsnapped(
        &position,
        world_bounds,
        width,
        height,
        &CameraState::default(),
    ) else {
        runtime.pending_focus_target = None;
        return;
    };
    let centered_x = selected_x - (width / 2.0);
    let centered_y = selected_y - (height / 2.0);
    runtime.camera.pan_x_px = -(centered_x * runtime.camera.zoom.max(0.5));
    runtime.camera.pan_y_px = -(centered_y * runtime.camera.zoom.max(0.5));
    runtime.camera_user_override = true;
    runtime.camera_fit_version = runtime.render_version;
    runtime.pending_focus_target = None;
    let _ = camera_viewport::emit(runtime);
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn high_zoom_focus_keeps_subpixel_world_displacement_before_render_snap() {
        let mut state = crate::lib_tests::sample_render_state_for_camera("agent");
        state.agents[0].pos = Some(Position {
            x_cm: 1_501_500.,
            y_cm: 1_000_000.,
            z_cm: 0.,
        });
        let mut runtime = BevyRuntimeState::default();
        runtime.camera.zoom = 2048.;
        runtime.pending_focus_target = Some(FocusTarget {
            kind: "agent".into(),
            id: state.agents[0].id.clone(),
        });
        runtime.render_state = Some(state);
        maybe_focus_selected_entity(&mut runtime, 1280., 720.);
        let state = runtime.render_state.as_ref().unwrap();
        let (x, y) = to_canvas_point(
            state.agents[0].pos.as_ref().unwrap(),
            state.world_bounds.as_ref().unwrap(),
            1280.,
            720.,
            &runtime.camera,
        )
        .unwrap();
        assert!(
            (x - 640.).abs() < 1e-6 && (y - 360.).abs() < 1e-6,
            "focused target must stay centered at supported zoom"
        );
        assert!((runtime.camera.pan_x_px + 1269.76).abs() < 1e-6);
    }
}
