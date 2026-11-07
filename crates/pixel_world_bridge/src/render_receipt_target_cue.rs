use std::collections::HashMap;

use super::selected_agent_cue::selected_agent_cue_outer_top_offset;
use super::*;

const RECEIPT_TARGET_CUE_COLOR: Color = Color::srgba_u8(226, 232, 240, 230);
const RECEIPT_TARGET_CUE_LAYER_Z_OFFSET: f32 = 0.04;
const RECEIPT_TARGET_CUE_THICKNESS_PX: f32 = 2.0;
const RECEIPT_TARGET_CUE_PADDING_PX: f32 = 4.0;
const RECEIPT_BADGE_BACKING_COLOR: Color = Color::srgba_u8(15, 23, 42, 240);
const NARROW_RECEIPT_BADGE_BACKING_COLOR: Color = Color::srgba_u8(15, 23, 42, 245);
const RECEIPT_BADGE_STROKE_COLOR: Color = Color::srgba_u8(248, 250, 252, 255);
const RECEIPT_BADGE_SIZE: Vec2 = Vec2::new(20.0, 16.0);
const NARROW_RECEIPT_BADGE_SIZE: Vec2 = Vec2::new(28.0, 22.0);
const NARROW_RECEIPT_BADGE_OUTLINE_COLOR: Color = Color::srgba_u8(203, 213, 225, 184);
const NARROW_RECEIPT_BADGE_MAX_WIDTH_PX: f64 = 360.0;
// At narrow canvas widths the hover tooltip occupies the upper-right overlay.
// Keep the blocked receipt badge on the target's upper-left shoulder so its
// pale X remains legible instead of being covered by that player feedback.
const NARROW_SELECTED_RECEIPT_BADGE_X_OFFSET_PX: f32 = -34.0;
const RECEIPT_BADGE_BACKING_LAYER_Z_OFFSET: f32 = 0.06;
const NARROW_RECEIPT_BADGE_OUTLINE_LAYER_Z_OFFSET: f32 = 0.065;
const RECEIPT_BADGE_STROKE_LAYER_Z_OFFSET: f32 = 0.07;
const RECEIPT_BADGE_SELECTED_FRAME_CLEARANCE_PX: f32 = 12.0;
const RECEIPT_BADGE_NONSELECTED_BODY_CLEARANCE_PX: f32 = 14.0;
const RECEIPT_BADGE_CROSS_DIAGONAL_PX: f32 = 12.0;

