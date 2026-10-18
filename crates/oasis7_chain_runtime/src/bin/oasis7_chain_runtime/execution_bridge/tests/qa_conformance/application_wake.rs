//! Real provider Wait checkpoint, canonical selection, ResumeWake and Act closure.
use super::*;
use oasis7::viewer::{
    ViewerLiveDecisionMode, ViewerRuntimeLiveServer, ViewerRuntimeLiveServerConfig,
};
use oasis7::world_service::client::WorldServiceAgentSignerConfig;
use oasis7_simulator::WorldScenario;
pub(super) fn required_child_marker(wake: bool) -> &'static str {
    if wake {
        "PRE2_APPLICATION_OS_DENIAL_AND_GENUINE_WAKE_PASSED"
    } else {
        "PRE2_APPLICATION_OS_DENIAL_AND_FIVE_OPS_PASSED"
    }
}

pub(super) struct ParentClock {
    stop: Arc<AtomicBool>,
    worker: Option<thread::JoinHandle<()>>,
}
impl Drop for ParentClock {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        if let Some(worker) = self.worker.take()
            && worker.join().is_err()
        {
            if thread::panicking() {
                eprintln!("parent genuine clock worker failed during child failure");
            } else {
                panic!("parent genuine clock worker failed");
            }
        }
    }
}
pub(super) fn start_parent_clock(fixture: &Fixture, enabled: bool) -> ParentClock {
    let stop = Arc::new(AtomicBool::new(false));
    let driver = fixture.driver.clone();
    let stopping = stop.clone();
    let worker = enabled.then(|| {
        thread::spawn(move || {
            let deadline = Instant::now() + Duration::from_secs(30);
            while !stopping.load(Ordering::SeqCst) && Instant::now() < deadline {
                {
                    let mut driver = driver.lock().unwrap();
                    if !driver
                        .execution_world
                        .cognition_in_flight_wakes()
                        .unwrap()
                        .is_empty()
                    {
                        return;
                    }
                    let active = !driver
                        .execution_world
                        .active_cognition_continuations()
                        .unwrap()
                        .is_empty();
                    let settled = driver
                        .execution_world
                        .cognition_economy()
                        .unwrap()
                        .leases
                        .values()
                        .any(|lease| {
                            lease.status == oasis7_runtime::CognitionLeaseStatusV1::Settled
                        });
                    if active && settled {
                        let mut before =
                            serde_json::to_value(driver.execution_world.state()).unwrap();
                        before["time"] = serde_json::json!(0);
                        commit_request(&mut driver, 0, None);
                        let mut after =
                            serde_json::to_value(driver.execution_world.state()).unwrap();
                        after["time"] = serde_json::json!(0);
                        assert_eq!(
                            before, after,
                            "genuine empty clock commit changed non-time WorldState"
                        );
                        println!(
                            "parent_genuine_empty_clock_commit non_time_world_state_unchanged=true"
                        );
                        if !driver
                            .execution_world
                            .cognition_in_flight_wakes()
                            .unwrap()
                            .is_empty()
                        {
                            return;
                        }
                    }
                }
                thread::sleep(Duration::from_millis(10));
            }
        })
    });
    ParentClock { stop, worker }
}

fn view(
    client: &RemoteWorldServiceClient,
) -> oasis7::world_service::verified_view::VerifiedWorldView {
    client
        .read_view(ReadWorldViewRequest {
            observation_nonce: None,
            formal_anchor: None,
            contract_version: 1,
            world: client.config().expected_world.clone(),
            scope_id: client.config().scope_id.clone(),
            min_commit: None,
            fixed_commit: None,
            deadline_unix_ms: None,
        })
        .unwrap()
}
fn retry(error: &str) -> bool {
    !error.contains("Rejected")
        && (error.contains("pending")
            || error.contains("unresolved")
            || error.contains("outcome unknown"))
}

