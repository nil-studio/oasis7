//! Real provider Wait twice must exhaust the existing canonical successor, not reset its budget.
use super::*;
use oasis7::viewer::ViewerRuntimeLiveServer;

#[test]
fn real_tcp_hosted_final_wait_budget_consumes_once_without_new_model_or_lease() {
    application_harness::run_isolated_application_mode(
        false,
        false,
        false,
        true,
        false,
        false,
        "hosted-final-budget",
    );
}

pub(super) struct Clock {
    stop: Arc<AtomicBool>,
    worker: Option<thread::JoinHandle<()>>,
}
impl Drop for Clock {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        if let Some(worker) = self.worker.take() {
            let joined = worker.join();
            if joined.is_err() {
                if thread::panicking() {
                    eprintln!("final-budget clock worker also failed during parent unwinding");
                } else {
                    panic!("final-budget clock worker failed");
                }
            }
        }
    }
}

pub(super) fn start_clock(fixture: &Fixture, root: std::path::PathBuf) -> Clock {
    let stop = Arc::new(AtomicBool::new(false));
    let stopped = stop.clone();
    let driver = fixture.driver.clone();
    let worker = thread::spawn(move || {
        let deadline = Instant::now() + Duration::from_secs(25);
        let mut selected = std::collections::BTreeSet::new();
        while !stopped.load(Ordering::SeqCst) && Instant::now() < deadline {
            for (file, marker) in [
                ("hosted-wait-origin.json", "hosted-wait-selected.json"),
                (
                    "hosted-resumed-model.json",
                    "hosted-final-budget-selected.json",
                ),
            ] {
                let bytes = match fs::read(root.join(file)) {
                    Ok(bytes) => bytes,
                    Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
                    Err(error) => panic!("final-budget clock marker read failed: {error}"),
                };
                let request: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
                let digest = request["request_digest"].as_str().unwrap();
                if selected.contains(digest) {
                    continue;
                }
                let mut guard = driver.lock().unwrap();
                let world = &guard.execution_world;
                let settled = world
                    .cognition_economy()
                    .unwrap()
                    .leases
                    .values()
                    .any(|lease| {
                        lease.request_digest == digest
                            && lease.status == oasis7_runtime::CognitionLeaseStatusV1::Settled
                    });
                let continuation = world
                    .active_cognition_continuations()
                    .unwrap()
                    .into_iter()
                    .find(|entry| {
                        if file == "hosted-resumed-model.json" {
                            entry.continuation_id == request["successor_id"].as_str().unwrap()
                        } else {
                            entry.origin_request_digest == digest
                        }
                    });
                if !settled {
                    continue;
                }
                let Some(continuation) = continuation else {
                    continue;
                };
                let wake = world
                    .cognition_in_flight_wakes()
                    .unwrap()
                    .into_iter()
                    .find(|wake| wake.continuation_id == continuation.continuation_id);
                if let Some(wake) = wake {
                    application_hosted_wait::publish_marker(&root.join(marker), &serde_json::to_vec(&serde_json::json!({
                        "continuation_id":continuation.continuation_id,"wake_id":wake.wake_id,
                        "origin_request_digest":digest,"remaining_budget":continuation.remaining_budget.value,
                        "settled_before_clock":true})).unwrap()).unwrap();
                    selected.insert(digest.to_owned());
                } else {
                    let mut before = serde_json::to_value(guard.execution_world.state()).unwrap();
                    before["time"] = serde_json::json!(0);
                    commit_request(&mut guard, 0, None);
                    let mut after = serde_json::to_value(guard.execution_world.state()).unwrap();
                    after["time"] = serde_json::json!(0);
                    assert_eq!(
                        before, after,
                        "canonical empty clock must preserve gameplay state"
                    );
                }
            }
            if selected.len() == 2 {
                return;
            }
            thread::sleep(Duration::from_millis(10));
        }
    });
    Clock {
        stop,
        worker: Some(worker),
    }
}

