use super::*;

#[test]
fn agent_profile_is_upright_while_location_is_a_footprint() {
    let mut app = render_test_app(sample_render_state(12_000.0));
    let world = app.world_mut();
    let agent = world
        .query_filtered::<&Sprite, With<PixelWorldAgentVisual>>()
        .single(world)
        .unwrap()
        .custom_size
        .unwrap();
    let location = world
        .query_filtered::<&Sprite, With<PixelWorldLocationVisual>>()
        .single(world)
        .unwrap()
        .custom_size
        .unwrap();
    assert!(
        agent.x == agent.y && agent.x % 16.0 == 0.0,
        "Agent canvas preserves the square shared atlas pixels"
    );
    assert!(
        location.x > location.y,
        "location must have a broad footprint"
    );
}

#[test]
fn minor_grid_is_subordinate_to_material_detail() {
    let layout = build_grid_layout(&CameraState::default(), 960.0, 540.0);
    assert!(grid_geometry(&layout).4.alpha() <= 0.06);
    let camera = CameraState {
        pan_x_px: 24.0,
        ..Default::default()
    };
    assert_ne!(
        layout,
        build_grid_layout(&camera, 960.0, 540.0),
        "one-cell pan must invalidate the major-line phase even when minor offsets repeat"
    );
}

fn material_route_fixture() -> RenderState {
    let mut state = sample_render_state_with_beacon_candidates("agent", "agent-0");
    state.agents[0].pos = Some(sample_position(800_000.0, 800_000.0));
    state.agents[0].position_source = AgentPositionSource::Snapshot;
    state.locations[0].pos = sample_position(1_300_000.0, 800_000.0);
    state.locations[1].pos = sample_position(1_900_000.0, 1_200_000.0);
    state.fragment_terrain[0].footprint_cm = 100_000.0;
    state.fragment_terrain[0].pos = sample_position(1_000_000.0, 1_200_000.0);
    let mut second = state.fragment_terrain[0].clone();
    second.id = "fragment-oxide".into();
    second.pos = sample_position(1_300_000.0, 1_200_000.0);
    second.dominant_compound = "iron_oxide".into();
    second.color = [190, 142, 103];
    state.fragment_terrain.push(second);
    state.micro_depot_facilities = vec![serde_json::from_value(serde_json::json!({
        "id": "facility-active", "facility_id": "depot", "location_id": "loc-0", "status": "active",
        "pos": { "x_cm": 1_900_000.0, "y_cm": 1_500_000.0, "z_cm": 0.0 }
    })).unwrap()];
    state.links = vec![
        Link {
            id: "published-primary".into(),
            kind: "route".into(),
            label: None,
            from: state.agents[0].pos.clone().unwrap(),
            to: state.locations[0].pos.clone(),
            emphasis: Some(1.0),
            status: None,
            source_class: None,
            freshness: None,
        },
        Link {
            id: "published-secondary".into(),
            kind: "route".into(),
            label: None,
            from: state.locations[0].pos.clone(),
            to: state.locations[1].pos.clone(),
            emphasis: Some(0.25),
            status: None,
            source_class: None,
            freshness: None,
        },
    ];
    state.visual_hotspots = vec![
        VisualHotspot {
            id: "transfer".into(),
            label: "Resource transfer".into(),
            kind: "resource_transfer".into(),
            pos: sample_position(1_000_000.0, 1_500_000.0),
            emphasis: Some(0.7),
            size_hint_px: Some(12.0),
        },
        VisualHotspot {
            id: "queue".into(),
            label: "Build queue".into(),
            kind: "build_queue".into(),
            pos: sample_position(1_300_000.0, 1_500_000.0),
            emphasis: Some(0.7),
            size_hint_px: Some(12.0),
        },
    ];
    state
}

#[test]
fn bevy_pixel_regression_exports_material_route_hierarchy() {
    let mut app = render_test_app(material_route_fixture());
    app.world_mut()
        .resource_mut::<BevyRuntimeState>()
        .camera
        .zoom = 1.0;
    app.update();
    let layers = collect_pixel_layers(&mut app);
    assert_eq!(layers.iter().filter(|l| l.kind == "link").count(), 2);
    let diagonal = layers
        .iter()
        .find(|l| l.kind == "link" && l.rotation.abs() > 0.1)
        .unwrap();
    let endpoint = Vec2::new(diagonal.center_x, diagonal.center_y)
        + Vec2::new(diagonal.rotation.cos(), diagonal.rotation.sin()) * diagonal.size.x * 0.5;
    let runtime = app.world().resource::<BevyRuntimeState>();
    let state = runtime.render_state.as_ref().unwrap();
    let expected = to_canvas_point(
        &state.links[1].to,
        state.world_bounds.as_ref().unwrap(),
        960.0,
        540.0,
        &runtime.camera,
    )
    .unwrap();
    assert!(
        endpoint.distance(Vec2::new(expected.0 as f32, expected.1 as f32)) < 0.1,
        "CPU readback must preserve the real Bevy route endpoint, including Y-axis inversion"
    );
    let (image, summary) = rasterize_pixel_regression(&mut app);
    assert!(summary.fragment_fleck_pixels > 0);
    assert!(summary.selected_agent_cue_pixels > 0);
    assert!(layers.iter().any(|l| l.kind == "hotspot_cue"));
    write_pixel_probe_if_requested(&image, &summary);
    app.world_mut()
        .resource_mut::<BevyRuntimeState>()
        .render_state
        .as_mut()
        .unwrap()
        .links
        .clear();
    app.update();
    assert!(
        !collect_pixel_layers(&mut app)
            .iter()
            .any(|l| l.kind == "link"),
        "empty scene must not invent routes"
    );
}

