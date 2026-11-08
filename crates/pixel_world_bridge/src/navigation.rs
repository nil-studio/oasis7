use super::*;

#[derive(Clone, Debug, Deserialize)]
pub(super) struct Navigation {
    pub(super) request_id: String,
    pub(super) mode: String,
    target: Option<Selection>,
    #[serde(default)]
    bounds: Option<Value>,
    #[serde(default)]
    pub(super) overview_query: overview_query::Declaration,
}

fn overview_allowed(runtime: &BevyRuntimeState, state: &RenderState, nav: &Navigation) -> bool {
    overview_query::permits(&nav.overview_query, state)
        && (matches!(nav.overview_query, overview_query::Declaration::Legacy)
            || (nav
                .request_id
                .parse::<u64>()
                .is_ok_and(|id| id > 0 && id >= runtime.navigation_highwater)
                && runtime.retired_overview_request.as_deref() != Some(&nav.request_id)))
}

pub(super) fn retire_overview_after_manual_camera(runtime: &mut BevyRuntimeState) {
    runtime.pending_overview_result = None;
    if let Some(nav) = runtime
        .render_state
        .as_ref()
        .and_then(|s| s.navigation.as_ref())
        && nav.mode == "overview"
        && !matches!(nav.overview_query, overview_query::Declaration::Legacy)
    {
        runtime.retired_overview_request = Some(nav.request_id.clone());
    }
}

pub(super) fn finish_overview(runtime: &mut BevyRuntimeState, status: &str) {
    let Some(request_id) = runtime.pending_overview_result.take() else {
        return;
    };
    #[cfg(target_arch = "wasm32")]
    let _ = emit_event_value(&json!({"type":"navigation_result","request_id":request_id,
        "status":status,"render_version":runtime.render_version}));
    #[cfg(not(target_arch = "wasm32"))]
    let _ = (request_id, status);
}

fn available(state: &RenderState, target: &FocusTarget) -> bool {
    match target.kind.as_str() {
        "agent" => state
            .agents
            .iter()
            .any(|agent| agent.id == target.id && agent.pos.is_some()),
        "location" => state
            .locations
            .iter()
            .any(|location| location.id == target.id),
        "world_event" => state.world_events.iter().any(|event| event.id == target.id),
        "module_visual" => state
            .module_visual_entities
            .iter()
            .any(|entity| entity.id == target.id),
        _ => false,
    }
}

pub(super) fn prepare_camera(
    runtime: &mut BevyRuntimeState,
    state: Option<&RenderState>,
    version: u64,
) -> bool {
    if let Some(id) = state
        .and_then(|s| s.navigation.as_ref())
        .and_then(|n| n.request_id.parse::<u64>().ok())
    {
        runtime.navigation_highwater = runtime.navigation_highwater.max(id);
    }
    if let Some((state, nav)) = state.and_then(|s| s.navigation.as_ref().map(|n| (s, n)))
        && nav.mode == "overview"
        && overview_query::terminally_invalid(&nav.overview_query, state)
    {
        runtime.retired_overview_request = Some(nav.request_id.clone());
        runtime.pending_overview_result = None;
    }
    let navigation_changed = state
        .and_then(|state| state.navigation.as_ref())
        .is_some_and(|next| {
            if next.mode == "overview"
                && !matches!(next.overview_query, overview_query::Declaration::Legacy)
            {
                return state.is_some_and(|state| overview_allowed(runtime, state, next));
            }
            runtime
                .render_state
                .as_ref()
                .and_then(|old| old.navigation.as_ref())
                .is_none_or(|old| old.request_id != next.request_id)
        });
    let overview = navigation_changed
        && state
            .and_then(|state| state.navigation.as_ref())
            .is_some_and(|nav| nav.mode == "overview");
    if state.is_some_and(|state| state.exploration_mode)
        && runtime.render_state.is_some()
        && !overview
    {
        runtime.camera_fit_version = version;
    }
    navigation_changed
}

