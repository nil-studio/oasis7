//! Transactional cleanup of a real provider Wait rejected before Runtime admission.
use super::super::test_support::BuiltinWaitBehavior;
use super::*;
use crate::simulator::{
    ActionCatalogEntry, COGNITION_CAPABILITY_CATALOG_DOMAIN,
    COGNITION_CAPABILITY_INVOCATION_CONTEXT_DOMAIN, DecisionResponse, Digest32, GoalSnapshotV1,
    MemoryContextSnapshotV1, MockDecisionProvider, ProviderBackedAgentBehavior,
    ProviderExecutionMode, golden_decision_provider_fixtures, h_v1,
};
use serde_json::{Value, json};

fn inner_request() -> Value {
    serde_json::to_value(
        &golden_decision_provider_fixtures()
            .into_iter()
            .next()
            .expect("golden provider fixture")
            .request,
    )
    .expect("serialize inner decision request")
}

fn request_fixture(transport_attempt: u64, timeout_budget_ms: u64) -> Value {
    let mut base_decision_request = inner_request();
    base_decision_request["timeout_budget_ms"] = json!(timeout_budget_ms);
    json!({
        "base_decision_request": base_decision_request,
        "context_discriminator": "oasis7.continuous-agent-context",
        "context_version": 1,
        "protocol_version": "continuous-agent-v1",
        "agent_session_id": "session.agent-1.v1",
        "agent_turn_id": "turn.agent-1.7",
        "decision_request_id": "request.agent-1.7",
        "retry_seq": 1,
        "transport_attempt": transport_attempt,
        "agent_subject": "agent-1",
        "runtime_binding": {
            "world_id": "world-1",
            "branch_id": "main",
            "finality_epoch": 7,
            "finality_block_hash": "blake3:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
            "finality_status": "verified",
            "base_tick": 42,
            "base_world_hash": "blake3:bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
            "reorg_epoch": 3,
            "runtime_manifest_hash": "blake3:cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc"
        },
        "observation_digest": "blake3:dddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddd",
        "capability_catalog_digest": "blake3:eeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee",
        "capability_invocation_context_digest": "blake3:ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff",
        "memory_snapshot_digest": "blake3:1111111111111111111111111111111111111111111111111111111111111111",
        "goal_snapshot_digest": "blake3:2222222222222222222222222222222222222222222222222222222222222222",
        "continuation_digest": "blake3:3333333333333333333333333333333333333333333333333333333333333333",
        "adapter_protocol_version": "loopback-http-v1",
        "budget_contract": {"max_latency_ms": 60_000, "max_repair_attempts": 2, "max_model_calls": 4, "max_tool_calls": 3},
        "request_digest": "blake3:4444444444444444444444444444444444444444444444444444444444444444"
    })
}

fn production_request_fixture(transport_attempt: u64, timeout_budget_ms: u64) -> Value {
    let mut fixture = request_fixture(transport_attempt, timeout_budget_ms);
    let subject = json!({
        "kind": "agent",
        "agent_id": "agent-1",
        "owner_binding": "owner-1",
        "generation": 1
    });
    let presenter = json!({
        "presenter_id": "provider-1",
        "presenter_kind": "provider",
        "session_id": "session.agent-1.v1"
    });
    let audience = json!({
        "world_id": "world-1",
        "branch_id": "main",
        "finality_epoch": 7,
        "target_kind": "world"
    });
    let catalog = json!({
        "snapshot_id": "catalog.agent-1.v1",
        "world_id": "world-1",
        "world_head": 42,
        "branch_id": "main",
        "finality_epoch": 7,
        "logical_tick": 42,
        "module_registry_hash": "blake3:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
        "policy_hash": "blake3:bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
        "revocation_epoch": 0,
        "subject": subject,
        "presenter": presenter,
        "audience": audience,
        "entries": [],
        "valid_until_tick": 100
    });
    let invocation = json!({
        "grant_id": "grant.agent-1.v1",
        "subject": catalog["subject"].clone(),
        "presenter": catalog["presenter"].clone(),
        "audience": catalog["audience"].clone(),
        "catalog_snapshot_id": "catalog.agent-1.v1",
        "module_id": "",
        "module_version": "",
        "response_nonce": "nonce.agent-1.v1"
    });
    fixture["base_decision_request"]["capability_catalog"] = catalog.clone();
    fixture["base_decision_request"]["capability_invocation_context"] = invocation.clone();
    fixture["capability_catalog_digest"] =
        json!(h_v1(COGNITION_CAPABILITY_CATALOG_DOMAIN, &catalog));
    fixture["capability_invocation_context_digest"] = json!(h_v1(
        COGNITION_CAPABILITY_INVOCATION_CONTEXT_DOMAIN,
        &invocation
    ));
    fixture
}

