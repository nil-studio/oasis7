use super::*;
#[path = "render_label_budget.rs"]
pub(crate) mod label_budget;
use bevy::ecs::system::SystemParam;
use std::collections::{HashMap, HashSet};
#[path = "render_fragment_visuals.rs"]
mod fragment_visuals;
#[path = "render_grounding.rs"]
mod grounding;
#[path = "render_spatial_overlays.rs"]
mod spatial_overlays;
use fragment_visuals::reconcile_fragments;
#[path = "render_micro_depot_facilities.rs"]
mod micro_depot_facilities;
use micro_depot_facilities::{
    MicroDepotFacilityOverlayQueries, PixelWorldMicroDepotVisual, reconcile_micro_depot_facilities,
};
#[cfg(test)]
use micro_depot_facilities::{
    PixelWorldMicroDepotDetailVisual, PixelWorldMicroDepotServiceRadiusVisual,
};
#[path = "render_module_visual_entities.rs"]
mod module_visual_entities;
use module_visual_entities::{
    ModuleIdentityChipQueries, PixelWorldModuleVisualEntity, despawn_module_identity_chips,
    reconcile_module_visual_entities,
};
#[path = "render_agent_silhouette.rs"]
mod agent_silhouette;
use agent_silhouette::{PixelWorldAgentSilhouetteVisual, reconcile_agent_silhouettes};
#[path = "render_agent_power_cue.rs"]
mod agent_power_cue;
use agent_power_cue::{
    PixelWorldAgentPowerCue, despawn_agent_power_cues, reconcile_agent_power_cues,
};
#[path = "render_agent_labels.rs"]
mod agent_labels;
use agent_labels::{AgentLabelQueries, despawn_agent_labels, reconcile_agent_labels};
#[path = "render_agent_position_provenance_cue.rs"]
mod agent_position_provenance_cue;
use agent_position_provenance_cue::*;
#[path = "render_agent_position_missing_cue.rs"]
mod agent_position_missing_cue;
use agent_position_missing_cue::{
    PixelWorldMissingPositionCue, despawn_missing_position_cues, reconcile_missing_position_cues,
};
#[path = "render_links.rs"]
mod links;
use links::{PixelWorldLinkVisual, reconcile_links};
#[path = "render_social_links.rs"]
mod social_links;
use social_links::{PixelWorldSocialLinkVisual, reconcile_social_links};
#[path = "render_assignment_cue.rs"]
mod assignment_cue;
#[cfg(test)]
use assignment_cue::AssignmentCuePart;
use assignment_cue::{
    PixelWorldAssignmentCueVisual, despawn_assignment_cues, reconcile_assignment_cues,
};
#[path = "render_selected_agent_cue.rs"]
mod selected_agent_cue;
use selected_agent_cue::AGENT_CORE_LAYER_Z_OFFSET;
#[cfg(test)]
use selected_agent_cue::{AGENT_CORE_COLOR, agent_core_size_px};
use selected_agent_cue::{
    PixelWorldAgentCoreVisual, PixelWorldSelectedAgentCue, reconcile_agent_cores,
    reconcile_selected_agent_cues,
};
#[path = "render_selected_location_cue.rs"]
mod selected_location_cue;
use selected_location_cue::{PixelWorldSelectedLocationCue, reconcile_selected_location_cues};
#[path = "render_location_labels.rs"]
mod location_labels;
use location_labels::{LocationLabelQueries, despawn_location_labels, reconcile_location_labels};
#[path = "render_map_labels.rs"]
pub(crate) mod map_labels;
use map_labels::{MapLabelQueries, despawn_map_labels, reconcile_map_labels};
#[path = "render_location_corner_frame.rs"]
mod location_corner_frame;
use location_corner_frame::{PixelWorldLocationCornerFrame, reconcile_location_corner_frames};
#[path = "render_location_resource_cue.rs"]
mod location_resource_cue;
use location_resource_cue::{PixelWorldLocationResourceCue, despawn, reconcile};
#[path = "render_selected_resource_readout.rs"]
mod selected_resource_readout;
use selected_resource_readout::{
    PixelWorldSelectedResourceReadout, despawn_selected_resource_readouts,
    reconcile_selected_resource_readout,
};
#[path = "render_receipt_target_cue.rs"]
mod receipt_target_cue;
use receipt_target_cue::{PixelWorldReceiptTargetCue, reconcile_receipt_target_cues};
#[path = "render_recommended_target_cue.rs"]
mod recommended_target_cue;
use recommended_target_cue::{PixelWorldRecommendedTargetCue, reconcile_recommended_target_cues};
#[path = "render_active_intent_cue.rs"]
mod active_intent_cue;
use active_intent_cue::{PixelWorldActiveIntentCue, reconcile_active_intent_cues};
#[path = "render_camera_fit.rs"]
mod camera_fit;
#[path = "render_canvas_resize.rs"]
mod canvas_resize;
use camera_fit::maybe_auto_fit_camera;
#[path = "render_hotspot_core.rs"]
mod hotspot_core;
#[cfg(test)]
use hotspot_core::PixelWorldHotspotCoreVisual;
use hotspot_core::{HotspotCoreQueries, despawn_hotspot_core_treatments, reconcile_hotspot_cores};
#[path = "render_hotspot_cues.rs"]
mod hotspot_cues;
use hotspot_cues::{HotspotCueQueries, despawn_hotspot_cues, reconcile_hotspot_cues};
const LOCATION_HIT_HALF_SIZE: f64 = 8.0;
const AGENT_HIT_HALF_SIZE: f64 = 8.0;
const HOTSPOT_HIT_HALF_SIZE: f64 = 8.0;
const MODULE_VISUAL_HIT_HALF_SIZE: f64 = 8.0;
const FRAGMENT_HIDDEN_THRESHOLD_PX: f64 = 1.5;
const FRAGMENT_DETAIL_THRESHOLD_PX: f64 = 10.0;
const FRAGMENT_LAYER_Z: f32 = 0.35;
const FRAGMENT_SHADOW_LAYER_Z: f32 = 0.34;
const FRAGMENT_INSET_LAYER_Z: f32 = 0.36;
const FRAGMENT_FLECK_LAYER_Z: f32 = 0.37;
const FRAGMENT_SHADOW_OFFSET_CAP: f32 = 0.12;
const FRAGMENT_SHADOW_ALPHA_CAP: f32 = 0.45;
const FRAGMENT_INSET_SIZE_SCALE: f32 = 0.36;
const FRAGMENT_INSET_OFFSET_SCALE: f32 = 0.22;
const FRAGMENT_FLECK_SIZE_SCALE: f32 = 0.16;
const FRAGMENT_FLECK_OFFSET_SCALE: f32 = 0.22;
const LOCATION_LAYER_Z: f32 = 1.0;
const MICRO_DEPOT_LAYER_Z: f32 = 2.55;
const MODULE_VISUAL_ENTITY_LAYER_Z: f32 = 2.7;
const SELECTED_LOCATION_CUE_LAYER_Z: f32 = 2.1;
const AGENT_LAYER_Z: f32 = 3.0;
const SELECTED_ENTITY_LAYER_Z_OFFSET: f32 = 1.0;
const BLOCKER_ATTENTION_LAYER_Z: f32 = AGENT_LAYER_Z + SELECTED_ENTITY_LAYER_Z_OFFSET + 0.04;

