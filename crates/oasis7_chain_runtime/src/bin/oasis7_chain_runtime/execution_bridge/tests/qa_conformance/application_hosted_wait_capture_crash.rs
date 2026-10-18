//! Real capture atomic-rename failure followed by whole-process death and recreation.
use super::*;
use oasis7_viewer_runtime::viewer::ViewerRuntimeLiveServer;
use std::io::Write;
use std::path::{Path, PathBuf};
const STORE: &str = "hosted-wait-private-lineage.json";
#[test]
fn real_tcp_hosted_wait_capture_write_failure_crash_recovers_original_queue() {
    application_harness::run_isolated_application_mode(
        false,
        false,
        false,
        true,
        false,
        false,
        "wait-capture-crash",
    );
}
#[test]
fn real_tcp_restored_queued_wait_cleanup_failure_restores_nonempty_provenance() {
    application_harness::run_isolated_application_mode(
        false,
        false,
        false,
        true,
        false,
        false,
        "wait-capture-nonempty",
    );
}

fn await_marker(root: &Path, name: &str) -> Result<(), String> {
    let end = Instant::now() + Duration::from_secs(15);
    while !root.join(name).exists() && Instant::now() < end {
        thread::sleep(Duration::from_millis(2));
    }
    if root.join(name).exists() {
        Ok(())
    } else {
        Err(format!("actual capture marker absent: {name}"))
    }
}
fn private_write(path: &Path, bytes: &[u8]) -> Result<(), String> {
    fs::write(path, bytes).map_err(|e| e.to_string())?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o600)).map_err(|e| e.to_string())?;
    }
    Ok(())
}
pub(super) fn start(root: PathBuf) -> thread::JoinHandle<Result<Vec<u8>, String>> {
    thread::spawn(move || {
        await_marker(&root, "wait-capture-before-persist")?;
        let target = root.join(STORE);
        let backup = root.join("wait-capture-checkpoint-backup.json");
        let bytes = fs::read(&target).map_err(|e| e.to_string())?;
        private_write(&root.join("wait-capture-original-checkpoint.json"), &bytes)?;
        fs::rename(&target, &backup).map_err(|e| e.to_string())?;
        if let Err(e) = fs::create_dir(&target) {
            fs::rename(&backup, &target).map_err(|e| e.to_string())?;
            return Err(e.to_string());
        }
        private_write(
            &root.join("wait-capture-release"),
            b"actual target directory",
        )?;
        await_marker(&root, "wait-capture-failed")?;
        if fs::read(&backup).map_err(|e| e.to_string())? != bytes {
            return Err("actual backup bytes changed".into());
        }
        Ok(bytes)
    })
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
    session.send(serde_json::json!({"type":"hello_v2","client":"PRE2 Wait Admit process recovery","version":2,"capabilities":[]})).unwrap();
    session
        .send(serde_json::json!({"type":"request_snapshot"}))
        .unwrap();
    let ordered = session.snapshot_ordered(Duration::from_secs(3), true);
    let eligibility = shared.lock().unwrap().test_agent_service_pump_status();
    let readout_started = Instant::now();
    let deadline = readout_started + Duration::from_secs(12);
    let mut summary = serde_json::Value::Null;
    let mut completed = false;
    let readout_stop = Arc::new(AtomicBool::new(false));
    let stop = readout_stop.clone();
    let readout_server = shared.clone();
    let (readout_tx, readout_rx) = std::sync::mpsc::sync_channel(1);
    let (finished_tx, finished_rx) = std::sync::mpsc::channel();
    let readout = thread::spawn(move || {
        while !stop.load(Ordering::SeqCst) && Instant::now() < deadline {
            // Queue a real live read instead of racing the serving/pump mutex
            // once after each socket timeout. No provider or factory is called.
            let actual = {
                let guard = readout_server.lock().unwrap();
                {
                    let mut actual = guard.test_canonical_provider_diagnostic_summary();
                    actual["pump_status"] = guard.test_agent_service_pump_status();
                    actual
                }
            };
            let captured_at = Instant::now();
            if captured_at < deadline && !stop.load(Ordering::SeqCst) {
                let _ = readout_tx.try_send((captured_at, actual));
            }
            thread::sleep(Duration::from_millis(5));
        }
        finished_tx.send(()).unwrap();
    });
    let mut phase_transitions = Vec::new();
    let mut last_phase_state = serde_json::Value::Null;
    let mut sample_count = 0;
    let mut snapshot_frames = 0;
    let mut next_snapshot = Instant::now();
    while Instant::now() < deadline {
        if Instant::now() >= next_snapshot {
            session
                .send(serde_json::json!({"type":"request_snapshot"}))
                .unwrap();
            next_snapshot = Instant::now() + Duration::from_millis(250);
        }
        snapshot_frames += usize::from(
            session.snapshot(
                deadline
                    .saturating_duration_since(Instant::now())
                    .min(Duration::from_millis(25)),
            ),
        );
        if crash && root.join("wait-capture-failed").exists() {
            let bytes = fs::read(root.join("wait-capture-checkpoint-backup.json")).unwrap();
            let value: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
            let queue = value["provider_completed_decisions"].as_array().unwrap();
            assert_eq!(queue.len(), 1);
            assert!(value["hosted_wait"].is_null());
            assert!(warmed && ordered && eligibility["eligible"] == true);
            println!(
                "hosted_wait_capture_real_crash_boundary=true disk_completed_queue=1 disk_hosted_wait_absent=true checkpoint_blake3={} factory_calls=0 direct_poll_calls=0",
                blake3::hash(&bytes)
            );
            std::io::stdout().flush().unwrap();
            std::process::exit(73);
        }
        if let Ok((captured_at, actual)) = readout_rx.recv_timeout(
            deadline
                .saturating_duration_since(Instant::now())
                .min(Duration::from_millis(25)),
        ) {
            if captured_at >= deadline || Instant::now() >= deadline {
                continue;
            }
            sample_count += 1;
            let phase_state = serde_json::json!({
                "phase": actual["hosted_service_phase"],
                "inflight": actual["hosted_service_inflight"],
                "pump_status": actual["pump_status"],
                "pump_error": actual["agent_service_pump_error"],
                "pending_intent_count": actual["pending_intent_count"],
                "terminal_states": actual["terminal_states"],
            });
            if phase_state != last_phase_state {
                // Capture times are measured before the unchanged acceptance deadline.
                // Never use a later cleanup transition as a successful readout.
                phase_transitions.push(serde_json::json!({
                    "captured_elapsed_ms": captured_at.duration_since(readout_started).as_millis(),
                    "received_elapsed_ms": Instant::now().duration_since(readout_started).as_millis(),
                    "state": phase_state,
                }));
                last_phase_state = phase_state;
            }
            summary = actual;
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
    readout_stop.store(true, Ordering::SeqCst);
    // Completion is fixed before shutdown. A later cleanup/readback must not
    // turn a missed pre-deadline predicate into a successful acceptance.
    fs::write(
        root.join("wait-capture-live-readout.json"),
        serde_json::to_vec(
            &serde_json::json!({"sample_count":sample_count,"snapshot_frames":snapshot_frames,
            "completed_before_deadline":completed,"phase_transitions":phase_transitions,"last_summary":summary}),
        )
        .unwrap(),
    )
    .unwrap();
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
    let readout_finished = finished_rx.recv_timeout(Duration::from_secs(2)).is_ok();
    assert!(
        readout_finished,
        "actual live readout worker must finish within cleanup budget"
    );
    readout.join().unwrap();
    println!(
        "hosted_wait_capture_recreated_process warmed={warmed} ordered={ordered} eligible={} native_runner_present={} retained_stage={stage:?} completed={completed} samples={sample_count} snapshot_frames={snapshot_frames} readout_joined={readout_finished} drained={drained} worker={joined:?} factory_calls=0 direct_poll_calls=0",
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
        "original Wait Admit process recovery must settle and resume to exact Act memory; diagnostic={}",
        application_completion::hosted_failure_diagnostic(&summary)
    );
    println!("PRE2_HOSTED_WAIT_RESUME_ACT_PASSED");
    println!("PRE2_HOSTED_WAIT_CAPTURE_PROCESS_RECOVERY_PASSED");
}
fn secure_artifact(
    fixture: &Fixture,
    root: &std::path::Path,
    output: &std::process::Output,
    stage: &str,
) {
    let dir = std::env::temp_dir().join(format!(
        "pre2-wait-capture-{stage}-child-{}-{}",
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
                .or_else(|_| fs::read(root.join("wait-capture-checkpoint-backup.json")))
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
            "hosted_wait_capture_process_artifact={} bytes={} blake3={}",
            path.display(),
            bytes.len(),
            blake3::hash(&bytes)
        );
    }
    for name in [
        "wait-capture-live-readout.json",
        "wait-capture-before.json",
        "wait-capture-staged.json",
        "wait-capture-after.json",
        "wait-capture-original-checkpoint.json",
    ] {
        if let Ok(bytes) = fs::read(root.join(name)) {
            private_write(&dir.join(name), &bytes).unwrap();
            println!(
                "hosted_wait_capture_snapshot_artifact={} bytes={} blake3={}",
                dir.join(name).display(),
                bytes.len(),
                blake3::hash(&bytes)
            );
        }
    }
    println!("{}", String::from_utf8_lossy(&output.stdout));
    println!("{}", String::from_utf8_lossy(&output.stderr));
}

pub(super) fn recover_parent(
    fixture: &Fixture,
    root: &Path,
    command: &mut std::process::Command,
    crashed: &std::process::Output,
    worker: thread::JoinHandle<Result<Vec<u8>, String>>,
    model_count: &std::sync::atomic::AtomicUsize,
    nonempty_cleanup: bool,
) {
    secure_artifact(fixture, root, crashed, "crash");
    let proof = worker
        .join()
        .expect("actual capture fault worker joins")
        .expect("actual capture rename failed");
    assert_eq!(crashed.status.code(), Some(73));
    assert!(
        String::from_utf8_lossy(&crashed.stdout)
            .contains("hosted_wait_capture_real_crash_boundary=true")
    );
    assert!(root.join("wait-capture-failed").exists());
    assert!(
        String::from_utf8_lossy(&crashed.stderr).contains("hosted_wait_capture_persistence_failed")
    );
    let before: serde_json::Value =
        serde_json::from_slice(&fs::read(root.join("wait-capture-before.json")).unwrap()).unwrap();
    let staged: serde_json::Value =
        serde_json::from_slice(&fs::read(root.join("wait-capture-staged.json")).unwrap()).unwrap();
    let after: serde_json::Value =
        serde_json::from_slice(&fs::read(root.join("wait-capture-after.json")).unwrap()).unwrap();
    assert!(before != staged && staged == after);
    for snapshot in [&before, &staged, &after] {
        let originals = snapshot["restored_queued_waits"]
            .as_object()
            .expect("actual decoded queued Wait provenance map");
        for (key, original) in originals {
            let request: oasis7::simulator::ContinuousAgentRequestContextV1 =
                serde_json::from_value(original["cognition"]["request"]["request_context"].clone())
                    .unwrap();
            request.validate_production_lane().unwrap();
            assert_eq!(*key, request.provider_invocation_key().to_string());
            assert_eq!(
                original["agent_id"].as_str(),
                Some(request.agent_subject.as_str())
            );
            assert!(
                snapshot["completed_queue"]
                    .as_array()
                    .unwrap()
                    .contains(original)
            );
        }
    }
    let value: serde_json::Value = serde_json::from_slice(&proof).unwrap();
    let queue = value["provider_completed_decisions"].as_array().unwrap();
    assert_eq!(queue.len(), 1);
    assert!(value["hosted_wait"].is_null());
    let cognition = &queue[0]["cognition"];
    let original: oasis7::simulator::ContinuousAgentRequestContextV1 =
        serde_json::from_value(cognition["request"]["request_context"].clone()).unwrap();
    original.validate_production_lane().unwrap();
    let canonical = fixture
        .driver
        .lock()
        .unwrap()
        .execution_world
        .capability_revocation_state()
        .world_service_results
        .values()
        .filter_map(|v| serde_json::from_value::<wire::CanonicalIntentResultV1>(v.clone()).ok())
        .collect::<Vec<_>>();
    assert!(!canonical.iter().any(|r|matches!(&r.request.signed_payload,WorldServicePayloadV1::Scheduler(s) if matches!(&s.request.operation,SchedulerOperationV1::AdmitContinuation(p) if p.origin_request_digest==original.request_digest.to_string()))));
    assert!(fs::read(root.join("wait-capture-checkpoint-backup.json")).unwrap() == proof);
    fs::remove_dir(root.join(STORE)).unwrap();
    fs::rename(
        root.join("wait-capture-checkpoint-backup.json"),
        root.join(STORE),
    )
    .unwrap();
    command
        .env_remove("PRE2_WAIT_CAPTURE_FS_ROOT")
        .env("PRE2_APP_ADMISSION", "wait-capture-recover");
    let cleanup_worker = nonempty_cleanup.then(|| {
        command.env("PRE2_WAIT_CLEANUP_FS_ROOT", root);
        application_hosted_wait_write_failure::start(root.to_path_buf())
    });
    let recovered = command.output().unwrap();
    secure_artifact(fixture, root, &recovered, "recover");
    if let Some(worker) = cleanup_worker {
        application_hosted_wait_write_failure::finish(
            fixture,
            root,
            &recovered,
            worker,
            model_count.load(Ordering::SeqCst),
        );
        let read = |name: &str| -> serde_json::Value {
            serde_json::from_slice(&fs::read(root.join(name)).unwrap()).unwrap()
        };
        let before = read("wait-cleanup-before.json");
        let staged = read("wait-cleanup-staged.json");
        let after = read("wait-cleanup-after.json");
        assert_eq!(before, after);
        let originals = before["restored_queued_waits"].as_object().unwrap();
        assert_eq!(originals.len(), 1);
        assert_eq!(
            originals.get(&original.provider_invocation_key().to_string()),
            Some(&queue[0])
        );
        assert_eq!(
            staged["restored_queued_waits"].as_object().unwrap().len(),
            0
        );
        assert_eq!(after["restored_queued_waits"].as_object().unwrap().len(), 1);
        println!(
            "PRE2_NONEMPTY_QUEUED_WAIT_ROLLBACK_PASSED actual_provenance_counts=1,0,1 full_snapshot_equal=true"
        );
    }
    let results=fixture.driver.lock().unwrap().execution_world.capability_revocation_state().world_service_results.values().filter_map(|v|serde_json::from_value::<wire::CanonicalIntentResultV1>(v.clone()).ok()).filter(|r|matches!(&r.request.signed_payload,WorldServicePayloadV1::Scheduler(s) if matches!(&s.request.operation,SchedulerOperationV1::AdmitContinuation(p) if p.origin_request_digest==original.request_digest.to_string()))).collect::<Vec<_>>();
    assert_eq!(results.len(), 1);
    assert!(results[0].rejected.is_none());
    let key = correlation::key_digest(&results[0].request.correlation.key).unwrap();
    let trace = fixture.lookup_digests.lock().unwrap();
    let submits = trace
        .iter()
        .filter(|v| **v == format!("submit:{key}"))
        .count();
    let lookups = trace.iter().filter(|v| *v == &key).count();
    drop(trace);
    println!(
        "hosted_wait_capture_actual_recovery crash73=true backup_unchanged=true disk_old_queue=true no_admit_before_crash=true original_submit_count={submits} original_lookup_count={lookups} typed_awaiting_failure={} recovery_exit={}",
        String::from_utf8_lossy(&recovered.stderr)
            .contains("hosted_wait_missing_awaiting_runtime_turn"),
        recovered.status
    );
    assert_eq!(submits, 1);
    assert!(lookups > 0);
    assert!(
        recovered.status.success(),
        "actual capture crash recovery failure; inspect preserved typed marker"
    );
    application_hosted_wait::report(fixture, root, model_count.load(Ordering::SeqCst));
}
