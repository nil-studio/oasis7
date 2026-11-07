use std::collections::HashMap;

use super::*;

const RECOMMENDED_TARGET_CUE_COLOR: Color = Color::srgba_u8(34, 211, 238, 224);
const RECOMMENDED_TARGET_CUE_LAYER_Z_OFFSET: f32 = 0.015;
const RECOMMENDED_TARGET_CUE_THICKNESS_PX: f32 = 2.0;
const RECOMMENDED_TARGET_CUE_WIDTH_PX: f32 = 12.0;
const RECOMMENDED_TARGET_CUE_SIDE_HEIGHT_PX: f32 = 5.0;
const RECOMMENDED_TARGET_CUE_PADDING_PX: f32 = 5.0;

/// A display-only upper bracket for the runtime-selected recommended action.
/// It deliberately has no hit region and sits below explicit selection/receipt cues.
#[derive(Component)]
pub(super) struct PixelWorldRecommendedTargetCue {
    agent_id: String,
    part: RecommendedTargetCuePart,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
enum RecommendedTargetCuePart {
    Top,
    Left,
    Right,
}

fn recommended_target_cue_visuals(
    body_half_size: f32,
) -> [(RecommendedTargetCuePart, Vec2, Vec2); 3] {
    let top_y = body_half_size + RECOMMENDED_TARGET_CUE_PADDING_PX;
    let side_offset = (RECOMMENDED_TARGET_CUE_WIDTH_PX - RECOMMENDED_TARGET_CUE_THICKNESS_PX) / 2.0;
    let side_y = top_y
        - ((RECOMMENDED_TARGET_CUE_SIDE_HEIGHT_PX - RECOMMENDED_TARGET_CUE_THICKNESS_PX) / 2.0);
    [
        (
            RecommendedTargetCuePart::Top,
            Vec2::new(0.0, top_y),
            Vec2::new(
                RECOMMENDED_TARGET_CUE_WIDTH_PX,
                RECOMMENDED_TARGET_CUE_THICKNESS_PX,
            ),
        ),
        (
            RecommendedTargetCuePart::Left,
            Vec2::new(-side_offset, side_y),
            Vec2::new(
                RECOMMENDED_TARGET_CUE_THICKNESS_PX,
                RECOMMENDED_TARGET_CUE_SIDE_HEIGHT_PX,
            ),
        ),
        (
            RecommendedTargetCuePart::Right,
            Vec2::new(side_offset, side_y),
            Vec2::new(
                RECOMMENDED_TARGET_CUE_THICKNESS_PX,
                RECOMMENDED_TARGET_CUE_SIDE_HEIGHT_PX,
            ),
        ),
    ]
}

pub(super) fn reconcile_recommended_target_cues(
    commands: &mut Commands,
    runtime: &BevyRuntimeState,
    existing_cues: &Query<(Entity, &PixelWorldRecommendedTargetCue)>,
    width: f64,
    height: f64,
    animation_ms: f64,
) {
    let mut existing_by_key = HashMap::new();
    for (entity, cue) in existing_cues.iter() {
        let key = (cue.agent_id.clone(), cue.part);
        if let Some(duplicate) = existing_by_key.insert(key, entity) {
            commands.entity(duplicate).despawn();
        }
    }

    let Some(render_state) = runtime.render_state.as_ref() else {
        for entity in existing_by_key.into_values() {
            commands.entity(entity).despawn();
        }
        return;
    };
    let Some(recommended_target) = render_state.recommended_target.as_ref() else {
        for entity in existing_by_key.into_values() {
            commands.entity(entity).despawn();
        }
        return;
    };
    if render_state
        .receipt_target
        .as_ref()
        .is_some_and(|receipt_target| receipt_target.agent_id == recommended_target.agent_id)
    {
        for entity in existing_by_key.into_values() {
            commands.entity(entity).despawn();
        }
        return;
    }
    let Some((index, agent)) = render_state
        .agents
        .iter()
        .enumerate()
        .find(|(_, agent)| agent.id == recommended_target.agent_id)
    else {
        for entity in existing_by_key.into_values() {
            commands.entity(entity).despawn();
        }
        return;
    };

    let (canvas_x, canvas_y) = render_state
        .world_bounds
        .as_ref()
        .and_then(|world_bounds| {
            agent
                .pos
                .as_ref()
                .and_then(|pos| to_canvas_point(pos, world_bounds, width, height, &runtime.camera))
        })
        .unwrap_or_else(|| fallback_point_for_entity(&agent.id, width, height, &runtime.camera));
    let is_selected = render_state
        .selection
        .as_ref()
        .is_some_and(|selection| selection.kind == "agent" && selection.id == agent.id);
    let style = agent_visual_style(agent, is_selected, animation_ms, index);
    let canvas_y =
        canvas_y - style.size_px * 7.0 / 16.0 * agent_pixel_scale(width, height).y as f64;
    let body_half_size = style.size_px as f32 * agent_pixel_scale(width, height).y / 2.0;

    for (part, offset, size) in recommended_target_cue_visuals(body_half_size) {
        let key = (agent.id.clone(), part);
        let sprite = sprite_for_rect(RECOMMENDED_TARGET_CUE_COLOR, size.x, size.y);
        let transform = Transform::from_translation(to_bevy_translation(
            canvas_x + f64::from(offset.x),
            canvas_y - f64::from(offset.y),
            width,
            height,
            style.layer_z + AGENT_CORE_LAYER_Z_OFFSET + RECOMMENDED_TARGET_CUE_LAYER_Z_OFFSET,
        ));
        if let Some(entity) = existing_by_key.remove(&key) {
            commands.entity(entity).insert((sprite, transform));
        } else {
            commands.spawn((
                sprite,
                transform,
                PixelWorldRecommendedTargetCue {
                    agent_id: agent.id.clone(),
                    part,
                },
            ));
        }
    }
    for entity in existing_by_key.into_values() {
        commands.entity(entity).despawn();
    }
}
