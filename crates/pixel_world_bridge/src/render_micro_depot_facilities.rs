use std::collections::{HashMap, HashSet};

use super::*;
use bevy::ecs::system::SystemParam;

#[derive(Component)]
pub(crate) struct PixelWorldMicroDepotVisual {
    pub(crate) id: String,
}

#[derive(Component)]
pub(crate) struct PixelWorldMicroDepotDetailVisual {
    id: String,
}

/// One passive stock/throughput runway segment. It deliberately has no hit
/// region: the existing depot glyph remains the only facility interaction
/// surface and this cue is observation-only.
#[derive(Component)]
pub(crate) struct PixelWorldMicroDepotStockRunwayVisual {
    pub(crate) id: String,
    pub(crate) segment: u8,
}

/// Dark passive backing for one runway segment. It deliberately has no hit
/// region and is reconciled with the segment so stale treatments cannot linger.
#[derive(Component)]
pub(crate) struct PixelWorldMicroDepotStockRunwayBackingVisual {
    id: String,
    segment: u8,
}

/// Crossing slash for a zero-stock slot. It is a separate visual entity so
/// opposite slopes can overlap without changing the four primary segments.
#[derive(Component)]
pub(crate) struct PixelWorldMicroDepotZeroStockCrossVisual {
    id: String,
    segment: u8,
}

/// Compact passive throughput readout for a depot's stock runway. It is
/// deliberately separate from the interactive depot glyph so reconciliation
/// can update and remove the readout without changing hit regions.
#[derive(Component)]
pub(crate) struct PixelWorldMicroDepotThroughputTextVisual {
    pub(super) id: String,
}

/// Opaque passive backing for the compact throughput readout. Keeping this
/// as a separate reconciled entity preserves a readable text silhouette on
/// narrow canvases without adding an interaction surface.
#[derive(Component)]
pub(crate) struct PixelWorldMicroDepotThroughputBackingVisual {
    pub(super) id: String,
}

/// One segment of the passive world-scale service-range outline. It deliberately
/// has no hit region: the existing depot glyph remains the only facility visual
/// identity and this range is observation-only.
#[derive(Component)]
pub(crate) struct PixelWorldMicroDepotServiceRadiusVisual {
    pub(crate) id: String,
    pub(crate) segment: u8,
}

