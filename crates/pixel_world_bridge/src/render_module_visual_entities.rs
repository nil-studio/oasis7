use bevy::math::DVec2;
use std::collections::{HashMap, HashSet};

use super::map_labels::map_label_obstacles;
use super::*;

pub(crate) const MODULE_VISUAL_ENTITY_COLOR: Color = Color::srgb_u8(129, 140, 248);
pub(crate) const MODULE_VISUAL_ENTITY_SIZE_PX: f32 = 6.0;
const MODULE_IDENTITY_CHIP_COLOR: Color = Color::srgba_u8(226, 232, 240, 230);
const MODULE_IDENTITY_CHIP_LAYER_Z_OFFSET: f32 = 0.01;
const MODULE_LABEL_COLOR: Color = Color::srgb_u8(226, 232, 240);
// Text must share the established foreground text plane rather than be depth
// occluded by the world sprites that surround a module marker.
pub(crate) const MODULE_LABEL_LAYER_Z: f32 = 5.0;
const MODULE_LABEL_MIN_ZOOM: f64 = 1.75;
const MODULE_LABEL_ABOVE_MARKER_PX: f64 = 12.0;
const MODULE_LABEL_FONT_SIZE_PX: f32 = 10.0;
const MODULE_LABEL_GLYPH_ADVANCE_PX: f64 = 5.8;
const MODULE_LABEL_MAX_CHARS: usize = 24;
const MODULE_LABEL_HEIGHT_PX: f64 = 12.0;

#[derive(Component)]
pub(crate) struct PixelWorldModuleVisualEntity {
    pub(crate) id: String,
}

/// The text identity is intentionally a separate, noninteractive visual. It
/// follows the marker's reconciled position but never changes world input.
#[derive(Component)]
pub(crate) struct PixelWorldModuleVisualLabel {
    pub(crate) id: String,
}

/// A small neutral glyph that distinguishes known module visual kinds without
/// changing the base marker's semantic color or interaction footprint.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub(crate) enum ModuleIdentityChipPart {
    BeaconStem,
    RelayBar,
    ArtifactDot,
    SensorLead,
    SensorTrail,
}

#[derive(Component)]
pub(crate) struct PixelWorldModuleIdentityChipVisual {
    pub(crate) id: String,
    pub(crate) part: ModuleIdentityChipPart,
}