pub(super) fn report_canonical_resume_failure(fixture: &Fixture) {
    let driver = fixture.driver.lock().unwrap();
    let handoffs = driver
        .execution_world
        .capability_revocation_state()
        .world_service_results
        .values()
        .filter_map(|value| {
            serde_json::from_value::<wire::CanonicalIntentResultV1>(value.clone()).ok()
        })
        .filter(|result| {
            matches!(&result.request.signed_payload, WorldServicePayloadV1::Scheduler(signed)
            if matches!(&signed.request.operation, SchedulerOperationV1::HandoffWake { .. }))
        })
        .map(|result| result.rejected.is_none())
        .collect::<Vec<_>>();
    println!(
        "canonical_handoff_operations count={} accepted={}",
        handoffs.len(),
        handoffs.iter().filter(|accepted| **accepted).count()
    );
    for (key, value) in driver
        .execution_world
        .capability_revocation_state()
        .world_service_results
        .iter()
        .take(64)
    {
        let Ok(result) = serde_json::from_value::<wire::CanonicalIntentResultV1>(value.clone())
        else {
            continue;
        };
        let WorldServicePayloadV1::Scheduler(signed) = &result.request.signed_payload else {
            continue;
        };
        if let SchedulerOperationV1::ResumeWake {
            wake_id,
            proposal,
            current_context,
            ..
        } = &signed.request.operation
        {
            let execution_tick = signed.request.captured_base_binding.base_tick;
            let derived_tick = oasis7_runtime::WakeConditionValidator::next_wake_tick_at(
                &proposal.wake_conditions,
                execution_tick,
            );
            println!(
                "canonical_resume_tick_witness execution_tick={execution_tick} proposal_next_tick={:?} derived_next_tick={derived_tick:?} valid_until_tick={:?} unexpired={}",
                proposal.next_wake_tick,
                proposal.valid_until_tick,
                proposal
                    .valid_until_tick
                    .is_none_or(|tick| tick >= execution_tick)
            );
            println!(
                "canonical_resume_result operation=ResumeWake correlation_digest={key} rejection={}",
                result
                    .rejected
                    .as_deref()
                    .unwrap_or("none")
                    .chars()
                    .take(512)
                    .collect::<String>()
            );
            if let Some(wake) = driver
                .execution_world
                .cognition_in_flight_wakes()
                .unwrap()
                .into_iter()
                .find(|wake| &wake.wake_id == wake_id)
                && let Some(context) = driver
                    .execution_world
                    .service_continuation_context(&wake.continuation_id)
            {
                println!(
                    "canonical_resume_context_axes baseline_observation={} goal={} policy={} precondition={}",
                    context["baseline_observation_digest"].as_str()
                        == Some(current_context.baseline_observation_digest.as_str()),
                    context["goal_digest"].as_str() == Some(current_context.goal_digest.as_str()),
                    context["policy_digest"].as_str()
                        == Some(current_context.policy_digest.as_str()),
                    context["precondition_digest"].as_str()
                        == Some(current_context.precondition_digest.as_str())
                );
            }
        }
    }
}
pub(super) fn verify_wait_wake(public_client: &RemoteWorldServiceClient) {
    let mut connection = public_client.config().clone();
    connection.scope_id = "agent:agent-a".into();
    let client = RemoteWorldServiceClient::new(connection.clone()).unwrap();
    let mut config = ViewerRuntimeLiveServerConfig::new(WorldScenario::Minimal);
    config.world_id = "w1".into();
    config.world_service = Some(connection);
    config.world_service_agent_signer = Some(WorldServiceAgentSignerConfig {
        private_key_hex: hex::encode([8u8; 32]),
        delegation_generation: 1,
    });
    config.decision_mode = ViewerLiveDecisionMode::Llm;
    config.provider_lineage_store = Some(
        std::env::current_dir()
            .unwrap()
            .join("wait-wake-lineage.json"),
    );
    let mut server = ViewerRuntimeLiveServer::new(config).unwrap();
    let action = oasis7_simulator::Action::MoveAgent {
        agent_id: "agent-a".into(),
        to: "runtime:2:2:0".into(),
    };
    let wait = server
        .test_prepare_canonical_wait_provider_response("agent-a", action.clone(), 2)
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(20);
    loop {
        match server.test_queue_canonical_provider_response(wait.clone(), action.clone()) {
            Ok(()) => break,
            Err(e) if retry(&e) => {}
            Err(e) => panic!("Wait queue: {e}"),
        }
        assert!(Instant::now() < deadline, "Wait queue timeout");
    }
    let proposal =
        loop {
            match server.test_poll_canonical_provider_response() {
                Ok(()) => {}
                Err(e) if retry(&e) => {}
                Err(e) => panic!("Wait poll: {e}"),
            }
            if let Ok(proposal) = server.test_canonical_provider_wait_proposal("agent-a") {
                let current = view(&client);
                let closure = server.test_canonical_provider_summary();
                if closure["pending_intent_count"] == 0
                    && closure["pending_action_count"] == 0
                    && closure["mirrored_lease_count"] == 0
                    && current
                        .projection()
                        .continuations
                        .iter()
                        .any(|c| c.continuation_proposal_id == proposal.continuation_proposal_id)
                    && current.projection().cognition_leases.iter().any(|lease| {
                        lease.status == oasis7_runtime::CognitionLeaseStatusV1::Settled
                    })
                {
                    break proposal;
                }
            }
            assert!(
                Instant::now() < deadline,
                "Wait canonical admission timeout"
            );
            thread::sleep(Duration::from_millis(10));
        };
    let drift = std::env::var("PRE2_APP_WAKE_DRIFT").unwrap() == "1";
    // The positive uses only parent-controlled genuine empty execution commits.
    // The separate negative intentionally changes protected resources.
    for nonce in 100..104 {
        if !drift {
            let deadline = Instant::now() + Duration::from_secs(10);
            while view(&client).projection().scheduler_wakes.is_empty() {
                assert!(
                    Instant::now() < deadline,
                    "real empty commits selected no wake"
                );
                thread::sleep(Duration::from_millis(10));
            }
            break;
        }
        use oasis7::viewer::{
            CollectDataCommand, CollectDataRequest, sign_collect_data_auth_proof,
        };
        let owner = &client.config().read_private_key_hex;
        let public = sign_read_request("owner", (), owner)
            .unwrap()
            .subject_public_key;
        let mut command = CollectDataCommand::Submit {
            request: CollectDataRequest {
                electricity_cost: 7,
                data_amount: 11,
                player_id: "owner-a".into(),
                public_key: Some(public.clone()),
                auth: None,
            },
        };
        let proof = sign_collect_data_auth_proof(&command, nonce, &public, owner).unwrap();
        let CollectDataCommand::Submit { request } = &mut command else {
            unreachable!()
        };
        request.auth = Some(proof);
        let payload = WorldServicePayloadV1::GameplayJson(serde_json::to_vec(&command).unwrap());
        let request = SubmitIntentRequest {
            contract_version: 1,
            correlation: derive_correlation(client.config().expected_world.clone(), &payload)
                .unwrap(),
            deadline_unix_ms: None,
            signed_payload: payload,
        };
        client.submit(request.clone()).unwrap();
        let commit_deadline = Instant::now() + Duration::from_secs(10);
        loop {
            assert!(
                Instant::now() < commit_deadline,
                "original resource mutation exhausted its bounded budget"
            );
            let outcome = client
                .lookup(
                    LookupIntentRequest {
                        contract_version: 1,
                        key: request.correlation.key.clone(),
                    },
                    request.signed_payload.clone(),
                )
                .unwrap()
                .outcome;
            assert!(
                Instant::now() < commit_deadline,
                "original resource mutation response arrived after its bounded budget"
            );
            match outcome {
                IntentOutcome::Committed { .. } => break,
                IntentOutcome::Unknown
                | IntentOutcome::Pending
                | IntentOutcome::Received { .. } => {
                    assert!(
                        Instant::now() < commit_deadline,
                        "original resource mutation did not commit within its bounded budget"
                    );
                    std::thread::sleep(Duration::from_millis(10));
                }
                other => panic!("original authenticated resource mutation failed: {other:?}"),
            }
        }
        if !view(&client).projection().scheduler_wakes.is_empty() {
            break;
        }
    }
    let selected = view(&client);
    assert!(
        !selected.projection().scheduler_wakes.is_empty(),
        "canonical scheduler selected no wake"
    );
    let continuation_id = selected
        .projection()
        .continuations
        .iter()
        .find(|c| c.continuation_proposal_id == proposal.continuation_proposal_id)
        .expect("original Wait continuation missing")
        .continuation_id
        .clone();
    assert!(
        selected
            .projection()
            .scheduler_wakes
            .iter()
            .any(|w| w.continuation_id == continuation_id)
    );
    let deadline = Instant::now() + Duration::from_secs(20);
    let resumed = loop {
        let last_resume_error = match server.test_prepare_canonical_wake_provider_response(
            "agent-a",
            action.clone(),
            proposal.clone(),
        ) {
            Ok(context) => break context,
            Err(e) if retry(&e) => e,
            Err(e)
                if drift
                    && e.contains("canonical scheduler rejected:")
                    && e.contains("Conflict") =>
            {
                let rejected = view(&client);
                assert!(
                    rejected
                        .projection()
                        .scheduler_wakes
                        .iter()
                        .any(|w| w.continuation_id == continuation_id)
                );
                assert_ne!(
                    rejected.projection().state.agents["agent-a"].state.pos,
                    oasis7_agent_api::world_action::GeoPos::new(2, 2, 0)
                );
                println!("PRE2_APPLICATION_RESOURCE_DRIFT_WAKE_REJECTED");
                return;
            }
            Err(e) => panic!("ResumeWake: {e}"),
        };
        assert!(
            Instant::now() < deadline,
            "ResumeWake timeout: {last_resume_error}"
        );
        thread::sleep(Duration::from_millis(10));
    };
    loop {
        match server.test_queue_canonical_provider_response(resumed.clone(), action.clone()) {
            Ok(()) => break,
            Err(e) if retry(&e) => {}
            Err(e) => panic!("wake Act queue: {e}"),
        }
        assert!(Instant::now() < deadline, "wake Act queue timeout");
    }
    let expected_request_digest = resumed["request"]["request_context"]["request_digest"]
        .as_str()
        .expect("real resumed request digest missing")
        .to_string();
    loop {
        let summary = server.test_canonical_provider_summary();
        let terminal = &summary["terminal_states"]["agent-a"];
        let current = view(&client);
        if terminal["status"] == "committed"
            && terminal["request_digest"] == expected_request_digest
            && summary["pending_intent_count"] == 0
            && summary["pending_action_count"] == 0
        {
            assert!(
                current
                    .projection()
                    .continuations
                    .iter()
                    .any(|entry| entry.continuation_id == continuation_id
                        && entry.status == oasis7_runtime::ContinuationStatusV1::Consumed)
            );
            assert!(
                current
                    .projection()
                    .continuations
                    .iter()
                    .any(|entry| entry.continuation_id != continuation_id
                        && entry.status == oasis7_runtime::ContinuationStatusV1::Scheduled)
            );
            assert!(
                !current
                    .projection()
                    .scheduler_wakes
                    .iter()
                    .any(|wake| wake.continuation_id == continuation_id)
            );
            assert_eq!(
                current.projection().state.agents["agent-a"].state.pos,
                oasis7_agent_api::world_action::GeoPos::new(2, 2, 0)
            );
            let lease = current
                .projection()
                .cognition_leases
                .iter()
                .find(|lease| lease.request_digest == expected_request_digest)
                .expect("canonical resumed Act lease missing");
            assert_eq!(
                lease.status,
                oasis7_runtime::CognitionLeaseStatusV1::Settled
            );
            assert!(
                !summary["mirrored_lease_identities"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .any(|identity| identity["request_digest"] == expected_request_digest),
                "resumed Act mirror not retired"
            );
            let feedback = terminal["feedback_id"]
                .as_str()
                .filter(|value| !value.is_empty())
                .unwrap();
            println!("provider_terminal_feedback_id={feedback}");
            println!("provider_original_continuation_id={continuation_id}");
            println!(
                "provider_resumed_request_digest={expected_request_digest} canonical_settled_lease_id={}",
                lease.lease_id
            );
            println!(
                "PRE2_APPLICATION_GENUINE_WAIT_RESUME_ACT_PASSED original_consumed=true future_replan_scheduled=true exact_act_lease_settled=true exact_act_mirror_retired=true"
            );
            break;
        }
        if Instant::now() >= deadline {
            let statuses: Vec<_> = current.projection().continuations.iter().map(|entry|
                serde_json::json!({"continuation_id":entry.continuation_id,"status":entry.status})).collect();
            println!(
                "canonical_wake_final_witness expected_continuation_id={continuation_id} statuses={} mirror_identities={} agent_position={:?}",
                serde_json::to_string(&statuses).unwrap(),
                summary["mirrored_lease_identities"],
                current.projection().state.agents["agent-a"].state.pos
            );
        }
        assert!(
            Instant::now() < deadline,
            "wake canonical Act closure timeout: {summary}"
        );
        match server.test_poll_canonical_provider_response() {
            Ok(()) => {}
            Err(e) if retry(&e) => {}
            Err(e) => panic!("wake Act poll: {e}"),
        }
        thread::sleep(Duration::from_millis(10));
    }
}

#[test]
fn real_tcp_application_resource_drift_wake_rejected() {
    super::run_isolated_application(true, true, false, false, false, false);
}
