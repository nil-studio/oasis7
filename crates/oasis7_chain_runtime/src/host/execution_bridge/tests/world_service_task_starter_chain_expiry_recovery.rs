use super::*;
use oasis7::world_service::client::{
    RemoteWorldServiceClient, WorldServiceClientConfig, WorldServiceClientError,
};
use oasis7::world_service::private_task_context::{
    TASK_CONTEXT_PREPARE_PATH, TASK_CONTEXT_READ_PATH, TaskContextGetV1, TaskContextPrepareV1,
};
use oasis7::world_service::{SchedulerIntentV1, SchedulerOperationV1};
use oasis7_agent_api::game::GameTaskFenceV1;
use oasis7_client_api::world_service::LookupIntentRequest;
use std::fs;
use std::time::{Duration, Instant};

#[test]
fn starter_context_live_conflict_fails_closed_then_expiry_aborts_and_reprepares() {
    let _scenario_permit = super::super::private_task_resource_permit();
    let ready = std::env::temp_dir().canonicalize().unwrap().join(format!(
        "oasis7-starter-expiry-recovery-{}-{}.json",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let deadline = Instant::now() + Duration::from_secs(90);
    std::thread::scope(|scope| {
        let host_ready = ready.clone();
        let host_permit = _scenario_permit.clone();
        let host = scope.spawn(move || {
            let _host_permit = host_permit;
            super::super::run_prepared_world_service_kind(
                host_ready,
                Duration::from_secs(1),
                false,
                None,
                false,
                Duration::from_secs(30),
                super::super::PreparedWorldKind::StarterChain,
                Duration::ZERO,
            )
        });
        while !ready.exists() {
            assert!(
                Instant::now() < deadline,
                "expiry fixture host readiness timeout"
            );
            std::thread::sleep(Duration::from_millis(20));
        }
        let metadata: serde_json::Value =
            serde_json::from_slice(&fs::read(&ready).unwrap()).unwrap();
        let root = std::path::PathBuf::from(metadata["root"].as_str().unwrap());
        struct StopHost(std::path::PathBuf);
        impl Drop for StopHost {
            fn drop(&mut self) {
                let _ = fs::write(self.0.join("service-stop"), b"stop");
            }
        }
        let stop_host = StopHost(root.clone());
        let config: serde_json::Value =
            serde_json::from_slice(&fs::read(metadata["config"].as_str().unwrap()).unwrap())
                .unwrap();
        let world = WorldIdentity {
            world_id: "w1".into(),
            genesis_digest: "fixture-genesis-v1".into(),
        };
        let delegate = hex::encode([73; 32]);
        let client = RemoteWorldServiceClient::new(WorldServiceClientConfig {
            formal_read_trust: None,
            endpoint: metadata["endpoint"].as_str().unwrap().into(),
            trusted_service_public_key: config["service_public_key"].as_str().unwrap().into(),
            expected_world: world.clone(),
            scope_id: "agent:agent-a".into(),
            read_private_key_hex: delegate.clone(),
            timeout: Duration::from_secs(5),
            max_response_bytes: 2 * 1024 * 1024,
        })
        .unwrap();
        let pin = || {
            super::super::super::super::super::world_service_read::pin(
                &root.join("records"),
                &root.join("store"),
                &world,
                None,
            )
            .unwrap()
        };
        let mut categories = CleanupCategories::default();
        let initial_control = game_task::current_control(&pin().world, "agent-a")
            .unwrap()
            .unwrap();
        let first_proof = sign_read_request(
            TASK_CONTEXT_PREPARE_PATH,
            TaskContextPrepareV1 {
                schema_revision: 1,
                fence: GameTaskFenceV1::from_control(&initial_control),
                client_operation_id: "starter-expiry-live-conflict".into(),
                client_nonce: format!("{:064x}", 0x5a01_u64),
            },
            &delegate,
        )
        .unwrap();
        let original_prepared =
            super::super::retry_private(|| client.prepare_task_context(&first_proof), deadline);
        let original_identity = original_prepared.identity.clone();
        let original_get = sign_read_request(
            TASK_CONTEXT_READ_PATH,
            TaskContextGetV1 {
                schema_revision: 1,
                context: original_identity.clone(),
                prepare_operation_id: first_proof.request.client_operation_id.clone(),
            },
            &delegate,
        )
        .unwrap();

        let live_tick = pin().world.state().time;
        assert!(live_tick <= original_identity.valid_until_tick.0);
        let live_error = match client.read_task_context(&original_get) {
            Err(error) => error,
            Ok(_) => panic!("unprefixed live context unexpectedly returned a capture"),
        };
        assert!(matches!(
            &live_error,
            WorldServiceClientError::Rejected { status: 409, .. }
        ));
        assert!(!context_expired_conflict(
            &live_error,
            pin().world.state().time,
            original_identity.valid_until_tick.0
        ));
        let current_base = || pin().world.current_cognition_base_binding().unwrap();
        let current_tick = || current_base().base_tick;
        let current_fence = || {
            let control = game_task::current_control(&pin().world, "agent-a")
                .unwrap()
                .unwrap();
            GameTaskFenceV1::from_control(&control)
        };
        let live_retry = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            read_context_until_expiry(&client, &original_get, deadline, &current_tick)
        }));
        assert!(live_retry.is_err(), "a live 409 must fail closed");
        assert!(pin().world.state().time <= original_identity.valid_until_tick.0);

        let original_prefix_intent = public_prefix(&original_prepared).unwrap();
        let prepared_base_tick = original_prefix_intent.captured_base_binding.base_tick;
        let base_change_deadline = Instant::now() + Duration::from_secs(20);
        let comparison_world = loop {
            assert!(
                Instant::now() < base_change_deadline && Instant::now() < deadline,
                "canonical cognition base did not advance within the bounded live-context window"
            );
            let pinned = pin();
            let tick = pinned.world.state().time;
            assert!(
                tick <= original_identity.valid_until_tick.0,
                "base-binding comparison requires the original context lease to remain live"
            );
            let base = pinned.world.current_cognition_base_binding().unwrap();
            if base.base_tick > prepared_base_tick {
                break pinned.world;
            }
            std::thread::sleep(Duration::from_millis(25));
        };
        let comparison_base = comparison_world.current_cognition_base_binding().unwrap();
        assert!(comparison_base.base_tick > prepared_base_tick);
        let retains_original_context = |intent: &SchedulerIntentV1| {
            matches!(
                &intent.operation,
                SchedulerOperationV1::TaskProviderPrefix { context }
                    if context == &original_identity
            )
        };
        assert!(retains_original_context(&original_prefix_intent));
        let stale_signed =
            sign_read_request("scheduler", original_prefix_intent.clone(), &delegate).unwrap();
        let mut stale_world = comparison_world.clone();
        let stale_result = stale_world.apply_authenticated_scheduler(stale_signed);
        assert!(
            matches!(stale_result, Err(ref error) if error == "scheduler base binding changed"),
            "a prefix signed against the earlier base must fail on the advanced clone"
        );
        eprintln!(
            "TASK_EXPIRY_PREFIX phase=clone_old_base_rejected tick_advanced=true lease_live=true"
        );

        // A prepared prefix is pinned to its captured base. After the live 409 negative,
        // acquire a new prepared identity and submit its exact signed prefix.
        let (capture_prepared, capture_get) = prepare_prefixed_context(
            &client,
            &delegate,
            &world,
            &current_fence,
            ("starter-expiry-live-capture", 0x5a10),
            deadline,
            &mut categories,
        );
        let capture_identity = capture_prepared.identity.clone();
        eprintln!("TASK_EXPIRY_PREFIX phase=live_capture_committed exact_prepared_prefix=true");
        assert!(
            read_context_until_expiry(&client, &capture_get, deadline, &current_tick)
                .captured()
                .is_some(),
            "the fresh context should be readable after its exact prefix commits"
        );

        let expiry_deadline = (Instant::now() + Duration::from_secs(30)).min(deadline);
        while pin().world.state().time <= capture_identity.valid_until_tick.0 {
            assert!(
                Instant::now() < expiry_deadline && Instant::now() < deadline,
                "canonical task context did not expire within the bounded fixture window"
            );
            std::thread::sleep(Duration::from_millis(50));
        }
        let expired_error = match client.read_task_context(&capture_get) {
            Err(error) => error,
            Ok(_) => panic!("expired context unexpectedly returned a capture"),
        };
        assert!(matches!(
            &expired_error,
            WorldServiceClientError::Rejected { status: 409, .. }
        ));
        let expired_tick = pin().world.state().time;
        assert!(expired_tick > capture_identity.valid_until_tick.0);
        assert!(context_expired_conflict(
            &expired_error,
            expired_tick,
            capture_identity.valid_until_tick.0
        ));
        assert!(matches!(
            read_context_until_expiry(&client, &capture_get, deadline, &current_tick),
            ContextRead::Expired
        ));

        let abort = settle_expired_context(
            &client,
            &delegate,
            &world,
            &capture_identity,
            &current_base,
            deadline,
            &mut categories,
        );
        let abort_result = client
            .lookup(
                LookupIntentRequest {
                    contract_version: 1,
                    key: abort.correlation.key.clone(),
                },
                abort.signed_payload.clone(),
            )
            .expect("exact expired-context abort lookup must be available");
        assert!(matches!(
            abort_result.outcome,
            IntentOutcome::Committed { .. }
        ));

        let (fresh_prepared, fresh_get) = prepare_prefixed_context(
            &client,
            &delegate,
            &world,
            &current_fence,
            ("starter-expiry-after-abort", 0x5a20),
            deadline,
            &mut categories,
        );
        assert!(
            fresh_prepared.identity != capture_identity,
            "prepare after canonical expiry abort must mint a distinct task-context identity"
        );
        eprintln!("TASK_EXPIRY_PREFIX phase=expired_context_reprepared exact_prepared_prefix=true");
        assert!(
            read_context_until_expiry(&client, &fresh_get, deadline, &current_tick)
                .captured()
                .is_some(),
            "fresh identity should be readable after its prefix commits"
        );

        drop(stop_host);
        assert_eq!(host.join().unwrap(), root);
        fs::remove_file(ready).unwrap();
        fs::remove_dir_all(root).unwrap();
    });
}

