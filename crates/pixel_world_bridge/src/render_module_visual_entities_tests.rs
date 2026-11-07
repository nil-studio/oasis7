use super::*;
use crate::render::module_visual_entities::{
    MODULE_LABEL_LAYER_Z, MODULE_VISUAL_ENTITY_COLOR, MODULE_VISUAL_ENTITY_SIZE_PX,
    ModuleIdentityChipPart, PixelWorldModuleIdentityChipVisual, PixelWorldModuleVisualEntity,
    PixelWorldModuleVisualLabel, module_co_anchor_offset, module_co_anchor_slots,
};
use bevy::math::DVec2;

fn module_visual(id: &str, pos: Position) -> ModuleVisualEntity {
    module_visual_with_kind(id, "opaque-kind", pos)
}

fn module_visual_with_kind(id: &str, kind: &str, pos: Position) -> ModuleVisualEntity {
    ModuleVisualEntity {
        factory_kind: None,
        id: id.to_string(),
        module_id: "opaque-module".to_string(),
        kind: kind.to_string(),
        label: None,
        pos,
    }
}

fn module_visual_with_label(
    id: &str,
    kind: &str,
    label: Option<&str>,
    pos: Position,
) -> ModuleVisualEntity {
    ModuleVisualEntity {
        factory_kind: None,
        id: id.to_string(),
        module_id: "opaque-module".to_string(),
        kind: kind.to_string(),
        label: label.map(ToString::to_string),
        pos,
    }
}

fn marker_raster_signature(app: &mut App) -> (usize, String) {
    let world = app.world_mut();
    let mut markers = world.query::<(&PixelWorldModuleVisualEntity, &Sprite, &Transform)>();
    let mut pixels = vec![[8_u8, 12, 20, 255]; 32 * 32];
    for (_, sprite, transform) in markers.iter(world) {
        let size = sprite
            .custom_size
            .expect("module marker uses an explicit pixel footprint");
        let color = sprite.color.to_srgba();
        let rotation = transform.rotation.to_euler(EulerRot::XYZ).2;
        let (sin, cos) = rotation.sin_cos();
        for y in 0..32 {
            for x in 0..32 {
                let relative_x = x as f32 + 0.5 - 16.0;
                let relative_y = y as f32 + 0.5 - 16.0;
                let local_x = (cos * relative_x) + (sin * relative_y);
                let local_y = (-sin * relative_x) + (cos * relative_y);
                if local_x.abs() <= size.x / 2.0 && local_y.abs() <= size.y / 2.0 {
                    pixels[y * 32 + x] = [
                        (color.red * 255.0).round() as u8,
                        (color.green * 255.0).round() as u8,
                        (color.blue * 255.0).round() as u8,
                        (color.alpha * 255.0).round() as u8,
                    ];
                }
            }
        }
    }
    let non_background = pixels
        .iter()
        .filter(|pixel| **pixel != [8, 12, 20, 255])
        .count();
    let signature = pixels
        .iter()
        .flatten()
        .fold(0xcbf29ce484222325_u64, |hash, byte| {
            (hash ^ u64::from(*byte)).wrapping_mul(0x100000001b3)
        });
    (non_background, format!("{signature:016x}"))
}