fn derived_request_digest(fixture: &Value) -> Digest32 {
    let mut canonical = fixture.clone();
    let object = canonical
        .as_object_mut()
        .expect("request fixture is an object");
    object.remove("request_digest");
    object.remove("transport_attempt");
    let base = object
        .get_mut("base_decision_request")
        .and_then(Value::as_object_mut)
        .expect("base decision request is an object");
    base.remove("timeout_budget_ms");
    base.get_mut("observation")
        .and_then(Value::as_object_mut)
        .expect("provider observation envelope is an object")
        .remove("timeout_budget_ms");
    let bytes = oasis7_wasm_abi::encode_canonical_cbor(&canonical)
        .expect("request fixture must be canonically encodable");
    h_v1("oasis7.cognition.request.v1", &bytes)
}

fn request_from_value(mut fixture: Value) -> ContinuousAgentRequestContextV1 {
    fixture["request_digest"] = json!(derived_request_digest(&fixture));
    serde_json::from_value(fixture).expect("decode ContinuousAgentRequestContextV1 fixture")
}

fn production_turn_context(
    request_context: &ContinuousAgentRequestContextV1,
) -> ContinuousAgentTurnContextV1 {
    ContinuousAgentTurnContextV1 {
        agent_id: request_context.agent_subject.clone(),
        agent_session_id: request_context.agent_session_id.clone(),
        agent_turn_id: request_context.agent_turn_id.clone(),
        decision_request_id: request_context.decision_request_id.clone(),
        request_digest: request_context.request_digest.clone(),
        memory_snapshot: MemoryContextSnapshotV1::empty("session_private"),
        goal_snapshot: GoalSnapshotV1::empty(),
        continuation: None,
    }
}

const WORLD_ID: &str = "world-continuation-fixture";
const AGENT_ID: &str = "agent-continuation-1";
const REQUEST_DIGEST: &str =
    "blake3:2222222222222222222222222222222222222222222222222222222222222222";

fn proposal_value() -> Value {
    json!({
        "schema_version": 1,
        "continuation_proposal_id": "proposal-1",
        "world_id": WORLD_ID,
        "agent_id": AGENT_ID,
        "agent_session_id": "session-continuation-1",
        "agent_turn_id": "turn-continuation-1",
        "decision_request_id": "request-continuation-1",
        "origin_turn_id": "turn-continuation-1",
        "origin_request_digest": REQUEST_DIGEST,
        "action_or_plan_kind": "wait_for_receipt",
        "action_or_envelope_digest": null,
        "remaining_budget": {"unit": "steps", "value": 2},
        "baseline_observation_digest": "blake3:3333333333333333333333333333333333333333333333333333333333333333",
        "goal_digest": "blake3:4444444444444444444444444444444444444444444444444444444444444444",
        "policy_digest": "blake3:5555555555555555555555555555555555555555555555555555555555555555",
        "policy_revision": 3,
        "precondition_summary": "receipt pending",
        "precondition_digest": "blake3:6666666666666666666666666666666666666666666666666666666666666666",
        "wake_conditions": [{
            "schema_version": "wake-condition.v1",
            "kind": "receipt_linked",
            "receipt_id": "blake3:9999999999999999999999999999999999999999999999999999999999999999"
        }],
        "valid_until_tick": 100,
        "source": "harness",
        "proposal_digest": null
    })
}

fn proposal() -> ContinuationProposalV1 {
    let mut value = proposal_value();
    value["proposal_digest"] = json!("");
    let mut proposal: ContinuationProposalV1 =
        serde_json::from_value(value).expect("decode ContinuationProposalV1");
    proposal.proposal_digest = proposal
        .proposal_digest()
        .expect("canonical continuation proposal digest")
        .to_string();
    proposal
}

