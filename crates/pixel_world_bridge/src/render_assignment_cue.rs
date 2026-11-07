use std::collections::{HashMap, HashSet};

use super::*;

const ASSIGNMENT_CUE_LAYER_Z: f32 = 0.55;
const ASSIGNMENT_CUE_COLOR: Color = Color::srgba_u8(147, 197, 253, 122);
const ASSIGNMENT_CUE_THICKNESS_PX: f32 = 1.25;
const ASSIGNMENT_CUE_ARM_LENGTH_PX: f64 = 6.0;
const ASSIGNMENT_CUE_BACKOFF_PX: f64 = 9.0;
const ASSIGNMENT_CUE_HALF_WIDTH_PX: f64 = 3.0;
const ASSIGNMENT_CUE_MIN_LINK_LENGTH_PX: f64 = 20.0;

/// One stroke of a static, display-only chevron that identifies the current
/// assignment anchor. It is deliberately not a movement or interaction cue.
#[derive(Component)]
pub(super) struct PixelWorldAssignmentCueVisual {
    pub(super) link_id: String,
    pub(super) part: AssignmentCuePart,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(super) enum AssignmentCuePart {
    Left,
    Right,
}

pub(super) fn despawn_assignment_cues(
    commands: &mut Commands,
    existing_cues: &Query<(Entity, &PixelWorldAssignmentCueVisual)>,
) {
    for (entity, _) in existing_cues.iter() {
        commands.entity(entity).despawn();
    }
}

pub(super) fn reconcile_assignment_cues(
    commands: &mut Commands,
    runtime: &BevyRuntimeState,
    existing_cues: &Query<(Entity, &PixelWorldAssignmentCueVisual)>,
    width: f64,
    height: f64,
) {
    let mut existing_by_key = HashMap::new();
    for (entity, cue) in existing_cues.iter() {
        let key = (cue.link_id.clone(), cue.part);
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
    let Some(world_bounds) = render_state.world_bounds.as_ref() else {
        for entity in existing_by_key.into_values() {
            commands.entity(entity).despawn();
        }
        return;
    };

    let pixel = world_strokes::pixel_unit(width, height);
    let mut active_keys = HashSet::new();
    for link in &render_state.links {
        if link.kind != "agent_assignment"
            || link.status.as_deref() != Some("active")
            || link.source_class.as_deref() != Some("runtime_projection")
            || link.freshness.as_deref() != Some("current")
        {
            continue;
        }
        let Some((from_x, from_y)) =
            to_canvas_point(&link.from, world_bounds, width, height, &runtime.camera)
        else {
            continue;
        };
        let Some((to_x, to_y)) =
            to_canvas_point(&link.to, world_bounds, width, height, &runtime.camera)
        else {
            continue;
        };
        let delta_x = to_x - from_x;
        let delta_y = to_y - from_y;
        let link_length = (delta_x.powi(2) + delta_y.powi(2)).sqrt();
        if !link_length.is_finite() || link_length < ASSIGNMENT_CUE_MIN_LINK_LENGTH_PX {
            continue;
        }

        let cue_pixel = pixel.min(bevy::math::DVec2::splat(
            link_length / ASSIGNMENT_CUE_MIN_LINK_LENGTH_PX,
        ));
        let direction_x = delta_x / link_length;
        let direction_y = delta_y / link_length;
        let tip_x = to_x - (direction_x * ASSIGNMENT_CUE_BACKOFF_PX * cue_pixel.x);
        let tip_y = to_y - (direction_y * ASSIGNMENT_CUE_BACKOFF_PX * cue_pixel.y);
        let perpendicular_x = -direction_y;
        let perpendicular_y = direction_x;
        for (part, side) in [
            (AssignmentCuePart::Left, -1.0),
            (AssignmentCuePart::Right, 1.0),
        ] {
            let tail_x = tip_x - (direction_x * ASSIGNMENT_CUE_ARM_LENGTH_PX * cue_pixel.x)
                + (perpendicular_x * ASSIGNMENT_CUE_HALF_WIDTH_PX * side * cue_pixel.x);
            let tail_y = tip_y - (direction_y * ASSIGNMENT_CUE_ARM_LENGTH_PX * cue_pixel.y)
                + (perpendicular_y * ASSIGNMENT_CUE_HALF_WIDTH_PX * side * cue_pixel.y);
            let key = (link.id.clone(), part);
            active_keys.insert(key.clone());
            let sprite = sprite_for_rect(
                ASSIGNMENT_CUE_COLOR,
                ((tip_x - tail_x).powi(2) + (tip_y - tail_y).powi(2)).sqrt() as f32,
                world_strokes::thickness(
                    ASSIGNMENT_CUE_THICKNESS_PX,
                    width,
                    height,
                    tip_x - tail_x,
                    tip_y - tail_y,
                ),
            );
            let transform = transform_for_line(
                tail_x,
                tail_y,
                tip_x,
                tip_y,
                width,
                height,
                ASSIGNMENT_CUE_LAYER_Z,
            );
            if let Some(entity) = existing_by_key.remove(&key) {
                commands.entity(entity).insert((sprite, transform));
            } else {
                commands.spawn((
                    sprite,
                    transform,
                    PixelWorldAssignmentCueVisual {
                        link_id: link.id.clone(),
                        part,
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
