use super::*;
use crate::render::social_links::SocialLinkPart;
use std::collections::HashMap;

fn social_entities(app: &mut App) -> HashMap<(String, SocialLinkPart), (Entity, Transform)> {
    let world = app.world_mut();
    let mut query = world.query::<(Entity, &PixelWorldSocialLinkVisual, &Transform)>();
    query
        .iter(world)
        .map(|(entity, visual, transform)| {
            ((visual.link_id.clone(), visual.part), (entity, *transform))
        })
        .collect()
}

fn social_state() -> RenderState {
    let mut state = sample_render_state(12_000.0);
    state.selection = None;
    state.social_links = vec![SocialLink {
        id: "social_edge:7".to_string(),
        from: sample_position(1_250_000.0, 750_000.0),
        to: sample_position(1_750_000.0, 1_250_000.0),
        relation_kind: "ally".to_string(),
        lifecycle: "active".to_string(),
    }];
    state
}

#[test]
fn social_links_are_noninteractive_stable_and_cleaned_up() {
    let mut app = render_test_app(social_state());
    let initial = social_entities(&mut app);
    assert_eq!(initial.len(), 3, "line plus two endpoint glyphs");
    assert!(initial.keys().all(|(id, _)| id == "social_edge:7"));
    let hits = hit_regions(&mut app);

    app.update();
    let repeated = social_entities(&mut app);
    assert_eq!(
        repeated.keys().collect::<std::collections::HashSet<_>>(),
        initial.keys().collect::<std::collections::HashSet<_>>()
    );
    assert_eq!(
        repeated
            .values()
            .map(|(entity, _)| *entity)
            .collect::<std::collections::HashSet<_>>(),
        initial
            .values()
            .map(|(entity, _)| *entity)
            .collect::<std::collections::HashSet<_>>()
    );
    assert_eq!(
        hit_regions(&mut app),
        hits,
        "social visuals do not add hit regions"
    );

    {
        let mut runtime = app.world_mut().resource_mut::<BevyRuntimeState>();
        let mut state = social_state();
        state.social_links.clear();
        runtime.render_state = Some(state);
        runtime.render_version += 1;
    }
    app.update();
    assert!(social_entities(&mut app).is_empty());

    let mut app = render_test_app(social_state());
    assert_eq!(social_entities(&mut app).len(), 3);
    {
        let mut runtime = app.world_mut().resource_mut::<BevyRuntimeState>();
        runtime.render_state = None;
        runtime.render_version += 1;
    }
    app.update();
    assert!(
        social_entities(&mut app).is_empty(),
        "clearing the render state must despawn social visuals"
    );
    assert!(hit_regions(&mut app).is_empty());
}

#[test]
fn social_links_have_a_stable_nonzero_raster_delta_without_hits() {
    let mut baseline_state = social_state();
    baseline_state.social_links.clear();
    let mut baseline = render_test_app(baseline_state);
    let (baseline_image, baseline_summary) = rasterize_pixel_regression(&mut baseline);
    let baseline_hits = hit_regions(&mut baseline);

    let mut visible = render_test_app(social_state());
    let (visible_image, visible_summary) = rasterize_pixel_regression(&mut visible);
    let mut repeated = render_test_app(social_state());
    let (repeated_image, repeated_summary) = rasterize_pixel_regression(&mut repeated);
    let pixel_delta = baseline_image
        .pixels()
        .zip(visible_image.pixels())
        .filter(|(baseline, visible)| baseline != visible)
        .count();

    assert!(
        visible_summary.non_background_pixels > baseline_summary.non_background_pixels,
        "social line and endpoint glyphs must add visible raster pixels"
    );
    assert!(
        pixel_delta > 0,
        "social visuals must create a nonzero pixel delta"
    );
    assert_ne!(
        visible_summary.raw_rgba_fnv1a64, baseline_summary.raw_rgba_fnv1a64,
        "social visuals must change the deterministic raster signature"
    );
    assert_eq!(
        visible_summary.raw_rgba_fnv1a64, repeated_summary.raw_rgba_fnv1a64,
        "social visuals must produce a stable raster hash"
    );
    assert_eq!(visible_image.as_raw(), repeated_image.as_raw());
    assert_eq!(hit_regions(&mut visible), baseline_hits);
}