pub(super) fn apply(runtime: &mut BevyRuntimeState, state: Option<&RenderState>, moved: bool) {
    let Some(state) = state else {
        return;
    };
    if runtime
        .render_state
        .as_ref()
        .is_none_or(|old| !old.exploration_mode)
    {
        runtime.active_follow_target = None;
        runtime.pending_focus_target = None;
    }
    // Losing a trusted location pauses tracking permanently until a new command.
    if runtime
        .active_follow_target
        .as_ref()
        .is_some_and(|target| !available(state, target))
    {
        runtime.active_follow_target = None;
        runtime.pending_focus_target = None;
    }
    let previous = runtime
        .render_state
        .as_ref()
        .and_then(|state| state.navigation.as_ref());
    let command = state.navigation.as_ref().filter(|command| {
        if command.mode == "overview"
            && !matches!(command.overview_query, overview_query::Declaration::Legacy)
        {
            return !command.request_id.is_empty() && overview_allowed(runtime, state, command);
        }
        !command.request_id.is_empty()
            && previous.is_none_or(|old| old.request_id != command.request_id)
    });
    if let Some(command) = command {
        runtime.pending_overview_result = None;
        runtime.active_follow_target = None;
        runtime.pending_focus_target = None;
        runtime.pending_fit_bounds = None;
        match command.mode.as_str() {
            "overview" => {
                if !matches!(command.overview_query, overview_query::Declaration::Legacy) {
                    runtime.retired_overview_request = Some(command.request_id.clone());
                    runtime.pending_overview_result = Some(command.request_id.clone());
                }
                runtime.camera_fit_version = 0;
                runtime.last_canvas_size = None;
                runtime.last_canvas_css = None;
                runtime.camera_user_override = false;
            }
            "fit_bounds" => {
                runtime.pending_fit_bounds = command.bounds.clone();
            }
            "locate" | "follow" => {
                let target = command
                    .target
                    .as_ref()
                    .map(|target| FocusTarget {
                        kind: target.kind.clone(),
                        id: target.id.clone(),
                    })
                    .filter(|target| {
                        available(state, target)
                            && (command.mode != "follow" || target.kind != "world_event")
                    });
                runtime.pending_focus_target = target.clone();
                if command.mode == "follow" {
                    runtime.active_follow_target = target;
                }
            }
            _ => {}
        }
        runtime.hit_regions_dirty = true;
    } else if moved {
        runtime.pending_focus_target = runtime.active_follow_target.clone();
    }
}

pub(super) const MAX_EXPLORATION_ZOOM: f64 = 65_536.0;

pub(super) fn apply_pending_fit_bounds(runtime: &mut BevyRuntimeState, width: f64, height: f64) {
    let Some(rect) = runtime.pending_fit_bounds.take() else {
        return;
    };
    let Some(bounds) = runtime
        .render_state
        .as_ref()
        .and_then(|state| state.world_bounds.as_ref())
    else {
        return;
    };
    let parse = |key: &str| {
        rect[key]
            .as_f64()
            .or_else(|| rect[key].as_str()?.parse::<f64>().ok())
            .filter(|v| v.is_finite() && v.abs() <= 9_007_199_254_740_991.0)
    };
    let (Some(x0), Some(x1), Some(y0), Some(y1)) = (
        parse("min_x_cm"),
        parse("max_x_cm"),
        parse("min_y_cm"),
        parse("max_y_cm"),
    ) else {
        return;
    };
    if width <= 64.0
        || height <= 64.0
        || x0 < 0.0
        || y0 < 0.0
        || x1 > bounds.width_cm
        || y1 > bounds.depth_cm
        || x0 >= x1
        || y0 >= y1
    {
        return;
    }
    let project = |x, y| {
        to_canvas_point_unsnapped(
            &Position {
                x_cm: x,
                y_cm: y,
                z_cm: 0.0,
            },
            bounds,
            width,
            height,
            &CameraState::default(),
        )
    };
    let (Some((a, b)), Some((c, d))) = (project(x0, y0), project(x1, y1)) else {
        return;
    };
    let zoom = ((width - 64.0) / (c - a))
        .min((height - 64.0) / (d - b))
        .clamp(0.6, MAX_EXPLORATION_ZOOM);
    runtime.camera.zoom = zoom;
    runtime.camera.pan_x_px = -((a + c) / 2.0 - width / 2.0) * zoom;
    runtime.camera.pan_y_px = -((b + d) / 2.0 - height / 2.0) * zoom;
    runtime.camera_user_override = true;
    runtime.camera_fit_version = runtime.render_version;
    runtime.hit_regions_dirty = true;
    let _ = camera_viewport::emit(runtime);
}

#[cfg(test)]
#[path = "navigation_tests.rs"]
mod tests;
