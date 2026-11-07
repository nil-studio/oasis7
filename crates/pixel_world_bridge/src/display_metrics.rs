//! Read-only ECS accounting; not a GPU fence, readability or permission proof.
use super::*;
use bevy::camera::visibility::VisibilitySystems;
use bevy::sprite::Anchor;
use bevy::text::TextLayoutInfo;

thread_local! {
    static COUNTS: RefCell<Value> = RefCell::new(json!({"status":"unobserved"}));
}

pub(crate) fn install(app: &mut App) {
    app.add_systems(
        PostUpdate,
        observe.after(VisibilitySystems::CheckVisibility),
    );
}

fn observe(
    runtime: Res<BevyRuntimeState>,
    agent_bodies: Query<&ViewVisibility, (With<Sprite>, With<render::PixelWorldAgentVisual>)>,
    labels: Query<(
        Entity,
        &Text2d,
        &Visibility,
        Option<&ViewVisibility>,
        Option<&TextLayoutInfo>,
        Option<&GlobalTransform>,
        Option<&Anchor>,
    )>,
) {
    let mut visible = 0;
    let mut in_view = 0;
    let mut hidden = 0;
    let mut layouts = Vec::new();
    let mut laid_out = 0;
    let css = runtime
        .last_canvas_css
        .map(|(width, height)| bevy::math::DVec2::new(width, height));
    for (entity, text, visibility, view, layout, transform, anchor) in &labels {
        if text.0.is_empty() {
            continue;
        }
        if *visibility == Visibility::Hidden {
            hidden += 1;
            continue;
        }
        visible += 1;
        if view.is_some_and(|view| view.get()) {
            in_view += 1;
            if let (Some(layout), Some(transform), Some(css)) = (layout, transform, css)
                && !layout.glyphs.is_empty()
            {
                laid_out += 1;
                if layouts.len() >= 200 {
                    continue;
                }
                let anchor = anchor.map_or(Vec2::ZERO, |anchor| anchor.0);
                let top_left = (Anchor::TOP_LEFT.0 - anchor) * layout.size;
                let points = [
                    top_left,
                    top_left + Vec2::new(layout.size.x, 0.0),
                    top_left + Vec2::new(0.0, -layout.size.y),
                    top_left + Vec2::new(layout.size.x, -layout.size.y),
                ]
                .map(|point| transform.transform_point(point.extend(0.0)))
                .map(|point| {
                    [
                        css.x / 2.0 + f64::from(point.x),
                        css.y / 2.0 - f64::from(point.y),
                    ]
                });
                let min_x = points.iter().map(|p| p[0]).fold(f64::INFINITY, f64::min);
                let max_x = points
                    .iter()
                    .map(|p| p[0])
                    .fold(f64::NEG_INFINITY, f64::max);
                let min_y = points.iter().map(|p| p[1]).fold(f64::INFINITY, f64::min);
                let max_y = points
                    .iter()
                    .map(|p| p[1])
                    .fold(f64::NEG_INFINITY, f64::max);
                layouts.push(json!({"frame_entity":entity.to_bits().to_string(),"glyph_count":layout.glyphs.len(),"css_rect":{"left":min_x,"top":min_y,"right":max_x,"bottom":max_y}}));
            }
        }
    }
    let state = runtime.render_state.as_ref();
    let agent_body_sprites = agent_bodies.iter().count();
    let in_view_agent_body_sprites = agent_bodies
        .iter()
        .filter(|visibility| visibility.get())
        .count();
    let profile = state.map_or(display_profile::DisplayProfile::Legacy, |state| {
        state.display_profile
    });
    let markers = state.map_or(0, |state| {
        state.agents.len()
            + state.locations.len()
            + state.module_visual_entities.len()
            + state.micro_depot_facilities.len()
            + state.industry_aggregates.len()
            + state.world_events.len()
    });
    COUNTS.with(|counts| {
        *counts.borrow_mut() = json!({
            "status":if state.is_some(){"observed"}else{"empty"},
            "render_version":runtime.render_version,"display_profile":profile,
        "source_cut":state.and_then(|state|state.display_source_cut.as_deref()),
            "projected_marker_count":markers,"label_cap":profile.labels(),
        "agent_body_sprites":agent_body_sprites,"in_view_agent_body_sprites":in_view_agent_body_sprites,
            "visible_nonempty_labels":visible,"in_view_nonempty_labels":in_view,
        "laid_out_in_view_labels":laid_out,"layout_rows_omitted":laid_out-layouts.len(),"label_layouts":layouts,
            "hidden_nonempty_labels":hidden,
            "scope":"ECS visibility after culling; not GPU completion, occlusion or readability"
        })
    });
}

#[wasm_bindgen]
pub fn display_metrics() -> String {
    let version = BRIDGE_SHARED.with(|shared| {
        let shared = shared.borrow();
        shared.mounted.then_some(shared.render_version)
    });
    let Some(version) = version else {
        return json!({"status":"unmounted","projected_marker_count":0,"agent_body_sprites":0,"in_view_agent_body_sprites":0,"visible_nonempty_labels":0,"in_view_nonempty_labels":0}).to_string();
    };
    COUNTS.with(|counts| {
        let counts = counts.borrow();
        if counts["render_version"].as_u64() != Some(version) {
            json!({"status":"awaiting_frame","render_version":version}).to_string()
        } else {
            counts.to_string()
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn pending_frame_never_reuses_old_scene_counts_and_unmount_clears_them() {
        BRIDGE_SHARED.with(|shared| {
            let mut shared = shared.borrow_mut();
            shared.mounted = true;
            shared.render_version = 10;
        });
        COUNTS.with(|counts| {
            *counts.borrow_mut() =
                json!({"status":"observed","render_version":9,"visible_nonempty_labels":200,"agent_body_sprites":1000})
        });
        let pending: Value = serde_json::from_str(&display_metrics()).unwrap();
        assert_eq!(pending["status"], "awaiting_frame");
        assert!(pending.get("visible_nonempty_labels").is_none());
        assert!(pending.get("agent_body_sprites").is_none());
        COUNTS.with(|counts| counts.borrow_mut()["render_version"] = json!(10));
        let current: Value = serde_json::from_str(&display_metrics()).unwrap();
        assert_eq!(current["visible_nonempty_labels"], 200);
        BRIDGE_SHARED.with(|shared| shared.borrow_mut().mounted = false);
        let cleared: Value = serde_json::from_str(&display_metrics()).unwrap();
        assert_eq!(cleared["visible_nonempty_labels"], 0);
        assert_eq!(cleared["agent_body_sprites"], 0);
        assert_eq!(cleared["status"], "unmounted");
    }
}