fn current_context() -> ContinuationCurrentContextV1 {
    ContinuationCurrentContextV1::from_observation(
        Observation {
            time: 10,
            agent_id: AGENT_ID.to_string(),
            pos: crate::geometry::GeoPos::new(0, 0, 0),
            self_resources: Default::default(),
            visibility_range_cm: 100,
            visible_agents: Vec::new(),
            visible_locations: Vec::new(),
            module_lifecycle: Default::default(),
            module_market: Default::default(),
            power_market: Default::default(),
            social_state: Default::default(),
        },
        &GoalSnapshotV1::empty(),
        "blake3:5555555555555555555555555555555555555555555555555555555555555555",
        "blake3:6666666666666666666666666666666666666666666666666666666666666666",
    )
}

fn proposal_for_current_context() -> ContinuationProposalV1 {
    let current = current_context();
    let mut value = serde_json::to_value(proposal()).expect("encode proposal");
    value["baseline_observation_digest"] =
        json!(current.authority.baseline_observation_digest.clone());
    value["goal_digest"] = json!(current.authority.goal_digest.clone());
    value["policy_digest"] = json!(current.authority.policy_digest.clone());
    value["precondition_digest"] = json!(current.authority.precondition_digest.clone());
    let mut proposal: ContinuationProposalV1 =
        serde_json::from_value(value).expect("decode current-context proposal");
    proposal.proposal_digest = proposal
        .proposal_digest()
        .expect("canonical current-context proposal digest")
        .to_string();
    proposal
}

fn fixture() -> (
    AsyncAgentRunner,
    ContinuationProposalV1,
    ContinuationCurrentContextV1,
    ContinuousAgentRequestContextV1,
    std::sync::Arc<std::sync::Mutex<crate::simulator::MockDecisionProviderState>>,
) {
    let original = request_from_value(production_request_fixture(1, 60_000));
    let mut current = current_context();
    current.observation.agent_id = "agent-1".into();
    current = ContinuationCurrentContextV1::from_observation(
        current.observation,
        &GoalSnapshotV1::empty(),
        &current.authority.policy_digest,
        &current.authority.precondition_digest,
    );
    let mut proposal = proposal_for_current_context();
    proposal.agent_id = original.agent_subject.clone();
    proposal.world_id = original.runtime_binding.world_id.clone();
    proposal.agent_session_id = original.agent_session_id.clone();
    proposal.agent_turn_id = original.agent_turn_id.clone();
    proposal.origin_turn_id = original.agent_turn_id.clone();
    proposal.decision_request_id = original.decision_request_id.clone();
    proposal.origin_request_digest = original.request_digest.to_string();
    proposal.baseline_observation_digest = current.authority.baseline_observation_digest.clone();
    proposal.goal_digest = current.authority.goal_digest.clone();
    proposal.proposal_digest = proposal.proposal_digest().unwrap().to_string();
    let provider = MockDecisionProvider::with_scripted_responses(
        "rejected-wait",
        vec![Ok(DecisionResponse::wait("rejected-wait"))],
    );
    let provider_state = provider.shared_state();
    let behavior = ProviderBackedAgentBehavior::new(
        "agent-1",
        provider,
        vec![ActionCatalogEntry::new("wait", "wait")],
    )
    .legacy_compatibility()
    .with_execution_mode(ProviderExecutionMode::HeadlessAgent);
    let mut runner = AsyncAgentRunner::new(16).unwrap();
    runner.register(behavior).unwrap();
    // This test exercises a production request, so install an explicitly
    // host-validated empty feedback history before the first production turn.
    runner
        .restore_preverified_scoped_feedback_history(&[])
        .unwrap();
    let id = runner
        .start_turn_with_request_context_and_observation(
            "agent-1",
            current.observation.clone(),
            production_turn_context(&original),
            original.clone(),
        )
        .unwrap();
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    loop {
        if let Some(outcome) = runner
            .poll_completed()
            .unwrap()
            .into_iter()
            .find(|outcome| outcome.turn_id == id)
        {
            assert_eq!(
                outcome.world_effect,
                super::super::AsyncWorldEffect::NoEffect
            );
            assert_eq!(outcome.feedback, super::super::AsyncTurnFeedback::Wait);
            break;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "real Wait did not complete"
        );
        std::thread::sleep(std::time::Duration::from_millis(1));
    }
    runner
        .submit_continuation_proposal_with_current_context("agent-1", proposal.clone(), &current)
        .unwrap();
    (runner, proposal, current, original, provider_state)
}
fn state(runner: &AsyncAgentRunner) -> String {
    format!(
        "{:?}{:?}{:?}{:?}{:?}",
        runner.continuation_harness,
        runner.continuations,
        runner.awaiting_runtime,
        runner.awaiting_outcomes,
        runner.harness.cognition_state_digest_for_test()
    )
}
#[test]
fn real_wait_rejected_cleanup_releases_exact_turn() {
    let (mut r, p, c, o, _) = fixture();
    let actors = r.actors.len();
    let rejected = r
        .with_rejected_unprojected_wait_cleanup("agent-1", &p, &c, &o, |h| {
            assert_eq!(h.status, "rejected");
            assert!(!h.active);
            assert!(!h.world_effect);
            Ok(())
        })
        .unwrap();
    assert_eq!(rejected.proposal, p);
    assert!(
        r.rejected_unprojected_wait_is_absent("agent-1", &p, &o)
            .unwrap()
    );
    assert_eq!(r.actors.len(), actors);
}
#[test]
fn persist_failure_restores_five_collections_and_can_retry() {
    let (mut r, p, c, o, provider_state) = fixture();
    let before = state(&r);
    assert!(
        r.with_rejected_unprojected_wait_cleanup("agent-1", &p, &c, &o, |_| Err(
            "actual persist failed".into()
        ))
        .is_err()
    );
    assert_eq!(state(&r), before);
    assert_eq!(
        provider_state
            .lock()
            .expect("provider state lock")
            .recorded_requests
            .len(),
        1,
        "failed persistence must not dispatch a successor provider request"
    );
    r.with_rejected_unprojected_wait_cleanup("agent-1", &p, &c, &o, |_| Ok(()))
        .unwrap();
    assert!(
        r.rejected_unprojected_wait_is_absent("agent-1", &p, &o)
            .unwrap()
    );
    assert_eq!(
        provider_state
            .lock()
            .expect("provider state lock")
            .recorded_requests
            .len(),
        1,
        "successful cleanup must not replay the predecessor provider request"
    );
}

