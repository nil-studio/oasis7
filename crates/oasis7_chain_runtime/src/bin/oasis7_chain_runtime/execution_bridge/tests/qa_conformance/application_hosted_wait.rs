//! Ordinary HTTP-provider Wait; observations precede the actual model response.
use super::*;
use oasis7::viewer::ViewerRuntimeLiveServer;

#[test]
fn real_tcp_hosted_ordinary_wait_resume_act_preserves_canonical_identity() {
    application_harness::run_isolated_application_mode(
        false,
        false,
        false,
        true,
        false,
        false,
        "hosted-wait",
    );
}

/// Successful signed View responses remain valid while cumulative stages exceed
/// the old twelve-second observation window. The child isolates the delay env.
#[test]
fn real_tcp_hosted_wait_delayed_success_preserves_completion_oracle() {
    let root = temp_dir("qa-hosted-wait-delayed-completion");
    fs::create_dir_all(&root).unwrap();
    let mut command = std::process::Command::new(std::env::current_exe().unwrap());
    command.args([
        "--exact",
        "execution_bridge_real_tests::real_execution_bridge::tests::qa_conformance::application_hosted_wait::real_tcp_hosted_ordinary_wait_resume_act_preserves_canonical_identity",
        "--nocapture",
    ]);
    command.env("PRE2_CONFORMANCE_VIEW_DELAY_MS", "1000");
    let started = Instant::now();
    let output = application_completion::run_child(&mut command, &root);
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    println!(
        "delayed_hosted_wait elapsed_ms={} exit={}\n{stdout}\n{stderr}",
        started.elapsed().as_millis(),
        output.status
    );
    fs::remove_dir_all(root).unwrap();
    assert!(
        output.status.success(),
        "delayed valid HostedWait must pass original canonical oracle"
    );
    assert!(stdout.contains("PRE2_HOSTED_WAIT_RESUME_ACT_PASSED"));
    assert!(
        started.elapsed() > Duration::from_secs(12),
        "causal delay must exceed former observation window"
    );
}

