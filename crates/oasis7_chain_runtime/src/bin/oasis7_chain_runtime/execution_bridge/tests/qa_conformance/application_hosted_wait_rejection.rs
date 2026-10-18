//! Actual original signed Wait Admit rejection after a real canonical clock commit.
use super::*;
use oasis7::viewer::ViewerRuntimeLiveServer;
use std::path::Path;
pub(super) const STORE: &str = "hosted-wait-private-lineage.json";
#[test]
fn real_tcp_hosted_wait_admit_rejection_compensates_original_charged_lease() {
    application_harness::run_isolated_application_mode(
        false,
        false,
        false,
        true,
        false,
        false,
        "wait-admit-rejected",
    );
}
pub(super) fn verify(client: &RemoteWorldServiceClient) {
    let root = std::path::PathBuf::from(std::env::var("PRE2_METADATA_DIR").unwrap());
    let mode = std::env::var("PRE2_APP_ADMISSION").unwrap();
    // This fixture measures compensation of one issued Admit. Fresh autonomous
    // turns after its terminal completion have their own cadence tests.
    let isolated = mode == "wait-admit-rejected" || mode.starts_with("wait-rejection-");
    if isolated {
        fs::write(
            root.join("rejected-wait-single-turn"),
            b"public cadence isolation",
        )
        .unwrap();
    }
    if std::env::var("PRE2_APP_ADMISSION").unwrap() == "wait-rejection-recover" {
        assert!(
            root.join("fresh-provider-counter-reset").exists(),
            "same actual preflight already completed before crash"
        );
    } else {
        application_fresh::preflight(client, &root);
    }
    let server = ViewerRuntimeLiveServer::new(application_hosted::server_config(
        client,
        true,
        if isolated {
            Duration::from_secs(60)
        } else {
            Duration::from_millis(200)
        },
        Some(root.join(STORE)),
    ))
    .unwrap();
    let shared = Arc::new(Mutex::new(server));
    let mut session = application_stream_boundaries::Session::start(&shared, false);
    let warmed = session.warm();
    session
        .send(serde_json::json!({"type":"live_control","mode":{"mode":"play"},"request_id":1301}))
        .unwrap();
    session.send(serde_json::json!({"type":"hello_v2","client":"PRE2 Wait Admit process recovery","version":2,"capabilities":[]})).unwrap();
    session
        .send(serde_json::json!({"type":"request_snapshot"}))
        .unwrap();
    let ordered = session.snapshot_ordered(Duration::from_secs(3), true);
    let eligibility = shared.lock().unwrap().test_agent_service_pump_status();
    let deadline = Instant::now() + Duration::from_secs(12);
    let mut summary = serde_json::Value::Null;
    let mut paused_ordered = false;
    let mut paused_playing_false = false;

    while Instant::now() < deadline {
        session.snapshot(Duration::from_millis(25));
        if let Ok(guard) = shared.try_lock() {
            summary = guard.test_canonical_provider_diagnostic_summary();
        }
        if root.join("wait-admit-actual-rejected.json").exists() {
            if isolated {
                session.send(serde_json::json!({"type":"live_control","mode":{"mode":"pause"},"request_id":1303})).unwrap();
                session.send(serde_json::json!({"type":"hello_v2","client":"PRE2 rejection paused","version":2,"capabilities":[]})).unwrap();
                session
                    .send(serde_json::json!({"type":"request_snapshot"}))
                    .unwrap();
                paused_ordered = session.snapshot_ordered(Duration::from_secs(3), true);
                paused_playing_false = shared.lock().unwrap().test_agent_service_pump_status()["play_enabled"]
                    == false;
            }
            let observe_until = Instant::now()
                + if isolated {
                    Duration::from_secs(10)
                } else {
                    Duration::from_secs(2)
                };
            while Instant::now() < observe_until {
                session.snapshot(Duration::from_millis(25));
                if let Ok(guard) = shared.try_lock() {
                    summary = guard.test_canonical_provider_diagnostic_summary();
                }
                if isolated
                    && summary["terminal_states"]["agent-a"]["status"] == "rejected"
                    && summary["terminal_states"]["agent-a"]["reject_reason"]
                        == "canonical_wait_admission_rejected"
                {
                    break;
                }
            }
            break;
        }
    }
    let stage = fs::read(root.join(STORE))
        .ok()
        .and_then(|bytes| serde_json::from_slice::<serde_json::Value>(&bytes).ok())
        .and_then(|value| value["hosted_wait"]["stage"].as_str().map(str::to_owned));
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
    if isolated {
        assert!(
            paused_ordered && paused_playing_false,
            "real ordered Pause must disable fresh planning while issued compensation reconciles"
        );
        assert_eq!(summary["terminal_states"]["agent-a"]["status"], "rejected");
        let count: usize =
            serde_json::from_slice(&fs::read(root.join("rejected-wait-model-count.json")).unwrap())
                .unwrap();
        assert_eq!(
            count, 1,
            "exact one actual parsed target model invocation, preflight excluded"
        );
        println!(
            "rejected_wait_single_turn actual_model_count={count} paused_ordered=true playing_false=true"
        );
    }
    println!(
        "hosted_wait_rejection_observer warmed={warmed} ordered={ordered} eligible={} native_runner_present={} retained_stage={stage:?} drained={drained} worker={joined:?} factory_calls=0 direct_poll_calls=0",
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
        root.join("wait-admit-actual-rejected.json").exists(),
        "actual canonical rejection prerequisite"
    );
    println!("PRE2_WAIT_ADMIT_REJECTED_OBSERVED");
}
pub(super) fn secure_artifact(
    fixture: &Fixture,
    root: &std::path::Path,
    output: &std::process::Output,
    stage: &str,
) {
    let dir = std::env::temp_dir().join(format!(
        "pre2-wait-rejection-{stage}-child-{}-{}",
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
            fs::read(root.join(STORE))
                .or_else(|_| fs::read(root.join("wait-rejection-checkpoint-backup.json")))
                .unwrap_or_default(),
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
            "hosted_wait_rejection_process_artifact={} bytes={} blake3={}",
            path.display(),
            bytes.len(),
            blake3::hash(&bytes)
        );
    }
    for name in [
        "wait-admit-actual-rejected.json",
        "world-admit-rejected-started.json",
        "world-admit-rejected-release.json",
        "world-admit-rejected-handler-returned.json",
        "world-admit-rejected-worker-joined.json",
        "world-compensation-settle-selector.json",
        "world-compensation-settle-view-started",
        "world-compensation-settle-view-release",
        "world-compensation-settle-view-gate-returned",
        "world-compensation-settle-view-handler-returned",
        "world-compensation-settle-view-worker-joined",
        "compensation-crash-original-checkpoint.json",
    ] {
        if let Ok(bytes) = fs::read(root.join(name)) {
            private_write(&dir.join(name), &bytes).unwrap();
            println!(
                "hosted_wait_rejection_snapshot_artifact={} bytes={} blake3={}",
                dir.join(name).display(),
                bytes.len(),
                blake3::hash(&bytes)
            );
        }
    }
    println!("{}", String::from_utf8_lossy(&output.stdout));
    println!("{}", String::from_utf8_lossy(&output.stderr));
}

