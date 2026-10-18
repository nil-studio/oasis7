//! Real process death after canonical Resume, before the minimum-view response.
use super::*;
use oasis7::viewer::ViewerRuntimeLiveServer;
use std::io::Write;

const STORE: &str = "hosted-wait-private-lineage.json";
#[test]
fn real_tcp_hosted_resume_process_crash_recovers_original_identity() {
    application_harness::run_isolated_application_mode(
        false,
        false,
        false,
        true,
        false,
        false,
        "hosted-wait-resume-crash",
    );
}

pub(super) fn start_selector(fixture: &Fixture) -> thread::JoinHandle<()> {
    let driver = fixture.driver.clone();
    let gate = fixture.world_gate.clone();
    gate.arm_resume_view_selector();
    let mut config = fixture.client.config().clone();
    config.scope_id = "agent:agent-a".into();
    thread::spawn(move || {
        let client = RemoteWorldServiceClient::new(config).unwrap();
        let deadline = Instant::now() + Duration::from_secs(25);
        loop {
            let original = driver
                .lock()
                .unwrap()
                .execution_world
                .capability_revocation_state()
                .world_service_results
                .values()
                .filter_map(|value| {
                    serde_json::from_value::<wire::CanonicalIntentResultV1>(value.clone()).ok()
                })
                .find(|result| {
                    result.rejected.is_none()
                        && matches!(&result.request.signed_payload,
                    WorldServicePayloadV1::Scheduler(signed) if matches!(&signed.request.operation,
                        SchedulerOperationV1::ResumeWake { .. }))
                })
                .map(|result| result.request);
            if let Some(original) = original {
                gate.install_resume_view_selector_from_authenticated_lookup(
                    &client,
                    "agent:agent-a",
                    original,
                )
                .expect("exact committed original Resume selector");
                println!("hosted_resume_selector_authenticated_lookup_committed=true");
                return;
            }
            if Instant::now() >= deadline {
                panic!("real canonical Resume selector prerequisite absent");
            }
            thread::sleep(Duration::from_millis(2));
        }
    })
}

fn retained_original(bytes: &[u8]) -> SubmitIntentRequest<WorldServicePayloadV1> {
    let checkpoint: serde_json::Value = serde_json::from_slice(bytes).unwrap();
    let resume = &checkpoint["hosted_resume"];
    assert!(resume.is_object(), "real durable hosted Resume missing");
    let request: oasis7::simulator::ContinuousAgentRequestContextV1 =
        serde_json::from_value(resume["context"]["request_context"].clone()).unwrap();
    request.validate_production_lane().unwrap();
    assert!(
        resume["proposal"].is_object()
            && resume["runtime"].is_object()
            && resume["current"].is_object()
            && resume["wake"].is_object()
    );
    checkpoint["provider_scheduler_pending"]
        .as_object()
        .unwrap()
        .values()
        .find_map(|pending| {
            let payload: WorldServicePayloadV1 =
                serde_json::from_value(pending["payload"].clone()).ok()?;
            let WorldServicePayloadV1::Scheduler(signed) = &payload else {
                return None;
            };
            let SchedulerOperationV1::ResumeWake {
                resume: identity, ..
            } = &signed.request.operation
            else {
                return None;
            };
            if identity.request_digest != request.request_digest.to_string()
                || identity.agent_session_id != request.agent_session_id
                || identity.agent_turn_id != request.agent_turn_id
                || identity.decision_request_id != request.decision_request_id
            {
                return None;
            }
            assert_eq!(pending["resume_context"], resume["context"]);
            assert_eq!(pending["resume_current_context"], resume["current"]);
            let correlation: oasis7_client_api::world_service::RequestCorrelation =
                serde_json::from_value(pending["correlation"].clone()).unwrap();
            assert_eq!(
                correlation::derive_correlation(correlation.key.world.clone(), &payload).unwrap(),
                correlation
            );
            Some(SubmitIntentRequest {
                contract_version: 1,
                deadline_unix_ms: None,
                correlation,
                signed_payload: payload,
            })
        })
        .expect("complete exact original signed Resume checkpoint")
}