pub(super) fn observation(
    fixture: &Fixture,
    root: std::path::PathBuf,
) -> provider_metadata::DecisionObservation {
    let driver = fixture.driver.clone();
    let mut config = fixture.client.config().clone();
    config.scope_id = "agent:agent-a".into();
    let mut read = fixture.view(None);
    read.scope_id = "agent:agent-a".into();
    Arc::new(move |incoming| {
        incoming.validate_production_lane().unwrap();
        let invocation = incoming.provider_invocation_key().to_string();
        let digest = incoming.request_digest.to_string();
        let results = {
            let guard = driver.lock().unwrap();
            guard
                .execution_world
                .capability_revocation_state()
                .world_service_results
                .values()
                .filter_map(|value| {
                    serde_json::from_value::<wire::CanonicalIntentResultV1>(value.clone()).ok()
                })
                .filter(|result| result.rejected.is_none())
                .collect::<Vec<_>>()
        }; // Authority lock is released before the independent signed ReadView.
        let reserve = results.iter().find(|result| matches!(&result.request.signed_payload,
            WorldServicePayloadV1::Scheduler(signed) if matches!(&signed.request.operation,
                SchedulerOperationV1::ReserveLease(request) if request.agent_id == incoming.agent_subject
                && request.agent_session_id == incoming.agent_session_id && request.agent_turn_id == incoming.agent_turn_id
                && request.decision_request_id == incoming.decision_request_id && request.request_digest == digest
                && request.idempotency_key == invocation && request.quote.authority_context == incoming.capability_invocation_context_digest.to_string()
                && request.quote.world_binding == incoming.runtime_binding.base_world_hash.to_string()
                && request.account_id == request.quote.payer_id))).expect("actual committed Reserve before model response");
        let mut logical = incoming.clone();
        logical.transport_attempt = 1;
        let context_digest =
            oasis7::simulator::h_v1("oasis7.cognition.context.v1", &logical).to_string();
        let incoming_bytes = serde_cbor::to_vec(incoming).expect("encode original host request");
        let prefix = results.iter().find(|result| matches!(&result.request.signed_payload,
            WorldServicePayloadV1::Scheduler(signed) if matches!(&signed.request.operation,
                SchedulerOperationV1::ProviderPrefix{request,context_digest:actual} if serde_cbor::to_vec(request).expect("encode contract request") == incoming_bytes && actual == &context_digest)))
            .expect("actual committed exact ProviderPrefix before model response");
        assert_eq!(prefix.receipt["dispatched"], true);
        assert_eq!(prefix.receipt["request_digest"], digest);
        let client = RemoteWorldServiceClient::new(config.clone()).unwrap();
        let IntentOutcome::Committed { commit, .. } = client
            .lookup(
                LookupIntentRequest {
                    contract_version: 1,
                    key: prefix.request.correlation.key.clone(),
                },
                prefix.request.signed_payload.clone(),
            )
            .unwrap()
            .outcome
        else {
            panic!("incoming model callback needs actual signed Prefix Lookup commit");
        };
        let mut minimum_read = read.clone();
        minimum_read.min_commit = Some(*commit);
        let view = client.read_view(minimum_read).unwrap();
        let lease = view
            .projection()
            .cognition_leases
            .iter()
            .find(|lease| {
                lease.request_digest == digest
                    && lease.agent_session_id == incoming.agent_session_id
                    && lease.agent_turn_id == incoming.agent_turn_id
                    && lease.decision_request_id == incoming.decision_request_id
                    && lease.status == oasis7_runtime::CognitionLeaseStatusV1::Reserved
            })
            .expect("independent signed view has exact Reserved lease");
        let evidence = serde_json::json!({"request_digest":digest,"agent_turn_id":incoming.agent_turn_id,
            "agent_session_id":incoming.agent_session_id,"decision_request_id":incoming.decision_request_id,"base_tick":incoming.runtime_binding.base_tick,
            "reserve_height":reserve.committed_height,"prefix_height":prefix.committed_height,
            "lease_id":lease.lease_id,"reserve_committed":true,"prefix_exact_request":true,"signed_reserved_lease":true});
        if let Ok(bytes) = fs::read(root.join("hosted-wait-origin.json")) {
            let origin: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
            let resume_result = results.iter().find(|result| matches!(&result.request.signed_payload,
                WorldServicePayloadV1::Scheduler(signed) if matches!(&signed.request.operation,
                    SchedulerOperationV1::ResumeWake{proposal,resume,..} if proposal.origin_request_digest == origin["request_digest"].as_str().unwrap()
                    && resume.request_digest == digest && resume.agent_session_id == incoming.agent_session_id
                    && resume.agent_turn_id == incoming.agent_turn_id && resume.decision_request_id == incoming.decision_request_id
                    && resume.context_digest == context_digest))).expect("resumed model requires preceding exact canonical Resume");
            assert!(resume_result.committed_height < reserve.committed_height);
            let receipt: oasis7_runtime::CognitionWakeHandoffResultV1 =
                serde_json::from_value(resume_result.receipt.clone()).unwrap();
            assert_eq!(
                receipt.continuation.status,
                oasis7_runtime::ContinuationStatusV1::Consumed
            );
            let next = receipt
                .replanned_continuation
                .as_ref()
                .expect("Resume exact successor");
            assert_eq!(next.status, oasis7_runtime::ContinuationStatusV1::Scheduled);
            assert_ne!(next.continuation_id, receipt.continuation.continuation_id);
            publish_marker(&root.join("hosted-resumed-model.json"), &serde_json::to_vec(&serde_json::json!({
                "request_digest":digest,"lease_id":lease.lease_id,"resume_height":resume_result.committed_height,
                "original_continuation_id":receipt.continuation.continuation_id,"successor_id":next.continuation_id,
                "resume_exact_context":true,"new_reserve_prefix_before_model":true})).unwrap()).unwrap();
        } else {
            publish_marker(
                &root.join("hosted-wait-origin.json"),
                &serde_json::to_vec(&evidence).unwrap(),
            )
            .unwrap();
        }
        fs::write(
            root.join(format!("model-boundary-{invocation}.json")),
            serde_json::to_vec(&evidence).unwrap(),
        )
        .unwrap();
        println!("hosted_model_request_boundary={evidence}");
    })
}