fn module_visual_raster_signature(kind: &str) -> (usize, String) {
    let mut state = sample_render_state(12_000.0);
    state.module_visual_entities = vec![module_visual_with_kind(
        "module-raster",
        kind,
        sample_position(1_530_000.0, 1_010_000.0),
    )];
    let mut app = render_test_app(state);
    let world = app.world_mut();
    let marker_position = {
        let mut markers = world.query::<(&PixelWorldModuleVisualEntity, &Transform)>();
        markers
            .single(world)
            .expect("module visual base is rendered")
            .1
            .translation
    };
    let mut sprites = world.query::<(
        &Sprite,
        &Transform,
        Option<&PixelWorldModuleVisualEntity>,
        Option<&PixelWorldModuleIdentityChipVisual>,
    )>();
    let mut sprites = sprites
        .iter(world)
        .filter(|(_, _, marker, chip)| marker.is_some() || chip.is_some())
        .collect::<Vec<_>>();
    sprites.sort_by(|(_, left, _, _), (_, right, _, _)| {
        left.translation
            .z
            .partial_cmp(&right.translation.z)
            .expect("sprite layer depth is finite")
    });
    let mut pixels = vec![[8_u8, 12, 20, 255]; 32 * 32];
    for (sprite, transform, _, _) in sprites {
        let Some(size) = sprite.custom_size else {
            continue;
        };
        let color = sprite.color.to_srgba();
        let rotation = transform.rotation.to_euler(EulerRot::XYZ).2;
        let (sin, cos) = rotation.sin_cos();
        for y in 0..32 {
            for x in 0..32 {
                let relative_x =
                    x as f32 + 0.5 - 16.0 - (transform.translation.x - marker_position.x);
                let relative_y =
                    y as f32 + 0.5 - 16.0 - (transform.translation.y - marker_position.y);
                let local_x = (cos * relative_x) + (sin * relative_y);
                let local_y = (-sin * relative_x) + (cos * relative_y);
                if local_x.abs() <= size.x / 2.0 && local_y.abs() <= size.y / 2.0 {
                    pixels[y * 32 + x] = [
                        (color.red * 255.0).round() as u8,
                        (color.green * 255.0).round() as u8,
                        (color.blue * 255.0).round() as u8,
                        (color.alpha * 255.0).round() as u8,
                    ];
                }
            }
        }
    }
    let non_background = pixels
        .iter()
        .filter(|pixel| **pixel != [8, 12, 20, 255])
        .count();
    let signature = pixels
        .iter()
        .flatten()
        .fold(0xcbf29ce484222325_u64, |hash, byte| {
            (hash ^ u64::from(*byte)).wrapping_mul(0x100000001b3)
        });
    (non_background, format!("{signature:016x}"))
}

#[test]
fn module_visual_entities_render_interactive_markers_and_reconcile_stale_markers() {
    let mut state = sample_render_state(12_000.0);
    state.module_visual_entities = vec![
        module_visual_with_kind(
            "module-z",
            "beacon",
            sample_position(1_530_000.0, 1_010_000.0),
        ),
        module_visual_with_kind(
            "module-a",
            "relay",
            sample_position(1_530_000.0, 1_010_000.0),
        ),
    ];
    let mut app = render_test_app(state);
    let world = app.world_mut();
    let mut markers = world.query::<(&PixelWorldModuleVisualEntity, &Sprite, &Transform)>();
    let rendered = markers
        .iter(world)
        .map(|(visual, sprite, transform)| {
            assert_eq!(sprite.color, MODULE_VISUAL_ENTITY_COLOR);
            assert_eq!(
                sprite.custom_size,
                Some(Vec2::splat(MODULE_VISUAL_ENTITY_SIZE_PX))
            );
            (visual.id.clone(), transform.translation)
        })
        .collect::<Vec<_>>();
    assert_eq!(rendered.len(), 2);
    let mut chips = world.query::<&PixelWorldModuleIdentityChipVisual>();
    assert_eq!(
        chips.iter(world).count(),
        2,
        "known module kinds each render one noninteractive identity chip"
    );
    assert_ne!(
        rendered[0].1, rendered[1].1,
        "co-anchors must receive distinct stable offsets"
    );
    let module_hit_regions = world
        .resource::<BevyRuntimeState>()
        .hit_regions
        .iter()
        .filter(|region| region.kind == "module_visual")
        .map(|region| region.id.as_str())
        .collect::<Vec<_>>();
    assert_eq!(
        module_hit_regions,
        vec!["module-a", "module-z"],
        "each visible module marker must expose a hit region for selection"
    );

    world
        .resource_mut::<BevyRuntimeState>()
        .render_state
        .as_mut()
        .expect("test render state")
        .module_visual_entities
        .truncate(1);
    app.update();
    let world = app.world_mut();
    let mut remaining = world.query::<&PixelWorldModuleVisualEntity>();
    assert_eq!(remaining.iter(world).count(), 1);
    let mut remaining_chips = world.query::<&PixelWorldModuleIdentityChipVisual>();
    assert_eq!(
        remaining_chips.iter(world).count(),
        1,
        "stale module identity chips must be removed with their base marker"
    );
    assert_eq!(
        world
            .resource::<BevyRuntimeState>()
            .module_visual_entities
            .len(),
        1
    );

    world
        .resource_mut::<BevyRuntimeState>()
        .render_state
        .as_mut()
        .expect("test render state")
        .module_visual_entities[0]
        .kind = "future_module_kind".to_string();
    app.update();
    let world = app.world_mut();
    let mut fallback_chips = world.query::<&PixelWorldModuleIdentityChipVisual>();
    assert_eq!(
        fallback_chips.iter(world).count(),
        0,
        "changing to an unknown kind must reconcile away the previous known-kind chip"
    );
}