pub(super) fn verify(client: &RemoteWorldServiceClient, crash: bool) {
    let root = std::path::PathBuf::from(std::env::var("PRE2_METADATA_DIR").unwrap());
    if crash {
        application_fresh::preflight(client, &root);
    }
    let server = ViewerRuntimeLiveServer::new(application_hosted::server_config(
        client,
        true,
        Duration::from_millis(200),
        Some(root.join(STORE)),
    ))
    .unwrap();
    let shared = Arc::new(Mutex::new(server));
    let mut session = application_stream_boundaries::Session::start(&shared, false);
    let warmed = session.warm();
    session
        .send(serde_json::json!({"type":"live_control","mode":{"mode":"play"},"request_id":1301}))
        .unwrap();
    session.send(serde_json::json!({"type":"hello_v2","client":"PRE2 Resume process recovery","version":2,"capabilities":[]})).unwrap();
    session
        .send(serde_json::json!({"type":"request_snapshot"}))
        .unwrap();
    let ordered = session.snapshot_ordered(Duration::from_secs(3), true);
    let eligibility = shared.lock().unwrap().test_agent_service_pump_status();
    let deadline = Instant::now() + Duration::from_secs(12);
    let mut summary = serde_json::Value::Null;
    let mut completed = false;
    while Instant::now() < deadline {
        session.snapshot(Duration::from_millis(25));
        if crash && root.join("world-resume-view-started").exists() {
            let bytes = fs::read(root.join(STORE)).unwrap();
            let original = retained_original(&bytes);
            let digest = correlation::key_digest(&original.correlation.key).unwrap();
            assert!(
                warmed && ordered && eligibility["eligible"] == true,
                "ordinary crash Play prerequisites"
            );
            println!(
                "hosted_resume_real_process_crash_boundary=true original_correlation_digest={digest} checkpoint_blake3={} factory_calls=0 direct_poll_calls=0",
                blake3::hash(&bytes)
            );
            std::io::stdout().flush().unwrap();
            std::process::exit(73);
        }
        if let Ok(guard) = shared.try_lock() {
            summary = guard.test_canonical_provider_diagnostic_summary();
            if let Ok(bytes) = fs::read(root.join("hosted-resumed-model.json")) {
                let resumed: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
                completed = summary["terminal_states"]["agent-a"]["status"] == "committed"
                    && summary["pending_intent_count"] == 0
                    && summary["pending_action_count"] == 0
                    && summary["memory_store"]
                        .to_string()
                        .contains(resumed["request_digest"].as_str().unwrap());
                if completed {
                    break;
                }
            }
        }
    }
    let stage = fs::read(root.join(STORE))
        .ok()
        .and_then(|bytes| serde_json::from_slice::<serde_json::Value>(&bytes).ok())
        .and_then(|value| value["hosted_resume"]["stage"].as_str().map(str::to_owned));
    session
        .send(serde_json::json!({"type":"live_control","mode":{"mode":"pause"},"request_id":1302}))
        .unwrap();
    session
        .send(serde_json::json!({"type":"request_snapshot"}))
        .unwrap();
    let drained = session.snapshot(Duration::from_secs(2));
    fs::write(
        root.join("hosted-wait-memory.json"),
        serde_json::to_vec(&summary["memory_store"]).unwrap(),
    )
    .unwrap();
    let joined = session.close();
    println!(
        "hosted_resume_recreated_process warmed={warmed} ordered={ordered} eligible={} native_runner_present={} retained_stage={stage:?} completed={completed} drained={drained} worker={joined:?} factory_calls=0 direct_poll_calls=0",
        eligibility["eligible"], summary["native_runner_present"]
    );
    assert!(
        warmed && ordered && eligibility["eligible"] == true,
        "ordinary recovery Play prerequisites"
    );
    assert!(
        drained && joined.is_ok(),
        "actual recovery cleanup before assertions"
    );
    assert!(
        completed,
        "original Resume process recovery must hydrate specific successor and commit resumed Act memory"
    );
    println!("PRE2_HOSTED_RESUME_PROCESS_RECOVERY_PASSED");
}

fn secure_artifact(
    fixture: &Fixture,
    root: &std::path::Path,
    output: &std::process::Output,
    stage: &str,
) {
    let dir = std::env::temp_dir().join(format!(
        "pre2-resume-{stage}-child-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    fs::create_dir(&dir).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&dir, fs::Permissions::from_mode(0o700)).unwrap();
    }
    let canonical = fixture
        .driver
        .lock()
        .unwrap()
        .execution_world
        .capability_revocation_state()
        .world_service_results
        .values()
        .cloned()
        .collect::<Vec<_>>();
    for (name, bytes) in [
        ("stdout.log", output.stdout.clone()),
        ("stderr.log", output.stderr.clone()),
        ("exit-status.txt", output.status.to_string().into_bytes()),
        (
            "private-lineage.json",
            fs::read(root.join(STORE)).unwrap_or_default(),
        ),
        (
            "canonical-results.json",
            serde_json::to_vec(&canonical).unwrap(),
        ),
    ] {
        let path = dir.join(name);
        fs::write(&path, &bytes).unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap();
        }
        println!(
            "hosted_resume_process_artifact={} bytes={} blake3={}",
            path.display(),
            bytes.len(),
            blake3::hash(&bytes)
        );
    }
    println!("{}", String::from_utf8_lossy(&output.stdout));
    println!("{}", String::from_utf8_lossy(&output.stderr));
}