pub(super) fn private_write(path: &Path, bytes: &[u8]) -> Result<(), String> {
    fs::write(path, bytes).map_err(|e| e.to_string())?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o600)).map_err(|e| e.to_string())?;
    }
    Ok(())
}
pub(super) fn observation(
    fixture: &Fixture,
    root: std::path::PathBuf,
) -> provider_metadata::DecisionObservation {
    let world = fixture.client.config().expected_world.clone();
    let actual = application_hosted_wait::observation(fixture, root.clone());
    let model_count = std::sync::atomic::AtomicUsize::new(0);
    let gate = fixture.world_gate.clone();
    let armed = AtomicBool::new(false);
    Arc::new(move |request| {
        if root.join("rejected-wait-single-turn").exists() {
            let count = model_count.fetch_add(1, Ordering::SeqCst) + 1;
            private_write(
                &root.join("rejected-wait-model-count.json"),
                &serde_json::to_vec(&count).unwrap(),
            )
            .unwrap();
        }
        actual(request);
        if !armed.swap(true, Ordering::SeqCst) {
            gate.arm_admit_rejected_gate(
                world.clone(),
                &request.agent_subject,
                request.request_digest.as_str(),
                &format!(
                    "provider-wait:{}:{}:{}",
                    request.agent_subject, request.agent_turn_id, request.decision_request_id
                ),
            )
            .unwrap();
        }
    })
}
pub(super) fn start(
    fixture: &Fixture,
    root: std::path::PathBuf,
) -> thread::JoinHandle<Result<(), String>> {
    let gate = fixture.world_gate.clone();
    let driver = fixture.driver.clone();
    let config = fixture.client.config().clone();
    let mut read = fixture.view(None);
    read.scope_id = "agent:agent-a".into();
    thread::spawn(move || {
        let candidate = gate.wait_admit_rejected_candidate(Duration::from_secs(15))?;
        let original = candidate.original_request().clone();
        {
            let mut actual = driver.lock().unwrap();
            commit_request(&mut actual, 0, None);
        }
        let mut config = config;
        config.scope_id = "agent:agent-a".into();
        let client = RemoteWorldServiceClient::new(config).map_err(|e| e.to_string())?;
        let fresh = client.read_view(read).map_err(|e| e.to_string())?;
        gate.release_admit_rejected_gate_after_clock_commit(&fresh)?;
        let deadline = Instant::now() + Duration::from_secs(15);
        loop {
            let actual = driver
                .lock()
                .unwrap()
                .execution_world
                .capability_revocation_state()
                .world_service_results
                .values()
                .filter_map(|v| {
                    serde_json::from_value::<wire::CanonicalIntentResultV1>(v.clone()).ok()
                })
                .find(|r| r.request == original);
            if let Some(actual) = actual {
                let key = correlation::key_digest(&actual.request.correlation.key)?;
                let outcome = client
                    .lookup(
                        LookupIntentRequest {
                            contract_version: 1,
                            key: actual.request.correlation.key.clone(),
                        },
                        actual.request.signed_payload.clone(),
                    )
                    .map_err(|e| e.to_string())?;
                if !matches!(outcome.outcome, IntentOutcome::Rejected { .. }) {
                    return Err("original signed Admit did not become actual Rejected".into());
                }
                private_write(
                    &root.join("wait-admit-actual-rejected.json"),
                    &serde_json::to_vec(&actual).map_err(|e| e.to_string())?,
                )?;
                println!(
                    "hosted_wait_rejection_original_authenticated=true correlation_digest={key}"
                );
                return Ok(());
            }
            if Instant::now() >= deadline {
                return Err("actual canonical Admit result absent".into());
            }
            thread::sleep(Duration::from_millis(2));
        }
    })
}
pub(super) fn finish(
    fixture: &Fixture,
    root: &Path,
    output: &std::process::Output,
    worker: thread::JoinHandle<Result<(), String>>,
) {
    let joined = worker.join().expect("real rejection driver worker joins");
    finish_with_result(fixture, root, output, joined);
}
pub(super) fn finish_with_result(
    fixture: &Fixture,
    root: &Path,
    output: &std::process::Output,
    joined: Result<(), String>,
) {
    let deadline = Instant::now() + Duration::from_secs(3);
    while !root
        .join("world-admit-rejected-worker-joined.json")
        .exists()
        && Instant::now() < deadline
    {
        thread::sleep(Duration::from_millis(2));
    }
    secure_artifact(fixture, root, output, "probe");
    joined.expect("real original signed rejection prerequisite");
    assert!(
        output.status.success(),
        "actual observer cleanup/eligibility prerequisite"
    );
    assert!(
        root.join("world-admit-rejected-handler-returned.json")
            .exists()
            && root
                .join("world-admit-rejected-worker-joined.json")
                .exists(),
        "actual rejected Admit handler return and listener-owned worker join"
    );
    let actual: wire::CanonicalIntentResultV1 =
        serde_json::from_slice(&fs::read(root.join("wait-admit-actual-rejected.json")).unwrap())
            .unwrap();
    assert!(actual.rejected.is_some());
    let raw = serde_json::to_string(&actual).unwrap();
    let known = raw.contains("scheduler base binding changed");
    let key = correlation::key_digest(&actual.request.correlation.key).unwrap();
    let trace = fixture.lookup_digests.lock().unwrap();
    let submits = trace
        .iter()
        .filter(|v| **v == format!("submit:{key}"))
        .count();
    let lookups = trace.iter().filter(|v| *v == &key).count();
    drop(trace);
    assert_eq!(submits, 1);
    assert!(lookups > 0);
    assert!(
        known,
        "actual rejection must be exact stale base binding cause"
    );
    let origin: serde_json::Value =
        serde_json::from_slice(&fs::read(root.join("hosted-wait-origin.json")).unwrap()).unwrap();
    let world = fixture.driver.lock().unwrap().execution_world.clone();
    let economy = world.cognition_economy().unwrap();
    let lease = economy
        .leases
        .get(origin["lease_id"].as_str().unwrap())
        .unwrap();
    let proposal_id = match &actual.request.signed_payload {
        WorldServicePayloadV1::Scheduler(s) => match &s.request.operation {
            SchedulerOperationV1::AdmitContinuation(p) => &p.continuation_proposal_id,
            _ => unreachable!(),
        },
        _ => unreachable!(),
    };
    let continuation_absent = !world
        .active_cognition_continuations()
        .unwrap()
        .iter()
        .any(|v| {
            v.origin_request_digest == origin["request_digest"].as_str().unwrap()
                && &v.continuation_proposal_id == proposal_id
        });
    let settled =
        lease.status == oasis7_runtime::CognitionLeaseStatusV1::Settled && lease.settled_amount > 0;
    let results = world
        .capability_revocation_state()
        .world_service_results
        .values()
        .filter_map(|v| serde_json::from_value::<wire::CanonicalIntentResultV1>(v.clone()).ok())
        .collect::<Vec<_>>();
    let settlements=results.iter().filter(|r|matches!(&r.request.signed_payload,WorldServicePayloadV1::Scheduler(s) if matches!(&s.request.operation,SchedulerOperationV1::SettleLease{lease_id,..} if lease_id==&lease.lease_id))).count();
    let checkpoint: serde_json::Value =
        serde_json::from_slice(&fs::read(root.join(STORE)).unwrap()).unwrap();
    let retained_stage = checkpoint["hosted_wait"]["stage"].as_str();
    println!(
        "hosted_wait_rejection_compensation actual_rejected_base_binding=true original_submit_count={submits} original_lookup_count={lookups} continuation_absent={continuation_absent} lease_status={:?} reserved_amount={} settled_amount={} settle_count={settlements} retained_stage={retained_stage:?} actual_worker_joined=true",
        lease.status, lease.reserved_amount, lease.settled_amount
    );
    assert!(continuation_absent);
    assert!(
        settled && settlements == 1 && checkpoint["hosted_wait"].is_null(),
        "canonical rejected Wait must compensate original charged lease and retire exact held state"
    );
    fixture
        .finish_http_workers()
        .expect("actual fixture listener and every HTTP worker must join successfully");
    println!("PRE2_WAIT_ADMIT_REJECTION_HTTP_WORKERS_JOINED");
    println!("PRE2_WAIT_ADMIT_REJECTION_COMPENSATION_PASSED");
}