#[test]
fn co_anchored_module_hit_stack_keeps_agent_and_location_centers_selectable() {
    let agent_anchor = sample_position(1_520_000.0, 1_015_000.0);
    let location_anchor = sample_position(1_500_000.0, 1_000_000.0);
    let mut state = sample_render_state(12_000.0);
    state.locations[0].pos = location_anchor.clone();
    state.agents[0].pos = Some(agent_anchor.clone());
    state.module_visual_entities = vec![
        module_visual_with_kind("module-agent-a", "beacon", agent_anchor.clone()),
        module_visual_with_kind("module-agent-b", "relay", agent_anchor.clone()),
        module_visual_with_kind("module-agent-c", "sensor", agent_anchor),
        module_visual_with_kind("module-location-a", "artifact", location_anchor.clone()),
        module_visual_with_kind("module-location-b", "relay", location_anchor),
    ];

    let mut app = render_test_app(state);
    let regions = hit_regions(&mut app);
    let center = |region: &HitRegion| {
        (
            (region.left + region.right) / 2.0,
            (region.top + region.bottom) / 2.0,
        )
    };
    let agent_region = regions
        .iter()
        .find(|region| region.kind == "agent" && region.id == "agent-0")
        .expect("co-anchored agent must retain a hit region");
    assert_eq!(
        hit_test(&regions, center(agent_region).0, center(agent_region).1),
        Some(("agent".to_string(), "agent-0".to_string())),
        "the parent Agent center must remain selectable when modules share its anchor"
    );
    let location_region = regions
        .iter()
        .find(|region| region.kind == "location" && region.id == "loc-0")
        .expect("co-anchored location must retain a hit region");
    assert_eq!(
        hit_test(
            &regions,
            center(location_region).0,
            center(location_region).1,
        ),
        Some(("location".to_string(), "loc-0".to_string())),
        "the parent Location center must remain selectable when modules share its anchor"
    );

    let module_regions = regions
        .iter()
        .filter(|region| region.kind == "module_visual")
        .collect::<Vec<_>>();
    assert_eq!(module_regions.len(), 5);
    for region in &module_regions {
        assert_eq!(
            hit_test(&regions, center(region).0, center(region).1),
            Some((region.kind.to_string(), region.id.clone())),
            "each displaced module center must resolve to its own Bevy hit region"
        );
    }
    for (left, right) in module_regions.iter().enumerate() {
        for other in module_regions.iter().skip(left + 1) {
            assert_ne!(
                center(right),
                center(other),
                "co-anchored module slots must stay distinct"
            );
        }
    }

    let marker_centers = {
        let world = app.world_mut();
        let mut markers = world.query::<(&PixelWorldModuleVisualEntity, &Transform)>();
        markers
            .iter(world)
            .map(|(marker, transform)| {
                (
                    marker.id.clone(),
                    (
                        f64::from(VIEWPORT_WIDTH as f32 / 2.0 + transform.translation.x),
                        f64::from(VIEWPORT_HEIGHT as f32 / 2.0 - transform.translation.y),
                    ),
                )
            })
            .collect::<std::collections::HashMap<_, _>>()
    };
    for region in module_regions {
        let marker_center = marker_centers
            .get(&region.id)
            .expect("each module hit region must have a rendered marker");
        let region_center = center(region);
        assert!((marker_center.0 - region_center.0).abs() < 0.001);
        assert!((marker_center.1 - region_center.1).abs() < 0.001);
    }
}