pub(super) fn recover_parent(
    fixture: &Fixture,
    root: &std::path::Path,
    command: &mut std::process::Command,
    crashed: &std::process::Output,
    selector: thread::JoinHandle<()>,
    model_count: &std::sync::atomic::AtomicUsize,
) {
    secure_artifact(fixture, root, crashed, "crash");
    assert_eq!(
        crashed.status.code(),
        Some(73),
        "real Resume boundary must terminate whole child"
    );
    selector.join().unwrap();
    let checkpoint = fs::read(root.join(STORE)).unwrap();
    let original = retained_original(&checkpoint);
    let key = correlation::key_digest(&original.correlation.key).unwrap();
    let IntentOutcome::Committed { receipt, .. } = fixture
        .client
        .lookup(
            LookupIntentRequest {
                contract_version: 1,
                key: original.correlation.key.clone(),
            },
            original.signed_payload.clone(),
        )
        .unwrap()
        .outcome
    else {
        panic!("original canonical Resume must be committed before recovery");
    };
    let receipt: oasis7_runtime::CognitionWakeHandoffResultV1 =
        serde_json::from_value(receipt).unwrap();
    assert_eq!(
        receipt.continuation.status,
        oasis7_runtime::ContinuationStatusV1::Consumed
    );
    assert_eq!(
        receipt.replanned_continuation.as_ref().unwrap().status,
        oasis7_runtime::ContinuationStatusV1::Scheduled
    );
    let world = fixture.driver.lock().unwrap().execution_world.clone();
    let actual = world
        .capability_revocation_state()
        .world_service_results
        .values()
        .filter_map(|value| {
            serde_json::from_value::<wire::CanonicalIntentResultV1>(value.clone()).ok()
        })
        .find(|result| result.request.correlation == original.correlation)
        .unwrap();
    assert_eq!(
        actual.request, original,
        "retained original must equal actual canonical signed request"
    );
    assert!(actual.rejected.is_none());
    let origin: serde_json::Value =
        serde_json::from_slice(&fs::read(root.join("hosted-wait-origin.json")).unwrap()).unwrap();
    assert!(
        world
            .cognition_economy()
            .unwrap()
            .leases
            .values()
            .any(
                |lease| lease.lease_id == origin["lease_id"].as_str().unwrap()
                    && lease.request_digest == origin["request_digest"].as_str().unwrap()
                    && lease.status == oasis7_runtime::CognitionLeaseStatusV1::Settled
            )
    );
    let selected: serde_json::Value =
        serde_json::from_slice(&fs::read(root.join("hosted-wait-selected.json")).unwrap()).unwrap();
    assert_eq!(selected["wake_id"], receipt.wake.wake_id);
    assert_eq!(
        selected["continuation_id"],
        receipt.continuation.continuation_id
    );
    fixture
        .world_gate
        .release_resume_view_gate_after_child_exit73(&crashed.status)
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(3);
    while !root.join("world-resume-view-worker-joined").exists() && Instant::now() < deadline {
        thread::sleep(Duration::from_millis(2));
    }
    let abandoned = root.join("world-resume-view-gate-returned").exists();
    let returned = root.join("world-resume-view-handler-returned").exists();
    let joined = root.join("world-resume-view-worker-joined").exists();
    println!(
        "hosted_resume_crash_node_gate abandoned_after_exit73={abandoned} node_view_handler_dispatched=false outer_handler_returned={returned} actual_worker_joined={joined}"
    );
    assert!(
        abandoned && returned && joined,
        "actual old HTTP worker must finish and join before recreation"
    );
    let before = fixture
        .lookup_digests
        .lock()
        .unwrap()
        .iter()
        .filter(|value| *value == &key)
        .count();
    let recovered = command
        .env("PRE2_APP_ADMISSION", "hosted-wait-resume-recover")
        .output()
        .unwrap();
    secure_artifact(fixture, root, &recovered, "recover");
    let trace = fixture.lookup_digests.lock().unwrap();
    let submits = trace
        .iter()
        .filter(|value| **value == format!("submit:{key}"))
        .count();
    let lookups = trace.iter().filter(|value| *value == &key).count();
    drop(trace);
    println!(
        "hosted_resume_original_recovery correlation_digest={key} submit_count={submits} lookups_before={before} lookups_after={lookups} original_payload_blake3={}",
        blake3::hash(&serde_json::to_vec(&original.signed_payload).unwrap())
    );
    assert_eq!(
        submits, 1,
        "recovery must not replace original Resume Submit"
    );
    assert!(
        lookups > before,
        "new process must Lookup exact original Resume"
    );
    println!(
        "hosted_resume_actual_missing_harness_witness={} recovery_exit={}",
        String::from_utf8_lossy(&recovered.stderr)
            .contains("hosted_resume_missing_harness_continuation"),
        recovered.status
    );
    assert!(
        recovered.status.success(),
        "actual Resume recovery failed after original Lookup; retained child stderr contains exact static error witness"
    );
    application_hosted_wait::report(fixture, root, model_count.load(Ordering::SeqCst));
}