pub(crate) fn hotspot_layer_z(kind: &str) -> f32 {
    if kind == "blocker" {
        BLOCKER_ATTENTION_LAYER_Z
    } else {
        1.5
    }
}
const SELECTED_ENTITY_SIZE_SCALE: f64 = 1.35;
const SELECTED_LOCATION_CUE_THICKNESS_PX: f32 = 2.0;
const SELECTED_LOCATION_CUE_PADDING_PX: f32 = 2.0;
const SELECTED_LOCATION_CUE_COLOR: Color = Color::srgb_u8(251, 191, 36);
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct GridLayoutKey {
    width: i32,
    height: i32,
    step_milli: i32,
    offset_x_milli: i32,
    offset_y_milli: i32,
    major_phase_x: i32,
    major_phase_y: i32,
}
#[derive(Component)]
pub(crate) struct PixelWorldGridVisual;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum FragmentTerrainLod {
    Hidden,
    Background,
    Detail,
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct FragmentVisualStyle {
    pub lod: FragmentTerrainLod,
    pub size_px: f64,
    pub alpha: f64,
    pub layer_z: f32,
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct LocationVisualStyle {
    pub size_px: f64,
    pub alpha: f64,
    pub layer_z: f32,
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct AgentVisualStyle {
    pub size_px: f64,
    pub layer_z: f32,
}
#[derive(Component)]
pub(crate) struct PixelWorldFragmentVisual {
    id: String,
}
#[derive(Component)]
struct PixelWorldFragmentShadowVisual {
    id: String,
}
#[derive(Component)]
struct PixelWorldFragmentInsetVisual {
    id: String,
}
#[derive(Component)]
struct PixelWorldFragmentFleckVisual {
    id: String,
}
#[derive(Component)]
pub(crate) struct PixelWorldLocationVisual {
    id: String,
}
#[derive(Component)]
pub(crate) struct PixelWorldAgentVisual {
    id: String,
}
#[derive(Component)]
pub(crate) struct PixelWorldHotspotVisual {
    id: String,
}
#[path = "render_camera_focus.rs"]
mod camera_focus;
use camera_focus::maybe_focus_selected_entity;
#[cfg(test)]
use camera_focus::selection_focus_position;

pub(crate) fn build_grid_layout(camera: &CameraState, width: f64, height: f64) -> GridLayoutKey {
    let grid_step = clamp(24.0 * camera.zoom.max(0.5), 12.0, 72.0);
    let offset_x = ((camera.pan_x_px % grid_step) + grid_step) % grid_step;
    let offset_y = ((camera.pan_y_px % grid_step) + grid_step) % grid_step;
    GridLayoutKey {
        width: width.round() as i32,
        height: height.round() as i32,
        step_milli: (grid_step * 1000.0).round() as i32,
        offset_x_milli: (offset_x * 1000.0).round() as i32,
        offset_y_milli: (offset_y * 1000.0).round() as i32,
        major_phase_x: (camera.pan_x_px / grid_step).floor().rem_euclid(5.0) as i32,
        major_phase_y: (camera.pan_y_px / grid_step).floor().rem_euclid(5.0) as i32,
    }
}
pub(crate) fn fragment_screen_size_px(
    footprint_cm: f64,
    world_bounds: &WorldBounds,
    width: f64,
    height: f64,
    camera: &CameraState,
) -> f64 {
    let scale_x = (width - 40.0).max(1.0) / world_bounds.width_cm.max(1.0);
    let scale_y = (height - 40.0).max(1.0) / world_bounds.depth_cm.max(1.0);
    footprint_cm.max(0.0) * scale_x.min(scale_y) * camera.zoom.max(0.5)
}
pub(crate) fn classify_fragment_lod(screen_size_px: f64) -> FragmentTerrainLod {
    if screen_size_px < FRAGMENT_HIDDEN_THRESHOLD_PX {
        FragmentTerrainLod::Hidden
    } else if screen_size_px < FRAGMENT_DETAIL_THRESHOLD_PX {
        FragmentTerrainLod::Background
    } else {
        FragmentTerrainLod::Detail
    }
}
fn fragment_alpha(fragment: &FragmentTerrainPatch, lod: FragmentTerrainLod) -> f64 {
    match lod {
        FragmentTerrainLod::Hidden => 0.0,
        FragmentTerrainLod::Background => {
            0.18 + (clamp(fragment.emphasis.unwrap_or(0.5), 0.0, 1.0) * 0.18)
        }
        FragmentTerrainLod::Detail => {
            0.38 + (clamp(fragment.emphasis.unwrap_or(0.5), 0.0, 1.0) * 0.28)
        }
    }
}
pub(crate) fn fragment_visual_style(
    fragment: &FragmentTerrainPatch,
    world_bounds: &WorldBounds,
    width: f64,
    height: f64,
    camera: &CameraState,
) -> Option<FragmentVisualStyle> {
    let screen_size =
        fragment_screen_size_px(fragment.footprint_cm, world_bounds, width, height, camera);
    let lod = classify_fragment_lod(screen_size);
    if lod == FragmentTerrainLod::Hidden {
        return None;
    }
    let size_px = match lod {
        FragmentTerrainLod::Hidden => 0.0,
        FragmentTerrainLod::Background => screen_size.clamp(2.0, 9.0),
        FragmentTerrainLod::Detail => screen_size.clamp(10.0, 42.0),
    };
    Some(FragmentVisualStyle {
        lod,
        size_px,
        alpha: fragment_alpha(fragment, lod).clamp(0.0, 1.0),
        layer_z: FRAGMENT_LAYER_Z,
    })
}
pub(crate) fn location_visual_style(location: &Location, animation_ms: f64) -> LocationVisualStyle {
    let is_logic_anchor = location.marker_role.as_deref() == Some("logic_anchor");
    let pulse = if is_logic_anchor {
        1.0
    } else {
        1.0 + (0.08 * ((animation_ms / 360.0) + location.id.len() as f64).sin())
    };
    let size_px = location
        .size_hint_px
        .unwrap_or(if is_logic_anchor { 10.0 } else { 16.0 })
        * pulse;
    let alpha = location
        .marker_alpha
        .unwrap_or(if is_logic_anchor { 0.32 } else { 0.72 })
        .clamp(0.16, 0.84);
    LocationVisualStyle {
        size_px,
        alpha,
        layer_z: LOCATION_LAYER_Z,
    }
}
fn selected_location_visual_style(
    location: &Location,
    is_selected: bool,
    animation_ms: f64,
) -> LocationVisualStyle {
    let mut style = location_visual_style(location, animation_ms);
    if is_selected {
        style.size_px *= SELECTED_ENTITY_SIZE_SCALE;
        style.layer_z += SELECTED_ENTITY_LAYER_Z_OFFSET;
    }
    style
}
pub(crate) fn agent_visual_style(
    agent: &Agent,
    is_selected: bool,
    animation_ms: f64,
    index: usize,
) -> AgentVisualStyle {
    let pulse = 1.0 + (0.12 * ((animation_ms / 240.0) + index as f64).sin());
    let base_size = agent_unanimated_size_px(agent, is_selected);
    AgentVisualStyle {
        size_px: base_size * pulse,
        layer_z: AGENT_LAYER_Z
            + if is_selected {
                SELECTED_ENTITY_LAYER_Z_OFFSET
            } else {
                0.0
            },
    }
}
fn agent_unanimated_size_px(agent: &Agent, is_selected: bool) -> f64 {
    let base_size = if is_selected {
        agent.size_hint_px.unwrap_or(15.0).max(15.0)
    } else {
        agent.size_hint_px.unwrap_or(12.0)
    };
    base_size
        * if is_selected {
            SELECTED_ENTITY_SIZE_SCALE
        } else {
            1.0
        }
}
fn grid_geometry(layout: &GridLayoutKey) -> (f64, f64, f64, f64, Color) {
    (
        layout.step_milli as f64 / 1000.0,
        layout.offset_x_milli as f64 / 1000.0,
        layout.offset_y_milli as f64 / 1000.0,
        layout.width as f64,
        Color::srgba_u8(99, 179, 255, 12),
    )
}
fn reconcile_grid(
    commands: &mut Commands,
    runtime: &mut BevyRuntimeState,
    existing_grid: &Query<Entity, With<PixelWorldGridVisual>>,
    width: f64,
    height: f64,
) {
    let next_layout = build_grid_layout(&runtime.camera, width, height);
    if runtime.grid_layout.as_ref() == Some(&next_layout) {
        return;
    }
    for entity in existing_grid.iter() {
        commands.entity(entity).despawn();
    }
    let (grid_step, offset_x, offset_y, layout_width, grid_color) = grid_geometry(&next_layout);
    let layout_height = next_layout.height as f64;
    let mut x = offset_x;
    while x <= layout_width {
        let color =
            if (((x - runtime.camera.pan_x_px) / grid_step).round() as i64).rem_euclid(5) == 0 {
                Color::srgba_u8(99, 179, 255, 20)
            } else {
                grid_color
            };
        commands.spawn((
            sprite_for_rect(color, 1.0, layout_height as f32),
            Transform::from_translation(to_bevy_translation(
                x,
                layout_height / 2.0,
                layout_width,
                layout_height,
                0.0,
            )),
            PixelWorldGridVisual,
        ));
        x += grid_step;
    }
    let mut y = offset_y;
    while y <= layout_height {
        let color =
            if (((y - runtime.camera.pan_y_px) / grid_step).round() as i64).rem_euclid(5) == 0 {
                Color::srgba_u8(99, 179, 255, 20)
            } else {
                grid_color
            };
        commands.spawn((
            sprite_for_rect(color, layout_width as f32, 1.0),
            Transform::from_translation(to_bevy_translation(
                layout_width / 2.0,
                y,
                layout_width,
                layout_height,
                0.0,
            )),
            PixelWorldGridVisual,
        ));
        y += grid_step;
    }
    runtime.grid_layout = Some(next_layout);
}
fn despawn_stale_entities(
    commands: &mut Commands,
    entities: &mut HashMap<String, Entity>,
    active_ids: &HashSet<String>,
) {
    entities.retain(|id, entity| {
        if active_ids.contains(id) {
            true
        } else {
            commands.entity(*entity).despawn();
            false
        }
    });
}
fn reconcile_locations(
    commands: &mut Commands,
    runtime: &mut BevyRuntimeState,
    width: f64,
    height: f64,
    animation_ms: f64,
    rebuild_hit_regions: bool,
) {
    let mut active_ids = HashSet::new();
    let Some(render_state) = runtime.render_state.as_ref() else {
        return;
    };
    let Some(world_bounds) = render_state.world_bounds.as_ref() else {
        for (_, entity) in runtime.location_entities.drain() {
            commands.entity(entity).despawn();
        }
        return;
    };
    for location in &render_state.locations {
        let Some((canvas_x, canvas_y)) =
            to_canvas_point(&location.pos, world_bounds, width, height, &runtime.camera)
        else {
            continue;
        };
        active_ids.insert(location.id.clone());
        let is_selected = render_state
            .selection
            .as_ref()
            .map(|selection| selection.kind == "location" && selection.id == location.id)
            .unwrap_or(false);
        let style = selected_location_visual_style(location, is_selected, animation_ms);
        let transform = Transform::from_translation(to_bevy_translation(
            canvas_x,
            canvas_y,
            width,
            height,
            style.layer_z,
        ));
        let sprite = sprite_for_rect(
            Color::srgba_u8(110, 231, 183, (style.alpha * 255.0).round() as u8),
            style.size_px as f32,
            style.size_px as f32 * 0.72,
        );
        if let Some(entity) = runtime.location_entities.get(&location.id).copied() {
            commands.entity(entity).insert((sprite, transform));
        } else {
            let entity = commands
                .spawn((
                    sprite,
                    transform,
                    PixelWorldLocationVisual {
                        id: location.id.clone(),
                    },
                ))
                .id();
            runtime
                .location_entities
                .insert(location.id.clone(), entity);
        }
        if rebuild_hit_regions {
            runtime.hit_regions.push(HitRegion {
                kind: "location",
                id: location.id.clone(),
                left: canvas_x - LOCATION_HIT_HALF_SIZE,
                top: canvas_y - LOCATION_HIT_HALF_SIZE,
                right: canvas_x + LOCATION_HIT_HALF_SIZE,
                bottom: canvas_y + LOCATION_HIT_HALF_SIZE,
            });
        }
    }
    despawn_stale_entities(commands, &mut runtime.location_entities, &active_ids);
}
fn reconcile_agents(
    commands: &mut Commands,
    runtime: &mut BevyRuntimeState,
    width: f64,
    height: f64,
    animation_ms: f64,
    rebuild_hit_regions: bool,
) {
    let Some(render_state) = runtime.render_state.as_ref() else {
        for (_, entity) in runtime.agent_entities.drain() {
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
            .map(|selection| selection.kind == "agent" && selection.id == agent.id)
            .unwrap_or(false);
        let style = agent_visual_style(agent, is_selected, animation_ms, index);
        let color = if is_selected {
            Color::srgb_u8(251, 191, 36)
        } else {
            Color::srgb_u8(99, 179, 255)
        };
        let transform = Transform::from_translation(to_bevy_translation(
            canvas_x,
            canvas_y,
            width,
            height,
            style.layer_z,
        ));
        let sprite = sprite_for_rect(color, style.size_px as f32 * 0.66, style.size_px as f32);
        if let Some(entity) = runtime.agent_entities.get(&agent.id).copied() {
            commands.entity(entity).insert((sprite, transform));
        } else {
            let entity = commands
                .spawn((
                    sprite,
                    transform,
                    PixelWorldAgentVisual {
                        id: agent.id.clone(),
                    },
                ))
                .id();
            runtime.agent_entities.insert(agent.id.clone(), entity);
        }
        if rebuild_hit_regions {
            runtime.hit_regions.push(HitRegion {
                kind: "agent",
                id: agent.id.clone(),
                left: canvas_x - AGENT_HIT_HALF_SIZE,
                top: canvas_y - AGENT_HIT_HALF_SIZE,
                right: canvas_x + AGENT_HIT_HALF_SIZE,
                bottom: canvas_y + AGENT_HIT_HALF_SIZE,
            });
        }
    }
    despawn_stale_entities(commands, &mut runtime.agent_entities, &active_ids);
}
fn reconcile_hotspots(
    commands: &mut Commands,
    runtime: &mut BevyRuntimeState,
    width: f64,
    height: f64,
    animation_ms: f64,
    rebuild_hit_regions: bool,
) {
    let Some(render_state) = runtime.render_state.as_ref() else {
        for (_, entity) in runtime.hotspot_entities.drain() {
            commands.entity(entity).despawn();
        }
        return;
    };
    let Some(world_bounds) = render_state.world_bounds.as_ref() else {
        for (_, entity) in runtime.hotspot_entities.drain() {
            commands.entity(entity).despawn();
        }
        return;
    };
    let mut active_ids = HashSet::new();
    for (index, hotspot) in render_state.visual_hotspots.iter().enumerate() {
        let Some((canvas_x, canvas_y)) =
            to_canvas_point(&hotspot.pos, world_bounds, width, height, &runtime.camera)
        else {
            continue;
        };
        active_ids.insert(hotspot.id.clone());
        if rebuild_hit_regions {
            runtime.hit_regions.insert(
                0,
                HitRegion {
                    kind: "hotspot",
                    id: hotspot.id.clone(),
                    left: canvas_x - HOTSPOT_HIT_HALF_SIZE,
                    top: canvas_y - HOTSPOT_HIT_HALF_SIZE,
                    right: canvas_x + HOTSPOT_HIT_HALF_SIZE,
                    bottom: canvas_y + HOTSPOT_HIT_HALF_SIZE,
                },
            );
        }
        let emphasis = clamp(hotspot.emphasis.unwrap_or(0.7), 0.35, 1.0);
        let pulse = 1.0 + (0.1 * ((animation_ms / 280.0) + index as f64).sin());
        let size = hotspot.size_hint_px.unwrap_or(10.0) * pulse;
        let color = match hotspot.kind.as_str() {
            "blocker" => Color::srgba_u8(249, 115, 22, 210),
            "goal" => Color::srgba_u8(250, 204, 21, 196),
            _ => Color::srgba(0.56, 0.84, 1.0, (0.28 + (emphasis * 0.48)) as f32),
        };
        let layer_z = hotspot_layer_z(&hotspot.kind);
        let mut transform = Transform::from_translation(to_bevy_translation(
            canvas_x, canvas_y, width, height, layer_z,
        ));
        transform.rotation = Quat::from_rotation_z(std::f32::consts::FRAC_PI_4);
        let sprite = sprite_for_square(color, size as f32);
        if let Some(entity) = runtime.hotspot_entities.get(&hotspot.id).copied() {
            commands.entity(entity).insert((sprite, transform));
        } else {
            let entity = commands
                .spawn((
                    sprite,
                    transform,
                    PixelWorldHotspotVisual {
                        id: hotspot.id.clone(),
                    },
                ))
                .id();
            runtime.hotspot_entities.insert(hotspot.id.clone(), entity);
        }
    }
    despawn_stale_entities(commands, &mut runtime.hotspot_entities, &active_ids);
}
fn clear_runtime_visuals(commands: &mut Commands, runtime: &mut BevyRuntimeState) {
    for (_, entity) in runtime.fragment_entities.drain() {
        commands.entity(entity).despawn();
    }
    for (_, entity) in runtime.micro_depot_entities.drain() {
        commands.entity(entity).despawn();
    }
    for (_, entity) in runtime.module_visual_entities.drain() {
        commands.entity(entity).despawn();
    }
    for (_, entity) in runtime.location_entities.drain() {
        commands.entity(entity).despawn();
    }
    for (_, entity) in runtime.agent_entities.drain() {
        commands.entity(entity).despawn();
    }
    for (_, entity) in runtime.link_entities.drain() {
        commands.entity(entity).despawn();
    }
    for (_, entity) in runtime.hotspot_entities.drain() {
        commands.entity(entity).despawn();
    }
    runtime.grid_layout = None;
    runtime.hit_regions.clear();
    runtime.hit_region_cache_key = None;
    runtime.hit_regions_dirty = true;
    runtime.hover_key = None;
}
#[derive(SystemParam)]
pub(crate) struct RenderSceneQueries<'w, 's> {
    windows: Query<'w, 's, &'static Window, With<PrimaryWindow>>,
    spatial_overlays: Query<'w, 's, (Entity, &'static spatial_overlays::SpatialOverlayVisual)>,
    current_grid: Query<'w, 's, Entity, With<PixelWorldGridVisual>>,
    grounding: Query<'w, 's, (Entity, &'static grounding::PixelWorldGroundingVisual)>,
    fragment_visuals: Query<'w, 's, (Entity, &'static PixelWorldFragmentVisual)>,
    fragment_shadows: Query<'w, 's, (Entity, &'static PixelWorldFragmentShadowVisual)>,
    fragment_insets: Query<'w, 's, (Entity, &'static PixelWorldFragmentInsetVisual)>,
    fragment_flecks: Query<'w, 's, (Entity, &'static PixelWorldFragmentFleckVisual)>,
    micro_depot_visuals: Query<'w, 's, (Entity, &'static PixelWorldMicroDepotVisual)>,
    micro_depot_overlays: MicroDepotFacilityOverlayQueries<'w, 's>,
    module_visual_entities: Query<'w, 's, (Entity, &'static PixelWorldModuleVisualEntity)>,
    module_identity_chips: ModuleIdentityChipQueries<'w, 's>,
    location_visuals: Query<'w, 's, (Entity, &'static PixelWorldLocationVisual)>,
    location_labels: LocationLabelQueries<'w, 's>,
    map_labels: MapLabelQueries<'w, 's>,
    location_resource_cues: Query<'w, 's, (Entity, &'static PixelWorldLocationResourceCue)>,
    selected_location_cues: Query<'w, 's, (Entity, &'static PixelWorldSelectedLocationCue)>,
    location_corner_frames: Query<'w, 's, (Entity, &'static PixelWorldLocationCornerFrame)>,
    selected_resource_readouts: Query<'w, 's, (Entity, &'static PixelWorldSelectedResourceReadout)>,
    agent_visuals: Query<'w, 's, (Entity, &'static PixelWorldAgentVisual)>,
    agent_labels: AgentLabelQueries<'w, 's>,
    agent_silhouettes: Query<'w, 's, (Entity, &'static PixelWorldAgentSilhouetteVisual)>,
    agent_power_cues: Query<'w, 's, (Entity, &'static PixelWorldAgentPowerCue)>,
    derived_position_cues: Query<'w, 's, (Entity, &'static PixelWorldDerivedPositionCue)>,
    missing_position_cues: Query<'w, 's, (Entity, &'static PixelWorldMissingPositionCue)>,
    assignment_cues: Query<'w, 's, (Entity, &'static PixelWorldAssignmentCueVisual)>,
    agent_cores: Query<'w, 's, (Entity, &'static PixelWorldAgentCoreVisual)>,
    selected_agent_cues: Query<'w, 's, (Entity, &'static PixelWorldSelectedAgentCue)>,
    receipt_target_cues: Query<'w, 's, (Entity, &'static PixelWorldReceiptTargetCue)>,
    recommended_target_cues: Query<'w, 's, (Entity, &'static PixelWorldRecommendedTargetCue)>,
    active_intent_cues: Query<'w, 's, (Entity, &'static PixelWorldActiveIntentCue)>,
    link_visuals: Query<'w, 's, (Entity, &'static PixelWorldLinkVisual)>,
    social_link_visuals: Query<'w, 's, (Entity, &'static PixelWorldSocialLinkVisual)>,
    hotspot_visuals: Query<'w, 's, (Entity, &'static PixelWorldHotspotVisual)>,
    hotspot_cues: HotspotCueQueries<'w, 's>,
    hotspot_cores: HotspotCoreQueries<'w, 's>,
}
pub(crate) fn render_scene(
    mut commands: Commands,
    mut runtime: ResMut<BevyRuntimeState>,
    queries: RenderSceneQueries,
    time: Res<Time>,
) {
    if !runtime.mounted {
        spatial_overlays::clear(&mut commands, &queries.spatial_overlays);
        grounding::clear(&mut commands, &queries.grounding);
        clear_runtime_visuals(&mut commands, &mut runtime);
        for (entity, _) in queries.selected_location_cues.iter() {
            commands.entity(entity).despawn();
        }
        for (entity, _) in queries.location_corner_frames.iter() {
            commands.entity(entity).despawn();
        }
        despawn_location_labels(&mut commands, &queries.location_labels);
        despawn_map_labels(&mut commands, &queries.map_labels);
        despawn(&mut commands, &queries.location_resource_cues);
        despawn_selected_resource_readouts(&mut commands, &queries.selected_resource_readouts);
        for (entity, _) in queries.fragment_insets.iter() {
            commands.entity(entity).despawn();
        }
        for (entity, _) in queries.fragment_flecks.iter() {
            commands.entity(entity).despawn();
        }
        queries.micro_depot_overlays.despawn(&mut commands);
        despawn_module_identity_chips(&mut commands, &queries.module_identity_chips);
        for (entity, _) in queries.fragment_shadows.iter() {
            commands.entity(entity).despawn();
        }
        for (entity, _) in queries.agent_cores.iter() {
            commands.entity(entity).despawn();
        }
        despawn_agent_labels(&mut commands, &queries.agent_labels);
        for (entity, _) in queries.derived_position_cues.iter() {
            commands.entity(entity).despawn();
        }
        despawn_missing_position_cues(&mut commands, &queries.missing_position_cues);
        despawn_assignment_cues(&mut commands, &queries.assignment_cues);
        social_links::despawn_social_links(&mut commands, &queries.social_link_visuals);
        for (entity, _) in queries.selected_agent_cues.iter() {
            commands.entity(entity).despawn();
        }
        for (entity, _) in queries.receipt_target_cues.iter() {
            commands.entity(entity).despawn();
        }
        for (entity, _) in queries.recommended_target_cues.iter() {
            commands.entity(entity).despawn();
        }
        for (entity, _) in queries.active_intent_cues.iter() {
            commands.entity(entity).despawn();
        }
        despawn_hotspot_core_treatments(&mut commands, &queries.hotspot_cores);
        despawn_hotspot_cues(&mut commands, &queries.hotspot_cues);
        for (entity, _) in queries.agent_silhouettes.iter() {
            commands.entity(entity).despawn();
        }
        despawn_agent_power_cues(&mut commands, &queries.agent_power_cues);
        for entity in queries.current_grid.iter() {
            commands.entity(entity).despawn();
        }
        return;
    }
    for (entity, visual) in queries.location_visuals.iter() {
        runtime
            .location_entities
            .entry(visual.id.clone())
            .or_insert(entity);
    }
    for (entity, visual) in queries.fragment_visuals.iter() {
        runtime
            .fragment_entities
            .entry(visual.id.clone())
            .or_insert(entity);
    }
    for (entity, visual) in queries.micro_depot_visuals.iter() {
        runtime
            .micro_depot_entities
            .entry(visual.id.clone())
            .or_insert(entity);
    }
    for (entity, visual) in queries.module_visual_entities.iter() {
        runtime
            .module_visual_entities
            .entry(visual.id.clone())
            .or_insert(entity);
    }
    for (entity, visual) in queries.agent_visuals.iter() {
        runtime
            .agent_entities
            .entry(visual.id.clone())
            .or_insert(entity);
    }
    for (entity, visual) in queries.link_visuals.iter() {
        runtime
            .link_entities
            .entry(visual.id.clone())
            .or_insert(entity);
    }
    for (entity, visual) in queries.hotspot_visuals.iter() {
        runtime
            .hotspot_entities
            .entry(visual.id.clone())
            .or_insert(entity);
    }
    let Ok(window) = queries.windows.single() else {
        return;
    };
    let Some(_) = runtime.render_state.as_ref() else {
        grounding::clear(&mut commands, &queries.grounding);
        clear_runtime_visuals(&mut commands, &mut runtime);
        for (entity, _) in queries.selected_location_cues.iter() {
            commands.entity(entity).despawn();
        }
        for (entity, _) in queries.location_corner_frames.iter() {
            commands.entity(entity).despawn();
        }
        despawn_location_labels(&mut commands, &queries.location_labels);
        despawn(&mut commands, &queries.location_resource_cues);
        despawn_selected_resource_readouts(&mut commands, &queries.selected_resource_readouts);
        for (entity, _) in queries.fragment_insets.iter() {
            commands.entity(entity).despawn();
        }
        for (entity, _) in queries.fragment_flecks.iter() {
            commands.entity(entity).despawn();
        }
        queries.micro_depot_overlays.despawn(&mut commands);
        despawn_module_identity_chips(&mut commands, &queries.module_identity_chips);
        for (entity, _) in queries.fragment_shadows.iter() {
            commands.entity(entity).despawn();
        }
        for (entity, _) in queries.agent_cores.iter() {
            commands.entity(entity).despawn();
        }
        despawn_agent_labels(&mut commands, &queries.agent_labels);
        for (entity, _) in queries.derived_position_cues.iter() {
            commands.entity(entity).despawn();
        }
        despawn_missing_position_cues(&mut commands, &queries.missing_position_cues);
        despawn_assignment_cues(&mut commands, &queries.assignment_cues);
        social_links::despawn_social_links(&mut commands, &queries.social_link_visuals);
        for (entity, _) in queries.selected_agent_cues.iter() {
            commands.entity(entity).despawn();
        }
        for (entity, _) in queries.receipt_target_cues.iter() {
            commands.entity(entity).despawn();
        }
        for (entity, _) in queries.recommended_target_cues.iter() {
            commands.entity(entity).despawn();
        }
        for (entity, _) in queries.active_intent_cues.iter() {
            commands.entity(entity).despawn();
        }
        despawn_hotspot_core_treatments(&mut commands, &queries.hotspot_cores);
        despawn_hotspot_cues(&mut commands, &queries.hotspot_cues);
        for (entity, _) in queries.agent_silhouettes.iter() {
            commands.entity(entity).despawn();
        }
        despawn_agent_power_cues(&mut commands, &queries.agent_power_cues);
        for entity in queries.current_grid.iter() {
            commands.entity(entity).despawn();
        }
        return;
    };
    let width = window.width() as f64;
    let height = window.height() as f64;
    let canvas_size_changed = runtime.last_canvas_size
        != Some((
            width.round().max(0.0) as u32,
            height.round().max(0.0) as u32,
        ));
    let static_reconcile =
        !runtime.reactive_scheduling || runtime.needs_reconcile || canvas_size_changed;
    let animation_reconcile = runtime.animation_dirty;
    if runtime.reactive_scheduling && !static_reconcile && !animation_reconcile {
        return;
    }
    let renderer_scale = if static_reconcile {
        renderer_to_css_scale(width, height)
    } else {
        Vec2::ONE
    };
    if runtime.reactive_scheduling {
        runtime.needs_reconcile = false;
        runtime.animation_dirty = false;
    }
    let animation_ms = if runtime.reduced_motion {
        0.0
    } else {
        time.elapsed_secs_f64() * 1000.0
    };
    let mut rebuild_hit_regions = false;
    if static_reconcile {
        canvas_resize::requeue_follow_target_after_resize(&mut runtime, width, height);
        maybe_auto_fit_camera(&mut runtime, width, height);
        navigation::apply_pending_fit_bounds(&mut runtime, width, height);
        maybe_focus_selected_entity(&mut runtime, width, height);
        let next_hit_region_cache_key = HitRegionCacheKey::new(&runtime, width, height);
        rebuild_hit_regions = runtime.hit_regions_dirty
            || runtime.hit_region_cache_key != Some(next_hit_region_cache_key);
        if rebuild_hit_regions {
            runtime.hit_regions.clear();
            runtime.hit_region_cache_key = Some(next_hit_region_cache_key);
            runtime.hit_regions_dirty = false;
        }
    }
    if static_reconcile {
        spatial_overlays::reconcile(
            &mut commands,
            &mut runtime,
            &queries.spatial_overlays,
            width,
            height,
            rebuild_hit_regions,
        );
        reconcile_grid(
            &mut commands,
            &mut runtime,
            &queries.current_grid,
            width,
            height,
        );
        reconcile_fragments(
            &mut commands,
            &mut runtime,
            &queries.fragment_shadows,
            &queries.fragment_insets,
            &queries.fragment_flecks,
            width,
            height,
        );
        reconcile_micro_depot_facilities(
            &mut commands,
            &mut runtime,
            &queries.micro_depot_overlays,
            width,
            height,
        );
        reconcile_module_visual_entities(
            &mut commands,
            &mut runtime,
            &queries.module_identity_chips,
            width,
            height,
            renderer_scale,
            rebuild_hit_regions,
        );
        reconcile_links(&mut commands, &mut runtime, width, height);
        reconcile_social_links(
            &mut commands,
            &runtime,
            &queries.social_link_visuals,
            width,
            height,
        );
        reconcile_assignment_cues(
            &mut commands,
            &runtime,
            &queries.assignment_cues,
            width,
            height,
        );
    }
    reconcile_locations(
        &mut commands,
        &mut runtime,
        width,
        height,
        animation_ms,
        rebuild_hit_regions,
    );
    reconcile_location_labels(
        &mut commands,
        &runtime,
        &queries.location_labels,
        width,
        height,
    );
    reconcile_map_labels(&mut commands, &runtime, &queries.map_labels, width, height);
    reconcile_location_corner_frames(
        &mut commands,
        &runtime,
        &queries.location_corner_frames,
        width,
        height,
        animation_ms,
    );
    reconcile(
        &mut commands,
        &runtime,
        &queries.location_resource_cues,
        width,
        height,
    );
    reconcile_selected_location_cues(
        &mut commands,
        &runtime,
        &queries.selected_location_cues,
        width,
        height,
        animation_ms,
    );
    reconcile_selected_resource_readout(
        &mut commands,
        &runtime,
        &queries.selected_resource_readouts,
        width,
        height,
    );
    reconcile_agents(
        &mut commands,
        &mut runtime,
        width,
        height,
        animation_ms,
        rebuild_hit_regions,
    );
    grounding::reconcile(
        &mut commands,
        &runtime,
        &queries.grounding,
        width,
        height,
        animation_ms,
    );
    reconcile_agent_labels(
        &mut commands,
        &runtime,
        &queries.agent_labels,
        width,
        height,
    );
    reconcile_agent_silhouettes(
        &mut commands,
        &runtime,
        &queries.agent_silhouettes,
        width,
        height,
        animation_ms,
    );
    reconcile_agent_power_cues(
        &mut commands,
        &runtime,
        &queries.agent_power_cues,
        width,
        height,
        animation_ms,
    );
    reconcile_derived_position_cues(
        &mut commands,
        &runtime,
        &queries.derived_position_cues,
        width,
        height,
        animation_ms,
    );
    reconcile_missing_position_cues(
        &mut commands,
        &runtime,
        &queries.missing_position_cues,
        width,
        height,
        animation_ms,
    );
    reconcile_agent_cores(
        &mut commands,
        &runtime,
        &queries.agent_cores,
        width,
        height,
        animation_ms,
    );
    reconcile_selected_agent_cues(
        &mut commands,
        &runtime,
        &queries.selected_agent_cues,
        width,
        height,
        animation_ms,
    );
    reconcile_active_intent_cues(
        &mut commands,
        &runtime,
        &queries.active_intent_cues,
        width,
        height,
    );
    reconcile_receipt_target_cues(
        &mut commands,
        &runtime,
        &queries.receipt_target_cues,
        width,
        height,
        animation_ms,
    );
    reconcile_recommended_target_cues(
        &mut commands,
        &runtime,
        &queries.recommended_target_cues,
        width,
        height,
        animation_ms,
    );
    reconcile_hotspots(
        &mut commands,
        &mut runtime,
        width,
        height,
        animation_ms,
        rebuild_hit_regions,
    );
    reconcile_hotspot_cues(
        &mut commands,
        &runtime,
        &queries.hotspot_cues,
        width,
        height,
        animation_ms,
    );
    reconcile_hotspot_cores(
        &mut commands,
        &runtime,
        &queries.hotspot_cores,
        width,
        height,
        animation_ms,
    );
    publish_hotspot_test_hit_targets(&runtime.hit_regions);
    publish_location_test_hit_targets(&runtime.hit_regions);
}
#[cfg(test)]
#[path = "render_tests.rs"]
mod tests;