#[derive(SystemParam)]
pub(super) struct MicroDepotFacilityOverlayQueries<'w, 's> {
    details: Query<'w, 's, (Entity, &'static PixelWorldMicroDepotDetailVisual)>,
    stock_runway_segments: Query<'w, 's, (Entity, &'static PixelWorldMicroDepotStockRunwayVisual)>,
    stock_runway_backings: Query<
        'w,
        's,
        (
            Entity,
            &'static PixelWorldMicroDepotStockRunwayBackingVisual,
        ),
    >,
    zero_stock_crosses: Query<'w, 's, (Entity, &'static PixelWorldMicroDepotZeroStockCrossVisual)>,
    throughput_texts: Query<'w, 's, (Entity, &'static PixelWorldMicroDepotThroughputTextVisual)>,
    throughput_backings:
        Query<'w, 's, (Entity, &'static PixelWorldMicroDepotThroughputBackingVisual)>,
    service_radius_outlines:
        Query<'w, 's, (Entity, &'static PixelWorldMicroDepotServiceRadiusVisual)>,
}

impl MicroDepotFacilityOverlayQueries<'_, '_> {
    pub(super) fn despawn(&self, commands: &mut Commands) {
        for (entity, _) in self.details.iter() {
            commands.entity(entity).despawn();
        }
        for (entity, _) in self.stock_runway_segments.iter() {
            commands.entity(entity).despawn();
        }
        for (entity, _) in self.stock_runway_backings.iter() {
            commands.entity(entity).despawn();
        }
        for (entity, _) in self.zero_stock_crosses.iter() {
            commands.entity(entity).despawn();
        }
        for (entity, _) in self.throughput_texts.iter() {
            commands.entity(entity).despawn();
        }
        for (entity, _) in self.throughput_backings.iter() {
            commands.entity(entity).despawn();
        }
        for (entity, _) in self.service_radius_outlines.iter() {
            commands.entity(entity).despawn();
        }
    }
}

const SERVICE_RADIUS_LAYER_Z: f32 = MICRO_DEPOT_LAYER_Z - 0.03;
const SERVICE_RADIUS_ALPHA: f32 = 0.22;
const SERVICE_RADIUS_THICKNESS_PX: f32 = 1.25;
// Keep the cue just above the glyph/detail pair so deterministic raster probes
// include its signature while its offset position keeps it visually subordinate.
const STOCK_RUNWAY_LAYER_Z: f32 = MICRO_DEPOT_LAYER_Z + 0.015;
const STOCK_RUNWAY_BACKING_LAYER_Z: f32 = STOCK_RUNWAY_LAYER_Z - 0.001;
const STOCK_RUNWAY_ZERO_CROSS_LAYER_Z: f32 = STOCK_RUNWAY_LAYER_Z + 0.001;
const STOCK_RUNWAY_SEGMENT_COUNT: u8 = 4;
const STOCK_RUNWAY_SEGMENT_SPACING_PX: f64 = 3.5;
const STOCK_RUNWAY_OFFSET_Y_PX: f64 = 8.0;
const STOCK_RUNWAY_SEGMENT_WIDTH_PX: f32 = 4.2;
const STOCK_RUNWAY_SEGMENT_HEIGHT_PX: f32 = 2.8;
const STOCK_RUNWAY_BACKING_PADDING_PX: f32 = 1.4;
const STOCK_RUNWAY_ZERO_CROSS_WIDTH_PX: f32 = 4.4;
const STOCK_RUNWAY_ZERO_CROSS_HEIGHT_PX: f32 = 1.8;
// Keep the fallback in the same stock lane, just above the runway treatment,
// while placing it beside (rather than over) the depot glyph.
const STOCK_THROUGHPUT_TEXT_LAYER_Z: f32 = STOCK_RUNWAY_LAYER_Z + 0.002;
// Keep the readout backing below the primary runway segments so the
// healthy/low fill geometry remains visible through the neutral plate.
const STOCK_THROUGHPUT_BACKING_LAYER_Z: f32 = STOCK_RUNWAY_BACKING_LAYER_Z;
const STOCK_THROUGHPUT_TEXT_OFFSET_X_PX: f64 = 10.0;
const STOCK_THROUGHPUT_TEXT_FONT_SIZE_PX: f32 = 12.0;
const STOCK_THROUGHPUT_TEXT_COLOR: Color = Color::srgb_u8(203, 213, 225);
const STOCK_THROUGHPUT_BACKING_COLOR: Color = Color::srgba_u8(2, 4, 8, 255);
const STOCK_THROUGHPUT_BACKING_WIDTH_PX: f32 = 24.0;
const STOCK_THROUGHPUT_BACKING_HEIGHT_PX: f32 = 14.0;

/// Return the bounded, truthful runway ratio published by the runtime.
///
/// The snapshot publishes no inventory-capacity total, so the denominator is
/// the current throughput limit for this epoch, not the sum of inventory
/// kinds. A non-empty inventory map with no positive units is an authoritative
/// zero-stock guard. If the limit is absent/zero (including legacy payloads),
/// no runway is drawn rather than implying a fabricated percentage.
fn stock_runway_ratio(facility: &MicroDepotFacility) -> Option<f32> {
    if facility.throughput_limit_units_per_epoch <= 0 {
        return None;
    }
    let available_units_total = facility
        .available_units_by_kind
        .values()
        .fold(0_i64, |total, units| total.saturating_add((*units).max(0)));
    let inventory_is_known_empty =
        !facility.available_units_by_kind.is_empty() && available_units_total == 0;
    if inventory_is_known_empty {
        return Some(0.0);
    }
    let limit = facility.throughput_limit_units_per_epoch as f64;
    let remaining = (facility.throughput_remaining_units.max(0) as f64).min(limit);
    Some((remaining / limit) as f32)
}

/// Format only a known, positive throughput limit. The DTO remains truthful;
/// the display clamps malformed remaining values to the logical [0, limit]
/// interval so a negative or over-limit snapshot cannot imply an impossible
/// stock state. Legacy/unknown limits intentionally produce no text.
fn stock_throughput_text(facility: &MicroDepotFacility) -> Option<String> {
    let limit = facility.throughput_limit_units_per_epoch;
    (limit > 0).then(|| {
        format!(
            "{}/{}",
            facility.throughput_remaining_units.clamp(0, limit),
            limit
        )
    })
}

fn runway_segment_style(ratio: f32, segment: u8) -> (Color, f32, f32, f32) {
    if ratio <= 0.0 {
        // A diagonal slash is paired with the opposite slope below to form an
        // X in every zero-stock slot; the crossing shape is color-independent.
        let rotation = if segment.is_multiple_of(2) {
            std::f32::consts::FRAC_PI_4
        } else {
            -std::f32::consts::FRAC_PI_4
        };
        return (
            Color::srgb_u8(248, 113, 113),
            STOCK_RUNWAY_ZERO_CROSS_WIDTH_PX,
            STOCK_RUNWAY_ZERO_CROSS_HEIGHT_PX,
            rotation,
        );
    }
    let filled_count = (ratio * f32::from(STOCK_RUNWAY_SEGMENT_COUNT)).round() as u8;
    if segment < filled_count {
        let color = if ratio < 0.5 {
            Color::srgb_u8(251, 191, 36)
        } else {
            Color::srgb_u8(52, 211, 153)
        };
        (
            color,
            STOCK_RUNWAY_SEGMENT_WIDTH_PX,
            STOCK_RUNWAY_SEGMENT_HEIGHT_PX,
            0.0,
        )
    } else {
        (
            Color::srgb_u8(51, 65, 85),
            STOCK_RUNWAY_SEGMENT_WIDTH_PX,
            STOCK_RUNWAY_SEGMENT_HEIGHT_PX,
            0.0,
        )
    }
}

fn service_radius_outline_points(
    canvas_x: f64,
    canvas_y: f64,
    radius_x: f64,
    radius_y: f64,
) -> [(f64, f64); 8] {
    std::array::from_fn(|index| {
        let angle = (index as f64) * std::f64::consts::FRAC_PI_4;
        (
            canvas_x + (angle.cos() * radius_x),
            canvas_y + (angle.sin() * radius_y),
        )
    })
}

fn facility_glyph(status: &str) -> Option<(Color, f32, f32, Vec2)> {
    match status {
        "active" => Some((Color::srgb_u8(52, 211, 153), 8.0, 0.0, Vec2::new(4.0, 1.5))),
        "suspended" => Some((
            Color::srgb_u8(251, 191, 36),
            8.0,
            std::f32::consts::FRAC_PI_4,
            Vec2::splat(3.0),
        )),
        "depleted" => Some((
            Color::srgb_u8(248, 113, 113),
            9.0,
            std::f32::consts::FRAC_PI_4,
            Vec2::new(9.0, 1.5),
        )),
        _ => None,
    }
}

pub(super) fn reconcile_micro_depot_facilities(
    commands: &mut Commands,
    runtime: &mut BevyRuntimeState,
    overlays: &MicroDepotFacilityOverlayQueries,
    width: f64,
    height: f64,
) {
    let Some(render_state) = runtime.render_state.as_ref() else {
        for (_, entity) in runtime.micro_depot_entities.drain() {
            commands.entity(entity).despawn();
        }
        overlays.despawn(commands);
        return;
    };
    let Some(world_bounds) = render_state.world_bounds.as_ref() else {
        for (_, entity) in runtime.micro_depot_entities.drain() {
            commands.entity(entity).despawn();
        }
        overlays.despawn(commands);
        return;
    };
    let details_by_id = overlays
        .details
        .iter()
        .map(|(entity, visual)| (visual.id.clone(), entity))
        .collect::<HashMap<_, _>>();
    let runway_segments_by_key = overlays
        .stock_runway_segments
        .iter()
        .map(|(entity, visual)| ((visual.id.clone(), visual.segment), entity))
        .collect::<HashMap<_, _>>();
    let runway_backings_by_key = overlays
        .stock_runway_backings
        .iter()
        .map(|(entity, visual)| ((visual.id.clone(), visual.segment), entity))
        .collect::<HashMap<_, _>>();
    let zero_stock_crosses_by_key = overlays
        .zero_stock_crosses
        .iter()
        .map(|(entity, visual)| ((visual.id.clone(), visual.segment), entity))
        .collect::<HashMap<_, _>>();
    let throughput_texts_by_id = overlays
        .throughput_texts
        .iter()
        .map(|(entity, visual)| (visual.id.clone(), entity))
        .collect::<HashMap<_, _>>();
    let throughput_backings_by_id = overlays
        .throughput_backings
        .iter()
        .map(|(entity, visual)| (visual.id.clone(), entity))
        .collect::<HashMap<_, _>>();
    let service_radius_segments_by_key = overlays
        .service_radius_outlines
        .iter()
        .map(|(entity, visual)| ((visual.id.clone(), visual.segment), entity))
        .collect::<HashMap<_, _>>();
    let mut active_ids = HashSet::new();
    let mut active_detail_ids = HashSet::new();
    let mut active_runway_segment_keys = HashSet::new();
    let mut active_runway_backing_keys = HashSet::new();
    let mut active_zero_stock_cross_keys = HashSet::new();
    let mut active_throughput_text_ids = HashSet::new();
    let mut active_throughput_backing_ids = HashSet::new();
    let mut active_service_radius_keys = HashSet::new();
    for facility in &render_state.micro_depot_facilities {
        let Some((color, size, rotation, detail_size)) = facility_glyph(&facility.status) else {
            continue;
        };
        let Some((canvas_x, canvas_y)) =
            to_canvas_point(&facility.pos, world_bounds, width, height, &runtime.camera)
        else {
            continue;
        };
        active_ids.insert(facility.id.clone());
        active_detail_ids.insert(facility.id.clone());
        let mut transform = Transform::from_translation(to_bevy_translation(
            canvas_x,
            canvas_y,
            width,
            height,
            MICRO_DEPOT_LAYER_Z,
        ));
        transform.rotation = Quat::from_rotation_z(rotation);
        let sprite = sprite_for_square(color, size);
        if let Some(entity) = runtime.micro_depot_entities.get(&facility.id).copied() {
            commands.entity(entity).insert((sprite, transform));
        } else {
            let entity = commands
                .spawn((
                    sprite,
                    transform,
                    PixelWorldMicroDepotVisual {
                        id: facility.id.clone(),
                    },
                ))
                .id();
            runtime
                .micro_depot_entities
                .insert(facility.id.clone(), entity);
        }

        let mut detail_transform = transform;
        detail_transform.translation.z += 0.01;
        if facility.status == "depleted" {
            detail_transform.rotation = Quat::from_rotation_z(-std::f32::consts::FRAC_PI_4);
        } else {
            detail_transform.rotation = Quat::IDENTITY;
        }
        let detail = Sprite::from_color(Color::srgb_u8(15, 23, 42), detail_size);
        if let Some(entity) = details_by_id.get(&facility.id) {
            commands.entity(*entity).insert((detail, detail_transform));
        } else {
            commands.spawn((
                detail,
                detail_transform,
                PixelWorldMicroDepotDetailVisual {
                    id: facility.id.clone(),
                },
            ));
        }

        if let Some(ratio) = stock_runway_ratio(facility) {
            for segment in 0..STOCK_RUNWAY_SEGMENT_COUNT {
                let (segment_color, segment_width, segment_height, rotation) =
                    runway_segment_style(ratio, segment);
                let segment_x = canvas_x
                    + (f64::from(segment) - (f64::from(STOCK_RUNWAY_SEGMENT_COUNT - 1) / 2.0))
                        * STOCK_RUNWAY_SEGMENT_SPACING_PX;
                let segment_y = canvas_y - STOCK_RUNWAY_OFFSET_Y_PX;
                let segment_transform = {
                    let mut transform = Transform::from_translation(to_bevy_translation(
                        segment_x,
                        segment_y,
                        width,
                        height,
                        STOCK_RUNWAY_LAYER_Z,
                    ));
                    transform.rotation = Quat::from_rotation_z(rotation);
                    transform
                };
                let segment_sprite = sprite_for_rect(segment_color, segment_width, segment_height);
                let key = (facility.id.clone(), segment);
                active_runway_segment_keys.insert(key.clone());
                if let Some(entity) = runway_segments_by_key.get(&key) {
                    commands
                        .entity(*entity)
                        .insert((segment_sprite, segment_transform));
                } else {
                    commands.spawn((
                        segment_sprite,
                        segment_transform,
                        PixelWorldMicroDepotStockRunwayVisual {
                            id: facility.id.clone(),
                            segment,
                        },
                    ));
                }

                // Keep each primary segment on one stable treatment layer,
                // then place a slightly larger dark proxy immediately below
                // it so narrow screens retain a readable AABB and contrast.
                let backing_transform = {
                    let mut transform = Transform::from_translation(to_bevy_translation(
                        segment_x,
                        segment_y,
                        width,
                        height,
                        STOCK_RUNWAY_BACKING_LAYER_Z,
                    ));
                    transform.rotation = Quat::from_rotation_z(rotation);
                    transform
                };
                let backing_sprite = sprite_for_rect(
                    Color::srgb_u8(2, 4, 8),
                    segment_width + STOCK_RUNWAY_BACKING_PADDING_PX,
                    segment_height + STOCK_RUNWAY_BACKING_PADDING_PX,
                );
                active_runway_backing_keys.insert(key.clone());
                if let Some(entity) = runway_backings_by_key.get(&key) {
                    commands
                        .entity(*entity)
                        .insert((backing_sprite, backing_transform));
                } else {
                    commands.spawn((
                        backing_sprite,
                        backing_transform,
                        PixelWorldMicroDepotStockRunwayBackingVisual {
                            id: facility.id.clone(),
                            segment,
                        },
                    ));
                }

                if ratio <= 0.0 {
                    // Draw the opposite slope in the same slot, creating an
                    // explicit X rather than relying on the red zero color.
                    let mut cross_transform = Transform::from_translation(to_bevy_translation(
                        segment_x,
                        segment_y,
                        width,
                        height,
                        STOCK_RUNWAY_ZERO_CROSS_LAYER_Z,
                    ));
                    cross_transform.rotation = Quat::from_rotation_z(-rotation);
                    let cross_sprite = sprite_for_rect(
                        segment_color,
                        STOCK_RUNWAY_ZERO_CROSS_WIDTH_PX,
                        STOCK_RUNWAY_ZERO_CROSS_HEIGHT_PX,
                    );
                    active_zero_stock_cross_keys.insert(key.clone());
                    if let Some(entity) = zero_stock_crosses_by_key.get(&key) {
                        commands
                            .entity(*entity)
                            .insert((cross_sprite, cross_transform));
                    } else {
                        commands.spawn((
                            cross_sprite,
                            cross_transform,
                            PixelWorldMicroDepotZeroStockCrossVisual {
                                id: facility.id.clone(),
                                segment,
                            },
                        ));
                    }
                }
            }
        }

        if let Some(display) = stock_throughput_text(facility) {
            let text_x = canvas_x + STOCK_THROUGHPUT_TEXT_OFFSET_X_PX;
            let text_y = canvas_y - STOCK_RUNWAY_OFFSET_Y_PX;
            let text_transform = Transform::from_translation(to_bevy_translation(
                text_x,
                text_y,
                width,
                height,
                STOCK_THROUGHPUT_TEXT_LAYER_Z,
            ));
            let backing_transform = Transform::from_translation(to_bevy_translation(
                text_x,
                text_y,
                width,
                height,
                STOCK_THROUGHPUT_BACKING_LAYER_Z,
            ));
            let backing_sprite = sprite_for_rect(
                STOCK_THROUGHPUT_BACKING_COLOR,
                STOCK_THROUGHPUT_BACKING_WIDTH_PX,
                STOCK_THROUGHPUT_BACKING_HEIGHT_PX,
            );
            let backing = PixelWorldMicroDepotThroughputBackingVisual {
                id: facility.id.clone(),
            };
            active_throughput_backing_ids.insert(facility.id.clone());
            if let Some(entity) = throughput_backings_by_id.get(&facility.id) {
                commands
                    .entity(*entity)
                    .insert((backing, backing_sprite, backing_transform));
            } else {
                commands.spawn((backing, backing_sprite, backing_transform));
            }
            let visuals = (
                Text2d::new(display),
                TextFont {
                    font_size: FontSize::Px(STOCK_THROUGHPUT_TEXT_FONT_SIZE_PX),
                    ..default()
                },
                TextColor(STOCK_THROUGHPUT_TEXT_COLOR),
                text_transform,
            );
            active_throughput_text_ids.insert(facility.id.clone());
            let label = PixelWorldMicroDepotThroughputTextVisual {
                id: facility.id.clone(),
            };
            if let Some(entity) = throughput_texts_by_id.get(&facility.id) {
                commands.entity(*entity).insert((label, visuals));
            } else {
                commands.spawn((label, visuals));
            }
        }

        if facility.service_radius_cm > 0.0 {
            let world_width_px = (width - 40.0).max(1.0) * runtime.camera.zoom.max(0.5);
            let world_depth_px = (height - 40.0).max(1.0) * runtime.camera.zoom.max(0.5);
            let radius_x =
                facility.service_radius_cm / world_bounds.width_cm.max(1.0) * world_width_px;
            let radius_y =
                facility.service_radius_cm / world_bounds.depth_cm.max(1.0) * world_depth_px;
            let points = service_radius_outline_points(canvas_x, canvas_y, radius_x, radius_y);
            let outline_color = Color::srgba(0.35, 0.88, 0.76, SERVICE_RADIUS_ALPHA);
            for index in 0..points.len() {
                let segment = index as u8;
                let key = (facility.id.clone(), segment);
                active_service_radius_keys.insert(key.clone());
                let (from_x, from_y) = points[index];
                let (to_x, to_y) = points[(index + 1) % points.len()];
                let length = ((to_x - from_x).powi(2) + (to_y - from_y).powi(2))
                    .sqrt()
                    .max(SERVICE_RADIUS_THICKNESS_PX as f64);
                let sprite =
                    sprite_for_rect(outline_color, length as f32, SERVICE_RADIUS_THICKNESS_PX);
                let transform = transform_for_line(
                    from_x,
                    from_y,
                    to_x,
                    to_y,
                    width,
                    height,
                    SERVICE_RADIUS_LAYER_Z,
                );
                if let Some(entity) = service_radius_segments_by_key.get(&key) {
                    commands.entity(*entity).insert((sprite, transform));
                } else {
                    commands.spawn((
                        sprite,
                        transform,
                        PixelWorldMicroDepotServiceRadiusVisual {
                            id: facility.id.clone(),
                            segment,
                        },
                    ));
                }
            }
        }
    }
    despawn_stale_entities(commands, &mut runtime.micro_depot_entities, &active_ids);
    for (id, entity) in details_by_id {
        if !active_detail_ids.contains(&id) {
            commands.entity(entity).despawn();
        }
    }
    for (key, entity) in runway_segments_by_key {
        if !active_runway_segment_keys.contains(&key) {
            commands.entity(entity).despawn();
        }
    }
    for (key, entity) in runway_backings_by_key {
        if !active_runway_backing_keys.contains(&key) {
            commands.entity(entity).despawn();
        }
    }
    for (key, entity) in zero_stock_crosses_by_key {
        if !active_zero_stock_cross_keys.contains(&key) {
            commands.entity(entity).despawn();
        }
    }
    for (id, entity) in throughput_texts_by_id {
        if !active_throughput_text_ids.contains(&id) {
            commands.entity(entity).despawn();
        }
    }
    for (id, entity) in throughput_backings_by_id {
        if !active_throughput_backing_ids.contains(&id) {
            commands.entity(entity).despawn();
        }
    }
    for (key, entity) in service_radius_segments_by_key {
        if !active_service_radius_keys.contains(&key) {
            commands.entity(entity).despawn();
        }
    }
}
