use super::*;

#[test]
fn bevy_ecs_reconciles_agent_cores_without_semantic_or_hit_region_changes() {
    let mut app = render_test_app(sample_render_state_with_beacon_candidates(
        "agent", "agent-0",
    ));
    let first = visual_probe_summary(&mut app);

    assert_eq!(first.agents.len(), 2);
    assert_eq!(first.agent_cores.len(), 2);
    assert_eq!(first.agent_core_entity_count, 2);
    assert_eq!(first.hit_regions, 4, "cores must not add hit regions");
    let render_state = sample_render_state_with_beacon_candidates("agent", "agent-0");
    for (base, core) in first.agents.iter().zip(&first.agent_cores) {
        assert_eq!(core.id, base.id);
        let agent = render_state
            .agents
            .iter()
            .find(|agent| agent.id == core.id)
            .expect("core agent source");
        assert_eq!(
            core.size_px,
            agent_core_size_px(agent, core.id == "agent-0")
        );
        assert_eq!(core.z, base.z - AGENT_CORE_LAYER_Z_OFFSET);
        assert_eq!(core.x, base.x);
        assert_eq!(core.y, base.y);
    }
    let world = app.world_mut();
    let mut core_query = world.query::<(&PixelWorldAgentCoreVisual, &Sprite)>();
    for (_, sprite) in core_query.iter(world) {
        assert_eq!(sprite.color, AGENT_CORE_COLOR);
    }

    app.update();
    assert_eq!(
        visual_probe_summary(&mut app).agent_core_entity_count,
        2,
        "a consecutive visible reconcile must reuse each agent core"
    );

    {
        let mut runtime = app.world_mut().resource_mut::<BevyRuntimeState>();
        let mut removed = sample_render_state(12_000.0);
        removed.agents.clear();
        runtime.render_state = Some(removed);
        runtime.render_version += 1;
        runtime.hit_regions_dirty = true;
    }
    app.update();
    let removed = visual_probe_summary(&mut app);
    assert!(removed.agent_cores.is_empty());
    assert_eq!(removed.agent_core_entity_count, 0);
    assert_eq!(
        removed.hit_regions, 1,
        "only the location hit region remains"
    );

    {
        let mut runtime = app.world_mut().resource_mut::<BevyRuntimeState>();
        runtime.render_state = None;
        runtime.render_version += 1;
    }
    app.update();
    let no_state = visual_probe_summary(&mut app);
    assert!(no_state.agent_cores.is_empty());
    assert_eq!(no_state.agent_core_entity_count, 0);
}

#[test]
fn agent_sprite_sizes_remain_discrete_across_animation() {
    let agent = sample_render_state(12_000.0).agents.remove(0);
    let base_at_start = agent_visual_style(&agent, true, 0.0, 0).size_px;
    let base_later = agent_visual_style(&agent, true, 120.0, 0).size_px;
    let core_at_start = agent_core_size_px(&agent, true);
    let core_later = agent_core_size_px(&agent, true);

    assert_eq!(
        base_at_start, base_later,
        "shared sprite must preserve its pixel silhouette"
    );
    assert_eq!(
        core_at_start, core_later,
        "core size must remain invariant across animation times"
    );
    assert_eq!(core_at_start, 6.912);
}

#[test]
fn shared_agent_texture_is_decoded_once_and_reused_after_reconcile() {
    let mut app = render_test_app(sample_render_state(12_000.0));
    let handle = {
        let world = app.world_mut();
        world
            .query_filtered::<&Sprite, With<PixelWorldAgentVisual>>()
            .single(world)
            .unwrap()
            .image
            .clone()
    };
    let image = app
        .world()
        .resource::<Assets<Image>>()
        .get(&handle)
        .unwrap();
    assert_eq!(image.size(), bevy::math::UVec2::new(16, 16));
    assert_eq!(image.sampler, bevy::image::ImageSampler::nearest());
    let bytes = image.data.as_ref().unwrap();
    assert!(bytes.chunks_exact(4).any(|pixel| pixel[3] == 0));
    assert!(bytes.chunks_exact(4).any(|pixel| pixel[3] == 255));
    let count = app.world().resource::<Assets<Image>>().len();
    app.update();
    assert_eq!(app.world().resource::<Assets<Image>>().len(), count);
    let world = app.world_mut();
    assert_eq!(
        world
            .query_filtered::<&Sprite, With<PixelWorldAgentVisual>>()
            .single(world)
            .unwrap()
            .image,
        handle
    );
}