#[test]
fn turn_release_rejects_mismatched_identity_before_persistence() {
    let (mut runner, _, _, original, _) = fixture();
    let before = state(&runner);
    let persisted = std::cell::Cell::new(false);
    assert!(
        runner
            .with_runtime_turn_release_for_continuation(
                "agent-1",
                &original.agent_session_id,
                &original.agent_turn_id,
                "different-request",
                original.request_digest.as_str(),
                |_| {
                    persisted.set(true);
                    Ok(())
                },
            )
            .is_err()
    );
    assert!(!persisted.get());
    assert_eq!(state(&runner), before);
}

#[test]
fn identity_pristine_and_awaiting_tampering_never_persists() {
    for case in 0..8 {
        let (mut r, mut p, c, mut o, _) = fixture();
        match case {
            0 => p.precondition_summary.push_str(" tamper"),
            1 => o.transport_attempt += 1,
            2 => r.continuations.get_mut("agent-1").unwrap().continuation_id = "admitted".into(),
            3 => r.continuations.get_mut("agent-1").unwrap().consumed_budget = 1,
            4 => {
                r.awaiting_runtime.remove("agent-1");
            }
            5 => {
                r.continuations.remove("agent-1");
            }
            6 => {
                r.awaiting_outcomes.clear();
            }
            _ => {
                r.continuations
                    .get_mut("agent-1")
                    .unwrap()
                    .terminal_disposition = Some("rejected".into())
            }
        }
        let before = state(&r);
        let called = std::cell::Cell::new(false);
        assert!(
            r.with_rejected_unprojected_wait_cleanup("agent-1", &p, &c, &o, |_| {
                called.set(true);
                Ok(())
            })
            .is_err(),
            "case {case}"
        );
        assert!(!called.get());
        assert_eq!(state(&r), before);
    }
}
#[test]
fn harness_disagreement_keeps_original_handle_and_turn() {
    let (mut r, p, c, o, _) = fixture();
    r.continuation_harness = Default::default();
    let before = state(&r);
    assert!(
        r.with_rejected_unprojected_wait_cleanup("agent-1", &p, &c, &o, |_| panic!(
            "must not persist"
        ))
        .is_err()
    );
    assert_eq!(state(&r), before);
}

