use std::collections::{HashMap, HashSet};

use super::*;

const SELECTED_AGENT_CUE_LAYER_Z_OFFSET: f32 = 0.02;
const SELECTED_AGENT_CUE_THICKNESS_PX: f32 = 2.0;
const SELECTED_AGENT_CUE_SEGMENT_LENGTH_PX: f32 = 6.0;
const SELECTED_AGENT_CUE_PADDING_PX: f32 = 3.0;
const SELECTED_AGENT_CUE_COLOR: Color = Color::srgb_u8(251, 191, 36);
pub(super) const AGENT_CORE_LAYER_Z_OFFSET: f32 = 0.01;
pub(super) const AGENT_CORE_SIZE_SCALE: f32 = 0.32;
pub(super) const AGENT_CORE_COLOR: Color = Color::srgba_u8(224, 242, 254, 238);

/// A non-interactive, neutral inner light chip that gives every agent marker
/// a stable pixel-world silhouette without encoding agent status.
#[derive(Component)]
pub(super) struct PixelWorldAgentCoreVisual {
    pub(super) id: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(super) enum SelectedAgentCueSegment {
    TopLeftHorizontal,
    TopLeftVertical,
    TopRightHorizontal,
    TopRightVertical,
    BottomLeftHorizontal,
    BottomLeftVertical,
    BottomRightHorizontal,
    BottomRightVertical,
}

#[derive(Component)]
pub(super) struct PixelWorldSelectedAgentCue {
    agent_id: String,
    segment: SelectedAgentCueSegment,
}

pub(super) fn agent_core_size_px(agent: &Agent, is_selected: bool) -> f32 {
    agent_unanimated_size_px(agent, is_selected) as f32 * AGENT_CORE_SIZE_SCALE
}

pub(super) fn reconcile_agent_cores(
    commands: &mut Commands,
    runtime: &BevyRuntimeState,
    existing_cores: &Query<(Entity, &PixelWorldAgentCoreVisual)>,
    width: f64,
    height: f64,
    animation_ms: f64,
) {
    let mut existing_by_id = HashMap::new();
    for (entity, core) in existing_cores.iter() {
        existing_by_id.insert(core.id.clone(), entity);
    }
    let Some(render_state) = runtime.render_state.as_ref() else {
        for entity in existing_by_id.into_values() {
            commands.entity(entity).despawn();
        }
        return;
    };

    let mut active_ids = HashSet::new();
    for (index, agent) in render_state.agents.iter().enumerate() {
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
            .is_some_and(|selection| selection.kind == "agent" && selection.id == agent.id);
        let style = agent_visual_style(agent, is_selected, animation_ms, index);
        let canvas_y =
            canvas_y - style.size_px * 7.0 / 16.0 * agent_pixel_scale(width, height).y as f64;
        let sprite = sprite_for_square(AGENT_CORE_COLOR, agent_core_size_px(agent, is_selected));
        let transform = Transform::from_translation(to_bevy_translation(
            canvas_x,
            canvas_y,
            width,
            height,
            style.layer_z - AGENT_CORE_LAYER_Z_OFFSET,
        ));
        if let Some(entity) = existing_by_id.get(&agent.id).copied() {
            commands.entity(entity).insert((sprite, transform));
        } else {
            commands.spawn((
                sprite,
                transform,
                PixelWorldAgentCoreVisual {
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

/// Returns the eight thin sprite segments that form four L-shaped corners.
/// The corners are intentionally outside the body so agent selection remains
/// distinguishable from the selected-location full outline.
fn selected_agent_cue_visuals(body_half_size: f32) -> [(SelectedAgentCueSegment, Vec2, Vec2); 8] {
    let outer_offset =
        body_half_size + SELECTED_AGENT_CUE_PADDING_PX + (SELECTED_AGENT_CUE_THICKNESS_PX / 2.0);
    let inner_offset = outer_offset - (SELECTED_AGENT_CUE_SEGMENT_LENGTH_PX / 2.0);
    let horizontal = Vec2::new(
        SELECTED_AGENT_CUE_SEGMENT_LENGTH_PX,
        SELECTED_AGENT_CUE_THICKNESS_PX,
    );
    let vertical = Vec2::new(
        SELECTED_AGENT_CUE_THICKNESS_PX,
        SELECTED_AGENT_CUE_SEGMENT_LENGTH_PX,
    );
    [
        (
            SelectedAgentCueSegment::TopLeftHorizontal,
            Vec2::new(-inner_offset, outer_offset),
            horizontal,
        ),
        (
            SelectedAgentCueSegment::TopLeftVertical,
            Vec2::new(-outer_offset, inner_offset),
            vertical,
        ),
        (
            SelectedAgentCueSegment::TopRightHorizontal,
            Vec2::new(inner_offset, outer_offset),
            horizontal,
        ),
        (
            SelectedAgentCueSegment::TopRightVertical,
            Vec2::new(outer_offset, inner_offset),
            vertical,
        ),
        (
            SelectedAgentCueSegment::BottomLeftHorizontal,
            Vec2::new(-inner_offset, -outer_offset),
            horizontal,
        ),
        (
            SelectedAgentCueSegment::BottomLeftVertical,
            Vec2::new(-outer_offset, -inner_offset),
            vertical,
        ),
        (
            SelectedAgentCueSegment::BottomRightHorizontal,
            Vec2::new(inner_offset, -outer_offset),
            horizontal,
        ),
        (
            SelectedAgentCueSegment::BottomRightVertical,
            Vec2::new(outer_offset, -inner_offset),
            vertical,
        ),
    ]
}

/// Distance from an Agent center to the outermost top edge of its corner frame.
pub(super) fn selected_agent_cue_outer_top_offset(body_half_size: f32) -> f32 {
    body_half_size + SELECTED_AGENT_CUE_PADDING_PX + SELECTED_AGENT_CUE_THICKNESS_PX
}

pub(super) fn reconcile_selected_agent_cues(
    commands: &mut Commands,
    runtime: &BevyRuntimeState,
    existing_cues: &Query<(Entity, &PixelWorldSelectedAgentCue)>,
    width: f64,
    height: f64,
    animation_ms: f64,
) {
    let mut existing_by_key = HashMap::new();
    for (entity, cue) in existing_cues.iter() {
        let key = (cue.agent_id.clone(), cue.segment);
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
    let Some(selection) = render_state.selection.as_ref() else {
        for entity in existing_by_key.into_values() {
            commands.entity(entity).despawn();
        }
        return;
    };
    if selection.kind != "agent" {
        for entity in existing_by_key.into_values() {
            commands.entity(entity).despawn();
        }
        return;
    }
    let Some((index, agent)) = render_state
        .agents
        .iter()
        .enumerate()
        .find(|(_, agent)| agent.id == selection.id)
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
    let style = agent_visual_style(agent, true, animation_ms, index);
    let canvas_y =
        canvas_y - style.size_px * 7.0 / 16.0 * agent_pixel_scale(width, height).y as f64;
    let body_half_size = style.size_px as f32 * agent_pixel_scale(width, height).y / 2.0;

    for (segment, offset, size) in selected_agent_cue_visuals(body_half_size) {
        let key = (agent.id.clone(), segment);
        let sprite = sprite_for_rect(SELECTED_AGENT_CUE_COLOR, size.x, size.y);
        let transform = Transform::from_translation(to_bevy_translation(
            canvas_x + f64::from(offset.x),
            canvas_y + f64::from(offset.y),
            width,
            height,
            style.layer_z + AGENT_CORE_LAYER_Z_OFFSET + SELECTED_AGENT_CUE_LAYER_Z_OFFSET,
        ));
        if let Some(entity) = existing_by_key.remove(&key) {
            commands.entity(entity).insert((sprite, transform));
        } else {
            commands.spawn((
                sprite,
                transform,
                PixelWorldSelectedAgentCue {
                    agent_id: agent.id.clone(),
                    segment,
                },
            ));
        }
    }

    for entity in existing_by_key.into_values() {
        commands.entity(entity).despawn();
    }
}
