use super::*;
use crate::render::micro_depot_facilities::PixelWorldMicroDepotStockRunwayVisual;
use serde_json::json;
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Copy, Debug)]
struct RunwayProbe {
    segment: Option<u8>,
    size: Vec2,
    translation: Vec3,
    rotation: f32,
    layer_z: f32,
}

fn depot(id: &str, status: &str) -> MicroDepotFacility {
    MicroDepotFacility {
        id: format!("micro_depot:{id}"),
        facility_id: id.to_string(),
        location_id: "loc-0".to_string(),
        status: status.to_string(),
        pos: sample_position(1_530_000.0, 1_010_000.0),
        service_radius_cm: 0.0,
        inventory_revision: 0,
        available_units_by_kind: BTreeMap::new(),
        throughput_epoch: 0,
        throughput_remaining_units: 0,
        throughput_limit_units_per_epoch: 0,
    }
}

fn depot_with_service_radius(id: &str, service_radius_cm: f64) -> MicroDepotFacility {
    MicroDepotFacility {
        service_radius_cm,
        ..depot(id, "active")
    }
}

fn service_radius_segments(app: &mut App) -> BTreeMap<(String, u8), (Entity, Vec3, Quat, Vec2)> {
    let world = app.world_mut();
    world
        .query::<(
            Entity,
            &PixelWorldMicroDepotServiceRadiusVisual,
            &Transform,
            &Sprite,
        )>()
        .iter(world)
        .map(|(entity, visual, transform, sprite)| {
            (
                (visual.id.clone(), visual.segment),
                (
                    entity,
                    transform.translation,
                    transform.rotation,
                    sprite
                        .custom_size
                        .expect("service-radius segment has a size"),
                ),
            )
        })
        .collect()
}

fn reconcile_depot_scene(app: &mut App) {
    {
        let mut runtime = app.world_mut().resource_mut::<BevyRuntimeState>();
        runtime.reactive_scheduling = true;
        runtime.needs_reconcile = true;
        runtime.camera_user_override = true;
    }
    app.update();
}

#[test]
fn facility_only_snapshots_reconcile_added_glyph_and_grounding() {
    assert_facility_snapshot_reconciles(false);
}

#[test]
fn facility_visual_changes_invalidate_content_without_refitting_camera() {
    let mut initial = sample_render_state(12_000.0);
    initial
        .micro_depot_facilities
        .push(depot("snapshot", "active"));
    let mutations: &[fn(&mut MicroDepotFacility)] = &[
        |facility| facility.id.push_str("-new"),
        |facility| facility.pos = sample_position(1_630_000.0, 1_010_000.0),
        |facility| facility.status = "suspended".into(),
        |facility| facility.service_radius_cm = 240_000.0,
        |facility| {
            facility.available_units_by_kind.insert("food".into(), 5);
        },
        |facility| facility.throughput_remaining_units = 10,
        |facility| facility.throughput_limit_units_per_epoch = 20,
    ];
    for (index, mutate) in mutations.iter().enumerate() {
        let mut changed = initial.clone();
        mutate(&mut changed.micro_depot_facilities[0]);
        assert_ne!(
            render_content_signature(Some(&initial)),
            render_content_signature(Some(&changed)),
            "rendered facility field mutation {index} must invalidate content"
        );
        assert_eq!(
            camera_content_signature(Some(&initial)),
            camera_content_signature(Some(&changed))
        );
    }
}

#[test]
fn facility_only_snapshots_reconcile_removed_glyph_and_grounding() {
    assert_facility_snapshot_reconciles(true);
}

fn assert_facility_snapshot_reconciles(initially_present: bool) {
    use crate::render::grounding::PixelWorldGroundingVisual;
    let mut state = sample_render_state(12_000.0);
    if initially_present {
        state
            .micro_depot_facilities
            .push(depot("snapshot", "active"));
    }
    let mut app = render_test_app(state.clone());
    let grounding_before = {
        let world = app.world_mut();
        world
            .query::<&PixelWorldGroundingVisual>()
            .iter(world)
            .count()
    };
    if initially_present {
        state.micro_depot_facilities.clear();
    } else {
        state
            .micro_depot_facilities
            .push(depot("snapshot", "active"));
    }
    {
        let mut runtime = app.world_mut().resource_mut::<BevyRuntimeState>();
        runtime.reactive_scheduling = true;
        runtime.animation_dirty = true;
        runtime.needs_reconcile = false;
        runtime.hit_regions_dirty = false;
        runtime.camera_user_override = true;
        runtime.render_content_signature = render_content_signature(runtime.render_state.as_ref());
        let version = runtime.render_version + 1;
        apply_external_render_snapshot(
            &mut runtime,
            true,
            RenderSnapshot::Changed {
                version,
                state: Some(Box::new(state)),
            },
        );
        assert!(!runtime.hit_regions_dirty);
        assert!(runtime.camera_user_override);
    }
    app.update();
    let world = app.world_mut();
    let expected = usize::from(!initially_present);
    assert_eq!(
        world
            .query::<&PixelWorldMicroDepotVisual>()
            .iter(world)
            .count(),
        expected,
        "facility-only snapshots must reconcile static glyphs"
    );
    assert_eq!(
        world
            .query::<&PixelWorldMicroDepotDetailVisual>()
            .iter(world)
            .count(),
        expected
    );
    let grounding_after = world
        .query::<&PixelWorldGroundingVisual>()
        .iter(world)
        .count();
    assert_eq!(
        grounding_after,
        if initially_present {
            grounding_before - 3
        } else {
            grounding_before + 3
        }
    );
}