pub(super) fn verify(client: &RemoteWorldServiceClient) {
    let root = std::path::PathBuf::from(std::env::var("PRE2_METADATA_DIR").unwrap());
    let mode = std::env::var("PRE2_APP_ADMISSION").unwrap();
    let recovery = mode == "hosted-final-budget-recover";
    if !recovery {
        application_fresh::preflight(client, &root);
    } else {
        assert!(
            root.join("fresh-provider-preflight-complete").exists(),
            "same fixture configuration was actually validated by original process"
        );
    }
    let server = ViewerRuntimeLiveServer::new(application_hosted::server_config(
        client,
        true,
        Duration::from_millis(200),
        Some(root.join("hosted-final-budget-private-lineage.json")),
    ))
    .unwrap();
    if mode == "hosted-final-budget-crash" {
        application_hosted_final_budget_recovery::start_crash_watcher(root.clone());
    }
    let shared = Arc::new(Mutex::new(server));
    let mut session = application_stream_boundaries::Session::start(&shared, false);
    let warmed = session.warm();
    session
        .send(serde_json::json!({"type":"live_control","mode":{"mode":if recovery { "pause" } else { "play" }},"request_id":1801}))
        .unwrap();
    session.send(serde_json::json!({"type":"hello_v2","client":"PRE2 final budget","version":2,"capabilities":[]})).unwrap();
    session
        .send(serde_json::json!({"type":"request_snapshot"}))
        .unwrap();
    let ordered = session.snapshot_ordered(Duration::from_secs(3), true);
    if recovery {
        let status = shared.lock().unwrap().test_agent_service_pump_status();
        assert_eq!(status["play_enabled"], false);
        assert_eq!(
            status["eligible"], false,
            "issued final Consume must reconcile while paused"
        );
    }
    let deadline = Instant::now() + Duration::from_secs(14);
    let mut completed = false;
    let mut summary = serde_json::Value::Null;
    while Instant::now() < deadline {
        session.snapshot(Duration::from_millis(25));
        if let Ok(guard) = shared.try_lock() {
            summary = guard.test_canonical_provider_diagnostic_summary();
            completed = summary["terminal_states"]["agent-a"]["status"] == "completed";
            if completed {
                break;
            }
        }
    }
    session
        .send(serde_json::json!({"type":"live_control","mode":{"mode":"pause"},"request_id":1802}))
        .unwrap();
    session
        .send(serde_json::json!({"type":"request_snapshot"}))
        .unwrap();
    let drained = session.snapshot(Duration::from_secs(2));
    let joined = session.close();
    fs::write(
        root.join("hosted-final-budget-summary.json"),
        serde_json::to_vec(&summary).unwrap(),
    )
    .unwrap();
    println!(
        "hosted_final_budget_child warmed={warmed} ordered={ordered} completed={completed} drained={drained} worker={joined:?}"
    );
    assert!(
        warmed && ordered && drained && joined.is_ok(),
        "actual Viewer prerequisites and complete worker join; diagnostic={}",
        application_completion::hosted_failure_diagnostic(&summary)
    );
    assert!(
        completed,
        "second ordinary Wait must preserve remaining unit and finish canonical Completed zero; diagnostic={}",
        application_completion::hosted_failure_diagnostic(&summary)
    );
    println!("PRE2_HOSTED_FINAL_BUDGET_PASSED");
}

pub(super) fn report(
    fixture: &Fixture,
    root: &std::path::Path,
    output: &std::process::Output,
    models: usize,
) {
    fixture
        .finish_http_workers()
        .expect("all actual HTTP worker joins precede completion assertions");
    let world = fixture.driver.lock().unwrap().execution_world.clone();
    let results = world
        .capability_revocation_state()
        .world_service_results
        .values()
        .filter_map(|value| {
            serde_json::from_value::<wire::CanonicalIntentResultV1>(value.clone()).ok()
        })
        .collect::<Vec<_>>();
    let count = |kind: &str| {
        results.iter().filter(|result| result.rejected.is_none() && matches!(&result.request.signed_payload,
        WorldServicePayloadV1::Scheduler(signed) if matches!((&signed.request.operation, kind),
            (SchedulerOperationV1::ReserveLease(_), "reserve") | (SchedulerOperationV1::ProviderPrefix {..}, "prefix")
            | (SchedulerOperationV1::SettleLease {..}, "settle") | (SchedulerOperationV1::ResumeWake {..}, "resume")
            | (SchedulerOperationV1::AdmitContinuation(_), "admit") | (SchedulerOperationV1::ConsumeContinuationBudget {..}, "consume")))).count()
    };
    println!(
        "hosted_final_budget_actual models={models} reserve={} prefix={} settle={} resume={} admit={} consume={} active={} wakes={}",
        count("reserve"),
        count("prefix"),
        count("settle"),
        count("resume"),
        count("admit"),
        count("consume"),
        world.active_cognition_continuations().unwrap().len(),
        world.cognition_in_flight_wakes().unwrap().len()
    );
    println!("{}", String::from_utf8_lossy(&output.stdout));
    assert!(
        output.status.success(),
        "actual isolated final-budget process failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        (models, count("reserve"), count("prefix"), count("settle")),
        (2, 2, 2, 2),
        "final wake must not invoke a third model or lease"
    );
    assert_eq!(
        (count("resume"), count("admit"), count("consume")),
        (1, 1, 1),
        "reuse genuine Resume successor, never another Admit"
    );
    let selected: serde_json::Value =
        serde_json::from_slice(&fs::read(root.join("hosted-final-budget-selected.json")).unwrap())
            .unwrap();
    assert_eq!(selected["remaining_budget"], 1);
    let consume = results
        .iter()
        .find(|result| {
            result.rejected.is_none()
                && matches!(&result.request.signed_payload,
        WorldServicePayloadV1::Scheduler(signed) if matches!(&signed.request.operation,
            SchedulerOperationV1::ConsumeContinuationBudget {continuation_id,budget_spent:1,..}
            if continuation_id == selected["continuation_id"].as_str().unwrap()))
        })
        .unwrap();
    let receipt: oasis7_runtime::CognitionBudgetConsumptionV1 =
        serde_json::from_value(consume.receipt.clone()).unwrap();
    assert_eq!(
        receipt.status,
        oasis7_runtime::ContinuationStatusV1::Completed
    );
    assert_eq!(receipt.remaining_budget.value, 0);
    assert!(
        world
            .active_cognition_continuations()
            .unwrap()
            .iter()
            .all(|entry| entry.status == oasis7_runtime::ContinuationStatusV1::Consumed),
        "only historical consumed predecessors may remain"
    );
    assert!(world.cognition_in_flight_wakes().unwrap().is_empty());
}
