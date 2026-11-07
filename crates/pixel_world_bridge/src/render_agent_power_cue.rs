use std::collections::{HashMap, HashSet};

use super::*;

const POWER_CUE_LAYER_Z_OFFSET: f32 = 0.011;
const POWER_CUE_THICKNESS_PX: f32 = 2.0;
const POWER_CUE_PADDING_PX: f32 = 2.0;
const POWER_LOW_COLOR: Color = Color::srgba_u8(251, 191, 36, 235);
const POWER_CRITICAL_COLOR: Color = Color::srgba_u8(249, 115, 22, 245);
const POWER_SHUTDOWN_COLOR: Color = Color::srgba_u8(248, 113, 113, 245);
const POWER_SHUTDOWN_DIM_COLOR: Color = Color::srgba_u8(15, 23, 42, 190);

/// Static, display-only Agent power treatment. It has no hit region and is
/// intentionally below selection, recommendation, and receipt treatments.
#[derive(Component)]
pub(super) struct PixelWorldAgentPowerCue {
    pub(crate) agent_id: String,
    part: AgentPowerCuePart,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum AgentPowerCuePart {
    LowEdge,
    CriticalEdgeTop,
    CriticalEdgeBottom,
    CriticalMarkStem,
    CriticalMarkDot,
    ShutdownDimBody,
    ShutdownEdgeTop,
    ShutdownEdgeBottom,
    ShutdownEdgeLeft,
    ShutdownEdgeRight,
    ShutdownStopBar,
}

struct PowerCueSpec {
    part: AgentPowerCuePart,
    offset: Vec2,
    size: Vec2,
    color: Color,
}

fn power_cue_specs(state: AgentPowerState, body_size: f32) -> Vec<PowerCueSpec> {
    let half = body_size / 2.0;
    let edge = half + POWER_CUE_PADDING_PX;
    match state {
        AgentPowerState::Normal => Vec::new(),
        AgentPowerState::LowPower => vec![PowerCueSpec {
            part: AgentPowerCuePart::LowEdge,
            offset: Vec2::new(0.0, edge),
            size: Vec2::new((body_size * 0.62).max(6.0), POWER_CUE_THICKNESS_PX),
            color: POWER_LOW_COLOR,
        }],
        AgentPowerState::Critical => vec![
            PowerCueSpec {
                part: AgentPowerCuePart::CriticalEdgeTop,
                offset: Vec2::new(0.0, edge),
                size: Vec2::new((body_size * 0.76).max(7.0), POWER_CUE_THICKNESS_PX),
                color: POWER_CRITICAL_COLOR,
            },
            PowerCueSpec {
                part: AgentPowerCuePart::CriticalEdgeBottom,
                offset: Vec2::new(0.0, -edge),
                size: Vec2::new((body_size * 0.76).max(7.0), POWER_CUE_THICKNESS_PX),
                color: POWER_CRITICAL_COLOR,
            },
            PowerCueSpec {
                part: AgentPowerCuePart::CriticalMarkStem,
                offset: Vec2::new(half * 0.45, 0.0),
                size: Vec2::new(POWER_CUE_THICKNESS_PX, (body_size * 0.34).max(4.0)),
                color: POWER_CRITICAL_COLOR,
            },
            PowerCueSpec {
                part: AgentPowerCuePart::CriticalMarkDot,
                offset: Vec2::new(half * 0.45, -(body_size * 0.27).max(3.0)),
                size: Vec2::splat(POWER_CUE_THICKNESS_PX),
                color: POWER_CRITICAL_COLOR,
            },
        ],
        AgentPowerState::Shutdown => vec![
            PowerCueSpec {
                part: AgentPowerCuePart::ShutdownDimBody,
                offset: Vec2::ZERO,
                size: Vec2::splat(body_size.max(6.0)),
                color: POWER_SHUTDOWN_DIM_COLOR,
            },
            PowerCueSpec {
                part: AgentPowerCuePart::ShutdownEdgeTop,
                offset: Vec2::new(0.0, edge),
                size: Vec2::new(body_size.max(6.0), POWER_CUE_THICKNESS_PX),
                color: POWER_SHUTDOWN_COLOR,
            },
            PowerCueSpec {
                part: AgentPowerCuePart::ShutdownEdgeBottom,
                offset: Vec2::new(0.0, -edge),
                size: Vec2::new(body_size.max(6.0), POWER_CUE_THICKNESS_PX),
                color: POWER_SHUTDOWN_COLOR,
            },
            PowerCueSpec {
                part: AgentPowerCuePart::ShutdownEdgeLeft,
                offset: Vec2::new(-edge, 0.0),
                size: Vec2::new(POWER_CUE_THICKNESS_PX, body_size.max(6.0)),
                color: POWER_SHUTDOWN_COLOR,
            },
            PowerCueSpec {
                part: AgentPowerCuePart::ShutdownEdgeRight,
                offset: Vec2::new(edge, 0.0),
                size: Vec2::new(POWER_CUE_THICKNESS_PX, body_size.max(6.0)),
                color: POWER_SHUTDOWN_COLOR,
            },
            PowerCueSpec {
                part: AgentPowerCuePart::ShutdownStopBar,
                offset: Vec2::ZERO,
                size: Vec2::new((body_size * 0.58).max(6.0), POWER_CUE_THICKNESS_PX),
                color: POWER_SHUTDOWN_COLOR,
            },
        ],
    }
}

pub(super) fn reconcile_agent_power_cues(
    commands: &mut Commands,
    runtime: &BevyRuntimeState,
    existing_cues: &Query<(Entity, &PixelWorldAgentPowerCue)>,
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

    let mut active_keys = HashSet::new();
    for (index, agent) in render_state.agents.iter().enumerate() {
        let Some(power_state) = agent.power_state.as_deref().and_then(|state| match state {
            "normal" => Some(AgentPowerState::Normal),
            "low_power" => Some(AgentPowerState::LowPower),
            "critical" => Some(AgentPowerState::Critical),
            "shutdown" => Some(AgentPowerState::Shutdown),
            _ => None,
        }) else {
            continue;
        };
        if power_state == AgentPowerState::Normal {
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
        let canvas_y = canvas_y - style.size_px * 7.0 / 16.0;
        let body_size = agent_unanimated_size_px(agent, is_selected) as f32;
        for spec in power_cue_specs(power_state, body_size) {
            let key = (agent.id.clone(), spec.part);
            active_keys.insert(key.clone());
            let sprite = sprite_for_rect(spec.color, spec.size.x, spec.size.y);
            let transform = Transform::from_translation(to_bevy_translation(
                canvas_x + f64::from(spec.offset.x),
                canvas_y + f64::from(spec.offset.y),
                width,
                height,
                style.layer_z + AGENT_CORE_LAYER_Z_OFFSET + POWER_CUE_LAYER_Z_OFFSET,
            ));
            if let Some(entity) = existing_by_key.remove(&key) {
                commands.entity(entity).insert((sprite, transform));
            } else {
                commands.spawn((
                    sprite,
                    transform,
                    PixelWorldAgentPowerCue {
                        agent_id: agent.id.clone(),
                        part: spec.part,
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

pub(super) fn despawn_agent_power_cues(
    commands: &mut Commands,
    existing_cues: &Query<(Entity, &PixelWorldAgentPowerCue)>,
) {
    for (entity, _) in existing_cues.iter() {
        commands.entity(entity).despawn();
    }
}
