use super::*;

pub(super) const LINK_DIRECTION_MIN_LENGTH_PX: f64 = 20.0;
const LINK_DIRECTION_LAYER_Z: f32 = 0.62;
const LINK_DIRECTION_COLOR: Color = Color::srgba_u8(186, 230, 253, 214);
const LINK_DIRECTION_THICKNESS_PX: f32 = 1.6;
const LINK_DIRECTION_ARM_LENGTH_PX: f64 = 7.0;
const LINK_DIRECTION_BACKOFF_PX: f64 = 9.0;
const LINK_DIRECTION_HALF_WIDTH_PX: f64 = 3.2;

#[derive(Component)]
pub(crate) struct PixelWorldLinkVisual {
    pub(crate) id: String,
}

pub(super) fn reconcile_links(
    commands: &mut Commands,
    runtime: &mut BevyRuntimeState,
    width: f64,
    height: f64,
) {
    let Some(render_state) = runtime.render_state.as_ref() else {
        for (_, entity) in runtime.link_entities.drain() {
            commands.entity(entity).despawn();
        }
        return;
    };
    let Some(world_bounds) = render_state.world_bounds.as_ref() else {
        for (_, entity) in runtime.link_entities.drain() {
            commands.entity(entity).despawn();
        }
        return;
    };

    let mut active_ids = HashSet::new();
    for link in &render_state.links {
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
        active_ids.insert(link.id.clone());
        let length = ((to_x - from_x).powi(2) + (to_y - from_y).powi(2))
            .sqrt()
            .max(4.0);
        let emphasis = clamp(link.emphasis.unwrap_or(0.7), 0.25, 1.0);
        let sprite = sprite_for_rect(
            Color::srgba(0.49, 0.83, 0.98, (0.18 + (emphasis * 0.34)) as f32),
            length as f32,
            world_strokes::thickness(
                (1.4 + (emphasis * 2.2)) as f32,
                width,
                height,
                to_x - from_x,
                to_y - from_y,
            ),
        );
        let transform = transform_for_line(from_x, from_y, to_x, to_y, width, height, 0.5);

        let pixel = world_strokes::pixel_unit(width, height);
        let should_render_direction = is_authoritative_directional_link(link);
        if should_render_direction {
            let delta_x = to_x - from_x;
            let delta_y = to_y - from_y;
            let link_length = (delta_x.powi(2) + delta_y.powi(2)).sqrt();
            if link_length.is_finite() && link_length >= LINK_DIRECTION_MIN_LENGTH_PX {
                let cue_pixel = pixel.min(bevy::math::DVec2::splat(
                    link_length / LINK_DIRECTION_MIN_LENGTH_PX,
                ));
                let direction_x = delta_x / link_length;
                let direction_y = delta_y / link_length;
                let tip_x = to_x - (direction_x * LINK_DIRECTION_BACKOFF_PX * cue_pixel.x);
                let tip_y = to_y - (direction_y * LINK_DIRECTION_BACKOFF_PX * cue_pixel.y);
                let perpendicular_x = -direction_y;
                let perpendicular_y = direction_x;
                for (part, side) in [("upper", -1.0), ("lower", 1.0)] {
                    let tail_x = tip_x - (direction_x * LINK_DIRECTION_ARM_LENGTH_PX * cue_pixel.x)
                        + (perpendicular_x * LINK_DIRECTION_HALF_WIDTH_PX * side * cue_pixel.x);
                    let tail_y = tip_y - (direction_y * LINK_DIRECTION_ARM_LENGTH_PX * cue_pixel.y)
                        + (perpendicular_y * LINK_DIRECTION_HALF_WIDTH_PX * side * cue_pixel.y);
                    let direction_id = format!("{}:direction:{}", link.id, part);
                    active_ids.insert(direction_id.clone());
                    let direction_sprite = sprite_for_rect(
                        LINK_DIRECTION_COLOR,
                        ((tip_x - tail_x).powi(2) + (tip_y - tail_y).powi(2)).sqrt() as f32,
                        world_strokes::thickness(
                            LINK_DIRECTION_THICKNESS_PX,
                            width,
                            height,
                            tip_x - tail_x,
                            tip_y - tail_y,
                        ),
                    );
                    let direction_transform = transform_for_line(
                        tail_x,
                        tail_y,
                        tip_x,
                        tip_y,
                        width,
                        height,
                        LINK_DIRECTION_LAYER_Z,
                    );
                    if let Some(entity) = runtime.link_entities.get(&direction_id).copied() {
                        commands
                            .entity(entity)
                            .insert((direction_sprite, direction_transform));
                    } else {
                        let entity = commands
                            .spawn((
                                direction_sprite,
                                direction_transform,
                                PixelWorldLinkVisual {
                                    id: direction_id.clone(),
                                },
                            ))
                            .id();
                        runtime.link_entities.insert(direction_id, entity);
                    }
                }
            }
        }

        if let Some(entity) = runtime.link_entities.get(&link.id).copied() {
            commands.entity(entity).insert((sprite, transform));
        } else {
            let entity = commands
                .spawn((
                    sprite,
                    transform,
                    PixelWorldLinkVisual {
                        id: link.id.clone(),
                    },
                ))
                .id();
            runtime.link_entities.insert(link.id.clone(), entity);
        }
    }

    despawn_stale_entities(commands, &mut runtime.link_entities, &active_ids);
}

pub(super) fn is_authoritative_directional_link(link: &Link) -> bool {
    // `kind` supplies the published semantic from->to meaning; the three
    // envelope fields prove that meaning is current runtime projection data.
    // Link ids are only cache keys and never establish direction authority.
    matches!(
        link.kind.as_str(),
        "route"
            | "logistics"
            | "logistics_route"
            | "supply_route"
            | "delivery_route"
            | "resource_flow"
            | "resource_transfer"
            | "material_transfer"
            | "material_transit"
    ) && link.kind != "agent_assignment"
        && link.status.as_deref() == Some("active")
        && link.source_class.as_deref() == Some("runtime_projection")
        && link.freshness.as_deref() == Some("current")
}
