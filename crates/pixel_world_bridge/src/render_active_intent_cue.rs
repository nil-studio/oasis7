use std::collections::HashMap;

use super::*;

const ACTIVE_INTENT_CUE_COLOR: Color = Color::srgba_u8(250, 204, 21, 224);
const ACTIVE_INTENT_CUE_LAYER_Z_OFFSET: f32 = 0.035;
const ACTIVE_INTENT_CUE_THICKNESS_PX: f32 = 2.0;
const ACTIVE_INTENT_CUE_WIDTH_PX: f32 = 14.0;
const ACTIVE_INTENT_CUE_PADDING_PX: f32 = 4.0;

/// A non-interactive presentation cue for an authoritative active Intent.
/// The target and status are already projected by host_state; this module only
/// places the cue and never adds a hit region or control path.
#[derive(Component)]
pub(super) struct PixelWorldActiveIntentCue {
    pub(super) agent_id: String,
}

pub(super) fn reconcile_active_intent_cues(
    commands: &mut Commands,
    runtime: &BevyRuntimeState,
    existing_cues: &Query<(Entity, &PixelWorldActiveIntentCue)>,
    width: f64,
    height: f64,
) {
    let mut existing_by_id = HashMap::new();
    for (entity, cue) in existing_cues.iter() {
        if let Some(duplicate) = existing_by_id.insert(cue.agent_id.clone(), entity) {
            commands.entity(duplicate).despawn();
        }
    }

    let Some(render_state) = runtime.render_state.as_ref() else {
        for entity in existing_by_id.into_values() {
            commands.entity(entity).despawn();
        }
        return;
    };
    let Some(target) = render_state.active_intent_target.as_ref() else {
        for entity in existing_by_id.into_values() {
            commands.entity(entity).despawn();
        }
        return;
    };
    let Some(agent) = render_state
        .agents
        .iter()
        .find(|agent| agent.id == target.agent_id)
    else {
        for entity in existing_by_id.into_values() {
            commands.entity(entity).despawn();
        }
        return;
    };
    if agent.position_source != AgentPositionSource::Snapshot {
        for entity in existing_by_id.into_values() {
            commands.entity(entity).despawn();
        }
        return;
    }

    let Some((canvas_x, canvas_y)) = render_state.world_bounds.as_ref().and_then(|world_bounds| {
        agent
            .pos
            .as_ref()
            .and_then(|pos| to_canvas_point(pos, world_bounds, width, height, &runtime.camera))
    }) else {
        for entity in existing_by_id.into_values() {
            commands.entity(entity).despawn();
        }
        return;
    };
    let is_selected = render_state
        .selection
        .as_ref()
        .is_some_and(|selection| selection.kind == "agent" && selection.id == agent.id);
    let style = agent_visual_style(agent, is_selected, 0.0, 0);
    let canvas_y = canvas_y - style.size_px * 7.0 / 16.0;
    let body_half_size = style.size_px as f32 / 2.0;
    let (cue_width, cue_height) = match target.status.as_str() {
        "submitted" => (ACTIVE_INTENT_CUE_THICKNESS_PX, ACTIVE_INTENT_CUE_WIDTH_PX),
        "accepted" => (ACTIVE_INTENT_CUE_WIDTH_PX, ACTIVE_INTENT_CUE_THICKNESS_PX),
        "blocked" => (8.0, 8.0),
        _ => {
            for entity in existing_by_id.into_values() {
                commands.entity(entity).despawn();
            }
            return;
        }
    };
    let sprite = sprite_for_rect(ACTIVE_INTENT_CUE_COLOR, cue_width, cue_height);
    let transform = Transform::from_translation(to_bevy_translation(
        canvas_x,
        canvas_y - f64::from(body_half_size + ACTIVE_INTENT_CUE_PADDING_PX),
        width,
        height,
        AGENT_LAYER_Z + SELECTED_ENTITY_LAYER_Z_OFFSET + ACTIVE_INTENT_CUE_LAYER_Z_OFFSET,
    ));
    if let Some(entity) = existing_by_id.remove(&agent.id) {
        commands.entity(entity).insert((sprite, transform));
    } else {
        commands.spawn((
            sprite,
            transform,
            PixelWorldActiveIntentCue {
                agent_id: agent.id.clone(),
            },
        ));
    }
    for entity in existing_by_id.into_values() {
        commands.entity(entity).despawn();
    }
}
