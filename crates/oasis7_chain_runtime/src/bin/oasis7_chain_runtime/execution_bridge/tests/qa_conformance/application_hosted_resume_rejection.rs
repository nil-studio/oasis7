//! Real stale-base Resume rejection retains the original selected wake authority.
use super::*;
use oasis7::viewer::ViewerRuntimeLiveServer;
#[test]
fn real_tcp_hosted_resume_rejection_compensates_original_selected_wake() {
    application_harness::run_isolated_application_mode(
        false,
        false,
        false,
        true,
        false,
        false,
        "resume-rejected",
    );
}
pub(super) fn observation(
    fixture: &Fixture,
    root: std::path::PathBuf,
) -> provider_metadata::DecisionObservation {
    let actual = application_hosted_wait::observation(fixture, root.clone());
    let count = std::sync::atomic::AtomicUsize::new(0);
    Arc::new(move |request| {
        let n = count.fetch_add(1, Ordering::SeqCst) + 1;
        fs::write(
            root.join("resume-rejected-model-count.json"),
            serde_json::to_vec(&n).unwrap(),
        )
        .unwrap();
        actual(request);
    })
}
pub(super) fn start(
    fixture: &Fixture,
    root: std::path::PathBuf,
) -> thread::JoinHandle<SubmitIntentRequest<WorldServicePayloadV1>> {
    let gate = fixture.world_gate.clone();
    let driver = fixture.driver.clone();
    let mut config = fixture.client.config().clone();
    config.scope_id = "agent:agent-a".into();
    thread::spawn(move || {
        let deadline = Instant::now() + Duration::from_secs(15);
        let original = loop {
            if let Some(request) = gate.rejected_resume_candidate() {
                break request;
            }
            assert!(
                Instant::now() < deadline,
                "actual original Resume boundary absent"
            );
            thread::sleep(Duration::from_millis(2));
        };
        let WorldServicePayloadV1::Scheduler(signed) = &original.signed_payload else {
            unreachable!()
        };
        let SchedulerOperationV1::ResumeWake {
            wake_id, proposal, ..
        } = &signed.request.operation
        else {
            unreachable!()
        };
        let client = RemoteWorldServiceClient::new(config).unwrap();
        let read = || ReadWorldViewRequest {
            observation_nonce: None,
            formal_anchor: None,
            contract_version: 1,
            world: client.config().expected_world.clone(),
            scope_id: client.config().scope_id.clone(),
            min_commit: None,
            fixed_commit: None,
            deadline_unix_ms: None,
        };
        let before = client.read_view(read()).unwrap();
        {
            let mut guard = driver.lock().unwrap();
            assert!(
                guard
                    .execution_world
                    .cognition_in_flight_wakes()
                    .unwrap()
                    .iter()
                    .any(|w| &w.wake_id == wake_id)
            );
            let origin: serde_json::Value =
                serde_json::from_slice(&fs::read(root.join("hosted-wait-origin.json")).unwrap())
                    .unwrap();
            let selected: serde_json::Value =
                serde_json::from_slice(&fs::read(root.join("hosted-wait-selected.json")).unwrap())
                    .unwrap();
            assert_eq!(wake_id, selected["wake_id"].as_str().unwrap());
            assert_eq!(selected["origin_request_digest"], origin["request_digest"]);
            assert_eq!(proposal.agent_id, "agent-a");
            assert!(
                guard
                    .execution_world
                    .cognition_economy()
                    .unwrap()
                    .leases
                    .values()
                    .any(|l| l.lease_id == origin["lease_id"].as_str().unwrap()
                        && l.status == oasis7_runtime::CognitionLeaseStatusV1::Settled)
            );
            let height = guard.state.last_applied_committed_height + 1;
            commit_request(&mut guard, height, None);
        }
        let after = client.read_view(read()).unwrap();
        assert_eq!(
            before.version().commit.binding,
            after.version().commit.binding
        );
        assert_eq!(before.version().commit.world, after.version().commit.world);
        assert!(after.version().commit.position > before.version().commit.position);
        gate.release_rejected_resume();
        let deadline = Instant::now() + Duration::from_secs(3);
        loop {
            let response = client
                .lookup(
                    LookupIntentRequest {
                        contract_version: 1,
                        key: original.correlation.key.clone(),
                    },
                    original.signed_payload.clone(),
                )
                .unwrap();
            if matches!(response.outcome, IntentOutcome::Rejected { .. }) {
                application_hosted_wait_rejection::private_write(
                    &root.join("resume-rejected-authenticated.json"),
                    &serde_json::to_vec(&response).unwrap(),
                )
                .unwrap();
                break;
            }
            assert!(
                Instant::now() < deadline,
                "real original Resume Rejected Lookup absent"
            );
            thread::sleep(Duration::from_millis(2));
        }
        original
    })
}
pub(super) fn verify(client: &RemoteWorldServiceClient) {
    let root = std::path::PathBuf::from(std::env::var("PRE2_METADATA_DIR").unwrap());
    if std::env::var("PRE2_APP_ADMISSION").as_deref() == Ok("resume-rejection-recover") {
        assert!(
            root.join("fresh-provider-counter-reset").exists(),
            "same preflight was completed before the actual crash"
        );
    } else {
        application_fresh::preflight(client, &root);
    }
    let server = ViewerRuntimeLiveServer::new(application_hosted::server_config(
        client,
        true,
        Duration::from_millis(200),
        Some(root.join(application_hosted_wait_rejection::STORE)),
    ))
    .unwrap();
    if std::env::var("PRE2_APP_ADMISSION").as_deref() == Ok("resume-rejection-crash") {
        let crash_root = root.clone();
        thread::spawn(move || {
            let deadline = Instant::now() + Duration::from_secs(15);
            while !crash_root.join("resume-rejection-before-persist").exists()
                && Instant::now() < deadline
            {
                thread::sleep(Duration::from_millis(2));
            }
            assert!(
                crash_root.join("resume-rejection-before-persist").exists(),
                "actual sole rejection persistence boundary absent"
            );
            let bytes =
                fs::read(crash_root.join(application_hosted_wait_rejection::STORE)).unwrap();
            let checkpoint: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
            assert_eq!(checkpoint["hosted_resume"]["stage"], "rejected_finalize");
            assert!(checkpoint["hosted_resume"]["rejection"].is_object());
            assert!(checkpoint["hosted_resume"]["rejected_predecessor"].is_object());
            application_hosted_wait_rejection::private_write(
                &crash_root.join("resume-rejection-crash-checkpoint.json"),
                &bytes,
            )
            .unwrap();
            println!(
                "PRE2_REJECTED_RESUME_CRASH_BOUNDARY true_process_exit=73 checkpoint_blake3={}",
                blake3::hash(&bytes)
            );
            use std::io::Write;
            std::io::stdout().flush().unwrap();
            std::io::stderr().flush().unwrap();
            std::process::exit(73);
        });
    }
    let recovery = std::env::var("PRE2_APP_ADMISSION").as_deref() == Ok("resume-rejection-recover");
    let shared = Arc::new(Mutex::new(server));
    let mut session = application_stream_boundaries::Session::start(&shared, false);
    let warmed = session.warm();
    session
        .send(serde_json::json!({"type":"live_control","mode":{"mode":if recovery { "pause" } else { "play" }},"request_id":1801}))
        .unwrap();
    session.send(serde_json::json!({"type":"hello_v2","client":"PRE2 Resume rejection","version":2,"capabilities":[]})).unwrap();
    session
        .send(serde_json::json!({"type":"request_snapshot"}))
        .unwrap();
    let ordered = session.snapshot_ordered(Duration::from_secs(3), true);
    let initial_status = shared.lock().unwrap().test_agent_service_pump_status();
    let eligible = initial_status["eligible"] == !recovery;
    if recovery {
        assert_eq!(
            initial_status["play_enabled"], false,
            "restore issued Resume while paused; no new admission"
        );
    }
    let deadline = Instant::now() + Duration::from_secs(15);
    // Pause once the original signed Resume is issued, rather than delaying its capture.
    while !root.join("resume-rejected-original.json").exists() && Instant::now() < deadline {
        session.snapshot(Duration::from_millis(25));
    }
    session
        .send(serde_json::json!({"type":"live_control","mode":{"mode":"pause"},"request_id":1802}))
        .unwrap();
    session.send(serde_json::json!({"type":"hello_v2","client":"PRE2 rejected issued Resume paused","version":2,"capabilities":[]})).unwrap();
    session
        .send(serde_json::json!({"type":"request_snapshot"}))
        .unwrap();
    let paused = session.snapshot_ordered(Duration::from_secs(3), true);
    let playing_false =
        shared.lock().unwrap().test_agent_service_pump_status()["play_enabled"] == false;
    let rejection_deadline = Instant::now() + Duration::from_secs(3);
    while !root.join("resume-rejected-authenticated.json").exists()
        && Instant::now() < rejection_deadline
    {
        session.snapshot(Duration::from_millis(25));
    }
    let until = Instant::now() + Duration::from_secs(8);
    let mut terminal_completed = false;
    while Instant::now() < until {
        session.snapshot(Duration::from_millis(25));
        let summary = shared
            .lock()
            .unwrap()
            .test_canonical_provider_diagnostic_summary();
        if summary["terminal_states"]["agent-a"]["status"] == "rejected" {
            terminal_completed = true;
            break;
        }
    }
    let pump_status = shared.lock().unwrap().test_agent_service_pump_status();
    println!(
        "resume_rejection_terminal_wait completed={terminal_completed} pump_status={pump_status}"
    );
    let summary = shared
        .lock()
        .unwrap()
        .test_canonical_provider_diagnostic_summary();
    application_hosted_wait_rejection::private_write(
        &root.join("resume-rejected-summary.json"),
        &serde_json::to_vec(&summary).unwrap(),
    )
    .unwrap();
    session
        .send(serde_json::json!({"type":"request_snapshot"}))
        .unwrap();
    let drained = session.snapshot(Duration::from_secs(2));
    let worker = session.close();
    println!(
        "resume_rejected_child warmed={warmed} ordered={ordered} eligible={eligible} paused={paused} playing_false={playing_false} drained={drained} worker={worker:?} terminal_completed={terminal_completed}"
    );
    assert!(
        warmed
            && ordered
            && eligible
            && paused
            && playing_false
            && drained
            && worker.is_ok()
            && terminal_completed
    );
}
pub(super) fn finish(
    fixture: &Fixture,
    root: &std::path::Path,
    output: &std::process::Output,
    worker: thread::JoinHandle<SubmitIntentRequest<WorldServicePayloadV1>>,
) {
    let original = worker.join().unwrap();
    application_hosted_wait_rejection::secure_artifact(fixture, root, output, "resume-rejected");
    let evidence = temp_dir("pre2-resume-rejected-evidence");
    fs::create_dir_all(&evidence).unwrap();
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(&evidence, fs::Permissions::from_mode(0o700)).unwrap();
    for name in [
        "resume-rejected-original.json",
        "resume-rejected-authenticated.json",
        "resume-rejected-summary.json",
    ] {
        application_hosted_wait_rejection::private_write(
            &evidence.join(name),
            &fs::read(root.join(name)).unwrap(),
        )
        .unwrap();
    }
    let world = fixture.driver.lock().unwrap().execution_world.clone();
    let key = correlation::key_digest(&original.correlation.key).unwrap();
    let results = world
        .capability_revocation_state()
        .world_service_results
        .values()
        .filter_map(|v| serde_json::from_value::<wire::CanonicalIntentResultV1>(v.clone()).ok())
        .collect::<Vec<_>>();
    let result = results.iter().find(|r| r.request == original).unwrap();
    application_hosted_wait_rejection::private_write(
        &evidence.join("canonical-rejected.json"),
        &serde_json::to_vec(result).unwrap(),
    )
    .unwrap();
    let rejected = serde_json::to_string(&result.rejected)
        .unwrap()
        .contains("scheduler base binding changed");
    let trace = fixture.lookup_digests.lock().unwrap();
    let submits = trace
        .iter()
        .filter(|s| **s == format!("submit:{key}"))
        .count();
    let lookups = trace.iter().filter(|s| **s == key).count();
    drop(trace);
    let summary: serde_json::Value =
        serde_json::from_slice(&fs::read(root.join("resume-rejected-summary.json")).unwrap())
            .unwrap();
    let memory = summary["memory_store"]["entries"]
        .as_object()
        .is_some_and(|v| v.is_empty())
        || summary["memory_store"]["entries"]
            .as_array()
            .is_some_and(|v| v.is_empty());
    let model: usize =
        serde_json::from_slice(&fs::read(root.join("resume-rejected-model-count.json")).unwrap())
            .unwrap();
    let terminal = summary["terminal_states"]["agent-a"]["status"] == "rejected";
    let origin: serde_json::Value =
        serde_json::from_slice(&fs::read(root.join("hosted-wait-origin.json")).unwrap()).unwrap();
    let selected: serde_json::Value =
        serde_json::from_slice(&fs::read(root.join("hosted-wait-selected.json")).unwrap()).unwrap();
    let active = world.active_cognition_continuations().unwrap();
    let predecessor_retained = active
        .iter()
        .any(|entry| entry.continuation_id == selected["continuation_id"].as_str().unwrap());
    let wake_retained = world
        .cognition_in_flight_wakes()
        .unwrap()
        .iter()
        .any(|wake| wake.wake_id == selected["wake_id"].as_str().unwrap());
    let old_settled = world
        .cognition_economy()
        .unwrap()
        .leases
        .values()
        .any(|lease| {
            lease.lease_id == origin["lease_id"].as_str().unwrap()
                && lease.status == oasis7_runtime::CognitionLeaseStatusV1::Settled
        });
    let no_successor = active.is_empty();
    let predecessors: Vec<oasis7_runtime::AgentContinuation> =
        serde_json::from_value(world.cognition_continuations()).unwrap();
    let predecessor_rejected = predecessors.iter().any(|entry| {
        entry.continuation_id == selected["continuation_id"].as_str().unwrap()
            && entry.status == oasis7_runtime::ContinuationStatusV1::Rejected
    });
    let reserve_count = results.iter().filter(|result| matches!(&result.request.signed_payload, WorldServicePayloadV1::Scheduler(signed) if matches!(signed.request.operation, SchedulerOperationV1::ReserveLease(_)))).count();
    let prefix_count = results.iter().filter(|result| matches!(&result.request.signed_payload, WorldServicePayloadV1::Scheduler(signed) if matches!(signed.request.operation, SchedulerOperationV1::ProviderPrefix { .. }))).count();
    let settle_count = results.iter().filter(|result| matches!(&result.request.signed_payload, WorldServicePayloadV1::Scheduler(signed) if matches!(signed.request.operation, SchedulerOperationV1::SettleLease { .. }))).count();
    println!(
        "resume_rejected_lifecycle predecessor_retained={predecessor_retained} wake_retained={wake_retained} old_wait_settled={old_settled} no_successor={no_successor} reserves={reserve_count} prefixes={prefix_count} settles={settle_count}"
    );

    let http = fixture.finish_http_workers();
    let joins = fixture.world_gate.rejected_resume_join_proof();
    println!(
        "resume_rejected_parent evidence_dir={} actual_base_rejected={rejected} original_submit={submits} original_lookup={lookups} model_count={model} memory_unchanged={memory} terminal_compensated={terminal} http_join={http:?} actual_gate_join={joins}",
        evidence.display()
    );
    assert!(
        output.status.success()
            && rejected
            && submits == 1
            && lookups > 0
            && model == 1
            && memory
            && http.is_ok()
            && joins,
        "real rejected Resume prerequisites and strict cleanup"
    );
    assert!(
        !predecessor_retained
            && !wake_retained
            && predecessor_rejected
            && old_settled
            && no_successor
            && reserve_count == 1
            && prefix_count == 1
            && settle_count == 1,
        "exact selected predecessor and original settled Wait economy"
    );
    assert!(
        terminal,
        "original rejected Resume must compensate its exact selected predecessor"
    );
}