#[test]
fn co_anchor_renderer_offsets_keep_the_48_css_gap_across_backing_scales() {
    for backing_scale in [1.0_f32, 1.5, 2.0] {
        let scale = Vec2::splat(backing_scale);
        let first = module_co_anchor_offset(0, scale, DVec2::ONE);
        let second = module_co_anchor_offset(1, scale, DVec2::ONE);
        let css_gap_x = (second.x - first.x) / f64::from(backing_scale);
        let css_gap_y = (second.y - first.y) / f64::from(backing_scale);
        assert!((css_gap_x - 48.0).abs() < f64::EPSILON);
        assert!(css_gap_y.abs() < f64::EPSILON);
    }
}

#[test]
fn module_co_anchor_grouping_preserves_deterministic_slots_at_capacity() {
    let position_keys = vec![Some((1, 2, 3)); 4096];
    let slots = module_co_anchor_slots(&position_keys);

    assert_eq!(
        position_keys.iter().filter(|key| key.is_some()).count(),
        4096
    );
    assert_eq!(slots.first().copied().flatten(), Some((0, 4096)));
    assert_eq!(slots.get(1).copied().flatten(), Some((1, 4096)));
    assert_eq!(slots.get(8).copied().flatten(), Some((8, 4096)));
    assert_eq!(slots.last().copied().flatten(), Some((4095, 4096)));
}

#[test]
fn module_visual_marker_has_a_fixed_diamond_raster_signature() {
    let mut state = sample_render_state(12_000.0);
    state.module_visual_entities = vec![module_visual(
        "module-raster",
        sample_position(1_530_000.0, 1_010_000.0),
    )];
    let mut app = render_test_app(state);
    let (non_background, signature) = marker_raster_signature(&mut app);
    assert_eq!(non_background, 40);
    assert_eq!(signature, "8ccc88bd200daee5");
}

#[test]
fn co_anchored_module_identity_chips_follow_their_own_displaced_base_markers() {
    let anchor = sample_position(1_530_000.0, 1_010_000.0);
    let mut state = sample_render_state(12_000.0);
    state.module_visual_entities = vec![
        module_visual_with_kind("module-beacon", "beacon", anchor.clone()),
        module_visual_with_kind("module-relay", "relay", anchor),
    ];
    let mut app = render_test_app(state);
    let world = app.world_mut();
    let mut markers = world.query::<(&PixelWorldModuleVisualEntity, &Transform)>();
    let marker_positions = markers
        .iter(world)
        .map(|(marker, transform)| (marker.id.clone(), transform.translation))
        .collect::<std::collections::BTreeMap<_, _>>();
    let mut chips = world.query::<(&PixelWorldModuleIdentityChipVisual, &Transform)>();
    let chip_positions = chips
        .iter(world)
        .map(|(chip, transform)| ((chip.id.clone(), chip.part), transform.translation))
        .collect::<std::collections::BTreeMap<_, _>>();

    for ((id, part), chip_position) in chip_positions {
        let base_position = marker_positions[&id];
        let glyph_local_offset = match part {
            ModuleIdentityChipPart::BeaconStem => Vec2::new(0.0, -2.5),
            ModuleIdentityChipPart::RelayBar => Vec2::new(2.5, 0.0),
            other => panic!("unexpected co-anchor fixture chip part: {other:?}"),
        };
        assert_eq!(
            chip_position.truncate() - base_position.truncate(),
            glyph_local_offset,
            "{id} chip must retain its glyph-local offset from its own displaced base"
        );
    }
}

#[test]
fn known_module_kinds_have_distinct_identity_glyphs_while_unknown_kinds_stay_neutral() {
    let beacon = module_visual_raster_signature("beacon");
    let relay = module_visual_raster_signature("relay");
    let unknown = module_visual_raster_signature("future_module_kind");
    let neutral = module_visual_raster_signature("opaque-kind");

    assert_ne!(
        beacon, relay,
        "known module kinds need distinct, first-glance identity glyphs"
    );
    assert_eq!(
        unknown, neutral,
        "unknown module kinds must retain the neutral fallback raster"
    );
}

