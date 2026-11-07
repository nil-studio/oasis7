use std::collections::{HashMap, HashSet};

use super::*;

const DERIVED_POSITION_CUE_LAYER_Z_OFFSET: f32 = -0.05;
const DERIVED_POSITION_CUE_THICKNESS_PX: f32 = 1.0;
const DERIVED_POSITION_CUE_SEGMENT_LENGTH_PX: f32 = 4.0;
const DERIVED_POSITION_CUE_PADDING_PX: f32 = 2.0;
const DERIVED_POSITION_CUE_COLOR: Color = Color::srgba_u8(125, 153, 178, 150);

/// Four display-only edge-midpoint ticks behind agents whose map position was
/// inferred from their location rather than supplied by a world snapshot.
#[derive(Component)]
pub(super) struct PixelWorldDerivedPositionCue {
    pub(super) agent_id: String,
    pub(super) segment: DerivedPositionCueSegment,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(super) enum DerivedPositionCueSegment {
    Top,
    Bottom,
    Left,
    Right,
}

fn derived_position_cue_visuals(
    body_half_size: f32,
) -> [(DerivedPositionCueSegment, Vec2, Vec2); 4] {
    let outer_offset = body_half_size
        + DERIVED_POSITION_CUE_PADDING_PX
        + (DERIVED_POSITION_CUE_THICKNESS_PX / 2.0);
    let horizontal = Vec2::new(
        DERIVED_POSITION_CUE_SEGMENT_LENGTH_PX,
        DERIVED_POSITION_CUE_THICKNESS_PX,
    );
    let vertical = Vec2::new(
        DERIVED_POSITION_CUE_THICKNESS_PX,
        DERIVED_POSITION_CUE_SEGMENT_LENGTH_PX,
    );
    [
        (
            DerivedPositionCueSegment::Top,
            Vec2::new(0.0, outer_offset),
            horizontal,
        ),
        (
            DerivedPositionCueSegment::Bottom,
            Vec2::new(0.0, -outer_offset),
            horizontal,
        ),
        (
            DerivedPositionCueSegment::Left,
            Vec2::new(-outer_offset, 0.0),
            vertical,
        ),
        (
            DerivedPositionCueSegment::Right,
            Vec2::new(outer_offset, 0.0),
            vertical,
        ),
    ]
}

pub(super) fn reconcile_derived_position_cues(
    commands: &mut Commands,
    runtime: &BevyRuntimeState,
    existing_cues: &Query<(Entity, &PixelWorldDerivedPositionCue)>,
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

    let mut active_keys = HashSet::new();
    for (index, agent) in render_state.agents.iter().enumerate() {
        if agent.position_source != AgentPositionSource::LocationDerived {
            continue;
        }
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
        let body_half_size = style.size_px as f32 * agent_pixel_scale(width, height).y / 2.0;
        for (segment, offset, size) in derived_position_cue_visuals(body_half_size) {
            let key = (agent.id.clone(), segment);
            active_keys.insert(key.clone());
            let sprite = sprite_for_rect(DERIVED_POSITION_CUE_COLOR, size.x, size.y);
            let transform = Transform::from_translation(to_bevy_translation(
                canvas_x + f64::from(offset.x),
                canvas_y + f64::from(offset.y),
                width,
                height,
                style.layer_z + DERIVED_POSITION_CUE_LAYER_Z_OFFSET,
            ));
            if let Some(entity) = existing_by_key.remove(&key) {
                commands.entity(entity).insert((sprite, transform));
            } else {
                commands.spawn((
                    sprite,
                    transform,
                    PixelWorldDerivedPositionCue {
                        agent_id: agent.id.clone(),
                        segment,
                    },
                ));
            }
        }
    }
    for (key, entity) in existing_by_key {
        if !active_keys.contains(&key) {
            commands.entity(entity).despawn();
        }
    }
}
