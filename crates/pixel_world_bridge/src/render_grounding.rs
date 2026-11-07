use super::*;

/// Passive pixel geometry; never contributes a hit region or world semantics.
#[derive(Component)]
pub(super) struct PixelWorldGroundingVisual {
    key: String,
}

pub(super) fn clear(commands: &mut Commands, query: &Query<(Entity, &PixelWorldGroundingVisual)>) {
    for (entity, _) in query.iter() {
        commands.entity(entity).despawn();
    }
}

pub(super) fn reconcile(
    commands: &mut Commands,
    runtime: &BevyRuntimeState,
    query: &Query<(Entity, &PixelWorldGroundingVisual)>,
    width: f64,
    height: f64,
    animation_ms: f64,
) {
    let mut existing = query
        .iter()
        .map(|(entity, visual)| (visual.key.clone(), entity))
        .collect::<HashMap<_, _>>();
    let mut specs = Vec::new();
    if let Some(state) = runtime.render_state.as_ref() {
        for (index, agent) in state.agents.iter().enumerate() {
            let selected = state
                .selection
                .as_ref()
                .is_some_and(|s| s.kind == "agent" && s.id == agent.id);
            let style = agent_visual_style(agent, selected, animation_ms, index);
            let point = state
                .world_bounds
                .as_ref()
                .and_then(|bounds| {
                    agent.pos.as_ref().and_then(|pos| {
                        to_canvas_point(pos, bounds, width, height, &runtime.camera)
                    })
                })
                .unwrap_or_else(|| {
                    fallback_point_for_entity(&agent.id, width, height, &runtime.camera)
                });
            let pixel = agent_pixel_scale(width, height);
            let point = (point.0, point.1 - 7.0 * pixel.y as f64);
            specs.push((
                format!("agent:{}", agent.id),
                point,
                style.size_px as f32,
                style.layer_z,
                true,
                pixel,
            ));
        }
        if let Some(bounds) = state.world_bounds.as_ref() {
            for location in &state.locations {
                if let Some(point) =
                    to_canvas_point(&location.pos, bounds, width, height, &runtime.camera)
                {
                    let selected = state
                        .selection
                        .as_ref()
                        .is_some_and(|s| s.kind == "location" && s.id == location.id);
                    let style = selected_location_visual_style(location, selected, animation_ms);
                    specs.push((
                        format!("location:{}", location.id),
                        point,
                        style.size_px as f32,
                        style.layer_z,
                        false,
                        Vec2::ONE,
                    ));
                }
            }
            for facility in &state.micro_depot_facilities {
                if !matches!(
                    facility.status.as_str(),
                    "active" | "suspended" | "depleted"
                ) {
                    continue;
                }
                if let Some(point) =
                    to_canvas_point(&facility.pos, bounds, width, height, &runtime.camera)
                {
                    specs.push((
                        format!("facility:{}", facility.id),
                        point,
                        9.0,
                        MICRO_DEPOT_LAYER_Z,
                        false,
                        Vec2::ONE,
                    ));
                }
            }
        }
    }
    for (id, (x, y), size, z, upright, pixel) in specs {
        let half_height = size * if upright { 0.5 } else { 0.36 };
        // Shared light direction: upper-left rim, lower-right contact shadow.
        for (part, dx, dy, w, h, color, layer) in [
            (
                "shadow",
                1.0,
                half_height + 1.0,
                size * 0.88,
                2.0,
                Color::srgba_u8(2, 6, 23, 180),
                z - 0.16,
            ),
            (
                "rim",
                -size * 0.12,
                -half_height + 1.0,
                size * if upright { 0.40 } else { 0.70 },
                1.0,
                Color::srgba_u8(226, 232, 240, 175),
                z + 0.006,
            ),
            (
                "profile",
                0.0,
                -half_height,
                size * if upright { 0.30 } else { 0.55 },
                if upright { 3.0 } else { 2.0 },
                Color::srgba_u8(148, 163, 184, 205),
                z - 0.01,
            ),
        ] {
            let key = format!("{id}:{part}");
            let sprite = sprite_for_rect(color, w * pixel.x, h * pixel.y);
            let transform = Transform::from_translation(to_bevy_translation(
                x + f64::from(dx * pixel.x),
                y + f64::from(dy * pixel.y),
                width,
                height,
                layer,
            ));
            if let Some(entity) = existing.remove(&key) {
                commands.entity(entity).insert((sprite, transform));
            } else {
                commands.spawn((sprite, transform, PixelWorldGroundingVisual { key }));
            }
        }
    }
    for entity in existing.into_values() {
        commands.entity(entity).despawn();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn agent_contact_and_rim_follow_scaled_sprite_body() {
        let mut app =
            super::super::tests::render_test_app(super::super::tests::sample_render_state(12000.));
        {
            let world = app.world_mut();
            world
                .query::<&mut Window>()
                .single_mut(world)
                .unwrap()
                .resolution
                .set(1280., 720.);
        }
        app.update();
        let world = app.world_mut();
        let body = *world
            .query_filtered::<&Transform, With<PixelWorldAgentVisual>>()
            .single(world)
            .unwrap();
        let mut query = world.query::<(&Transform, &Sprite, &PixelWorldGroundingVisual)>();
        let (shadow, sprite, _) = query
            .iter(world)
            .find(|(_, _, v)| v.key.starts_with("agent:") && v.key.ends_with(":shadow"))
            .unwrap();
        assert_eq!(shadow.translation.x, body.translation.x + 2.);
        assert_eq!(shadow.translation.y, body.translation.y - 18.);
        assert_eq!(sprite.custom_size.unwrap().y, 4.);
        let (rim, _, _) = query
            .iter(world)
            .find(|(_, _, v)| v.key.starts_with("agent:") && v.key.ends_with(":rim"))
            .unwrap();
        assert_eq!(rim.translation.y, body.translation.y + 14.);
    }
}
