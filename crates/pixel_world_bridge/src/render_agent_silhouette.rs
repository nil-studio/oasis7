use std::collections::{HashMap, HashSet};

use super::*;

const AGENT_SILHOUETTE_LAYER_Z_OFFSET: f32 = -0.15;
const AGENT_SILHOUETTE_SIZE_SCALE: f32 = 0.34;
const AGENT_SILHOUETTE_OFFSET_SCALE: f64 = 0.35;
const AGENT_SILHOUETTE_MIN_BODY_SIZE_PX: f64 = 6.0;
const AGENT_SILHOUETTE_COLOR: Color = Color::srgba_u8(148, 163, 184, 176);

/// A non-interactive neutral offset chip behind an agent body. Its position is
/// derived only from the stable agent id, so it adds silhouette variety without
/// carrying runtime status or animation semantics.
#[derive(Component)]
pub(super) struct PixelWorldAgentSilhouetteVisual {
    pub(super) id: String,
}

pub(super) fn agent_silhouette_size_px(agent: &Agent) -> Option<f32> {
    let body_size = agent.size_hint_px.unwrap_or(12.0);
    (body_size >= AGENT_SILHOUETTE_MIN_BODY_SIZE_PX)
        .then_some(body_size as f32 * AGENT_SILHOUETTE_SIZE_SCALE)
}

pub(super) fn agent_silhouette_offset(agent_id: &str, body_size: f64) -> Vec2 {
    let hash = agent_id.bytes().fold(2_166_136_261_u32, |hash, byte| {
        (hash ^ u32::from(byte)).wrapping_mul(16_777_619)
    });
    let offset = (body_size * AGENT_SILHOUETTE_OFFSET_SCALE) as f32;
    match hash % 4 {
        0 => Vec2::new(-offset, offset),
        1 => Vec2::new(offset, offset),
        2 => Vec2::new(-offset, -offset),
        _ => Vec2::new(offset, -offset),
    }
}

pub(super) fn reconcile_agent_silhouettes(
    commands: &mut Commands,
    runtime: &BevyRuntimeState,
    existing_silhouettes: &Query<(Entity, &PixelWorldAgentSilhouetteVisual)>,
    width: f64,
    height: f64,
    animation_ms: f64,
) {
    let mut existing_by_id = HashMap::new();
    for (entity, silhouette) in existing_silhouettes.iter() {
        if let Some(duplicate) = existing_by_id.insert(silhouette.id.clone(), entity) {
            commands.entity(duplicate).despawn();
        }
    }

    let Some(render_state) = runtime.render_state.as_ref() else {
        for entity in existing_by_id.into_values() {
            commands.entity(entity).despawn();
        }
        return;
    };

    let mut active_ids = HashSet::new();
    for (index, agent) in render_state.agents.iter().enumerate() {
        let Some(size_px) = agent_silhouette_size_px(agent) else {
            continue;
        };
        active_ids.insert(agent.id.clone());
        let (canvas_x, canvas_y) = render_state
            .world_bounds
            .as_ref()
            .and_then(|world_bounds| {
                agent.pos.as_ref().and_then(|pos| {
                    to_canvas_point(pos, world_bounds, width, height, &runtime.camera)
                })
            })
            .unwrap_or_else(|| {
                fallback_point_for_entity(&agent.id, width, height, &runtime.camera)
            });
        let is_selected = render_state
            .selection
            .as_ref()
            .map(|selection| selection.kind == "agent" && selection.id == agent.id)
            .unwrap_or(false);
        let style = agent_visual_style(agent, is_selected, animation_ms, index);
        let canvas_y =
            canvas_y - style.size_px * 7.0 / 16.0 * agent_pixel_scale(width, height).y as f64;
        let pixel = agent_pixel_scale(width, height);
        let offset = agent_silhouette_offset(&agent.id, agent.size_hint_px.unwrap_or(12.0)) * pixel;
        let sprite = sprite_for_rect(AGENT_SILHOUETTE_COLOR, size_px * pixel.x, size_px * pixel.y);
        let transform = Transform::from_translation(to_bevy_translation(
            canvas_x + f64::from(offset.x),
            canvas_y + f64::from(offset.y),
            width,
            height,
            style.layer_z + AGENT_SILHOUETTE_LAYER_Z_OFFSET,
        ));

        if let Some(entity) = existing_by_id.get(&agent.id).copied() {
            commands.entity(entity).insert((sprite, transform));
        } else {
            commands.spawn((
                sprite,
                transform,
                PixelWorldAgentSilhouetteVisual {
                    id: agent.id.clone(),
                },
            ));
        }
    }

    for (id, entity) in existing_by_id {
        if !active_ids.contains(&id) {
            commands.entity(entity).despawn();
        }
    }
}