pub(super) fn verify(client: &RemoteWorldServiceClient) {
    let root = std::path::PathBuf::from(std::env::var("PRE2_METADATA_DIR").unwrap());
    application_fresh::preflight(client, &root);
    let server = ViewerRuntimeLiveServer::new(application_hosted::server_config(
        client,
        true,
        Duration::from_millis(200),
        Some(root.join("hosted-wait-private-lineage.json")),
    ))
    .unwrap();
    let shared = Arc::new(Mutex::new(server));
    let mut session = application_stream_boundaries::Session::start(&shared, false);
    let warmed = session.warm();
    session
        .send(serde_json::json!({"type":"live_control","mode":{"mode":"play"},"request_id":1201}))
        .unwrap();
    session.send(serde_json::json!({"type":"hello_v2","client":"PRE2 ordinary Wait Play","version":2,"capabilities":[]})).unwrap();
    session
        .send(serde_json::json!({"type":"request_snapshot"}))
        .unwrap();
    let ordered = session.snapshot_ordered(Duration::from_secs(3), true);
    let eligibility = shared.lock().unwrap().test_agent_service_pump_status();
    let mut completion = application_completion::CompletionWatch::new();
    let mut completed = false;
    let mut summary = serde_json::Value::Null;
    loop {
        session.snapshot(Duration::from_millis(25)); // Continuous bounded drain, no repeated queued requests.
        if let Ok(guard) = shared.try_lock() {
            summary = guard.test_canonical_provider_diagnostic_summary();
            if let Ok(bytes) = fs::read(root.join("hosted-resumed-model.json")) {
                let resumed: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
                let digest = resumed["request_digest"].as_str().unwrap();
                completed = summary["terminal_states"]["agent-a"]["status"] == "committed"
                    && summary["pending_intent_count"] == 0
                    && summary["pending_action_count"] == 0
                    && summary["memory_store"].to_string().contains(digest);
            }
        }
        let rollback_observed = [
            ("PRE2_WAIT_CLEANUP_FS_ROOT", "wait-cleanup-rollback"),
            ("PRE2_RESUME_HANDOFF_FS_ROOT", "resume-handoff-rollback"),
        ]
        .iter()
        .any(|(env, marker)| {
            std::env::var_os(env).as_deref() == Some(root.as_os_str())
                && root.join(marker).is_file()
        });
        if completion.finished_after_injected_rollback(&summary, completed, rollback_observed) {
            break;
        }
    }
    session
        .send(serde_json::json!({"type":"live_control","mode":{"mode":"pause"},"request_id":1202}))
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
        "hosted_wait_listener warmed={warmed} ordered={ordered} eligible={} completed={completed} drained={drained} worker={joined:?} factory_calls=0 direct_poll_calls=0",
        eligibility["eligible"]
    );
    assert!(
        warmed && ordered && eligibility["eligible"] == true,
        "ordinary trusted Play prerequisite"
    );
    assert!(
        drained && joined.is_ok(),
        "actual complete cleanup must precede Wait assertions"
    );
    assert!(
        completed,
        "ordinary Wait/Resume/Act must reach exact committed memory; diagnostic={}",
        application_completion::hosted_failure_diagnostic(&summary)
    );
    println!("PRE2_HOSTED_WAIT_RESUME_ACT_PASSED");
}