#[test]
fn module_visual_labels_are_zoom_gated_stably_suppressed_and_reconciled() {
    let anchor = sample_position(1_530_000.0, 1_010_000.0);
    let mut state = sample_render_state(12_000.0);
    state.module_visual_entities = vec![
        module_visual_with_label("module-z", "relay", Some("Relay marker"), anchor.clone()),
        module_visual_with_label("module-a", "beacon", None, anchor),
    ];
    let mut app = render_test_app(state);
    let world = app.world_mut();
    let mut labels = world.query::<(&PixelWorldModuleVisualLabel, &Text2d, &Transform)>();
    let rendered = labels
        .iter(world)
        .map(|(label, text, transform)| {
            assert_eq!(transform.translation.z, MODULE_LABEL_LAYER_Z);
            (label.id.clone(), text.0.clone())
        })
        .collect::<Vec<_>>();
    assert_eq!(
        rendered,
        vec![("module-a".to_string(), "beacon:module-a".to_string())],
        "the stable ID-first label wins a co-anchor collision and uses the documented fallback"
    );

    world.resource_mut::<BevyRuntimeState>().camera.zoom = 1.0;
    app.update();
    let world = app.world_mut();
    let mut labels = world.query::<&PixelWorldModuleVisualLabel>();
    assert_eq!(
        labels.iter(world).count(),
        0,
        "overview zoom must return to glyph-only rendering"
    );

    world.resource_mut::<BevyRuntimeState>().camera.zoom = 3.0;
    world
        .resource_mut::<BevyRuntimeState>()
        .render_state
        .as_mut()
        .expect("test render state")
        .module_visual_entities
        .truncate(1);
    app.update();
    let world = app.world_mut();
    let mut labels = world.query::<(&PixelWorldModuleVisualLabel, &Text2d)>();
    let updated = labels
        .iter(world)
        .map(|(label, text)| (label.id.clone(), text.0.clone()))
        .collect::<Vec<_>>();
    assert_eq!(
        updated,
        vec![("module-z".to_string(), "Relay marker".to_string())]
    );

    world
        .resource_mut::<BevyRuntimeState>()
        .render_state
        .as_mut()
        .expect("test render state")
        .module_visual_entities
        .clear();
    {
        let mut runtime = world.resource_mut::<BevyRuntimeState>();
        runtime.render_version += 1;
        runtime.hit_regions_dirty = true;
    }
    app.update();
    let world = app.world_mut();
    let mut labels = world.query::<&PixelWorldModuleVisualLabel>();
    assert_eq!(
        labels.iter(world).count(),
        0,
        "removed markers leave no stale labels"
    );
    assert!(
        world
            .resource::<BevyRuntimeState>()
            .hit_regions
            .iter()
            .all(|region| region.kind != "module_visual")
    );
}

#[test]
fn module_visual_labels_fallback_when_explicit_label_matches_entity_id() {
    let mut state = sample_render_state(12_000.0);
    state.locale = "zh-CN".to_string();
    state.module_visual_entities = vec![module_visual_with_label(
        "module-equal",
        "relay",
        Some("module-equal"),
        sample_position(1_530_000.0, 1_010_000.0),
    )];

    let mut app = render_test_app(state);
    let world = app.world_mut();
    let mut labels = world.query::<(&PixelWorldModuleVisualLabel, &Text2d)>();
    let rendered = labels
        .iter(world)
        .map(|(_, text)| text.0.clone())
        .collect::<Vec<_>>();
    assert_eq!(rendered, vec!["模块 relay:module-equal"]);
}

