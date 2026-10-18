//! Two real processes reconcile one rejected Admit and its committed settlement.
use super::*;
use oasis7::viewer::ViewerRuntimeLiveServer;
use std::{io::Write, path::Path};
#[test]
fn real_tcp_hosted_rejected_wait_compensation_crash_recovers_original_identity() {
    application_harness::run_isolated_application_mode(
        false,
        false,
        false,
        true,
        false,
        false,
        "wait-rejection-crash",
    );
}
pub(super) fn observation(
    fixture: &Fixture,
    root: std::path::PathBuf,
) -> provider_metadata::DecisionObservation {
    let actual = application_hosted_wait_rejection::observation(fixture, root.clone());
    let gate = fixture.world_gate.clone();
    let world = fixture.client.config().expected_world.clone();
    let armed = AtomicBool::new(false);
    Arc::new(move |request| {
        actual(request);
        if !armed.swap(true, Ordering::SeqCst) {
            let origin: serde_json::Value =
                serde_json::from_slice(&fs::read(root.join("hosted-wait-origin.json")).unwrap())
                    .unwrap();
            gate.arm_compensation_settle_view(
                world.clone(),
                &request.agent_subject,
                origin["lease_id"].as_str().unwrap(),
            )
            .unwrap();
        }
    })
}
pub(super) fn start_selector(
    fixture: &Fixture,
) -> thread::JoinHandle<Result<SubmitIntentRequest<WorldServicePayloadV1>, String>> {
    let gate = fixture.world_gate.clone();
    let mut config = fixture.client.config().clone();
    config.scope_id = "agent:agent-a".into();
    thread::spawn(move || {
        let client = RemoteWorldServiceClient::new(config).map_err(|e| e.to_string())?;
        gate.install_compensation_settle_selector(&client)
    })
}
pub(super) fn verify(client: &RemoteWorldServiceClient, crash: bool) {
    if !crash {
        application_hosted_wait_rejection::verify(client);
        return;
    }
    let root = std::path::PathBuf::from(std::env::var("PRE2_METADATA_DIR").unwrap());
    fs::write(
        root.join("rejected-wait-single-turn"),
        b"same public cadence isolation",
    )
    .unwrap();
    application_fresh::preflight(client, &root);
    let server = ViewerRuntimeLiveServer::new(application_hosted::server_config(
        client,
        true,
        Duration::from_secs(60),
        Some(root.join(application_hosted_wait_rejection::STORE)),
    ))
    .unwrap();
    let shared = Arc::new(Mutex::new(server));
    let mut session = application_stream_boundaries::Session::start(&shared, false);
    assert!(session.warm());
    session
        .send(serde_json::json!({"type":"live_control","mode":{"mode":"play"},"request_id":1401}))
        .unwrap();
    session.send(serde_json::json!({"type":"hello_v2","client":"PRE2 compensation crash","version":2,"capabilities":[]})).unwrap();
    session
        .send(serde_json::json!({"type":"request_snapshot"}))
        .unwrap();
    assert!(session.snapshot_ordered(Duration::from_secs(3), true));
    assert_eq!(
        shared.lock().unwrap().test_agent_service_pump_status()["eligible"],
        true
    );
    let deadline = Instant::now() + Duration::from_secs(20);
    let mut paused = false;
    while Instant::now() < deadline {
        session.snapshot(Duration::from_millis(25));
        if !paused && root.join("wait-admit-actual-rejected.json").exists() {
            session.send(serde_json::json!({"type":"live_control","mode":{"mode":"pause"},"request_id":1402})).unwrap();
            session.send(serde_json::json!({"type":"hello_v2","client":"PRE2 issued compensation paused","version":2,"capabilities":[]})).unwrap();
            session
                .send(serde_json::json!({"type":"request_snapshot"}))
                .unwrap();
            assert!(session.snapshot_ordered(Duration::from_secs(3), true));
            assert_eq!(
                shared.lock().unwrap().test_agent_service_pump_status()["play_enabled"],
                false
            );
            paused = true;
        }
        if root.join("world-compensation-settle-view-started").exists() {
            assert!(paused, "issued compensation continues after genuine Pause");
            let bytes = fs::read(root.join(application_hosted_wait_rejection::STORE)).unwrap();
            let checkpoint: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
            assert_eq!(checkpoint["hosted_wait"]["stage"], "compensate_settle_view");
            assert!(checkpoint["hosted_wait"]["rejection"].is_object());
            assert!(checkpoint["hosted_wait"]["commit"].is_object());
            let started: serde_json::Value = serde_json::from_slice(
                &fs::read(root.join("world-compensation-settle-view-started")).unwrap(),
            )
            .unwrap();
            assert_eq!(started["min_commit"], checkpoint["hosted_wait"]["commit"]);
            application_hosted_wait_rejection::private_write(
                &root.join("compensation-crash-original-checkpoint.json"),
                &bytes,
            )
            .unwrap();
            println!(
                "PRE2_REJECTED_WAIT_COMPENSATION_CRASH_BOUNDARY checkpoint_blake3={} true_process_exit=73",
                blake3::hash(&bytes)
            );
            std::io::stdout().flush().unwrap();
            std::io::stderr().flush().unwrap();
            std::process::exit(73);
        }
    }
    let result = session.close();
    assert!(result.is_ok());
    panic!("actual committed compensation minimum View boundary absent");
}
pub(super) fn assert_original_terminal(fixture: &Fixture, root: &Path) {
    let checkpoint: serde_json::Value = serde_json::from_slice(
        &fs::read(root.join(application_hosted_wait_rejection::STORE)).unwrap(),
    )
    .unwrap();
    let origin: serde_json::Value =
        serde_json::from_slice(&fs::read(root.join("hosted-wait-origin.json")).unwrap()).unwrap();
    let terminal = &checkpoint["provider_terminal_states"]["agent-a"];
    assert_eq!(terminal["request_digest"], origin["request_digest"]);
    assert_eq!(terminal["status"], "rejected");
    assert_eq!(
        terminal["reject_reason"],
        "canonical_wait_admission_rejected"
    );
    assert!(terminal["feedback_id"].is_null());
    assert_eq!(terminal.get("feedback"), Some(&serde_json::Value::Null));
    assert!(checkpoint["hosted_wait"].is_null());
    let memory = &checkpoint["provider_memory_store"];
    assert!(
        memory["entries"].as_object().is_some_and(|v| v.is_empty())
            || memory["entries"].as_array().is_some_and(|v| v.is_empty())
    );
    let world = fixture.driver.lock().unwrap().execution_world.clone();
    let results = world
        .capability_revocation_state()
        .world_service_results
        .values()
        .filter_map(|v| serde_json::from_value::<wire::CanonicalIntentResultV1>(v.clone()).ok())
        .collect::<Vec<_>>();
    let matching=results.iter().filter(|result| matches!(&result.request.signed_payload,WorldServicePayloadV1::Scheduler(signed) if matches!(&signed.request.operation,SchedulerOperationV1::SettleLease{lease_id,consumed_amount} if lease_id==origin["lease_id"].as_str().unwrap() && *consumed_amount==1))).collect::<Vec<_>>();
    assert_eq!(matching.len(), 1);
    assert!(matching[0].rejected.is_none());
    let original = &matching[0].request;
    let key = correlation::key_digest(&original.correlation.key).unwrap();
    let trace = fixture.lookup_digests.lock().unwrap();
    assert_eq!(
        trace
            .iter()
            .filter(|s| **s == format!("submit:{key}"))
            .count(),
        1
    );
    assert!(trace.iter().any(|s| s == &key));
    drop(trace);
    let mut config = fixture.client.config().clone();
    config.scope_id = "agent:agent-a".into();
    let client = RemoteWorldServiceClient::new(config).unwrap();
    let response = client
        .lookup(
            LookupIntentRequest {
                contract_version: 1,
                key: original.correlation.key.clone(),
            },
            original.signed_payload.clone(),
        )
        .unwrap();
    let IntentOutcome::Committed { commit, receipt } = response.outcome else {
        panic!("original compensation authenticated Lookup must commit")
    };
    let receipt: oasis7_runtime::CognitionReceiptV1 = serde_json::from_value(receipt).unwrap();
    receipt.validate().unwrap();
    assert_eq!(receipt.lease_id, origin["lease_id"].as_str().unwrap());
    assert_eq!(receipt.consumed_amount, 1);
    assert_eq!(receipt.net_amount, 1);
    assert_eq!(receipt.refunded_amount, 0);
    client
        .read_view(ReadWorldViewRequest {
            observation_nonce: None,
            formal_anchor: None,
            contract_version: 1,
            world: client.config().expected_world.clone(),
            scope_id: client.config().scope_id.clone(),
            min_commit: Some(*commit),
            fixed_commit: None,
            deadline_unix_ms: None,
        })
        .unwrap();
    println!(
        "rejected_wait_terminal_exact=true feedback_none=true memory_unchanged=true compensation_submit_count=1 authenticated_lookup_min_view=true"
    );
}
pub(super) fn recover_parent(
    fixture: &Fixture,
    root: &Path,
    command: &mut std::process::Command,
    first: &std::process::Output,
    rejection: thread::JoinHandle<Result<(), String>>,
    selector: thread::JoinHandle<Result<SubmitIntentRequest<WorldServicePayloadV1>, String>>,
) {
    application_hosted_wait_rejection::secure_artifact(fixture, root, first, "compensation-crash");
    let rejected = rejection.join().unwrap();
    let original = selector
        .join()
        .unwrap()
        .expect("actual signed compensation authenticated selector");
    assert_eq!(first.status.code(), Some(73));
    fixture
        .world_gate
        .release_compensation_settle_after_exit73(&first.status)
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(3);
    while !root
        .join("world-compensation-settle-view-worker-joined")
        .exists()
        && Instant::now() < deadline
    {
        thread::sleep(Duration::from_millis(2));
    }
    assert!(
        root.join("world-compensation-settle-view-handler-returned")
            .exists()
    );
    assert!(
        root.join("world-compensation-settle-view-worker-joined")
            .exists()
    );
    let bytes = fs::read(root.join("compensation-crash-original-checkpoint.json")).unwrap();
    assert_eq!(
        bytes,
        fs::read(root.join(application_hosted_wait_rejection::STORE)).unwrap()
    );
    let checkpoint: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert!(
        checkpoint["provider_scheduler_pending"]
            .as_object()
            .unwrap()
            .values()
            .any(
                |v| v["payload"] == serde_json::to_value(&original.signed_payload).unwrap()
                    && v["correlation"] == serde_json::to_value(&original.correlation).unwrap()
            )
    );
    let compensation_key = correlation::key_digest(&original.correlation.key).unwrap();
    let lookup_before = fixture
        .lookup_digests
        .lock()
        .unwrap()
        .iter()
        .filter(|entry| *entry == &compensation_key)
        .count();
    command.env("PRE2_APP_ADMISSION", "wait-rejection-recover");
    let output = command.output().unwrap();
    let lookup_after = fixture
        .lookup_digests
        .lock()
        .unwrap()
        .iter()
        .filter(|entry| *entry == &compensation_key)
        .count();
    application_hosted_wait_rejection::secure_artifact(
        fixture,
        root,
        &output,
        "compensation-recovered",
    );
    println!("compensation_recovery_original_lookup before={lookup_before} after={lookup_after}");
    assert!(output.status.success());
    assert!(
        lookup_after > lookup_before,
        "new recovery child must Lookup the exact original compensation key before the parent's final query"
    );
    assert_original_terminal(fixture, root);
    application_hosted_wait_rejection::finish_with_result(fixture, root, &output, rejected);
    println!("PRE2_REJECTED_WAIT_COMPENSATION_PROCESS_RECOVERY_PASSED");
}
