use std::collections::{HashMap, HashSet};

use super::*;

const SOCIAL_LINK_LAYER_Z: f32 = 0.42;
const SOCIAL_LINK_THICKNESS_PX: f32 = 1.5;
const SOCIAL_LINK_COLOR: Color = Color::srgba_u8(196, 181, 253, 118);
const SOCIAL_ENDPOINT_COLOR: Color = Color::srgba_u8(221, 214, 254, 180);
const SOCIAL_MIN_LINK_LENGTH_PX: f64 = 8.0;

#[derive(Component)]
pub(super) struct PixelWorldSocialLinkVisual {
    pub(super) link_id: String,
    pub(super) part: SocialLinkPart,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(super) enum SocialLinkPart {
    Line,
    FromGlyph,
    ToGlyph,
    RelationMarkerUpper,
    RelationMarkerLower,
}

pub(super) fn despawn_social_links(
    commands: &mut Commands,
    existing: &Query<(Entity, &PixelWorldSocialLinkVisual)>,
) {
    for (entity, _) in existing.iter() {
        commands.entity(entity).despawn();
    }
}

pub(super) fn reconcile_social_links(
    commands: &mut Commands,
    runtime: &BevyRuntimeState,
    existing: &Query<(Entity, &PixelWorldSocialLinkVisual)>,
    width: f64,
    height: f64,
) {
    let mut by_key = HashMap::new();
    for (entity, visual) in existing.iter() {
        let key = (visual.link_id.clone(), visual.part);
        if let Some(duplicate) = by_key.insert(key, entity) {
            commands.entity(duplicate).despawn();
        }
    }
    let Some(render_state) = runtime.render_state.as_ref() else {
        despawn_existing(commands, by_key);
        return;
    };
    let Some(bounds) = render_state.world_bounds.as_ref() else {
        despawn_existing(commands, by_key);
        return;
    };
    let mut active = HashSet::new();
    for link in &render_state.social_links {
        let Some((from_x, from_y)) =
            to_canvas_point(&link.from, bounds, width, height, &runtime.camera)
        else {
            continue;
        };
        let Some((to_x, to_y)) = to_canvas_point(&link.to, bounds, width, height, &runtime.camera)
        else {
            continue;
        };
        let dx = to_x - from_x;
        let dy = to_y - from_y;
        let length = (dx * dx + dy * dy).sqrt();
        if !length.is_finite() || length < SOCIAL_MIN_LINK_LENGTH_PX {
            continue;
        }
        upsert_line(
            commands,
            &mut by_key,
            &mut active,
            &link.id,
            SocialLinkPart::Line,
            from_x,
            from_y,
            to_x,
            to_y,
            SOCIAL_LINK_COLOR,
            width,
            height,
        );
        upsert_glyph(
            commands,
            &mut by_key,
            &mut active,
            SocialLinkGlyph {
                link_id: &link.id,
                part: SocialLinkPart::FromGlyph,
                x: from_x,
                y: from_y,
                color: SOCIAL_ENDPOINT_COLOR,
                width,
                height,
            },
        );
        if is_directional_relation(&link.relation_kind) {
            let marker_segments = directional_marker_segments(
                from_x,
                from_y,
                to_x,
                to_y,
                agent_pixel_scale(width, height),
            );
            upsert_line_at_layer(
                commands,
                &mut by_key,
                &mut active,
                &link.id,
                SocialLinkPart::RelationMarkerUpper,
                marker_segments.upper_from.0,
                marker_segments.upper_from.1,
                marker_segments.upper_to.0,
                marker_segments.upper_to.1,
                SOCIAL_ENDPOINT_COLOR,
                width,
                height,
                SOCIAL_LINK_LAYER_Z + 0.02,
            );
            upsert_line_at_layer(
                commands,
                &mut by_key,
                &mut active,
                &link.id,
                SocialLinkPart::RelationMarkerLower,
                marker_segments.lower_from.0,
                marker_segments.lower_from.1,
                marker_segments.lower_to.0,
                marker_segments.lower_to.1,
                SOCIAL_ENDPOINT_COLOR,
                width,
                height,
                SOCIAL_LINK_LAYER_Z + 0.02,
            );
        }
        upsert_glyph(
            commands,
            &mut by_key,
            &mut active,
            SocialLinkGlyph {
                link_id: &link.id,
                part: SocialLinkPart::ToGlyph,
                x: to_x,
                y: to_y,
                color: SOCIAL_ENDPOINT_COLOR,
                width,
                height,
            },
        );
    }
    for (key, entity) in by_key {
        if !active.contains(&key) {
            commands.entity(entity).despawn();
        }
    }
}

fn is_directional_relation(relation_kind: &str) -> bool {
    matches!(relation_kind, "supports" | "supply")
}

type ScreenPoint = (f64, f64);

struct DirectionalMarkerSegments {
    upper_from: ScreenPoint,
    upper_to: ScreenPoint,
    lower_from: ScreenPoint,
    lower_to: ScreenPoint,
}

struct SocialLinkGlyph<'a> {
    link_id: &'a str,
    part: SocialLinkPart,
    x: f64,
    y: f64,
    color: Color,
    width: f64,
    height: f64,
}

