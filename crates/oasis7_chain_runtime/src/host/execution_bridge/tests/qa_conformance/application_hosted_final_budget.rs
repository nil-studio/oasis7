//! Real provider Wait twice must exhaust the existing canonical successor, not reset its budget.
use super::*;
use oasis7_viewer_runtime::viewer::ViewerRuntimeLiveServer;
#[path = "application_hosted_final_budget_window.rs"]
mod completion_window;

// Same-machine native CI diagnostic only: raw Instant Debug is implementation
// specific, not a portable clock contract. Record it with local elapsed time;
// never substitute wall-clock timestamps or print requests/keys/digests.
fn timing(event: &str, started: Instant, facts: serde_json::Value) {
    let now = Instant::now();
    eprintln!(
        "hosted_final_budget_timing={}",
        serde_json::json!({
            "event": event, "monotonic_raw": format!("{now:?}"),
            "process_elapsed_us": now.duration_since(started).as_micros(), "facts": facts,
        })
    );
}

fn canonical_outcome(rejected: Option<&str>) -> &'static str {
    match rejected {
        None => "committed",
        Some("scheduler base binding changed") => "rejected_base_binding_changed",
        Some("cognition_provisioning_binding_mismatch") => "rejected_provisioning_binding_mismatch",
        Some(_) => "rejected_other",
    }
}

// Failure output must survive CI without publishing the private canonical payloads.
// Classify the authority's actual results, rather than inferring rejection from
// a repeated Lookup or from the App's last pump error.
pub(super) fn canonical_diagnostic<'a>(
    values: impl Iterator<Item = &'a serde_json::Value>,
) -> serde_json::Value {
    let mut stages = std::collections::BTreeMap::<&str, serde_json::Value>::new();
    let mut undecodable = 0;
    for value in values {
        let Ok(result) = serde_json::from_value::<wire::CanonicalIntentResultV1>(value.clone())
        else {
            undecodable += 1;
            continue;
        };
        let WorldServicePayloadV1::Scheduler(signed) = &result.request.signed_payload else {
            continue;
        };
        let stage = match &signed.request.operation {
            SchedulerOperationV1::ReserveLease(_) => "reserve",
            SchedulerOperationV1::ProviderPrefix { .. } => "prefix",
            SchedulerOperationV1::SettleLease { .. } => "settle",
            SchedulerOperationV1::ResumeWake { .. } => "resume",
            SchedulerOperationV1::AdmitContinuation(_) => "admit",
            SchedulerOperationV1::ConsumeContinuationBudget { .. } => "consume",
            _ => "other",
        };
        let outcome = canonical_outcome(result.rejected.as_deref());
        let counts = stages.entry(stage).or_insert_with(|| serde_json::json!({}));
        counts[outcome] = serde_json::json!(counts[outcome].as_u64().unwrap_or(0) + 1);
    }
    serde_json::json!({"scheduler_results": stages, "undecodable_results": undecodable})
}