/// Non-interactive receipt feedback attached only to the currently rendered
/// target Agent. It deliberately carries no hit region or gameplay state.
#[derive(Component)]
pub(super) struct PixelWorldReceiptTargetCue {
    agent_id: String,
    part: ReceiptTargetCuePart,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
enum ReceiptTargetCuePart {
    BadgeBacking,
    BadgeOutlineTop,
    BadgeOutlineBottom,
    BadgeOutlineLeft,
    BadgeOutlineRight,
    CrossAscending,
    CrossDescending,
    RejectionSlash,
    UpwardStem,
    ConfirmationBar,
    UnknownDot,
}

struct ReceiptCueSpec {
    part: ReceiptTargetCuePart,
    offset: Vec2,
    size: Vec2,
    rotation: f32,
    color: Color,
    layer_z_offset: f32,
}

fn receipt_cue_specs(
    state: &str,
    body_half_size: f32,
    is_selected: bool,
    is_narrow: bool,
) -> Vec<ReceiptCueSpec> {
    let top = body_half_size + RECEIPT_TARGET_CUE_PADDING_PX;
    match state {
        "blocked" | "rejected" => {
            let badge_size = if is_narrow {
                NARROW_RECEIPT_BADGE_SIZE
            } else {
                RECEIPT_BADGE_SIZE
            };
            let badge_top = if is_selected {
                selected_agent_cue_outer_top_offset(body_half_size)
                    + if is_narrow {
                        (badge_size.y / 2.0) + 8.0
                    } else {
                        RECEIPT_BADGE_SELECTED_FRAME_CLEARANCE_PX
                    }
            } else if is_narrow {
                body_half_size + (badge_size.y / 2.0) + 8.0
            } else {
                body_half_size + RECEIPT_BADGE_NONSELECTED_BODY_CLEARANCE_PX
            };
            let badge_offset = Vec2::new(
                if is_narrow && is_selected {
                    NARROW_SELECTED_RECEIPT_BADGE_X_OFFSET_PX
                } else {
                    0.0
                },
                badge_top,
            );
            let mut specs = vec![ReceiptCueSpec {
                part: ReceiptTargetCuePart::BadgeBacking,
                offset: badge_offset,
                size: badge_size,
                rotation: 0.0,
                color: if is_narrow {
                    NARROW_RECEIPT_BADGE_BACKING_COLOR
                } else {
                    RECEIPT_BADGE_BACKING_COLOR
                },
                layer_z_offset: RECEIPT_BADGE_BACKING_LAYER_Z_OFFSET,
            }];
            let stroke_size = Vec2::new(
                if is_narrow {
                    18.0
                } else {
                    RECEIPT_BADGE_CROSS_DIAGONAL_PX
                },
                if is_narrow {
                    3.0
                } else {
                    RECEIPT_TARGET_CUE_THICKNESS_PX
                },
            );
            specs.push(ReceiptCueSpec {
                part: if state == "rejected" {
                    ReceiptTargetCuePart::RejectionSlash
                } else {
                    ReceiptTargetCuePart::CrossAscending
                },
                offset: badge_offset,
                size: stroke_size,
                rotation: std::f32::consts::FRAC_PI_4,
                color: RECEIPT_BADGE_STROKE_COLOR,
                layer_z_offset: RECEIPT_BADGE_STROKE_LAYER_Z_OFFSET,
            });
            if state == "blocked" {
                specs.push(ReceiptCueSpec {
                    part: ReceiptTargetCuePart::CrossDescending,
                    offset: badge_offset,
                    size: stroke_size,
                    rotation: -std::f32::consts::FRAC_PI_4,
                    color: RECEIPT_BADGE_STROKE_COLOR,
                    layer_z_offset: RECEIPT_BADGE_STROKE_LAYER_Z_OFFSET,
                });
            }
            if is_narrow {
                let horizontal_offset = (badge_size.y - 1.0) / 2.0;
                let vertical_offset = (badge_size.x - 1.0) / 2.0;
                for (part, offset, size) in [
                    (
                        ReceiptTargetCuePart::BadgeOutlineTop,
                        Vec2::new(0.0, horizontal_offset),
                        Vec2::new(badge_size.x, 1.0),
                    ),
                    (
                        ReceiptTargetCuePart::BadgeOutlineBottom,
                        Vec2::new(0.0, -horizontal_offset),
                        Vec2::new(badge_size.x, 1.0),
                    ),
                    (
                        ReceiptTargetCuePart::BadgeOutlineLeft,
                        Vec2::new(-vertical_offset, 0.0),
                        Vec2::new(1.0, badge_size.y),
                    ),
                    (
                        ReceiptTargetCuePart::BadgeOutlineRight,
                        Vec2::new(vertical_offset, 0.0),
                        Vec2::new(1.0, badge_size.y),
                    ),
                ] {
                    specs.push(ReceiptCueSpec {
                        part,
                        offset: badge_offset + offset,
                        size,
                        rotation: 0.0,
                        color: NARROW_RECEIPT_BADGE_OUTLINE_COLOR,
                        layer_z_offset: NARROW_RECEIPT_BADGE_OUTLINE_LAYER_Z_OFFSET,
                    });
                }
            }
            specs
        }
        "accepted" | "submitted" | "queued" | "ack" => vec![ReceiptCueSpec {
            part: ReceiptTargetCuePart::UpwardStem,
            offset: Vec2::new(0.0, top + 1.5),
            size: Vec2::new(RECEIPT_TARGET_CUE_THICKNESS_PX, 7.0),
            rotation: 0.0,
            color: RECEIPT_TARGET_CUE_COLOR,
            layer_z_offset: RECEIPT_TARGET_CUE_LAYER_Z_OFFSET,
        }],
        "completed" => vec![ReceiptCueSpec {
            part: ReceiptTargetCuePart::ConfirmationBar,
            offset: Vec2::new(0.0, top),
            size: Vec2::new(7.0, RECEIPT_TARGET_CUE_THICKNESS_PX),
            rotation: 0.0,
            color: RECEIPT_TARGET_CUE_COLOR,
            layer_z_offset: RECEIPT_TARGET_CUE_LAYER_Z_OFFSET,
        }],
        _ => vec![ReceiptCueSpec {
            part: ReceiptTargetCuePart::UnknownDot,
            offset: Vec2::new(0.0, top),
            size: Vec2::splat(3.0),
            rotation: 0.0,
            color: RECEIPT_TARGET_CUE_COLOR,
            layer_z_offset: RECEIPT_TARGET_CUE_LAYER_Z_OFFSET,
        }],
    }
}

pub(super) fn reconcile_receipt_target_cues(
    commands: &mut Commands,
    runtime: &BevyRuntimeState,
    existing_cues: &Query<(Entity, &PixelWorldReceiptTargetCue)>,
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
    let Some(receipt_target) = render_state.receipt_target.as_ref() else {
        for entity in existing_by_key.into_values() {
            commands.entity(entity).despawn();
        }
        return;
    };
    let Some((index, agent)) = render_state
        .agents
        .iter()
        .enumerate()
        .find(|(_, agent)| agent.id == receipt_target.agent_id)
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
    let receipt_base_layer_z =
        (AGENT_LAYER_Z + SELECTED_ENTITY_LAYER_Z_OFFSET + 0.01).max(style.layer_z);

    for spec in receipt_cue_specs(
        &receipt_target.state,
        body_half_size,
        is_selected,
        width <= NARROW_RECEIPT_BADGE_MAX_WIDTH_PX,
    ) {
        let key = (agent.id.clone(), spec.part);
        let sprite = sprite_for_rect(spec.color, spec.size.x, spec.size.y);
        let mut transform = Transform::from_translation(to_bevy_translation(
            canvas_x + f64::from(spec.offset.x),
            canvas_y - f64::from(spec.offset.y),
            width,
            height,
            receipt_base_layer_z + AGENT_CORE_LAYER_Z_OFFSET + spec.layer_z_offset,
        ));
        transform.rotation = Quat::from_rotation_z(spec.rotation);
        if let Some(entity) = existing_by_key.remove(&key) {
            commands.entity(entity).insert((sprite, transform));
        } else {
            commands.spawn((
                sprite,
                transform,
                PixelWorldReceiptTargetCue {
                    agent_id: agent.id.clone(),
                    part: spec.part,
                },
            ));
        }
    }
    for entity in existing_by_key.into_values() {
        commands.entity(entity).despawn();
    }
}