/// A pair of short chevrons at the destination makes the existing `from -> to`
/// relation legible for the directional social-edge kinds already emitted by
/// the runtime. Unknown relation kinds retain the neutral endpoint treatment.
fn directional_marker_segments(
    from_x: f64,
    from_y: f64,
    to_x: f64,
    to_y: f64,
    pixel: Vec2,
) -> DirectionalMarkerSegments {
    let dx = to_x - from_x;
    let dy = to_y - from_y;
    let length = (dx * dx + dy * dy).sqrt().max(1.0);
    let ux = dx / length;
    let uy = dy / length;
    let nx = -uy;
    let ny = ux;
    let tip = (to_x, to_y);
    let base = (
        to_x - ux * 7.0 * f64::from(pixel.x),
        to_y - uy * 7.0 * f64::from(pixel.y),
    );
    let spread = 3.5;
    DirectionalMarkerSegments {
        upper_from: tip,
        upper_to: (
            base.0 + nx * spread * f64::from(pixel.x),
            base.1 + ny * spread * f64::from(pixel.y),
        ),
        lower_from: tip,
        lower_to: (
            base.0 - nx * spread * f64::from(pixel.x),
            base.1 - ny * spread * f64::from(pixel.y),
        ),
    }
}

fn despawn_existing(commands: &mut Commands, entities: HashMap<(String, SocialLinkPart), Entity>) {
    for entity in entities.into_values() {
        commands.entity(entity).despawn();
    }
}

#[allow(clippy::too_many_arguments)]
fn upsert_line(
    commands: &mut Commands,
    by_key: &mut HashMap<(String, SocialLinkPart), Entity>,
    active: &mut HashSet<(String, SocialLinkPart)>,
    link_id: &str,
    part: SocialLinkPart,
    from_x: f64,
    from_y: f64,
    to_x: f64,
    to_y: f64,
    color: Color,
    width: f64,
    height: f64,
) {
    upsert_line_at_layer(
        commands,
        by_key,
        active,
        link_id,
        part,
        from_x,
        from_y,
        to_x,
        to_y,
        color,
        width,
        height,
        SOCIAL_LINK_LAYER_Z,
    );
}

#[allow(clippy::too_many_arguments)]
fn upsert_line_at_layer(
    commands: &mut Commands,
    by_key: &mut HashMap<(String, SocialLinkPart), Entity>,
    active: &mut HashSet<(String, SocialLinkPart)>,
    link_id: &str,
    part: SocialLinkPart,
    from_x: f64,
    from_y: f64,
    to_x: f64,
    to_y: f64,
    color: Color,
    width: f64,
    height: f64,
    layer_z: f32,
) {
    let key = (link_id.to_string(), part);
    active.insert(key.clone());
    let length = ((to_x - from_x).powi(2) + (to_y - from_y).powi(2)).sqrt();
    let sprite = sprite_for_rect(
        color,
        length as f32,
        world_strokes::thickness(
            SOCIAL_LINK_THICKNESS_PX,
            width,
            height,
            to_x - from_x,
            to_y - from_y,
        ),
    );
    let transform = transform_for_line(from_x, from_y, to_x, to_y, width, height, layer_z);
    if let Some(entity) = by_key.remove(&key) {
        commands.entity(entity).insert((sprite, transform));
        by_key.insert(key, entity);
    } else {
        let entity = commands
            .spawn((
                sprite,
                transform,
                PixelWorldSocialLinkVisual {
                    link_id: link_id.to_string(),
                    part,
                },
            ))
            .id();
        by_key.insert(key, entity);
    }
}

fn upsert_glyph(
    commands: &mut Commands,
    by_key: &mut HashMap<(String, SocialLinkPart), Entity>,
    active: &mut HashSet<(String, SocialLinkPart)>,
    glyph: SocialLinkGlyph<'_>,
) {
    let key = (glyph.link_id.to_string(), glyph.part);
    active.insert(key.clone());
    let mut transform = Transform::from_translation(to_bevy_translation(
        glyph.x,
        glyph.y,
        glyph.width,
        glyph.height,
        SOCIAL_LINK_LAYER_Z + 0.01,
    ));
    transform.rotation = Quat::from_rotation_z(std::f32::consts::FRAC_PI_4);
    let pixel = agent_pixel_scale(glyph.width, glyph.height);
    let sprite = sprite_for_rect(glyph.color, 5.0 * pixel.x, 5.0 * pixel.y);
    if let Some(entity) = by_key.remove(&key) {
        commands.entity(entity).insert((sprite, transform));
        by_key.insert(key, entity);
    } else {
        let entity = commands
            .spawn((
                sprite,
                transform,
                PixelWorldSocialLinkVisual {
                    link_id: glyph.link_id.to_string(),
                    part: glyph.part,
                },
            ))
            .id();
        by_key.insert(key, entity);
    }
}