#[test]
fn final_budget_canonical_diagnostic_does_not_publish_unparsed_private_payloads() {
    assert_eq!(canonical_outcome(None), "committed");
    assert_eq!(
        canonical_outcome(Some("scheduler base binding changed")),
        "rejected_base_binding_changed"
    );
    assert_eq!(
        canonical_outcome(Some("scheduler base binding changed; private detail")),
        "rejected_other"
    );
    assert_eq!(
        canonical_outcome(Some("private rejection")),
        "rejected_other"
    );
    let private = serde_json::json!({"private_request": "must-not-appear"});
    assert_eq!(
        canonical_diagnostic(std::iter::once(&private)),
        serde_json::json!({"scheduler_results": {}, "undecodable_results": 1})
    );
}

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
        let started = Instant::now();
        let deadline = started + Duration::from_secs(25);
        timing(
            "clock_started",
            started,
            serde_json::json!({"budget_ms":25000}),
        );
        let mut selected = std::collections::BTreeSet::new();
        let mut handoff = application_hosted_final_budget_clock::Handoff::default();
        while !stopped.load(Ordering::SeqCst) && Instant::now() < deadline {
            let mut origin_wake_created = false;
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
                if !completion_window::clock_ready(&root, file) {
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
                let mut wake = world
                    .cognition_in_flight_wakes()
                    .unwrap()
                    .into_iter()
                    .find(|wake| wake.continuation_id == continuation.continuation_id);
                if wake.is_none() {
                    let mut before = serde_json::to_value(guard.execution_world.state()).unwrap();
                    before["time"] = serde_json::json!(0);
                    timing(
                        "clock_empty_commit_begin",
                        started,
                        serde_json::json!({
                            "marker":file,"authority_height":guard.execution_world.state().time,
                        }),
                    );
                    commit_request(&mut guard, 0, None);
                    timing(
                        "clock_empty_commit_completed",
                        started,
                        serde_json::json!({
                            "marker":file,"authority_height":guard.execution_world.state().time,
                            "authority_wake_count":guard.execution_world.cognition_in_flight_wakes().unwrap().len(),
                        }),
                    );
                    let mut after = serde_json::to_value(guard.execution_world.state()).unwrap();
                    after["time"] = serde_json::json!(0);
                    assert_eq!(
                        before, after,
                        "canonical empty clock must preserve gameplay state"
                    );
                    // Keep the driver locked through selection: the App may
                    // consume this wake before the next clock poll. Missing it
                    // would advance the old continuation again and stale the
                    // resumed turn's already captured Reserve binding.
                    wake = guard
                        .execution_world
                        .cognition_in_flight_wakes()
                        .unwrap()
                        .into_iter()
                        .find(|wake| wake.continuation_id == continuation.continuation_id);
                    origin_wake_created |= file == "hosted-wait-origin.json" && wake.is_some();
                }
                if let Some(wake) = wake {
                    application_hosted_wait::publish_marker(&root.join(marker), &serde_json::to_vec(&serde_json::json!({
                        "continuation_id":continuation.continuation_id,"wake_id":wake.wake_id,
                        "origin_request_digest":digest,"remaining_budget":continuation.remaining_budget.value,
                        "settled_before_clock":true})).unwrap()).unwrap();
                    timing(
                        "clock_selected_published",
                        started,
                        serde_json::json!({
                            "marker":marker,"authority_height":guard.execution_world.state().time,
                            "remaining_budget":continuation.remaining_budget.value,
                            "authority_wake_count":guard.execution_world.cognition_in_flight_wakes().unwrap().len(),
                        }),
                    );
                    selected.insert(digest.to_owned());
                }
            }
            handoff.after_iteration(&root, origin_wake_created);
            if selected.len() == 2 {
                timing(
                    "clock_finished",
                    started,
                    serde_json::json!({"selected_count":2}),
                );
                return;
            }
            thread::sleep(Duration::from_millis(10));
        }
        timing(
            "clock_stopped",
            started,
            serde_json::json!({
                "selected_count":selected.len(),"deadline_reached":Instant::now() >= deadline,
                "stop_requested":stopped.load(Ordering::SeqCst),
            }),
        );
    });
    Clock {
        stop,
        worker: Some(worker),
    }
}

pub(super) fn verify(client: &RemoteWorldServiceClient) {
    let started = Instant::now();
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
    // The producer may use its existing 25s to deliver the final selected wake.
    // Only then does the unchanged 14s final-consumption observation begin.
    let mut window = completion_window::Window::new(application_completion::child_deadline());
    let mut completed = false;
    let mut summary = serde_json::Value::Null;
    while window.keep_waiting(&root, started) {
        session.snapshot(Duration::from_millis(25));
        if let Ok(guard) = shared.try_lock() {
            summary = guard.test_canonical_provider_diagnostic_summary();
            completed = summary["terminal_states"]["agent-a"]["status"] == "completed";
            if completed {
                break;
            }
        }
    }
    timing(
        "child_completion_window_finished",
        started,
        serde_json::json!({
            "completed":completed,"deadline_reached":window.deadline_reached(),
            "phase":window.phase(),
            "host":application_completion::hosted_failure_diagnostic(&summary),
        }),
    );
    timing("child_pause_send", started, serde_json::json!({}));
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
    println!(
        "hosted_final_budget_canonical_diagnostic={}",
        canonical_diagnostic(
            world
                .capability_revocation_state()
                .world_service_results
                .values()
        )
    );
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