fn render_state_with_stock(remaining_units: i64, limit_units: i64) -> RenderState {
    serde_json::from_value(json!({
        "world_bounds": {
            "width_cm": 3_000_000.0,
            "depth_cm": 2_000_000.0,
            "height_cm": 500_000.0
        },
        "locations": [],
        "fragment_terrain": [],
        "micro_depot_facilities": [{
            "id": "micro_depot:runway",
            "facility_id": "runway",
            "location_id": "loc-0",
            "status": "active",
            "pos": { "x_cm": 1_500_000.0, "y_cm": 1_000_000.0, "z_cm": 0.0 },
            "service_radius_cm": 0.0,
            "inventory_revision": 7,
            "available_units_by_kind": { "data": remaining_units },
            "throughput_epoch": 2,
            "throughput_remaining_units": remaining_units,
            "throughput_limit_units_per_epoch": limit_units
        }],
        "module_visual_entities": [],
        "agents": [],
        "links": [],
        "social_links": [],
        "visual_hotspots": [],
        "selection": null,
        "receipt_target": null,
        "recommended_target": null
    }))
    .expect("stock runway render fixture must deserialize")
}

fn render_state_with_stock_throughput_texts() -> RenderState {
    let mut state = render_state_with_stock(8, 8);
    let mut healthy = depot("throughput-healthy", "active");
    healthy.pos = sample_position(700_000.0, 1_000_000.0);
    healthy.inventory_revision = 10;
    healthy
        .available_units_by_kind
        .insert("data".to_string(), 8);
    healthy.throughput_epoch = 3;
    healthy.throughput_remaining_units = 8;
    healthy.throughput_limit_units_per_epoch = 8;

    let mut low = depot("throughput-low", "active");
    low.pos = sample_position(1_500_000.0, 1_000_000.0);
    low.inventory_revision = 11;
    low.available_units_by_kind.insert("data".to_string(), 2);
    low.throughput_epoch = 3;
    low.throughput_remaining_units = 2;
    low.throughput_limit_units_per_epoch = 8;

    let mut zero = depot("throughput-zero", "active");
    zero.pos = sample_position(2_300_000.0, 1_000_000.0);
    zero.inventory_revision = 12;
    zero.available_units_by_kind.insert("data".to_string(), 0);
    zero.throughput_epoch = 3;
    zero.throughput_remaining_units = 0;
    zero.throughput_limit_units_per_epoch = 8;

    let mut unknown = depot("throughput-unknown", "active");
    unknown.pos = sample_position(700_000.0, 1_500_000.0);

    let mut no_limit = depot("throughput-no-limit", "active");
    no_limit.pos = sample_position(1_500_000.0, 1_500_000.0);
    no_limit
        .available_units_by_kind
        .insert("data".to_string(), 8);
    no_limit.throughput_remaining_units = 8;
    no_limit.throughput_limit_units_per_epoch = 0;

    state.micro_depot_facilities = vec![healthy, low, zero, unknown, no_limit];
    state
}

fn depot_stage_sprite_count(app: &mut App) -> usize {
    let world = app.world_mut();
    let mut sprites = world.query::<(&Sprite, &Transform)>();
    sprites
        .iter(world)
        .filter(|(_, transform)| {
            transform.translation.z > MICRO_DEPOT_LAYER_Z - 0.2
                && transform.translation.z < MICRO_DEPOT_LAYER_Z + 0.02
        })
        .count()
}