fn admitted_projection(p: &ContinuationProposalV1) -> RuntimeAgentContinuation {
    let mut runtime: RuntimeAgentContinuation = serde_json::from_value(json!({
        "schema_version": "agent-continuation.v1",
        "continuation_id": "continuation-1", "wake_id": "wake-1",
        "world_id": p.world_id, "branch_id": "main", "finality_epoch": 7,
        "finality_block_hash": "blake3:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
        "finality_status": "verified", "reorg_epoch": 3,
        "runtime_manifest_hash": "blake3:cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc",
        "agent_id": p.agent_id, "agent_session_id": p.agent_session_id,
        "agent_turn_id": p.agent_turn_id, "decision_request_id": p.decision_request_id,
        "origin_turn_id": p.origin_turn_id, "origin_request_digest": p.origin_request_digest,
        "continuation_proposal_id": p.continuation_proposal_id, "proposal_digest": p.proposal_digest,
        "action_or_envelope_digest": p.action_or_envelope_digest,
        "wake_conditions": p.wake_conditions, "remaining_budget": p.remaining_budget,
        "valid_until_tick": p.valid_until_tick, "precondition_digest": p.precondition_digest,
        "wake_seq": 1, "logical_tick": 42, "status": "scheduled"
    })).unwrap();
    runtime.refresh_status_digest();
    runtime.validate_authoritative().unwrap();
    runtime
}
fn rejection_fixture() -> (
    AsyncAgentRunner,
    RuntimeAgentContinuation,
    ContinuationCurrentContextV1,
) {
    let (mut r, p, c, _, _) = fixture();
    let mut runtime = admitted_projection(&p);
    r.apply_runtime_continuation_projection_with_current_context("agent-1", runtime.clone(), &c)
        .unwrap();
    runtime.status = crate::runtime::ContinuationStatusV1::Rejected;
    runtime.terminal_disposition = Some("rejected".into());
    runtime.refresh_status_digest();
    (r, runtime, c)
}
#[test]
fn resume_rejected_cleanup_handles_released_or_exact_wait_turn() {
    for released in [false, true] {
        let (mut r, runtime, c) = rejection_fixture();
        if released {
            r.release_runtime_turn_for_continuation(
                "agent-1",
                &runtime.agent_session_id,
                &runtime.agent_turn_id,
                &runtime.decision_request_id,
                &runtime.origin_request_digest,
            )
            .unwrap();
        }
        let actors = r.actors.len();
        let h = r
            .with_rejected_runtime_continuation_cleanup("agent-1", runtime, &c.authority, || Ok(()))
            .unwrap();
        assert_eq!(h.status, "rejected");
        assert!(!h.active);
        assert_eq!(r.actors.len(), actors);
        assert!(!r.continuations.contains_key("agent-1"));
        assert!(!r.awaiting_runtime.contains_key("agent-1"));
        assert!(
            !r.awaiting_outcomes
                .values()
                .any(|o| o.agent_id == "agent-1")
        );
    }
}
#[test]
fn resume_rejected_persist_failure_restores_all_five_ledgers_then_retries() {
    let (mut r, runtime, c) = rejection_fixture();
    let before = r.rejected_wait_test_ledger_digests();
    let observed = std::cell::RefCell::new(None);
    assert!(
        r.with_rejected_runtime_continuation_cleanup_observed(
            "agent-1",
            runtime.clone(),
            &c.authority,
            |digests| {
                *observed.borrow_mut() = Some(digests);
                Err("durable write failed".into())
            }
        )
        .is_err()
    );
    assert_ne!(observed.into_inner().unwrap(), before);
    assert_eq!(r.rejected_wait_test_ledger_digests(), before);
    r.with_rejected_runtime_continuation_cleanup("agent-1", runtime, &c.authority, || Ok(()))
        .unwrap();
}
#[test]
fn resume_rejected_cleanup_never_releases_newer_or_inconsistent_turn() {
    for case in 0..5 {
        let (mut r, mut runtime, c) = rejection_fixture();
        let old_id = *r.awaiting_runtime.get("agent-1").unwrap();
        match case {
            0 => {
                r.awaiting_runtime
                    .insert("agent-1".into(), AsyncTurnId(old_id.get() + 1));
            }
            1 => {
                r.awaiting_runtime.remove("agent-1");
            }
            2 => {
                let mut extra = r.awaiting_outcomes.get(&old_id).unwrap().clone();
                extra.turn_id = AsyncTurnId(old_id.get() + 1);
                r.awaiting_outcomes.insert(extra.turn_id, extra);
            }
            3 => {
                runtime.continuation_id = "other-continuation".into();
                runtime.refresh_status_digest();
            }
            _ => {
                runtime.wake_id = "other-wake".into();
                runtime.refresh_status_digest();
            }
        }
        let before = r.rejected_wait_test_ledger_digests();
        let called = std::cell::Cell::new(false);
        let result =
            r.with_rejected_runtime_continuation_cleanup("agent-1", runtime, &c.authority, || {
                called.set(true);
                Ok(())
            });
        assert!(result.is_err(), "case {case}");
        assert!(!called.get(), "case {case}");
        assert_eq!(r.rejected_wait_test_ledger_digests(), before, "case {case}");
    }
}