impl ContextRead {
    fn captured(self) -> Option<CapturedTaskContextV1> {
        match self {
            Self::Captured(captured) => Some(*captured),
            Self::Expired => None,
        }
    }
}

fn prepare_prefixed_context<F: Fn() -> GameTaskFenceV1>(
    client: &RemoteWorldServiceClient,
    delegate: &str,
    world: &WorldIdentity,
    current_fence: &F,
    operation: (&str, u64),
    deadline: Instant,
    categories: &mut CleanupCategories,
) -> (PreparedTaskContextV1, SignedReadRequest<TaskContextGetV1>) {
    let (operation_prefix, nonce_base) = operation;
    for attempt in 0..8_u64 {
        assert!(
            Instant::now() < deadline,
            "task context prepare/prefix deadline exhausted"
        );
        let proof = sign_read_request(
            TASK_CONTEXT_PREPARE_PATH,
            TaskContextPrepareV1 {
                schema_revision: 1,
                fence: current_fence(),
                client_operation_id: format!("{operation_prefix}-{attempt}"),
                client_nonce: format!("{:064x}", nonce_base + attempt),
            },
            delegate,
        )
        .unwrap();
        let prepared =
            super::super::retry_private(|| client.prepare_task_context(&proof), deadline);
        let prefix_intent = public_prefix(&prepared).unwrap();
        assert_eq!(
            prefix_intent.captured_base_binding, prepared.snapshot.captured_base_binding,
            "prefix must preserve the base captured by its prepare"
        );
        let original = request(
            world,
            WorldServicePayloadV1::Scheduler(
                sign_read_request("scheduler", prefix_intent, delegate).unwrap(),
            ),
        );
        match super::super::settle_cleanup_original(client, &original, deadline, categories) {
            Ok(_) => {
                let get = sign_read_request(
                    TASK_CONTEXT_READ_PATH,
                    TaskContextGetV1 {
                        schema_revision: 1,
                        context: prepared.identity.clone(),
                        prepare_operation_id: proof.request.client_operation_id.clone(),
                    },
                    delegate,
                )
                .unwrap();
                return (prepared, get);
            }
            Err("canonical cleanup rejected") => {
                // A terminal lookup of these exact bytes is required before selecting a new
                // prepare identity. Unknown, pending, or history-unavailable stays blocked.
                assert_terminal(client, &original);
            }
            Err(error) => panic!("task context prefix did not settle: {error}"),
        }
    }
    panic!("bounded task context prepare/prefix attempts exhausted");
}