#[derive(SystemParam)]
pub(crate) struct ModuleIdentityChipQueries<'w, 's> {
    chips: Query<'w, 's, (Entity, &'static PixelWorldModuleIdentityChipVisual)>,
    labels: Query<'w, 's, (Entity, &'static PixelWorldModuleVisualLabel)>,
}

pub(crate) fn despawn_module_identity_chips(
    commands: &mut Commands,
    queries: &ModuleIdentityChipQueries,
) {
    for (entity, _) in queries.chips.iter() {
        commands.entity(entity).despawn();
    }
    for (entity, _) in queries.labels.iter() {
        commands.entity(entity).despawn();
    }
}

// The Rust hit regions are 16 renderer px wide, but the matching transparent
// Web targets are 44 CSS px wide. Keep the public co-anchor spacing in CSS
// units, then scale it into the live renderer backing space at reconciliation.
const CO_ANCHOR_RING_OFFSETS: [Vec2; 8] = [
    Vec2::new(-48.0, -48.0),
    Vec2::new(0.0, -48.0),
    Vec2::new(48.0, -48.0),
    Vec2::new(-48.0, 0.0),
    Vec2::new(48.0, 0.0),
    Vec2::new(-48.0, 48.0),
    Vec2::new(0.0, 48.0),
    Vec2::new(48.0, 48.0),
];

pub(crate) fn module_co_anchor_offset(
    index: usize,
    renderer_to_css_scale: Vec2,
    pixel: DVec2,
) -> DVec2 {
    let ring = (index / CO_ANCHOR_RING_OFFSETS.len() + 1) as f64;
    let css_offset = CO_ANCHOR_RING_OFFSETS[index % CO_ANCHOR_RING_OFFSETS.len()];
    DVec2::new(
        f64::from(css_offset.x) / 48.0
            * ((48.0_f64.max(16.0 * pixel.x + 4.0) / pixel.x).ceil() * pixel.x)
            * f64::from(renderer_to_css_scale.x),
        f64::from(css_offset.y) / 48.0
            * ((48.0_f64.max(16.0 * pixel.y + 4.0) / pixel.y).ceil() * pixel.y)
            * f64::from(renderer_to_css_scale.y),
    ) * ring
}

pub(crate) type PositionKey = (u64, u64, u64);

fn position_key(position: &Position) -> Option<PositionKey> {
    if !position.x_cm.is_finite() || !position.y_cm.is_finite() {
        return None;
    }
    let bits = |value: f64| if value == 0.0 { 0 } else { value.to_bits() };
    let z = if position.z_cm.is_finite() {
        position.z_cm
    } else {
        0.0
    };
    Some((bits(position.x_cm), bits(position.y_cm), bits(z)))
}

fn parent_position_keys(render_state: &RenderState) -> HashSet<PositionKey> {
    render_state
        .locations
        .iter()
        .filter_map(|location| position_key(&location.pos))
        .chain(
            render_state
                .agents
                .iter()
                .filter_map(|agent| agent.pos.as_ref().and_then(position_key)),
        )
        .collect()
}

pub(crate) fn module_co_anchor_slots(
    position_keys: &[Option<PositionKey>],
) -> Vec<Option<(usize, usize)>> {
    let mut groups = HashMap::<PositionKey, Vec<usize>>::new();
    for (index, key) in position_keys.iter().copied().enumerate() {
        if let Some(key) = key {
            groups.entry(key).or_default().push(index);
        }
    }
    let mut slots = vec![None; position_keys.len()];
    for group in groups.values() {
        for (index, entity_index) in group.iter().copied().enumerate() {
            slots[entity_index] = Some((index, group.len()));
        }
    }
    slots
}

pub(super) fn reconcile_module_visual_entities(
    commands: &mut Commands,
    runtime: &mut BevyRuntimeState,
    chip_queries: &ModuleIdentityChipQueries,
    width: f64,
    height: f64,
    renderer_to_css_scale: Vec2,
    rebuild_hit_regions: bool,
) {
    let existing_chips = chip_queries
        .chips
        .iter()
        .map(|(entity, chip)| ((chip.id.clone(), chip.part), entity))
        .collect::<HashMap<_, _>>();
    let existing_labels = chip_queries
        .labels
        .iter()
        .map(|(entity, label)| (label.id.clone(), entity))
        .collect::<HashMap<_, _>>();
    let Some(render_state) = runtime.render_state.as_ref() else {
        for (_, entity) in runtime.module_visual_entities.drain() {
            commands.entity(entity).despawn();
        }
        despawn_module_identity_chips(commands, chip_queries);
        return;
    };
    let Some(world_bounds) = render_state.world_bounds.as_ref() else {
        for (_, entity) in runtime.module_visual_entities.drain() {
            commands.entity(entity).despawn();
        }
        despawn_module_identity_chips(commands, chip_queries);
        return;
    };
    let map_obstacles = map_label_obstacles(render_state, width, height, &runtime.camera);

    let mut entities = render_state
        .module_visual_entities
        .iter()
        .collect::<Vec<_>>();
    entities.sort_by(|left, right| left.id.cmp(&right.id));
    let module_position_keys = entities
        .iter()
        .map(|entity| position_key(&entity.pos))
        .collect::<Vec<_>>();
    let module_co_anchor_slots = module_co_anchor_slots(&module_position_keys);
    let parent_positions = parent_position_keys(render_state);
    let mut active_ids = HashSet::new();
    let mut active_chips = HashSet::new();
    let mut active_labels = HashSet::new();
    let mut accepted_label_rects: Vec<ModuleLabelRect> = Vec::new();
    let pixel = agent_pixel_scale(width, height);
    let offset_pixel = pixel_pipeline::canvas_layout(width, height)
        .map(|layout| layout.css / layout.frame.as_dvec2() * layout.scale as f64)
        .unwrap_or(DVec2::ONE);
    for (index, entity) in entities.iter().enumerate() {
        let Some((canvas_x, canvas_y)) =
            to_canvas_point(&entity.pos, world_bounds, width, height, &runtime.camera)
        else {
            continue;
        };
        let co_anchor_slot = module_co_anchor_slots[index];
        let has_parent =
            module_position_keys[index].is_some_and(|key| parent_positions.contains(&key));
        let co_anchor_offset = if has_parent || co_anchor_slot.is_some_and(|(_, count)| count > 1) {
            module_co_anchor_offset(
                co_anchor_slot.map_or(index, |(co_anchor_index, _)| co_anchor_index),
                renderer_to_css_scale,
                offset_pixel,
            )
        } else {
            DVec2::ZERO
        };
        active_ids.insert(entity.id.clone());
        let mut transform = Transform::from_translation(to_bevy_translation(
            canvas_x + f64::from(co_anchor_offset.x),
            canvas_y + f64::from(co_anchor_offset.y),
            width,
            height,
            MODULE_VISUAL_ENTITY_LAYER_Z,
        ));
        transform.rotation = Quat::from_rotation_z(std::f32::consts::FRAC_PI_4);
        if rebuild_hit_regions {
            runtime.hit_regions.push(HitRegion {
                kind: "module_visual",
                id: entity.id.clone(),
                left: canvas_x + f64::from(co_anchor_offset.x)
                    - MODULE_VISUAL_HIT_HALF_SIZE * f64::from(pixel.x),
                top: canvas_y + f64::from(co_anchor_offset.y)
                    - MODULE_VISUAL_HIT_HALF_SIZE * f64::from(pixel.y),
                right: canvas_x
                    + f64::from(co_anchor_offset.x)
                    + MODULE_VISUAL_HIT_HALF_SIZE * f64::from(pixel.x),
                bottom: canvas_y
                    + f64::from(co_anchor_offset.y)
                    + MODULE_VISUAL_HIT_HALF_SIZE * f64::from(pixel.y),
            });
        }
        let sprite = sprite_for_rect(
            MODULE_VISUAL_ENTITY_COLOR,
            MODULE_VISUAL_ENTITY_SIZE_PX * pixel.x,
            MODULE_VISUAL_ENTITY_SIZE_PX * pixel.y,
        );
        if let Some(existing) = runtime.module_visual_entities.get(&entity.id).copied() {
            commands.entity(existing).insert((sprite, transform));
        } else {
            let spawned = commands
                .spawn((
                    sprite,
                    transform,
                    PixelWorldModuleVisualEntity {
                        id: entity.id.clone(),
                    },
                ))
                .id();
            runtime
                .module_visual_entities
                .insert(entity.id.clone(), spawned);
        }
        for (part, glyph_offset, size) in module_identity_chip_specs(&entity.kind) {
            let glyph_offset = glyph_offset * pixel;
            let size = size * pixel;
            let key = (entity.id.clone(), part);
            active_chips.insert(key.clone());
            let chip_transform = Transform::from_translation(to_bevy_translation(
                canvas_x + co_anchor_offset.x + f64::from(glyph_offset.x),
                canvas_y + co_anchor_offset.y + f64::from(glyph_offset.y),
                width,
                height,
                MODULE_VISUAL_ENTITY_LAYER_Z + MODULE_IDENTITY_CHIP_LAYER_Z_OFFSET,
            ));
            let chip_sprite = sprite_for_rect(MODULE_IDENTITY_CHIP_COLOR, size.x, size.y);
            if let Some(existing) = existing_chips.get(&key) {
                commands
                    .entity(*existing)
                    .insert((chip_sprite, chip_transform));
            } else {
                commands.spawn((
                    chip_sprite,
                    chip_transform,
                    PixelWorldModuleIdentityChipVisual {
                        id: entity.id.clone(),
                        part,
                    },
                ));
            }
        }
        if runtime.camera.zoom >= MODULE_LABEL_MIN_ZOOM {
            let display = module_visual_label(
                entity.label.as_deref(),
                &entity.kind,
                &entity.id,
                &render_state.locale,
            );
            let label_x = canvas_x + f64::from(co_anchor_offset.x);
            let extra_half_height = f64::from(MODULE_VISUAL_ENTITY_SIZE_PX)
                * std::f64::consts::FRAC_1_SQRT_2
                * (((f64::from(pixel.x) + f64::from(pixel.y)) / 2.0 - 1.0).max(0.0));
            let label_y = canvas_y + f64::from(co_anchor_offset.y)
                - MODULE_LABEL_ABOVE_MARKER_PX
                - extra_half_height;
            let label_rect = ModuleLabelRect::above_marker(label_x, label_y, &display);
            if accepted_label_rects
                .iter()
                .all(|accepted| !accepted.overlaps(label_rect))
                && map_obstacles.iter().all(|obstacle| {
                    !obstacle.overlaps_bounds(
                        label_rect.left,
                        label_rect.right,
                        label_rect.top,
                        label_rect.bottom,
                    )
                })
            {
                accepted_label_rects.push(label_rect);
                active_labels.insert(entity.id.clone());
                let label_visuals = (
                    Text2d::new(display.clone()),
                    TextFont {
                        font_size: FontSize::Px(MODULE_LABEL_FONT_SIZE_PX),
                        ..default()
                    },
                    TextColor(MODULE_LABEL_COLOR),
                    Transform::from_translation(to_bevy_translation(
                        label_x,
                        label_y,
                        width,
                        height,
                        MODULE_LABEL_LAYER_Z,
                    )),
                );
                let label = PixelWorldModuleVisualLabel {
                    id: entity.id.clone(),
                };
                if let Some(existing) = existing_labels.get(&entity.id) {
                    commands.entity(*existing).insert((label, label_visuals));
                } else {
                    commands.spawn((label, label_visuals));
                }
            }
        }
    }
    despawn_stale_entities(commands, &mut runtime.module_visual_entities, &active_ids);
    for (key, entity) in existing_chips {
        if !active_chips.contains(&key) {
            commands.entity(entity).despawn();
        }
    }
    for (id, entity) in existing_labels {
        if !active_labels.contains(&id) {
            commands.entity(entity).despawn();
        }
    }
}

#[derive(Clone, Copy)]
struct ModuleLabelRect {
    left: f64,
    right: f64,
    top: f64,
    bottom: f64,
}

impl ModuleLabelRect {
    fn above_marker(canvas_x: f64, canvas_y: f64, display: &str) -> Self {
        let half_width = module_label_width(display) / 2.0;
        Self {
            left: canvas_x - half_width,
            right: canvas_x + half_width,
            top: canvas_y - MODULE_LABEL_HEIGHT_PX,
            bottom: canvas_y,
        }
    }

    fn overlaps(self, other: Self) -> bool {
        self.left < other.right
            && self.right > other.left
            && self.top < other.bottom
            && self.bottom > other.top
    }
}

fn module_visual_label(label: Option<&str>, kind: &str, id: &str, locale: &str) -> String {
    let fallback = if locale.trim().to_ascii_lowercase().starts_with("zh") {
        format!("模块 {kind}:{id}")
    } else {
        format!("{kind}:{id}")
    };
    match label.map(str::trim).filter(|label| !label.is_empty()) {
        Some(explicit) if explicit != id => truncate_module_label(explicit),
        _ => truncate_module_label(&fallback),
    }
}

fn truncate_module_label(label: &str) -> String {
    let mut characters = label.chars();
    let visible = characters
        .by_ref()
        .take(MODULE_LABEL_MAX_CHARS)
        .collect::<String>();
    if characters.next().is_some() {
        format!("{visible}…")
    } else {
        visible
    }
}

fn module_label_width(display: &str) -> f64 {
    display.chars().count() as f64 * MODULE_LABEL_GLYPH_ADVANCE_PX
}

fn module_identity_chip_specs(kind: &str) -> Vec<(ModuleIdentityChipPart, Vec2, Vec2)> {
    match kind {
        "beacon" => vec![(
            ModuleIdentityChipPart::BeaconStem,
            Vec2::new(0.0, 2.5),
            Vec2::new(1.0, 2.0),
        )],
        "relay" => vec![(
            ModuleIdentityChipPart::RelayBar,
            Vec2::new(2.5, 0.0),
            Vec2::new(2.0, 1.0),
        )],
        "artifact" => vec![(
            ModuleIdentityChipPart::ArtifactDot,
            Vec2::new(0.0, 0.0),
            Vec2::ONE,
        )],
        "sensor" => vec![
            (
                ModuleIdentityChipPart::SensorLead,
                Vec2::new(-2.0, 1.5),
                Vec2::ONE,
            ),
            (
                ModuleIdentityChipPart::SensorTrail,
                Vec2::new(2.0, -1.5),
                Vec2::ONE,
            ),
        ],
        _ => Vec::new(),
    }
}