fn depot_stage_raster_signature(app: &mut App) -> String {
    let world = app.world_mut();
    let mut sprites = world.query::<(&Sprite, &Transform)>();
    let mut layers = sprites
        .iter(world)
        .filter(|(_, transform)| {
            transform.translation.z > MICRO_DEPOT_LAYER_Z - 0.2
                && transform.translation.z < MICRO_DEPOT_LAYER_Z + 0.02
        })
        .map(|(sprite, transform)| {
            (
                sprite
                    .custom_size
                    .expect("depot stage sprites have explicit dimensions"),
                sprite.color.to_srgba(),
                transform.rotation.to_euler(EulerRot::XYZ).2,
                transform.translation.z,
            )
        })
        .collect::<Vec<_>>();
    layers.sort_by(|left, right| {
        left.3
            .partial_cmp(&right.3)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    let mut pixels = vec![[0_u8; 4]; 32 * 32];
    for (size, color, rotation, _) in layers {
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
    let bytes = pixels.iter().flatten().copied().collect::<Vec<_>>();
    fnv1a64(&bytes)
}

fn runway_probes(app: &mut App) -> Vec<RunwayProbe> {
    let world = app.world_mut();
    let mut runway = world.query::<(&PixelWorldMicroDepotStockRunwayVisual, &Sprite, &Transform)>();
    runway
        .iter(world)
        .map(|(visual, sprite, transform)| RunwayProbe {
            segment: Some(visual.segment),
            size: sprite
                .custom_size
                .expect("stock runway segment has an explicit screen size"),
            translation: transform.translation,
            rotation: transform.rotation.to_euler(EulerRot::XYZ).2,
            layer_z: transform.translation.z,
        })
        .collect()
}

fn stock_throughput_texts(app: &mut App) -> Vec<(String, Vec3)> {
    let world = app.world_mut();
    let mut labels = world.query::<(&Text2d, &Transform)>();
    labels
        .iter(world)
        .filter(|(_, transform)| {
            transform.translation.z >= MICRO_DEPOT_LAYER_Z + 0.001
                && transform.translation.z <= MICRO_DEPOT_LAYER_Z + 0.1
        })
        .map(|(text, transform)| (text.0.clone(), transform.translation))
        .collect()
}

fn stock_throughput_text_styles(app: &mut App) -> Vec<(String, f32, Vec3)> {
    let world = app.world_mut();
    let mut labels = world.query::<(&Text2d, &TextFont, &Transform)>();
    labels
        .iter(world)
        .filter(|(_, _, transform)| {
            transform.translation.z >= MICRO_DEPOT_LAYER_Z + 0.001
                && transform.translation.z <= MICRO_DEPOT_LAYER_Z + 0.1
        })
        .map(|(text, font, transform)| {
            let size = match font.font_size {
                FontSize::Px(size) => size,
                _ => 0.0,
            };
            (text.0.clone(), size, transform.translation)
        })
        .collect()
}

fn stock_throughput_backings(app: &mut App) -> Vec<(Vec2, Vec3)> {
    let world = app.world_mut();
    let mut sprites = world.query::<(&Sprite, &Transform)>();
    sprites
        .iter(world)
        .filter_map(|(sprite, transform)| {
            let size = sprite.custom_size?;
            let color = sprite.color.to_srgba();
            let stock_layer = transform.translation.z >= MICRO_DEPOT_LAYER_Z + 0.001
                && transform.translation.z <= MICRO_DEPOT_LAYER_Z + 0.1;
            let dark = color.alpha >= 0.8 && color.red + color.green + color.blue < 1.0;
            let readable_size = size.x >= 20.0 && size.y >= 12.0;
            (stock_layer && dark && readable_size).then_some((size, transform.translation))
        })
        .collect()
}

fn stock_stage_probes(app: &mut App) -> Vec<RunwayProbe> {
    let world = app.world_mut();
    let mut sprites = world.query::<(&Sprite, &Transform)>();
    sprites
        .iter(world)
        .filter(|(_, transform)| {
            transform.translation.z > MICRO_DEPOT_LAYER_Z + 0.001
                && transform.translation.z < MICRO_DEPOT_LAYER_Z + 0.1
        })
        .map(|(sprite, transform)| RunwayProbe {
            segment: None,
            size: sprite
                .custom_size
                .expect("stock runway treatment has an explicit screen size"),
            translation: transform.translation,
            rotation: transform.rotation.to_euler(EulerRot::XYZ).2,
            layer_z: transform.translation.z,
        })
        .collect()
}

fn rotated_screen_footprint(size: Vec2, rotation: f32) -> Vec2 {
    let (sin, cos) = rotation.sin_cos();
    Vec2::new(
        size.x * cos.abs() + size.y * sin.abs(),
        size.x * sin.abs() + size.y * cos.abs(),
    )
}

fn has_dark_backing_for_each_segment(app: &mut App, segments: &[RunwayProbe]) -> bool {
    let world = app.world_mut();
    let mut sprites = world.query::<(&Sprite, &Transform)>();
    segments.iter().all(|segment| {
        sprites.iter(world).any(|(sprite, transform)| {
            let color = sprite.color.to_srgba();
            (transform.translation.truncate() - segment.translation.truncate()).length() <= 0.01
                && transform.translation.z < segment.layer_z
                && color.alpha >= 0.5
                && color.red + color.green + color.blue < 1.0
        })
    })
}

fn fnv1a64(bytes: &[u8]) -> String {
    let mut hash = 0xcbf29ce484222325u64;
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    format!("{hash:016x}")
}

fn glyph_raster_signature(app: &mut App) -> (Vec<[u8; 4]>, String) {
    let world = app.world_mut();
    let mut layers = Vec::new();
    let mut glyphs = world.query::<(&PixelWorldMicroDepotVisual, &Sprite, &Transform)>();
    layers.extend(glyphs.iter(world).map(|(_, sprite, transform)| {
        (
            sprite
                .custom_size
                .expect("glyph sprites have explicit dimensions"),
            sprite.color.to_srgba(),
            transform.rotation.to_euler(EulerRot::XYZ).2,
            transform.translation.z,
        )
    }));
    let mut details = world.query::<(&PixelWorldMicroDepotDetailVisual, &Sprite, &Transform)>();
    layers.extend(details.iter(world).map(|(_, sprite, transform)| {
        (
            sprite
                .custom_size
                .expect("glyph sprites have explicit dimensions"),
            sprite.color.to_srgba(),
            transform.rotation.to_euler(EulerRot::XYZ).2,
            transform.translation.z,
        )
    }));
    layers.sort_by(|left, right| {
        left.3
            .partial_cmp(&right.3)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    let mut pixels = vec![[0_u8; 4]; 32 * 32];
    for (size, color, rotation, _) in layers {
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
    let bytes = pixels.iter().flatten().copied().collect::<Vec<_>>();
    let signature = fnv1a64(&bytes);
    (pixels, signature)
}

#[test]
fn micro_depot_glyphs_are_status_distinct_non_interactive_and_cleanup_stale_entities() {
    let mut state = sample_render_state(12_000.0);
    state.micro_depot_facilities = vec![
        depot("active", "active"),
        depot("suspended", "suspended"),
        depot("depleted", "depleted"),
    ];
    let mut app = render_test_app(state);
    let world = app.world_mut();
    let mut glyphs = world.query::<(&PixelWorldMicroDepotVisual, &Sprite, &Transform)>();
    let rendered = glyphs
        .iter(world)
        .map(|(visual, sprite, transform)| {
            (
                visual.id.clone(),
                (
                    sprite.color.to_srgba(),
                    transform.rotation.to_scaled_axis().z,
                ),
            )
        })
        .collect::<BTreeMap<_, _>>();
    assert_eq!(rendered.len(), 3);
    assert_ne!(
        rendered["micro_depot:active"].0,
        rendered["micro_depot:suspended"].0
    );
    assert_ne!(
        rendered["micro_depot:active"].1,
        rendered["micro_depot:suspended"].1
    );
    assert!(
        world
            .resource::<BevyRuntimeState>()
            .hit_regions
            .iter()
            .all(|region| region.kind != "micro_depot"),
        "facility glyphs must not add an interaction region"
    );

    world
        .resource_mut::<BevyRuntimeState>()
        .render_state
        .as_mut()
        .expect("test render state")
        .micro_depot_facilities
        .truncate(1);
    app.update();
    let world = app.world_mut();
    let mut remaining = world.query::<&PixelWorldMicroDepotVisual>();
    let mut details = world.query::<&PixelWorldMicroDepotDetailVisual>();
    assert_eq!(remaining.iter(world).count(), 1);
    assert_eq!(details.iter(world).count(), 1);
}

#[test]
fn micro_depot_statuses_have_shape_sensitive_raster_signatures() {
    let raster_for = |status: &str| {
        let mut state = sample_render_state(12_000.0);
        state.selection = None;
        state.micro_depot_facilities = vec![depot(status, status)];
        let mut app = render_test_app(state);
        let (pixels, signature) = glyph_raster_signature(&mut app);
        assert!(
            pixels.iter().any(|pixel| match status {
                "active" => *pixel == [52, 211, 153, 255],
                "suspended" => *pixel == [251, 191, 36, 255],
                "depleted" => *pixel == [248, 113, 113, 255],
                _ => false,
            }),
            "{status} must contribute its published status color to the raster"
        );
        signature
    };
    let active = raster_for("active");
    let suspended = raster_for("suspended");
    let depleted = raster_for("depleted");
    assert_ne!(active, suspended);
    assert_ne!(suspended, depleted);
    assert_ne!(active, depleted);
}

#[test]
fn micro_depot_service_radius_outline_is_noninteractive_and_cleans_up_when_invalid() {
    let mut state = sample_render_state(12_000.0);
    state.micro_depot_facilities = vec![depot_with_service_radius("active", 240_000.0)];
    let mut app = render_test_app(state);
    let world = app.world_mut();
    let mut glyphs = world.query::<(&PixelWorldMicroDepotVisual, &Transform)>();
    let glyph_z = glyphs
        .iter(world)
        .find(|(visual, _)| visual.id == "micro_depot:active")
        .map(|(_, transform)| transform.translation.z)
        .expect("active depot glyph renders");
    let mut outlines = world.query::<(
        &PixelWorldMicroDepotServiceRadiusVisual,
        &Sprite,
        &Transform,
    )>();
    let rendered = outlines
        .iter(world)
        .filter(|(visual, _, _)| visual.id == "micro_depot:active")
        .collect::<Vec<_>>();
    assert_eq!(
        rendered.len(),
        8,
        "a positive published service radius needs a visible world-scale outline"
    );
    assert!(rendered.iter().all(|(_, sprite, transform)| {
        sprite.color.to_srgba().alpha < 0.5 && transform.translation.z < glyph_z
    }));
    assert!(
        world
            .resource::<BevyRuntimeState>()
            .hit_regions
            .iter()
            .all(|region| region.kind != "micro_depot_service_radius"),
        "service-radius outlines must remain non-interactive"
    );

    world
        .resource_mut::<BevyRuntimeState>()
        .render_state
        .as_mut()
        .expect("test render state")
        .micro_depot_facilities[0]
        .service_radius_cm = 0.0;
    app.update();
    let world = app.world_mut();
    let mut outlines = world.query::<&PixelWorldMicroDepotServiceRadiusVisual>();
    assert_eq!(
        outlines.iter(world).count(),
        0,
        "zero radius cleans up its outline"
    );

    world
        .resource_mut::<BevyRuntimeState>()
        .render_state
        .as_mut()
        .expect("test render state")
        .micro_depot_facilities[0]
        .service_radius_cm = 240_000.0;
    app.update();
    assert_eq!(service_radius_segments(&mut app).len(), 8);
    app.world_mut()
        .resource_mut::<BevyRuntimeState>()
        .render_state
        .as_mut()
        .expect("test render state")
        .world_bounds = None;
    app.update();
    let world = app.world_mut();
    let mut outlines = world.query::<&PixelWorldMicroDepotServiceRadiusVisual>();
    assert_eq!(
        outlines.iter(world).count(),
        0,
        "missing world bounds cannot leave a stale service-radius outline"
    );
}

#[test]
fn micro_depot_service_radius_reuses_entities_across_static_reconcile_and_reorder() {
    let mut state = sample_render_state(12_000.0);
    state.selection = None;
    state.micro_depot_facilities = vec![
        depot_with_service_radius("first", 240_000.0),
        depot_with_service_radius("second", 120_000.0),
    ];
    let mut app = render_test_app(state);
    let initial = service_radius_segments(&mut app);
    assert_eq!(initial.len(), 16);

    reconcile_depot_scene(&mut app);
    assert_eq!(service_radius_segments(&mut app), initial);

    {
        let mut runtime = app.world_mut().resource_mut::<BevyRuntimeState>();
        let state = runtime.render_state.as_mut().expect("render state");
        state.micro_depot_facilities.reverse();
        state.agents[0]
            .status_badges
            .push("unrelated update".into());
    }
    reconcile_depot_scene(&mut app);
    assert_eq!(
        service_radius_segments(&mut app),
        initial,
        "DTO order and unrelated updates must not recreate or move radius segments"
    );
}

#[test]
fn micro_depot_service_radius_updates_geometry_without_churning_on_animation_frames() {
    let mut state = sample_render_state(12_000.0);
    state.selection = None;
    state.micro_depot_facilities = vec![depot_with_service_radius("active", 240_000.0)];
    let mut app = render_test_app(state);
    let initial = service_radius_segments(&mut app);
    {
        let mut runtime = app.world_mut().resource_mut::<BevyRuntimeState>();
        runtime.camera.zoom *= 1.5;
        runtime.camera.pan_x_px += 10.0;
    }
    reconcile_depot_scene(&mut app);
    let moved = service_radius_segments(&mut app);
    assert_ne!(moved, initial, "camera changes update outline geometry");
    for (key, segment) in &initial {
        assert_eq!(moved[key].0, segment.0, "camera changes retain entity IDs");
    }

    {
        let mut runtime = app.world_mut().resource_mut::<BevyRuntimeState>();
        runtime
            .render_state
            .as_mut()
            .expect("render state")
            .micro_depot_facilities[0]
            .service_radius_cm = 120_000.0;
        runtime.needs_reconcile = false;
        runtime.animation_dirty = true;
    }
    app.update();
    assert_eq!(
        service_radius_segments(&mut app),
        moved,
        "animation-only updates must not reconcile depot geometry"
    );
    reconcile_depot_scene(&mut app);
    let resized = service_radius_segments(&mut app);
    assert_ne!(resized, moved, "static updates apply the changed radius");
    for (key, segment) in &initial {
        assert_eq!(
            resized[key].0, segment.0,
            "radius changes retain entity IDs"
        );
    }
}

#[test]
fn micro_depot_service_radius_cleans_up_stale_keys_and_unmounts_without_touching_survivors() {
    let mut state = sample_render_state(12_000.0);
    state.selection = None;
    state.micro_depot_facilities = vec![
        depot_with_service_radius("removed", 240_000.0),
        depot_with_service_radius("survivor", 120_000.0),
    ];
    let mut app = render_test_app(state);
    let expected_survivor = service_radius_segments(&mut app)
        .into_iter()
        .filter(|((id, _), _)| id == "micro_depot:survivor")
        .collect::<BTreeMap<_, _>>();
    assert_eq!(expected_survivor.len(), 8);
    app.world_mut()
        .resource_mut::<BevyRuntimeState>()
        .render_state
        .as_mut()
        .expect("render state")
        .micro_depot_facilities
        .remove(0);
    reconcile_depot_scene(&mut app);
    assert_eq!(service_radius_segments(&mut app), expected_survivor);

    app.world_mut().resource_mut::<BevyRuntimeState>().mounted = false;
    app.update();
    assert!(service_radius_segments(&mut app).is_empty());
}

#[test]
fn micro_depot_stock_runway_is_deterministic_across_healthy_low_and_zero_states() {
    let mut healthy = render_test_app(render_state_with_stock(8, 8));
    let mut low = render_test_app(render_state_with_stock(2, 8));
    let mut zero = render_test_app(render_state_with_stock(0, 8));

    let healthy_count = depot_stage_sprite_count(&mut healthy);
    let low_count = depot_stage_sprite_count(&mut low);
    let zero_count = depot_stage_sprite_count(&mut zero);
    assert!(
        healthy_count > 2,
        "healthy stock should include runway segments in addition to the glyph/detail pair: {healthy_count}"
    );
    assert!(
        low_count > 0 && zero_count > 0,
        "low and zero stock keep a visible runway cue: low={low_count}, zero={zero_count}"
    );

    let healthy_signature = depot_stage_raster_signature(&mut healthy);
    let low_signature = depot_stage_raster_signature(&mut low);
    let zero_signature = depot_stage_raster_signature(&mut zero);
    assert_ne!(healthy_signature, low_signature);
    assert_ne!(low_signature, zero_signature);
    assert_ne!(healthy_signature, zero_signature);

    for app in [&mut healthy, &mut low, &mut zero] {
        let world = app.world_mut();
        assert!(
            world
                .resource::<BevyRuntimeState>()
                .hit_regions
                .iter()
                .all(|region| region.kind != "micro_depot"),
            "stock runway must not add a depot hit region"
        );
    }
}

#[test]
fn micro_depot_stock_runway_reconciles_and_cleans_up_stale_visuals() {
    let mut app = render_test_app(render_state_with_stock(8, 8));
    let before = depot_stage_sprite_count(&mut app);
    assert!(
        before > 2,
        "healthy stock should include runway sprites in addition to the glyph/detail pair"
    );

    let world = app.world_mut();
    world
        .resource_mut::<BevyRuntimeState>()
        .render_state
        .as_mut()
        .expect("stock runway render state")
        .micro_depot_facilities
        .clear();
    app.update();

    let after = depot_stage_sprite_count(&mut app);
    assert_eq!(after, 0, "stale stock runway sprites must be despawned");
    let world = app.world_mut();
    let mut glyphs = world.query::<&PixelWorldMicroDepotVisual>();
    let mut details = world.query::<&PixelWorldMicroDepotDetailVisual>();
    assert_eq!(glyphs.iter(world).count(), 0);
    assert_eq!(details.iter(world).count(), 0);
}

#[test]
fn micro_depot_stock_runway_segments_have_narrow_screen_minimum_and_dark_backing() {
    let mut app = render_test_app(render_state_with_stock(8, 8));
    let segments = runway_probes(&mut app);
    assert_eq!(
        segments.len(),
        4,
        "healthy stock keeps four runway segments"
    );
    assert_eq!(
        segments
            .iter()
            .map(|segment| segment.segment.expect("runway probe has a segment id"))
            .collect::<BTreeSet<_>>()
            .len(),
        4,
        "runway segment identity remains stable"
    );
    assert!(
        segments.iter().all(|segment| {
            let footprint = rotated_screen_footprint(segment.size, segment.rotation);
            footprint.x >= 4.0 && footprint.y >= 2.5
        }),
        "every runway segment needs at least a 4x2.5px screen proxy footprint"
    );
    assert!(
        segments
            .iter()
            .all(|segment| (segment.layer_z - segments[0].layer_z).abs() <= 0.001),
        "runway segments share one stable treatment layer"
    );
    assert!(
        has_dark_backing_for_each_segment(&mut app, &segments),
        "each runway segment needs a dark backing/outline at the same screen position"
    );
    let world = app.world_mut();
    assert!(
        world
            .resource::<BevyRuntimeState>()
            .hit_regions
            .iter()
            .all(|region| region.kind != "micro_depot"),
        "runway geometry must remain non-interactive"
    );
}

#[test]
fn micro_depot_zero_stock_runway_has_color_independent_crossing_geometry() {
    let mut app = render_test_app(render_state_with_stock(0, 8));
    let runway = runway_probes(&mut app);
    assert_eq!(runway.len(), 4, "zero stock keeps four runway slots");
    let diagonals = stock_stage_probes(&mut app)
        .into_iter()
        .filter(|probe| probe.rotation.sin().abs() >= 0.25 && probe.rotation.cos().abs() >= 0.25)
        .collect::<Vec<_>>();
    assert!(
        diagonals.iter().enumerate().any(|(index, left)| {
            diagonals.iter().skip(index + 1).any(|right| {
                (left.rotation.signum() != right.rotation.signum())
                    && (left.translation.truncate() - right.translation.truncate()).length() <= 0.1
            })
        }),
        "zero stock needs a visible double-diagonal/X shape at one slot, independent of color"
    );
    let world = app.world_mut();
    assert!(
        world
            .resource::<BevyRuntimeState>()
            .hit_regions
            .iter()
            .all(|region| region.kind != "micro_depot"),
        "zero-stock runway geometry must remain non-interactive"
    );
}

#[test]
fn micro_depot_stock_throughput_texts_use_compact_neutral_fallback_and_reconcile() {
    let mut app = render_test_app(render_state_with_stock_throughput_texts());
    let runway = runway_probes(&mut app);
    let labels = stock_throughput_texts(&mut app);
    let mut rendered = labels
        .iter()
        .map(|(text, _)| text.clone())
        .collect::<Vec<_>>();
    rendered.sort();
    assert_eq!(
        rendered,
        vec!["0/8", "2/8", "8/8"],
        "known positive throughput limits render compact neutral stock text"
    );
    assert!(
        labels.iter().all(|(_, position)| {
            position.z >= MICRO_DEPOT_LAYER_Z + 0.001
                && position.z <= MICRO_DEPOT_LAYER_Z + 0.1
                && runway.iter().any(|segment| {
                    (position.truncate() - segment.translation.truncate()).length() <= 24.0
                })
        }),
        "throughput text stays in the depot stock visual lane and layer"
    );
    let world = app.world_mut();
    assert!(
        world
            .resource::<BevyRuntimeState>()
            .hit_regions
            .iter()
            .all(|region| region.kind != "micro_depot"),
        "throughput text must remain non-interactive"
    );

    {
        let world = app.world_mut();
        let mut runtime = world.resource_mut::<BevyRuntimeState>();
        let facilities = &mut runtime
            .render_state
            .as_mut()
            .expect("throughput text render state")
            .micro_depot_facilities;
        let low = facilities
            .iter_mut()
            .find(|facility| facility.id == "micro_depot:throughput-low")
            .expect("low throughput depot");
        low.available_units_by_kind.insert("data".to_string(), 0);
        low.throughput_remaining_units = 0;
    }
    app.update();

    let mut updated = stock_throughput_texts(&mut app)
        .into_iter()
        .map(|(text, _)| text)
        .collect::<Vec<_>>();
    updated.sort();
    assert_eq!(
        updated,
        vec!["0/8", "0/8", "8/8"],
        "throughput text updates in place without stale labels"
    );
    let world = app.world_mut();
    assert!(
        world
            .resource::<BevyRuntimeState>()
            .hit_regions
            .iter()
            .all(|region| region.kind != "micro_depot")
    );

    world
        .resource_mut::<BevyRuntimeState>()
        .render_state
        .as_mut()
        .expect("throughput text render state")
        .micro_depot_facilities
        .clear();
    app.update();
    assert_eq!(
        stock_throughput_texts(&mut app).len(),
        0,
        "removed depots leave no stale throughput text"
    );
    let world = app.world_mut();
    assert!(
        world
            .resource::<BevyRuntimeState>()
            .hit_regions
            .iter()
            .all(|region| region.kind != "micro_depot")
    );
}

#[test]
fn micro_depot_stock_throughput_texts_meet_readable_font_and_backing_gate() {
    let mut app = render_test_app(render_state_with_stock_throughput_texts());
    let labels = stock_throughput_text_styles(&mut app);
    assert_eq!(
        labels.len(),
        3,
        "known throughput states render three labels"
    );
    assert!(
        labels.iter().all(|(_, font_size, _)| *font_size >= 12.0),
        "throughput labels need a narrow-screen-readable font size of at least 12px"
    );

    let backings = stock_throughput_backings(&mut app);
    assert_eq!(
        backings.len(),
        labels.len(),
        "every throughput label needs one opaque dark backing sprite"
    );
    assert!(
        labels.iter().all(|(_, _, label_position)| {
            backings.iter().any(|(_, backing_position)| {
                backing_position.z < label_position.z
                    && (backing_position.truncate() - label_position.truncate()).length() <= 2.0
            })
        }),
        "throughput backing stays below and adjacent to its text"
    );

    let world = app.world_mut();
    assert!(
        world
            .resource::<BevyRuntimeState>()
            .hit_regions
            .iter()
            .all(|region| region.kind != "micro_depot")
    );

    {
        let world = app.world_mut();
        let mut runtime = world.resource_mut::<BevyRuntimeState>();
        let low = runtime
            .render_state
            .as_mut()
            .expect("throughput text render state")
            .micro_depot_facilities
            .iter_mut()
            .find(|facility| facility.id == "micro_depot:throughput-low")
            .expect("low throughput depot");
        low.available_units_by_kind.insert("data".to_string(), 0);
        low.throughput_remaining_units = 0;
    }
    app.update();

    let updated_labels = stock_throughput_text_styles(&mut app);
    assert_eq!(
        updated_labels.len(),
        3,
        "throughput font and backing reconcile without dropping labels"
    );
    assert!(
        updated_labels
            .iter()
            .all(|(_, font_size, _)| *font_size >= 12.0)
    );
    let updated_backings = stock_throughput_backings(&mut app);
    assert_eq!(updated_backings.len(), updated_labels.len());
    assert!(updated_labels.iter().all(|(_, _, label_position)| {
        updated_backings.iter().any(|(_, backing_position)| {
            backing_position.z < label_position.z
                && (backing_position.truncate() - label_position.truncate()).length() <= 2.0
        })
    }));
    let world = app.world_mut();
    assert!(
        world
            .resource::<BevyRuntimeState>()
            .hit_regions
            .iter()
            .all(|region| region.kind != "micro_depot")
    );

    world
        .resource_mut::<BevyRuntimeState>()
        .render_state
        .as_mut()
        .expect("throughput text render state")
        .micro_depot_facilities
        .clear();
    app.update();
    assert!(stock_throughput_text_styles(&mut app).is_empty());
    assert!(stock_throughput_backings(&mut app).is_empty());
    let world = app.world_mut();
    assert!(
        world
            .resource::<BevyRuntimeState>()
            .hit_regions
            .iter()
            .all(|region| region.kind != "micro_depot")
    );
}

#[test]
fn depot_authored_body_detail_and_native_readout_keep_one_unit_contract() {
    let mut state = sample_render_state(12000.);
    let mut facility = depot("pixels", "active");
    facility.throughput_limit_units_per_epoch = 10;
    facility.throughput_remaining_units = 7;
    state.micro_depot_facilities = vec![facility];
    let mut app = render_test_app(state);
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
    let (body, transform) = world
        .query_filtered::<(&Sprite, &Transform), With<PixelWorldMicroDepotVisual>>()
        .single(world)
        .unwrap();
    assert_eq!(body.custom_size, Some(Vec2::splat(16.)));
    let y = transform.translation.y;
    let detail = world
        .query_filtered::<&Sprite, With<PixelWorldMicroDepotDetailVisual>>()
        .single(world)
        .unwrap();
    assert_eq!(detail.custom_size, Some(Vec2::new(8., 3.)));
    let (text,transform,font)=world.query_filtered::<(&Text2d,&Transform,&TextFont),With<crate::render::micro_depot_facilities::PixelWorldMicroDepotThroughputTextVisual>>().single(world).unwrap();
    assert_eq!(text.0, "7/10");
    assert_eq!(font.font_size, FontSize::Px(12.));
    assert_eq!(transform.translation.y - y, 12.);
    assert!(
        world
            .resource::<BevyRuntimeState>()
            .hit_regions
            .iter()
            .all(|hit| hit.kind != "micro_depot")
    );
}
