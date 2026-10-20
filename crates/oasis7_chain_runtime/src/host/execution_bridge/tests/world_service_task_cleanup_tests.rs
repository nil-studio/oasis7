//! Cleanup authority, exact-original retry and scoped listener regressions.
use super::*;

#[test]
fn private_task_prepared_world_no_model_authority_cleanup() {
    let _probe_slot = prepared_world_probe_slot();
    let ready = std::env::temp_dir().canonicalize().unwrap().join(format!(
        "oasis7-task-cleanup-ready-{}-{}.json",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let root = run_prepared_world_service(
        ready.clone(),
        Duration::from_millis(100),
        true,
        None,
        false,
        Duration::from_secs(30),
    );
    let cleanup: serde_json::Value =
        serde_json::from_slice(&fs::read(root.join("task-playtest-cleanup.json")).unwrap())
            .unwrap();
    let operations = cleanup["authority_operations"].as_array().unwrap();
    assert_eq!(operations.len(), 2);
    assert!(
        operations
            .iter()
            .all(|result| result["outcome"]["status"] == "committed")
    );
    let identity =
        super::super::super::super::world_service_read::identity(&root.join("world"), "w1")
            .unwrap();
    let pinned = super::super::super::super::world_service_read::pin(
        &root.join("records"),
        &root.join("store"),
        &identity,
        None,
    )
    .unwrap();
    assert!(
        game_task::current_control(&pinned.world, "agent-a")
            .unwrap()
            .unwrap()
            .executor_binding
            .is_none()
    );
    assert!(
        pinned
            .world
            .capability_revocation_state()
            .agent_signer_delegations["agent-a"]
            .revoked
    );
    assert_eq!(
        pinned.world.state().agents["agent-a"].state.pos,
        oasis7_agent_api::world_action::GeoPos::new(0, 0, 0)
    );
    fs::remove_file(ready).unwrap();
    fs::remove_dir_all(root).unwrap();
}
#[test]
fn private_task_prepared_world_listener_guard_stops_scoped_thread_on_panic() {
    let stop = AtomicBool::new(false);
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        std::thread::scope(|scope| {
            let _guard = ListenerStopOnDrop(&stop);
            scope.spawn(|| {
                let until = Instant::now() + Duration::from_secs(2);
                while !stop.load(Ordering::SeqCst) {
                    assert!(Instant::now() < until, "scoped listener leaked after panic");
                    std::thread::sleep(Duration::from_millis(10));
                }
            });
            panic!("intentional fixture unwinding fault");
        })
    }));
    assert!(result.is_err());
    assert!(stop.load(Ordering::SeqCst));
}

#[test]
fn private_task_prepared_world_aged_cleanup_diagnostic() {
    let _probe_slot = prepared_world_probe_slot();
    let ready = std::env::temp_dir().canonicalize().unwrap().join(format!(
        "oasis7-task-aged-cleanup-ready-{}-{}.json",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    println!("TASK_AGED_CLEANUP_READY {}", ready.display());
    let root = run_prepared_world_service(
        ready.clone(),
        Duration::from_millis(100),
        false,
        Some(72),
        false,
        Duration::from_secs(30),
    );
    let cleanup: serde_json::Value =
        serde_json::from_slice(&fs::read(root.join("task-playtest-cleanup.json")).unwrap())
            .unwrap();
    println!("TASK_AGED_CLEANUP_CATEGORIES {}", cleanup["categories"]);
    assert_eq!(cleanup["authority_operations"].as_array().unwrap().len(), 2);
    assert!(
        cleanup["authority_operations"]
            .as_array()
            .unwrap()
            .iter()
            .all(|result| result["outcome"]["status"] == "committed")
    );
    fs::remove_file(ready).unwrap();
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn private_task_prepared_world_unknown_cleanup_retries_exact_original() {
    let _probe_slot = prepared_world_probe_slot();
    let ready = std::env::temp_dir().canonicalize().unwrap().join(format!(
        "oasis7-task-unknown-cleanup-ready-{}-{}.json",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    println!("TASK_UNKNOWN_CLEANUP_READY {}", ready.display());
    // One-second retained-original resend + four-second adverse admission +
    // a five-second worker tick + CI canonical commit total up to 4.6 seconds
    // (including retention/persistence).
    // Four ticks provide a bounded 20-second per-original settlement budget.
    let cadence = Duration::from_secs(5);
    let root = run_prepared_world_service_scenario(
        ready.clone(),
        cadence,
        true,
        None,
        true,
        cadence * 4,
        false,
        Duration::from_secs(4),
    );
    let cleanup: serde_json::Value =
        serde_json::from_slice(&fs::read(root.join("task-playtest-cleanup.json")).unwrap())
            .unwrap();
    println!("TASK_UNKNOWN_CLEANUP_CATEGORIES {}", cleanup["categories"]);
    assert_eq!(cleanup["authority_operations"].as_array().unwrap().len(), 2);
    assert!(
        cleanup["authority_operations"]
            .as_array()
            .unwrap()
            .iter()
            .all(|result| result["outcome"]["status"] == "committed")
    );
    assert_eq!(cleanup["categories"][0]["submit_unknown"], 1);
    let identity =
        super::super::super::super::world_service_read::identity(&root.join("world"), "w1")
            .unwrap();
    let pinned = super::super::super::super::world_service_read::pin(
        &root.join("records"),
        &root.join("store"),
        &identity,
        None,
    )
    .unwrap();
    let stops=pinned.world.capability_revocation_state().world_service_results.values().filter_map(|value|serde_json::from_value::<CanonicalIntentResultV1>(value.clone()).ok()).filter(|result|matches!(&result.request.signed_payload,WorldServicePayloadV1::GameExecutorControl(signed) if signed.request.client_operation_id=="playtest-cleanup-stop")).collect::<Vec<_>>();
    assert_eq!(stops.len(), 1);
    assert!(stops[0].rejected.is_none());
    assert_eq!(
        game_task::current_control(&pinned.world, "agent-a")
            .unwrap()
            .unwrap()
            .executor_epoch
            .0,
        2
    );
    fs::remove_file(ready).unwrap();
    fs::remove_dir_all(root).unwrap();
}
