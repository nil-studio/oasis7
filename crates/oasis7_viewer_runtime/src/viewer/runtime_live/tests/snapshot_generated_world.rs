use super::*;
use std::path::PathBuf;

fn unique_runtime_live_test_dir(label: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "oasis7-runtime-live-{label}-{}-{:?}",
        std::process::id(),
        std::thread::current().id()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    dir
}

fn write_generated_world_fixture(label: &str) -> PathBuf {
    let generated_world_dir = unique_runtime_live_test_dir(label);
    let sidecar_dir = generated_world_dir.join("generated-scenario-world");
    std::fs::create_dir_all(&sidecar_dir).expect("create generated sidecar dir");
    let config = WorldConfig::default();
    let init = WorldInitConfig::from_scenario(WorldScenario::AsteroidFragmentBootstrap, &config);
    let (kernel, _) =
        oasis7::simulator::initialize_kernel(config, init).expect("initialize kernel");
    kernel
        .save_to_dir(&sidecar_dir)
        .expect("save generated sidecar world");
    std::fs::write(
        generated_world_dir.join("world-generation-provenance.json"),
        r#"{"scenario_id":"asteroid_fragment_bootstrap","seed":42}"#,
    )
    .expect("write generated world provenance");
    generated_world_dir
}

#[test]
fn generated_world_sidecar_initializes_runtime_live_snapshot_map() {
    let _guard = runtime_provider_env_lock().lock().expect("env lock");
    let generated_world_dir = write_generated_world_fixture("generated-sidecar-snapshot-map");
    let mut server = ViewerRuntimeLiveServer::new(
        ViewerRuntimeLiveServerConfig::formal_release_default()
            .with_generated_world_dir(generated_world_dir.clone()),
    )
    .expect("runtime live from generated sidecar");

    let snapshot = server.compat_snapshot(None);
    assert!(snapshot.model.locations.len() > 1);
    assert!(
        snapshot
            .model
            .locations
            .values()
            .any(|location| location.fragment_profile.is_some()),
        "generated asteroid locations should survive viewer runtime-live mapping"
    );
    let first_agent = snapshot
        .model
        .agents
        .values()
        .next()
        .expect("generated sidecar seeds at least one agent");
    assert!(
        snapshot
            .model
            .locations
            .contains_key(&first_agent.location_id),
        "agent should keep a generated sidecar location ref"
    );
    assert!(
        !first_agent.location_id.starts_with("runtime-"),
        "viewer should not replace sidecar location ids with runtime fallback ids"
    );
    assert!(
        server.llm_sidecar.chunk_runtime.asteroid_fragment_enabled,
        "generated sidecar chunk runtime config should retain its initialization settings"
    );
    let chunk = server
        .seed_model
        .as_ref()
        .expect("generated sidecar seed model")
        .chunks
        .iter()
        .find_map(|(coord, state)| {
            matches!(state, oasis7_simulator::ChunkState::Generated).then_some(*coord)
        })
        .expect("generated sidecar chunk");
    let (public_key, private_key) = test_signer(252);
    register_runtime_session(
        &mut server,
        "generated-sidecar-refill-preview-player",
        Some(first_agent.id.as_str()),
        269,
        public_key.as_str(),
        private_key.as_str(),
    );
    let mut request = crate::viewer::FragmentRefillPreviewRequest {
        chunk,
        player_id: "generated-sidecar-refill-preview-player".to_string(),
        public_key: Some(public_key.clone()),
        auth: None,
    };
    request.auth = Some(
        crate::viewer::sign_fragment_refill_preview_auth_proof(
            &request,
            270,
            public_key.as_str(),
            private_key.as_str(),
        )
        .expect("sign generated sidecar refill preview"),
    );
    let quote = server
        .handle_fragment_refill_preview(request)
        .expect("generated sidecar refill preview");
    assert!(!quote.replenishment_enabled);
    assert_eq!(quote.next_replenish_tick, None);
    assert_eq!(quote.ticks_until_replenish, None);
    assert_eq!(quote.estimated_replenished_frag_count, 0);
    assert_eq!(
        quote.recommended_resource_action,
        "move_or_switch_material_route"
    );

    std::fs::remove_dir_all(generated_world_dir).expect("cleanup generated sidecar fixture");
}