pub(super) fn report(fixture: &Fixture, root: &std::path::Path, models: usize) {
    let observations = fs::read_dir(root)
        .unwrap()
        .filter_map(Result::ok)
        .filter(|entry| {
            entry
                .file_name()
                .to_string_lossy()
                .starts_with("model-boundary-")
        })
        .count();
    let world = fixture.driver.lock().unwrap().execution_world.clone();
    let origin: serde_json::Value =
        serde_json::from_slice(&fs::read(root.join("hosted-wait-origin.json")).unwrap()).unwrap();
    let origin_digest = origin["request_digest"].as_str().unwrap();
    let admits = world.capability_revocation_state().world_service_results.values()
        .filter_map(|value| serde_json::from_value::<wire::CanonicalIntentResultV1>(value.clone()).ok())
        .filter(|result| result.rejected.is_none() && matches!(&result.request.signed_payload,
            WorldServicePayloadV1::Scheduler(signed) if matches!(&signed.request.operation, SchedulerOperationV1::AdmitContinuation(proposal) if proposal.origin_request_digest == origin_digest))).count();
    let admit_results = world
        .capability_revocation_state()
        .world_service_results
        .values()
        .filter_map(|value| {
            serde_json::from_value::<wire::CanonicalIntentResultV1>(value.clone()).ok()
        })
        .filter(|result| {
            matches!(&result.request.signed_payload, WorldServicePayloadV1::Scheduler(signed)
            if matches!(&signed.request.operation, SchedulerOperationV1::AdmitContinuation(proposal)
                if proposal.origin_request_digest == origin_digest))
        })
        .collect::<Vec<_>>();
    for result in &admit_results {
        // Only names verified in actual admission validators are public diagnostics.
        const KNOWN: &[(&str, &str)] = &[
            (
                "scheduler base binding changed",
                "scheduler_base_binding_changed",
            ),
            (
                "continuation subject mismatch",
                "continuation_subject_mismatch",
            ),
            (
                "continuation_turn_unregistered",
                "continuation_turn_unregistered",
            ),
            (
                "continuation_proposal_invalid",
                "continuation_proposal_invalid",
            ),
            (
                "continuation_proposal_conflict",
                "continuation_proposal_conflict",
            ),
            (
                "continuation_budget_chain_conflict",
                "continuation_budget_chain_conflict",
            ),
            (
                "foreign_continuation_proposal",
                "foreign_continuation_proposal",
            ),
            ("runtime_manifest_mismatch", "runtime_manifest_mismatch"),
            ("runtime_binding_required", "runtime_binding_required"),
            ("scheduler_unconfigured", "scheduler_unconfigured"),
            ("wake_condition_invalid", "wake_condition_invalid"),
        ];
        let error_codes = KNOWN
            .iter()
            .filter_map(|(needle, category)| {
                result
                    .rejected
                    .as_deref()
                    .is_some_and(|reason| reason.contains(needle))
                    .then_some(*category)
            })
            .collect::<Vec<_>>();
        println!(
            "hosted_wait_actual_scheduler kind=admit_continuation correlation_digest={} rejected={} allowlisted_error_codes={error_codes:?}",
            correlation::key_digest(&result.request.correlation.key).unwrap(),
            result.rejected.is_some()
        );
    }
    println!(
        "hosted_wait_actual_admit_results_count={}",
        admit_results.len()
    );
    let checkpoint: serde_json::Value =
        serde_json::from_slice(&fs::read(root.join("hosted-wait-private-lineage.json")).unwrap())
            .unwrap();
    let processed = checkpoint["provider_completed_decisions"]
        .as_array()
        .unwrap();
    let wait = processed
        .iter()
        .find(|decision| decision["decision"] == serde_json::json!({"WaitTicks":2}));
    let native_wait_consumed = wait.is_some_and(|decision| {
        let request = &decision["cognition"]["request"]["request_context"];
        let actual_digest = request["request_digest"].as_str().unwrap();
        let exact_callback = fs::read_dir(root)
            .unwrap()
            .filter_map(Result::ok)
            .filter(|entry| {
                entry
                    .file_name()
                    .to_string_lossy()
                    .starts_with("model-boundary-")
            })
            .any(|entry| {
                let evidence: serde_json::Value =
                    serde_json::from_slice(&fs::read(entry.path()).unwrap()).unwrap();
                evidence["request_digest"] == actual_digest
                    && evidence["agent_turn_id"] == request["agent_turn_id"]
            });
        exact_callback
            && decision["cognition"]["response"]["request_digest"] == request["request_digest"]
            && decision["cognition"]["response"]["base_decision_response"]["decision"]
                == serde_json::to_value(oasis7::simulator::ProviderDecision::WaitTicks { ticks: 2 })
                    .unwrap()
            && decision["cognition"]["cognition_lease"]["request_digest"]
                == request["request_digest"]
    });
    println!(
        "hosted_wait_native_consumed={native_wait_consumed} completed_queue_count={} canonical_admit_absent={}",
        processed.len(),
        admits == 0
    );
    assert!(
        native_wait_consumed || admits > 0,
        "genuine native queue must persist exact processed WaitTicks2 or its canonical admission"
    );
    println!(
        "hosted_wait_actual model_requests={models} model_boundary_observations={observations} canonical_admit_count={admits} factory_calls=0 direct_poll_calls=0"
    );
    assert!(
        admits > 0,
        "native queue processed exact WaitTicks2 but canonical AdmitContinuation is absent"
    );
    assert!(
        models > 0 && observations == models,
        "actual Wait model request must pass canonical callback prerequisites"
    );
    let admitted_proposal = world
        .capability_revocation_state()
        .world_service_results
        .values()
        .filter_map(|value| {
            serde_json::from_value::<wire::CanonicalIntentResultV1>(value.clone()).ok()
        })
        .find_map(|result| match result.request.signed_payload {
            WorldServicePayloadV1::Scheduler(signed) if result.rejected.is_none() => {
                match signed.request.operation {
                    SchedulerOperationV1::AdmitContinuation(proposal)
                        if proposal.origin_request_digest == origin_digest =>
                    {
                        Some(proposal)
                    }
                    _ => None,
                }
            }
            _ => None,
        })
        .unwrap();
    assert_eq!(admitted_proposal.agent_id, "agent-a");
    assert_eq!(
        admitted_proposal.agent_session_id,
        origin["agent_session_id"].as_str().unwrap()
    );
    assert_eq!(
        admitted_proposal.origin_turn_id,
        origin["agent_turn_id"].as_str().unwrap()
    );
    let expected_wake = origin["base_tick"].as_u64().unwrap() + 2;
    assert!(
        admitted_proposal
            .wake_conditions
            .iter()
            .any(|condition| condition.kind == "at_or_after_tick"
                && condition.logical_tick == Some(expected_wake))
    );
    assert_eq!(admitted_proposal.valid_until_tick, Some(expected_wake + 16));
    assert_eq!(admitted_proposal.remaining_budget.value, 2);
    let selected: serde_json::Value =
        serde_json::from_slice(&fs::read(root.join("hosted-wait-selected.json")).unwrap()).unwrap();
    let resumed: serde_json::Value =
        serde_json::from_slice(&fs::read(root.join("hosted-resumed-model.json")).unwrap()).unwrap();
    assert_eq!(
        selected["continuation_id"],
        resumed["original_continuation_id"]
    );
    assert_eq!(selected["origin_request_digest"], origin["request_digest"]);
    let digest = resumed["request_digest"].as_str().unwrap();
    let results = world
        .capability_revocation_state()
        .world_service_results
        .values()
        .filter_map(|value| {
            serde_json::from_value::<wire::CanonicalIntentResultV1>(value.clone()).ok()
        })
        .collect::<Vec<_>>();
    let acts = results
        .iter()
        .filter(|result| {
            matches!(&result.request.signed_payload,
        WorldServicePayloadV1::Cognition(signed) if signed.request.request.request_digest == digest)
        })
        .collect::<Vec<_>>();
    assert_eq!(acts.len(), 1, "exact resumed Act executes once");
    let act = acts[0];
    assert!(act.rejected.is_none());
    let IntentOutcome::Committed { receipt, .. } = fixture
        .client
        .lookup(
            LookupIntentRequest {
                contract_version: 1,
                key: act.request.correlation.key.clone(),
            },
            act.request.signed_payload.clone(),
        )
        .unwrap()
        .outcome
    else {
        panic!("actual signed resumed Act Lookup receipt required");
    };
    assert_eq!(receipt["commit_record"]["request_digest"], digest);
    let mut config = fixture.client.config().clone();
    config.scope_id = "agent:agent-a".into();
    let client = RemoteWorldServiceClient::new(config).unwrap();
    let mut read = fixture.view(None);
    read.scope_id = "agent:agent-a".into();
    let view = client.read_view(read).unwrap();
    assert_eq!(
        view.projection().state.agents["agent-a"].state.pos,
        oasis7_agent_api::world_action::GeoPos::new(2, 2, 0)
    );
    for (lease_id, request_digest) in [
        (origin["lease_id"].as_str().unwrap(), origin_digest),
        (resumed["lease_id"].as_str().unwrap(), digest),
    ] {
        assert!(
            view.projection()
                .cognition_leases
                .iter()
                .any(|lease| lease.lease_id == lease_id
                    && lease.request_digest == request_digest
                    && lease.status == oasis7_runtime::CognitionLeaseStatusV1::Settled)
        );
    }
    let memory: serde_json::Value =
        serde_json::from_slice(&fs::read(root.join("hosted-wait-memory.json")).unwrap()).unwrap();
    assert!(memory.to_string().contains(digest));
    assert!(
        memory
            .to_string()
            .contains(receipt["lineage"]["receipt_id"].as_str().unwrap())
    );
    println!(
        "hosted_wait_canonical_positive original_consumed=true exact_successor_scheduled=true resumed_act_once=true exact_wait_and_act_leases_settled=true memory_exact_receipt=true future_work_preserved=true"
    );
}

