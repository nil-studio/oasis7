use super::*;
use oasis7::world_service::private_goal_store::client_state::{ClientStateKind, ClientStateV1};
fn stage<T, E>(height: u64, name: &str, result: Result<T, E>) -> Option<T> {
    println!(
        "TASK_CONTEXT_DIAGNOSTIC height={height} stage={name} passed={}",
        result.is_ok()
    );
    result.ok()
}
#[test]
#[ignore = "explicit readonly failed prepared-world diagnosis; no private body output"]
fn private_task_context_failed_live_stage_diagnostic() {
    let ready = std::path::PathBuf::from(
        std::env::var_os("OASIS7_TASK_DIAGNOSTIC_READY")
            .expect("explicit failed ready file required"),
    );
    let ready: Value = serde_json::from_slice(&std::fs::read(ready).unwrap()).unwrap();
    let root = std::path::PathBuf::from(ready["root"].as_str().unwrap());
    let config: Value =
        serde_json::from_slice(&std::fs::read(ready["config"].as_str().unwrap()).unwrap()).unwrap();
    let key = oasis7::world_service::private_key_file::read_private_key_file(std::path::Path::new(
        config["signer_file"].as_str().unwrap(),
    ))
    .expect("diagnostic delegate key guard");
    let cache_key = Zeroizing::new(blake3::derive_key(
        "oasis7/private-task-client-key/v1",
        key.as_slice(),
    ));
    let device = PrivateGoalStore::open(
        std::path::Path::new(config["state_directory"].as_str().unwrap()),
        *cache_key,
    )
    .expect("diagnostic existing cache guard");
    let Some(ClientStateV1::ContextPrepare(original)) = device
        .read_client_state(ClientStateKind::Prepare, "play-fe567997-r1")
        .expect("diagnostic original cache record")
    else {
        panic!("diagnostic original request missing");
    };
    assert!(
        verify_prepare(&original).is_ok(),
        "diagnostic original signature/schema"
    );
    let records = root.join("records");
    let storage = root.join("store");
    let identity =
        world_service_read::identity(&root.join("world"), "w1").expect("diagnostic identity");
    let private = PrivateGoalStore::open(&root.join("private-goals"), [7; 32])
        .expect("diagnostic existing private store guard");
    let service_key = hex::encode([9; 32]);
    let signer = FeedbackSubmitSigner {
        public_key_hex: sign_read_request("service", (), &service_key)
            .unwrap()
            .subject_public_key,
        private_key_hex: service_key,
    };
    let runtime = Arc::new(Mutex::new(NodeRuntime::new(
        oasis7_node::NodeConfig::new("diagnostic", "w1", oasis7_node::NodeRole::Sequencer).unwrap(),
    )));
    let service = Service {
        restore_context: world_service_read::RestoreContext::default(),
        records: &records,
        storage: &storage,
        identity,
        signer: &signer,
        runtime: &runtime,
        guarded: None,
    };
    let mut completed = 0;
    for height in [40, 50, 60] {
        let Some(pin) = stage(
            height,
            "current_cas_and_history",
            service.pin_at_height(height),
        ) else {
            continue;
        };
        if stage(
            height,
            "executor_fence",
            game_task::validate_task_executor(
                &pin.world,
                &original.request.fence,
                &original.subject_public_key,
            ),
        )
        .is_none()
        {
            continue;
        }
        let Some(task) = stage(
            height,
            "validated_pinned_task",
            validate_pinned_task(
                &service,
                &pin,
                &original.request.fence,
                &original.subject_public_key,
                true,
            ),
        ) else {
            continue;
        };
        let Some(Some(control)) = stage(
            height,
            "task_control",
            game_task::current_control(&pin.world, "agent-a"),
        ) else {
            continue;
        };
        let Some(Some(publication)) = stage(
            height,
            "active_publish",
            game_task::active_publish(&pin.world, "agent-a"),
        ) else {
            continue;
        };
        let Some(historical) = stage(
            height,
            "activation_cas_and_history",
            service.pin_at_height(publication.committed_height),
        ) else {
            continue;
        };
        let key = correlation::key_digest(&publication.request.correlation.key).unwrap();
        assert_eq!(
            historical
                .world
                .capability_revocation_state()
                .world_service_results
                .get(&key),
            Some(&serde_json::to_value(&publication).unwrap()),
            "diagnostic exact public activation proof"
        );
        let WorldServicePayloadV1::GameGoalPublish(public) = &publication.request.signed_payload
        else {
            panic!("diagnostic activation kind");
        };
        let Some(active_goal) = stage(
            height,
            "encrypted_goal_lookup",
            private.get_prepared(&public.request),
        ) else {
            continue;
        };
        let Some(authority) = stage(
            height,
            "actual_provider_authority",
            pin.world.canonical_agent_service_context("agent-a"),
        ) else {
            continue;
        };
        let Some(runtime_binding) = stage(
            height,
            "actual_runtime_binding",
            pin.world.current_cognition_runtime_binding(),
        ) else {
            continue;
        };
        let Some(captured_base_binding) = stage(
            height,
            "actual_cognition_base",
            pin.world.current_cognition_base_binding(),
        ) else {
            continue;
        };
        println!(
            "TASK_CONTEXT_DIAGNOSTIC height={height} state_time={} runtime_tick={} catalog_tick={} catalog_until={}",
            pin.world.state().time,
            runtime_binding.base_tick,
            authority.capability_catalog.logical_tick,
            authority.capability_catalog.valid_until_tick
        );
        let output = GoalExecutionSnapshotV1 {
            schema_revision: 1,
            control,
            read_commit: pin.commit.clone(),
            activation_commit: historical.commit,
            active_goal,
            actor_observation_schema: "world-agent-state/v1".into(),
            actor_observation: serde_json::to_value(&pin.world.state().agents["agent-a"]).unwrap(),
            provider_authority: Some(authority),
            native_authority: None,
            runtime_binding,
            captured_base_binding,
        };
        if stage(height, "complete_snapshot_validation", output.validate()).is_none() {
            continue;
        }
        assert!(
            snapshot(&service, &task, &original.request.fence, None, &private).is_ok(),
            "diagnostic production snapshot"
        );
        let scratch = std::env::temp_dir().canonicalize().unwrap().join(format!(
            "oasis7-private-context-diagnostic-{}-{height}",
            std::process::id()
        ));
        assert!(!scratch.exists());
        let scratch_store = PrivateGoalStore::open(&scratch, [81; 32]).unwrap();
        let prepared = stage(
            height,
            "context_create_encrypt_publish",
            scratch_store.prepare_task_context(&original, output),
        );
        if let Some(prepared) = prepared {
            assert!(
                check_record_live(&pin, &prepared, None).is_ok(),
                "diagnostic unchanged live authority"
            );
            assert!(
                prepared.validate_against(&original).is_ok(),
                "diagnostic exact original consumer proof"
            );
            completed += 1;
        }
        drop(scratch_store);
        std::fs::remove_dir_all(scratch).unwrap();
    }
    assert_eq!(
        completed, 3,
        "all actual stop-before snapshots must prepare complete private contexts"
    );
}