#[test]
fn restored_rejected_resume_absence_preserves_origin_and_fences_newer_state() {
    for case in 0..7 {
        let (mut runner, mut proposal, current, mut request, _) = fixture();
        let old_outcome = runner.awaiting_outcomes.values().next().unwrap().clone();
        runner
            .with_rejected_unprojected_wait_cleanup(
                "agent-1",
                &proposal,
                &current,
                &request,
                |_| Ok(()),
            )
            .unwrap();
        proposal.origin_turn_id = "previous-wait-turn".into();
        proposal.origin_request_digest =
            "blake3:9999999999999999999999999999999999999999999999999999999999999999".into();
        proposal.proposal_digest = proposal.proposal_digest().unwrap().to_string();
        let mut turn = production_turn_context(&request);
        turn.continuation = Some(proposal.clone());
        request.continuation_digest = h_v1("oasis7.cognition.continuation.v1", &turn.continuation);
        request.request_digest = request.request_digest();
        turn.request_digest = request.request_digest.clone();
        assert!(
            runner
                .rejected_unprojected_wait_is_absent("agent-1", &proposal, &request)
                .is_err()
        );
        match case {
            0 => {}
            1 => turn.agent_turn_id = "different-turn".into(),
            2 => turn.continuation = None,
            3 => {
                request.continuation_digest = Digest32::from(
                    "blake3:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
                )
            }
            4 => {
                // Exercise a real active turn in the shared actor, rather than
                // mutating its internal association map. The reduced lane must
                // still fence the production Resume absence check.
                runner.start_turn("agent-1").unwrap();
            }
            5 => {
                runner
                    .awaiting_runtime
                    .insert("agent-1".into(), AsyncTurnId(987654));
            }
            _ => {
                runner
                    .awaiting_outcomes
                    .insert(old_outcome.turn_id, old_outcome);
            }
        }
        let before = state(&runner);
        let result =
            runner.rejected_unprojected_resume_is_absent("agent-1", &proposal, &request, &turn);
        if case == 0 {
            assert!(result.unwrap());
        } else {
            assert!(!result.unwrap_or(false), "case {case}");
        }
        assert_eq!(state(&runner), before);
    }
}

#[test]
fn combined_poll_preserves_production_and_compatibility_completions() {
    let mut runner = AsyncAgentRunner::new(16).unwrap();
    runner
        .register(BuiltinWaitBehavior::new("agent-compat".to_string()))
        .unwrap();
    runner
        .register(BuiltinWaitBehavior::new("agent-1".to_string()))
        .unwrap();
    runner
        .restore_preverified_scoped_feedback_history(&[])
        .unwrap();

    let compatibility_turn = runner.start_turn("agent-compat").unwrap();
    let request = request_from_value(production_request_fixture(1, 60_000));
    let production_turn = runner
        .start_turn_with_request_context("agent-1", production_turn_context(&request), request)
        .unwrap();

    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    let mut completions = Vec::new();
    while completions.len() < 2 {
        completions.extend(runner.poll_completed().unwrap());
        assert!(
            std::time::Instant::now() < deadline,
            "combined poll lost a production or compatibility completion: active={}, compat_in_flight={}, production_in_flight={}, outcomes={}",
            runner.active_turn_count(),
            runner.provider_is_still_in_flight("agent-compat"),
            runner.provider_is_still_in_flight("agent-1"),
            completions.len(),
        );
        if completions.len() < 2 {
            std::thread::sleep(std::time::Duration::from_millis(1));
        }
    }

    let compatibility = completions
        .iter()
        .find(|outcome| outcome.turn_id == compatibility_turn)
        .expect("compatibility completion is returned");
    assert!(compatibility.prepared_request_context.is_none());
    let production = completions
        .iter()
        .find(|outcome| outcome.turn_id == production_turn)
        .expect("production completion is returned");
    assert!(production.prepared_request_context.is_some());
    assert_eq!(runner.active_turn_count(), 0);
    assert!(runner.poll_completed().unwrap().is_empty());
}

#[path = "async_agent_runner_terminal_budget_tests.rs"]
mod terminal_budget_tests;