#[test]
fn directional_social_links_add_destination_chevrons_without_new_hit_regions() {
    let mut state = social_state();
    state.social_links[0].relation_kind = "supports".to_string();
    let mut app = render_test_app(state);
    let entities = social_entities(&mut app);
    assert_eq!(
        entities.len(),
        5,
        "line, endpoints, and two destination chevrons"
    );
    assert!(entities.contains_key(&(
        "social_edge:7".to_string(),
        SocialLinkPart::RelationMarkerUpper,
    )));
    assert!(entities.contains_key(&(
        "social_edge:7".to_string(),
        SocialLinkPart::RelationMarkerLower,
    )));
    let baseline_hits = hit_regions(&mut app);
    app.update();
    assert_eq!(social_entities(&mut app).len(), 5);
    assert_eq!(hit_regions(&mut app), baseline_hits);

    {
        let mut runtime = app.world_mut().resource_mut::<BevyRuntimeState>();
        let mut neutral = social_state();
        neutral.social_links[0].relation_kind = "ally".to_string();
        runtime.render_state = Some(neutral);
        runtime.render_version += 1;
    }
    app.update();
    let neutral_entities = social_entities(&mut app);
    assert_eq!(neutral_entities.len(), 3);
    assert!(!neutral_entities.keys().any(|(_, part)| matches!(
        part,
        SocialLinkPart::RelationMarkerUpper | SocialLinkPart::RelationMarkerLower
    )));

    let mut supply = social_state();
    supply.social_links[0].relation_kind = "supply".to_string();
    {
        let mut runtime = app.world_mut().resource_mut::<BevyRuntimeState>();
        runtime.render_state = Some(supply);
        runtime.render_version += 1;
    }
    app.update();
    let supply_entities = social_entities(&mut app);
    assert_eq!(supply_entities.len(), 5);
    assert!(
        supply_entities
            .keys()
            .any(|(_, part)| { *part == SocialLinkPart::RelationMarkerUpper })
    );
    assert!(
        supply_entities
            .keys()
            .any(|(_, part)| { *part == SocialLinkPart::RelationMarkerLower })
    );

    let mut ally = social_state();
    ally.social_links[0].relation_kind = "ally".to_string();
    let mut ally_app = render_test_app(ally);
    let (ally_image, ally_summary) = rasterize_pixel_regression(&mut ally_app);
    let mut supports = social_state();
    supports.social_links[0].relation_kind = "supports".to_string();
    let mut supports_app = render_test_app(supports);
    let (supports_image, supports_summary) = rasterize_pixel_regression(&mut supports_app);
    let pixel_delta = ally_image
        .pixels()
        .zip(supports_image.pixels())
        .filter(|(ally, supports)| ally != supports)
        .count();
    assert!(
        pixel_delta > 0,
        "directional relation markers must add a visible raster delta"
    );
    assert_ne!(
        ally_summary.raw_rgba_fnv1a64, supports_summary.raw_rgba_fnv1a64,
        "directional relation markers must change the deterministic raster signature"
    );
}

#[test]
fn social_stroke_and_endpoint_use_the_world_logical_pixel_unit() {
    let mut app = render_test_app(social_state());
    {
        let world = app.world_mut();
        world
            .query::<&mut Window>()
            .single_mut(world)
            .unwrap()
            .resolution
            .set(1280.0, 720.0);
    }
    app.update();
    let world = app.world_mut();
    let mut count = 0;
    for (visual, sprite) in world
        .query::<(&PixelWorldSocialLinkVisual, &Sprite)>()
        .iter(world)
    {
        let size = sprite.custom_size.unwrap();
        if visual.part == SocialLinkPart::Line {
            assert!((size.y - 3.0).abs() < 0.001);
        } else {
            assert_eq!(size, Vec2::splat(10.0));
        }
        count += 1;
    }
    assert_eq!(count, 3);
}

#[test]
fn same_css_dpr_change_reconciles_world_units_and_grid() {
    let mut app = render_test_app(social_state());
    {
        let world = app.world_mut();
        world
            .query::<&mut Window>()
            .single_mut(world)
            .unwrap()
            .resolution
            .set(390.0, 844.0);
        world.resource_mut::<BevyRuntimeState>().reactive_scheduling = true;
    }
    app.update();
    let before_grid = app
        .world_mut()
        .resource::<BevyRuntimeState>()
        .grid_layout
        .clone();
    {
        let world = app.world_mut();
        let mut window = world.query::<&mut Window>().single_mut(world).unwrap();
        window.resolution.set_scale_factor_override(Some(2.0));
        window.resolution.set_physical_resolution(780, 1688);
        let mut runtime = world.resource_mut::<BevyRuntimeState>();
        runtime.needs_reconcile = false;
        runtime.animation_dirty = false;
    }
    app.update();
    let world = app.world_mut();
    assert_ne!(
        world.resource::<BevyRuntimeState>().grid_layout,
        before_grid
    );
    for (visual, sprite) in world
        .query::<(&PixelWorldSocialLinkVisual, &Sprite)>()
        .iter(world)
    {
        if visual.part == SocialLinkPart::Line {
            assert!((sprite.custom_size.unwrap().y - 0.75).abs() < 0.001);
        }
    }
    for (_, sprite) in world
        .query::<(&PixelWorldAgentVisual, &Sprite)>()
        .iter(world)
    {
        assert_eq!(sprite.custom_size, Some(Vec2::splat(8.0)));
    }
    assert!(
        world
            .resource::<BevyRuntimeState>()
            .hit_regions
            .iter()
            .filter(|hit| hit.kind == "agent")
            .all(|hit| (hit.right - hit.left - 8.0).abs() < 0.001)
    );
}