// Provider/clock markers are read concurrently by genuine process watchers.
pub(super) fn publish_marker(path: &std::path::Path, bytes: &[u8]) -> std::io::Result<()> {
    publish_marker_with_pause(path, bytes, |_| {})
}
fn publish_marker_with_pause(
    path: &std::path::Path,
    bytes: &[u8],
    after_create: impl FnOnce(&std::path::Path),
) -> std::io::Result<()> {
    use std::io::Write;
    // Publish only complete JSON; watchers never observe the staging inode.
    let stage = path.with_extension(format!(
        "staging-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    // Failed exclusive creation never authorizes removing another writer's file.
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&stage)?;
    let result = (|| {
        after_create(&stage);
        file.write_all(bytes)?;
        drop(file);
        fs::rename(&stage, path)
    })();
    if result.is_err() {
        let _ = fs::remove_file(&stage);
    }
    result
}

#[test]
fn provider_clock_marker_is_not_visible_during_partial_publication() {
    let root = temp_dir("pre2-atomic-provider-clock-marker");
    fs::create_dir_all(&root).unwrap();
    let target = root.join("marker.json");
    let worker_target = target.clone();
    let (ready_tx, ready_rx) = std::sync::mpsc::sync_channel(1);
    let (release_tx, release_rx) = std::sync::mpsc::sync_channel(1);
    let bytes =
        br#"{"request_digest":"actual-marker-publication","successor_id":"actual-successor"}"#;
    let worker = thread::spawn(move || {
        publish_marker_with_pause(&worker_target, bytes, |stage| {
            fs::write(stage, b"{").unwrap();
            ready_tx.send(()).unwrap();
            release_rx.recv().unwrap();
        })
    });
    ready_rx.recv_timeout(Duration::from_secs(2)).unwrap();
    let observed = fs::read(&target);
    release_tx.send(()).unwrap();
    worker.join().unwrap().unwrap();
    let parsed: serde_json::Value = serde_json::from_slice(&fs::read(&target).unwrap()).unwrap();
    assert_eq!(parsed["request_digest"], "actual-marker-publication");
    assert!(
        matches!(observed, Err(error) if error.kind() == std::io::ErrorKind::NotFound),
        "published marker must remain absent while staging is partial"
    );
    fs::write(&target, b"{").unwrap();
    assert!(
        serde_json::from_slice::<serde_json::Value>(&fs::read(&target).unwrap()).is_err(),
        "actually malformed published JSON remains an error"
    );
    fs::remove_dir_all(root).unwrap();
}