#[test]
fn grounding_is_passive_reused_and_removed_with_scene() {
    let mut app = render_test_app(material_route_fixture());
    let before = hit_regions(&mut app).len();
    let entities = |app: &mut App| {
        let world = app.world_mut();
        let mut ids = world
            .query_filtered::<Entity, With<grounding::PixelWorldGroundingVisual>>()
            .iter(world)
            .collect::<Vec<_>>();
        ids.sort();
        ids
    };
    let first = entities(&mut app);
    assert!(!first.is_empty());
    app.update();
    assert_eq!(first, entities(&mut app));
    assert_eq!(before, hit_regions(&mut app).len());
    app.world_mut()
        .resource_mut::<BevyRuntimeState>()
        .render_state = None;
    app.update();
    assert!(entities(&mut app).is_empty());
}

#[test]
fn paused_animation_keeps_event_shapes_static_and_above_core() {
    let mut app = render_test_app(material_route_fixture());
    let cue_geometry = |app: &mut App| {
        let world = app.world_mut();
        world
            .query::<(
                &hotspot_cues::PixelWorldHotspotCueVisual,
                &Sprite,
                &Transform,
            )>()
            .iter(world)
            .map(|(cue, sprite, transform)| {
                (
                    cue.id.clone(),
                    sprite.custom_size,
                    transform.translation,
                    transform.rotation,
                )
            })
            .collect::<Vec<_>>()
    };
    let first = cue_geometry(&mut app);
    assert!(first.iter().all(|(_, _, pos, _)| pos.z
        > hotspot_layer_z("resource_transfer") + hotspot_core::HOTSPOT_CORE_LAYER_Z_OFFSET));
    {
        let mut runtime = app.world_mut().resource_mut::<BevyRuntimeState>();
        runtime.reactive_scheduling = true;
        runtime.needs_reconcile = false;
        runtime.animation_dirty = false;
    }
    app.update();
    assert_eq!(first, cue_geometry(&mut app));
    app.world_mut()
        .resource_mut::<Time>()
        .advance_by(Duration::from_millis(70));
    app.world_mut()
        .resource_mut::<BevyRuntimeState>()
        .animation_dirty = true;
    app.update();
    assert_ne!(
        first,
        cue_geometry(&mut app),
        "animation tick must retain the existing pulse"
    );
}

#[test]
fn same_color_compound_update_reconciles_inset_without_resetting_camera() {
    let mut state = sample_render_state(20_000.0);
    state.fragment_terrain[0].dominant_compound = "compound_a".into();
    let mut app = render_test_app(state.clone());
    let inset = |app: &mut App| {
        let world = app.world_mut();
        world
            .query_filtered::<(Entity, &Sprite), With<PixelWorldFragmentInsetVisual>>()
            .single(world)
            .map(|(entity, sprite)| (entity, sprite.custom_size.unwrap()))
            .unwrap()
    };
    let before = inset(&mut app);
    let camera_signature = camera_content_signature(Some(&state));
    state.fragment_terrain[0].dominant_compound = "compound_b".into();
    {
        let mut runtime = app.world_mut().resource_mut::<BevyRuntimeState>();
        runtime.reactive_scheduling = true;
        runtime.animation_dirty = false;
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
                state: Some(Box::new(state.clone())),
            },
        );
        assert!(
            runtime.needs_reconcile,
            "compound-only update must reconcile"
        );
        assert!(!runtime.hit_regions_dirty);
        assert!(runtime.camera_user_override);
    }
    assert_eq!(camera_signature, camera_content_signature(Some(&state)));
    app.update();
    let after = inset(&mut app);
    assert_eq!(before.0, after.0, "reuse the existing inset entity");
    assert_eq!(before.1.x, after.1.x);
    assert_ne!(
        before.1.y, after.1.y,
        "compound-only update changes inset shape"
    );
}

#[test]
fn compound_shading_preserves_channels_without_saturation_or_overflow() {
    let mut patch = sample_render_state(20_000.0).fragment_terrain.remove(0);
    patch.color = [200, 100, 0];
    let inset = fragment_visuals::fragment_inset_color(&patch).to_srgba();
    let fleck = fragment_visuals::fragment_fleck_color(&patch).to_srgba();
    assert!((inset.red - 120.0 / 255.0).abs() < 0.001);
    assert!((inset.green - 60.0 / 255.0).abs() < 0.001);
    assert!((fleck.blue - 102.0 / 255.0).abs() < 0.001);
}