#[test]
fn module_visual_labels_yield_to_shared_map_label_obstacles() {
    let anchor = sample_position(1_530_000.0, 1_010_000.0);
    let mut state = sample_render_state(12_000.0);
    state.agents.clear();
    state.locations.clear();
    state.links.clear();
    state.visual_hotspots = vec![VisualHotspot {
        id: "goal-highlight".to_string(),
        label: "Current objective".to_string(),
        kind: "goal".to_string(),
        pos: anchor.clone(),
        emphasis: Some(1.0),
        size_hint_px: Some(14.0),
    }];
    state.module_visual_entities = vec![module_visual_with_label(
        "module-goal",
        "relay",
        Some("Relay marker"),
        anchor,
    )];

    let mut app = render_test_app(state);
    let world = app.world_mut();
    let mut module_labels = world.query::<&PixelWorldModuleVisualLabel>();
    assert_eq!(
        module_labels.iter(world).count(),
        0,
        "module identity labels must yield to the shared objective label obstacle"
    );
    let mut map_labels = world.query::<(&crate::render::map_labels::PixelWorldMapLabel, &Text2d)>();
    assert_eq!(
        map_labels
            .iter(world)
            .map(|(_, text)| text.0.clone())
            .collect::<Vec<_>>(),
        vec!["Current objective"],
        "the shared map label remains the readable priority"
    );
}

#[test]
fn authored_module_glyph_pixels_scale_with_world_unit_without_scaling_labels() {
    let mut state = sample_render_state(12000.);
    state.module_visual_entities = vec![module_visual_with_kind(
        "pixel",
        "artifact",
        sample_position(1_500_000., 1_000_000.),
    )];
    state.selection = Some(Selection {
        kind: "module_visual".into(),
        id: "pixel".into(),
    });
    let mut app = render_test_app(state);
    {
        let world = app.world_mut();
        world
            .query::<&mut Window>()
            .single_mut(world)
            .unwrap()
            .resolution
            .set(1280., 720.);
        let mut runtime = world.resource_mut::<BevyRuntimeState>();
        runtime.camera.zoom = 2.;
        runtime.camera_user_override = true;
    }
    app.update();
    let world = app.world_mut();
    let (body, transform) = world
        .query_filtered::<(&Sprite, &Transform), With<PixelWorldModuleVisualEntity>>()
        .single(world)
        .unwrap();
    assert_eq!(body.custom_size, Some(Vec2::splat(12.)));
    let center = transform.translation;
    let dot = world
        .query_filtered::<&Sprite, With<PixelWorldModuleIdentityChipVisual>>()
        .single(world)
        .unwrap();
    assert_eq!(
        dot.custom_size,
        Some(Vec2::splat(2.)),
        "one authored pixel must occupy one low-res texel, not half a texel"
    );
    let hit = &world
        .resource::<BevyRuntimeState>()
        .hit_regions
        .iter()
        .find(|hit| hit.kind == "module_visual")
        .unwrap();
    assert_eq!(hit.right - hit.left, 32.);
    let (label, font) = world
        .query_filtered::<(&Transform, &TextFont), With<PixelWorldModuleVisualLabel>>()
        .single(world)
        .unwrap();
    assert!((label.translation.y - center.y - 16.24264).abs() < 0.001);
    assert_eq!(font.font_size, FontSize::Px(10.));
}

#[test]
fn large_pixel_module_slots_keep_hit_regions_separate() {
    let pixel = DVec2::splat(6.0);
    let first = module_co_anchor_offset(0, Vec2::ONE, pixel);
    let second = module_co_anchor_offset(1, Vec2::ONE, pixel);
    assert_eq!(first, DVec2::splat(-102.0));
    assert_eq!(second.x - first.x, 102.0);
    assert!(second.x - first.x > 2.0 * 8.0 * pixel.x);
    assert_eq!(first.x % pixel.x, 0.0);
}

#[test]
fn fractional_css_module_offset_keeps_web_f64_ceil_boundary() {
    let unit = DVec2::new(959.99999999 / 1480.0 * 2.0, 720.0 / 1110.0 * 2.0);
    let offset = module_co_anchor_offset(0, Vec2::ONE, unit);
    assert!((offset.x + 38.0 * unit.x).abs() < 1e-12);
    assert!(offset.x < -49.0);
}
