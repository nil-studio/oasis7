//! Harness-policy fixtures for ContinuationProposalV1 and wake progression.
//!
//! Harness owns the bounded proposal and policy fields; Runtime owns the
//! durable schedule/status projection and finality truth.  These fixtures
//! intentionally do not cover GoalGraph, belief/preference state or billing.

use crate::runtime::{AgentContinuation, ContinuationBudgetV1, ContinuationStatusV1};
use crate::simulator::{
    AsyncAgentRunner, ContinuationAuthorityContextV1, ContinuationCurrentContextV1,
    ContinuationHarness, ContinuationInvalidationReason, ContinuationProposalV1,
    ContinuousAgentTurnContextV1, Digest32, GoalSnapshotV1, MemoryContextSnapshotV1,
    RuntimeContinuationStatusV1,
};
use oasis7_simulator::Observation;
use serde_json::{Value, json};

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

fn authority_context() -> ContinuationAuthorityContextV1 {
    ContinuationAuthorityContextV1 {
        baseline_observation_digest:
            "blake3:3333333333333333333333333333333333333333333333333333333333333333".to_string(),
        goal_digest: "blake3:4444444444444444444444444444444444444444444444444444444444444444"
            .to_string(),
        policy_digest: "blake3:5555555555555555555555555555555555555555555555555555555555555555"
            .to_string(),
        precondition_digest:
            "blake3:6666666666666666666666666666666666666666666666666666666666666666".to_string(),
    }
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

fn runtime_projection(
    proposal: &ContinuationProposalV1,
    status: ContinuationStatusV1,
    terminal_disposition: Option<&str>,
) -> AgentContinuation {
    let wake_conditions = serde_json::from_value(
        serde_json::to_value(&proposal.wake_conditions).expect("encode proposal wake conditions"),
    )
    .expect("decode Runtime wake conditions");
    let mut runtime = AgentContinuation {
        schema_version: "agent-continuation.v1".to_string(),
        continuation_id: "continuation-runtime-1".to_string(),
        wake_id: "wake-runtime-1".to_string(),
        world_id: proposal.world_id.clone(),
        branch_id: "branch-continuation-fixture".to_string(),
        finality_epoch: 7,
        finality_block_hash: Some(
            "blake3:7777777777777777777777777777777777777777777777777777777777777777".to_string(),
        ),
        finality_status: "verified".to_string(),
        reorg_epoch: 1,
        runtime_manifest_hash:
            "blake3:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".to_string(),
        agent_id: proposal.agent_id.clone(),
        agent_session_id: proposal.agent_session_id.clone(),
        agent_turn_id: proposal.agent_turn_id.clone(),
        decision_request_id: proposal.decision_request_id.clone(),
        origin_turn_id: proposal.origin_turn_id.clone(),
        origin_request_digest: proposal.origin_request_digest.clone(),
        continuation_proposal_id: proposal.continuation_proposal_id.clone(),
        proposal_digest: proposal.proposal_digest.clone(),
        action_or_envelope_digest: proposal.action_or_envelope_digest.clone(),
        wake_conditions,
        next_wake_tick: Some(10),
        remaining_budget: ContinuationBudgetV1 {
            unit: proposal.remaining_budget.unit.clone(),
            value: proposal.remaining_budget.value,
        },
        valid_until_tick: proposal.valid_until_tick,
        precondition_digest: proposal.precondition_digest.clone(),
        wake_seq: 1,
        logical_tick: 10,
        status,
        continuation_status_digest: None,
        terminal_disposition: terminal_disposition.map(str::to_string),
    };
    runtime.continuation_status_digest = Some(runtime.status_digest());
    runtime
        .validate_authoritative()
        .expect("Runtime continuation projection must be authoritative");
    runtime
}

#[test]
fn proposal_canonical_digest_covers_full_identity_binding_policy_and_budget() {
    let first = proposal();
    assert_eq!(
        first.proposal_digest,
        "blake3:a2390a5908bd4568a1ebd77d3e82c876556d8e93697ea60890ba3137d9adb8ea"
    );
    first.validate().expect("valid continuation proposal");
    let first_digest = first.proposal_digest().expect("proposal digest");
    assert!(first_digest.as_str().starts_with("blake3:"));
    assert_eq!(
        first_digest,
        first.proposal_digest().expect("stable digest")
    );

    for (field, changed) in [
        ("agent_session_id", json!("session-other")),
        ("origin_request_digest", json!("blake3:other-request")),
        (
            "baseline_observation_digest",
            json!("blake3:other-observation"),
        ),
        ("goal_digest", json!("blake3:other-goal")),
        ("policy_digest", json!("blake3:other-policy")),
    ] {
        let mut value = serde_json::to_value(&first).expect("encode proposal");
        value[field] = changed;
        let candidate: ContinuationProposalV1 =
            serde_json::from_value(value).expect("decode changed proposal");
        assert_ne!(
            first_digest,
            candidate
                .proposal_digest()
                .expect("changed proposal digest"),
            "{field} must remain in proposal identity"
        );
    }

    for unit in ["steps", "ticks"] {
        let mut value = serde_json::to_value(&first).expect("encode proposal");
        value["remaining_budget"] = json!({"unit": unit, "value": 2});
        let mut candidate: ContinuationProposalV1 =
            serde_json::from_value(value).expect("decode budget variant");
        candidate.proposal_digest = candidate
            .proposal_digest()
            .expect("recompute bounded budget digest")
            .to_string();
        candidate.validate().expect("bounded budget variant");
        assert!(candidate.remaining_budget.value > 0);
    }
}

#[test]
fn runtime_admission_payload_preserves_full_proposal_contract_and_digest() {
    let proposal = proposal();
    let payload = proposal
        .runtime_admission_payload()
        .expect("validated proposal admission payload");
    for field in [
        "agent_session_id",
        "agent_turn_id",
        "decision_request_id",
        "origin_request_digest",
        "action_or_plan_kind",
        "remaining_budget",
        "baseline_observation_digest",
        "goal_digest",
        "policy_digest",
        "policy_revision",
        "precondition_digest",
        "wake_conditions",
        "source",
        "proposal_digest",
    ] {
        assert!(
            payload.get(field).is_some(),
            "admission payload lost {field}"
        );
    }
    assert_eq!(
        proposal
            .runtime_admission_digest()
            .expect("admission digest"),
        proposal.proposal_digest().expect("proposal digest")
    );
    assert!(
        !proposal
            .runtime_admission_bytes()
            .expect("canonical admission bytes")
            .is_empty()
    );
}

#[test]
fn invalid_proposal_binding_budget_or_digest_fails_closed_without_projection() {
    let mut invalid = serde_json::to_value(proposal()).expect("encode proposal");
    invalid["agent_session_id"] = Value::Null;
    serde_json::from_value::<ContinuationProposalV1>(invalid).expect_err("missing binding");

    let mut invalid_budget = serde_json::to_value(proposal()).expect("encode proposal");
    invalid_budget["remaining_budget"] = json!({"unit": "minutes", "value": 2});
    let error = serde_json::from_value::<ContinuationProposalV1>(invalid_budget)
        .expect("decode bounded input for validation")
        .validate()
        .expect_err("unknown budget unit");
    assert_eq!(error.code(), "continuation_budget_invalid");

    let mut invalid_digest = serde_json::to_value(proposal()).expect("encode proposal");
    invalid_digest["proposal_digest"] = json!("blake3:tampered");
    let candidate: ContinuationProposalV1 =
        serde_json::from_value(invalid_digest).expect("decode tampered digest");
    assert_eq!(
        candidate
            .validate()
            .expect_err("tampered proposal digest")
            .code(),
        "continuation_digest_mismatch"
    );
}

#[test]
fn runtime_status_projection_is_consumed_and_harness_never_invents_terminal_truth() {
    let mut harness = ContinuationHarness::default();
    let submitted_proposal = proposal();
    let pending = harness
        .submit(submitted_proposal)
        .expect("submit proposal to Harness");
    let projection = harness
        .consume_runtime_projection(
            pending,
            &runtime_projection(&proposal(), ContinuationStatusV1::Pending, None),
        )
        .expect("consume Runtime pending projection");
    let projection = serde_json::to_value(projection).expect("encode pending projection");
    assert_eq!(projection["status"], "pending");
    assert_eq!(projection["terminal_disposition"], Value::Null);
    assert_eq!(projection["world_effect"], false);

    let mut committed = harness.submit(proposal()).expect("submit second proposal");
    let receipt = runtime_projection(
        &committed.proposal,
        ContinuationStatusV1::Completed,
        Some("completed"),
    );
    committed = harness
        .consume_runtime_projection(committed, &receipt)
        .expect("consume committed Runtime projection");
    let committed = serde_json::to_value(committed).expect("encode committed projection");
    assert_eq!(committed["status"], "completed");
    assert_eq!(committed["provenance"], "runtime_authoritative");
    assert_eq!(committed["terminal_disposition"], "completed");
    assert_eq!(committed["world_effect"], false);
}

#[test]
fn observation_goal_policy_and_terminal_runtime_changes_clear_continuation_deterministically() {
    for (reason, expected_status) in [
        (
            ContinuationInvalidationReason::ObservationChanged,
            "invalidated",
        ),
        (ContinuationInvalidationReason::GoalChanged, "invalidated"),
        (ContinuationInvalidationReason::PolicyChanged, "invalidated"),
        (ContinuationInvalidationReason::Rejected, "rejected"),
        (ContinuationInvalidationReason::Stale, "invalidated"),
        (ContinuationInvalidationReason::Timeout, "expired"),
        (ContinuationInvalidationReason::Expired, "expired"),
        (ContinuationInvalidationReason::Reorg, "invalidated"),
        (ContinuationInvalidationReason::Cancelled, "cancelled"),
    ] {
        let mut harness = ContinuationHarness::default();
        let active = harness.submit(proposal()).expect("submit continuation");
        let result = harness
            .invalidate(active, reason)
            .expect("invalidate continuation");
        let result = serde_json::to_value(result).expect("encode invalidation result");
        assert_eq!(result["status"], expected_status);
        assert_eq!(result["active"], false);
        assert_eq!(result["provider_invocation_count"], 0);
        assert_eq!(result["world_effect"], false);
    }
}

#[test]
fn continuation_submission_is_proposal_only_and_rejects_uncorrelated_runtime_status() {
    let submitted_proposal = proposal();
    let mut runner = AsyncAgentRunner::builtin_fixture(AGENT_ID);
    let handle = runner
        .submit_continuation_proposal(AGENT_ID, submitted_proposal)
        .expect("submit Harness continuation proposal");

    let proposal_only = handle.continuation_id.is_empty()
        && handle.wake_id.is_empty()
        && handle.wake_seq == 0
        && handle.continuation_digest.is_empty()
        && handle.continuation_status_digest.is_empty()
        && handle.status == "scheduled"
        && handle.provenance == "harness_policy"
        && !handle.world_effect;

    let mut harness = ContinuationHarness::default();
    let submitted = harness
        .submit(proposal())
        .expect("submit proposal to Harness");
    let unrelated_status = RuntimeContinuationStatusV1 {
        status: "pending".to_string(),
        terminal_disposition: None,
        continuation_id: "runtime-unrelated-continuation".to_string(),
        wake_id: "runtime-unrelated-wake".to_string(),
        wake_seq: 99,
        continuation_digest:
            "blake3:9999999999999999999999999999999999999999999999999999999999999999".to_string(),
    };
    let uncorrelated_status_rejected = harness
        .consume_runtime_status(submitted, unrelated_status)
        .is_err();
    assert!(
        proposal_only && uncorrelated_status_rejected,
        "Harness must remain proposal-only and reject uncorrelated Runtime status: proposal_only={proposal_only}, uncorrelated_status_rejected={uncorrelated_status_rejected}, handle={handle:?}"
    );
}

#[test]
fn strict_continuation_admission_revalidates_every_authoritative_digest() {
    for (field, replacement) in [
        (
            "baseline_observation_digest",
            "blake3:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
        ),
        (
            "goal_digest",
            "blake3:bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
        ),
        (
            "policy_digest",
            "blake3:cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc",
        ),
        (
            "precondition_digest",
            "blake3:dddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddd",
        ),
    ] {
        let mut current = authority_context();
        match field {
            "baseline_observation_digest" => {
                current.baseline_observation_digest = replacement.to_string()
            }
            "goal_digest" => current.goal_digest = replacement.to_string(),
            "policy_digest" => current.policy_digest = replacement.to_string(),
            "precondition_digest" => current.precondition_digest = replacement.to_string(),
            _ => unreachable!("fixture field is exhaustive"),
        }
        let mut harness = ContinuationHarness::default();
        let error = harness
            .submit_with_context(proposal(), &current)
            .expect_err("changed authoritative context must reject the old proposal");
        assert_eq!(error.code(), "continuation_context_stale", "field={field}");
        assert_eq!(harness.active_count(), 0, "field={field}");
    }
}

#[test]
fn continuation_budget_is_chain_owned_monotonic_and_duplicate_wake_idempotent() {
    let mut harness = ContinuationHarness::default();
    let context = authority_context();
    let mut current = harness
        .submit_with_context(proposal(), &context)
        .expect("strict continuation admission");

    let first = harness
        .consume_wake(&mut current, "wake-delivery-1", 1)
        .expect("consume one step");
    assert_eq!(first.remaining, 1);
    assert!(!first.exhausted);

    let duplicate = harness
        .consume_wake(&mut current, "wake-delivery-1", 99)
        .expect("duplicate delivery is idempotent");
    assert_eq!(duplicate.remaining, 1);
    assert!(duplicate.duplicate);
    assert_eq!(current.remaining_budget.value, 1);

    let mut reset_attempt: Value = serde_json::to_value(proposal()).expect("encode proposal");
    reset_attempt["continuation_proposal_id"] = json!("proposal-reset-attempt");
    reset_attempt["remaining_budget"] = json!({"unit": "steps", "value": 2});
    let mut reset_attempt: ContinuationProposalV1 =
        serde_json::from_value(reset_attempt).expect("decode reset attempt");
    reset_attempt.proposal_digest = reset_attempt
        .proposal_digest()
        .expect("canonical reset attempt digest")
        .to_string();
    let error = harness
        .submit_with_context(reset_attempt, &context)
        .expect_err("new proposal ID cannot reset a chain budget");
    assert_eq!(error.code(), "continuation_budget_increase");

    let exhausted = harness
        .consume_wake(&mut current, "wake-delivery-2", 1)
        .expect("consume final step");
    assert_eq!(exhausted.remaining, 0);
    assert!(exhausted.exhausted);
    assert_eq!(
        exhausted.terminal_disposition.as_deref(),
        Some("budget_exhausted")
    );
    assert!(!current.active);
}

#[test]
fn oasis_continuation_adapter_matches_harness_accounting_projection() {
    let proposal = proposal();
    let context = authority_context();
    let harness_context: oasis7_agent_harness::ContinuationAuthorityContextV1 =
        serde_json::from_value(serde_json::to_value(&context).unwrap()).unwrap();
    let mut oasis = ContinuationHarness::default();
    let mut harness = oasis7_agent_harness::ContinuationHarness::default();

    let mut oasis_handle = oasis
        .submit_with_context(proposal.clone(), &context)
        .expect("Oasis adapter admits a matching proposal");
    let mut harness_handle = harness
        .submit_with_context(proposal, &harness_context)
        .expect("Harness core admits the same matching proposal");
    assert_eq!(
        serde_json::to_value(&oasis_handle).unwrap(),
        serde_json::to_value(&harness_handle).unwrap()
    );

    let oasis_progress = oasis
        .consume_wake(&mut oasis_handle, "parity-wake-1", 1)
        .expect("Oasis adapter consumes the wake");
    let harness_progress = harness
        .consume_wake(&mut harness_handle, "parity-wake-1", 1)
        .expect("Harness core consumes the same wake");
    assert_eq!(
        serde_json::to_value(oasis_progress).unwrap(),
        serde_json::to_value(harness_progress).unwrap()
    );
    assert_eq!(
        serde_json::to_value(&oasis_handle).unwrap(),
        serde_json::to_value(&harness_handle).unwrap()
    );

    let oasis_terminal = oasis
        .invalidate(oasis_handle, ContinuationInvalidationReason::Cancelled)
        .expect("Oasis adapter retires the continuation");
    let harness_terminal = harness
        .invalidate(
            harness_handle,
            oasis7_agent_harness::ContinuationInvalidationReason::Cancelled,
        )
        .expect("Harness core retires the same continuation");
    assert_eq!(
        serde_json::to_value(oasis_terminal).unwrap(),
        serde_json::to_value(harness_terminal).unwrap()
    );
}

#[test]
fn consumed_runtime_wake_must_revalidate_context_and_expose_next_step() {
    let mut harness = ContinuationHarness::default();
    let context = authority_context();
    let submitted = harness
        .submit_with_context(proposal(), &context)
        .expect("strict continuation admission");
    let mut runtime = runtime_projection(&submitted.proposal, ContinuationStatusV1::Consumed, None);
    runtime.remaining_budget.value = 1;
    runtime.refresh_status_digest();
    runtime
        .validate_authoritative()
        .expect("consumed projection remains authoritative");

    let ready = harness
        .advance_ready_wake(submitted, &runtime, &context)
        .expect("valid consumed wake advances to the next cognition step");
    assert_eq!(ready.status, "consumed");
    assert_eq!(ready.remaining_budget.value, 1);
    assert!(!ready.active, "the consumed wake is retired before replan");

    let mut stale = authority_context();
    stale.goal_digest =
        "blake3:eeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee".to_string();
    let mut harness = ContinuationHarness::default();
    let submitted = harness
        .submit_with_context(proposal(), &context)
        .expect("strict continuation admission");
    let error = harness
        .advance_ready_wake(submitted, &runtime, &stale)
        .expect_err("changed goal must stop wake-to-action progression");
    assert_eq!(error.code(), "continuation_context_stale");
    assert_eq!(harness.active_count(), 1);
}

#[test]
fn current_context_attestation_rejects_changed_observation_before_admission() {
    let current = current_context();
    let proposal = proposal_for_current_context();
    let mut changed = current.clone();
    changed.observation.pos = crate::geometry::GeoPos::new(1, 0, 0);
    let mut runner = AsyncAgentRunner::builtin_fixture(AGENT_ID);
    let error = runner
        .submit_continuation_proposal_with_current_context(AGENT_ID, proposal, &changed)
        .expect_err("changed current observation must not admit the old proposal");
    assert!(error.to_string().contains("observation"));
}

#[test]
fn fresh_runner_hydrates_runtime_pending_continuation_and_rejects_stale_wake() {
    let current = current_context();
    let proposal = proposal_for_current_context();
    let runtime = runtime_projection(&proposal, ContinuationStatusV1::Pending, None);
    let mut runner = AsyncAgentRunner::builtin_fixture(AGENT_ID);
    let hydrated = runner
        .hydrate_runtime_continuation(AGENT_ID, proposal.clone(), &current, runtime.clone())
        .expect("fresh Agent runner hydrates Runtime continuation");
    assert!(hydrated.active);
    assert_eq!(hydrated.status, "pending");
    assert_eq!(hydrated.continuation_id, runtime.continuation_id);
    assert_eq!(hydrated.wake_id, runtime.wake_id);
    assert_eq!(
        hydrated.remaining_budget.value,
        proposal.remaining_budget.value
    );

    let mut stale = current;
    stale.authority.policy_digest =
        "blake3:9999999999999999999999999999999999999999999999999999999999999999".to_string();
    let error = runner
        .apply_runtime_continuation_projection_with_current_context(AGENT_ID, runtime, &stale)
        .expect_err("hydrated continuation must reject a changed policy digest");
    assert!(error.to_string().contains("stale"));
}

#[test]
fn fresh_runner_authority_hydration_rejects_runtime_lineage_mismatch_without_mutation() {
    let current = current_context();
    let proposal = proposal_for_current_context();
    let mut runtime = runtime_projection(&proposal, ContinuationStatusV1::Pending, None);
    runtime.proposal_digest =
        "blake3:9999999999999999999999999999999999999999999999999999999999999999".to_string();
    runtime.refresh_status_digest();
    let mut runner = AsyncAgentRunner::builtin_fixture(AGENT_ID);
    let error = runner
        .hydrate_runtime_continuation_with_authority(
            AGENT_ID,
            proposal,
            &current.authority,
            runtime,
        )
        .expect_err("mismatched Runtime proposal digest must fail closed");
    assert!(error.to_string().contains("lineage"));
    assert_eq!(runner.active_continuation_proposal_id(AGENT_ID), None);
}

#[test]
fn consumed_wake_dispatches_one_next_actor_turn_and_rejects_late_duplicate() {
    let current = current_context();
    let proposal = proposal_for_current_context();
    let mut runtime = runtime_projection(&proposal, ContinuationStatusV1::Consumed, None);
    runtime.remaining_budget.value = 1;
    runtime.refresh_status_digest();
    runtime
        .validate_authoritative()
        .expect("consumed Runtime projection remains authoritative");
    let mut runner = AsyncAgentRunner::builtin_fixture(AGENT_ID);
    runner
        .submit_continuation_proposal_with_current_context(AGENT_ID, proposal, &current)
        .expect("admit current continuation");
    let turn_context = ContinuousAgentTurnContextV1 {
        agent_id: AGENT_ID.to_string(),
        agent_session_id: "session-continuation-1".to_string(),
        agent_turn_id: "turn-continuation-next".to_string(),
        decision_request_id: "request-continuation-next".to_string(),
        request_digest: Digest32::from(
            "blake3:7777777777777777777777777777777777777777777777777777777777777777",
        ),
        memory_snapshot: MemoryContextSnapshotV1::empty("continuation-test"),
        goal_snapshot: GoalSnapshotV1::empty(),
        continuation: None,
    };
    let next = runner
        .resume_consumed_continuation(
            AGENT_ID,
            current.observation.clone(),
            turn_context,
            &current.authority,
            runtime.clone(),
        )
        .expect("consumed wake enters the next actor turn")
        .expect("consumed wake is not terminal");
    let mut saw_next = false;
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
    while std::time::Instant::now() < deadline {
        if runner
            .poll_completed()
            .expect("poll next actor turn")
            .iter()
            .any(|outcome| outcome.turn_id == next)
        {
            saw_next = true;
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(1));
    }
    assert!(
        saw_next,
        "wake dispatch must produce one next actor outcome"
    );
    let error = runner
        .resume_consumed_continuation(
            AGENT_ID,
            current.observation,
            ContinuousAgentTurnContextV1 {
                agent_id: AGENT_ID.to_string(),
                agent_session_id: "session-continuation-1".to_string(),
                agent_turn_id: "turn-continuation-late".to_string(),
                decision_request_id: "request-continuation-late".to_string(),
                request_digest: Digest32::from(
                    "blake3:8888888888888888888888888888888888888888888888888888888888888888",
                ),
                memory_snapshot: MemoryContextSnapshotV1::empty("continuation-test"),
                goal_snapshot: GoalSnapshotV1::empty(),
                continuation: None,
            },
            &current.authority,
            runtime,
        )
        .expect_err("late duplicate wake cannot re-enter after retirement");
    assert!(error.to_string().contains("unknown continuation"));
}

#[test]
fn failed_next_turn_dispatch_keeps_the_consumed_continuation_retryable() {
    let current = current_context();
    let proposal = proposal_for_current_context();
    let mut runtime = runtime_projection(&proposal, ContinuationStatusV1::Consumed, None);
    runtime.remaining_budget.value = 1;
    runtime.refresh_status_digest();
    let mut runner = AsyncAgentRunner::blocking_provider_fixture(AGENT_ID);
    // Occupy the actor before admitting the continuation. The proposal lane
    // is independent of actor availability, while the resumed dispatch must
    // reject the busy actor after the Runtime wake is reconciled.
    runner.start_turn(AGENT_ID).expect("occupy actor mailbox");
    runner
        .submit_continuation_proposal_with_current_context(AGENT_ID, proposal, &current)
        .expect("admit current continuation");

    // The resumed dispatch fails after the Runtime wake is reconciled. The
    // continuation must be restored as one transaction, allowing a retry once
    // the actor is available.
    let turn_context = ContinuousAgentTurnContextV1 {
        agent_id: AGENT_ID.to_string(),
        agent_session_id: "session-continuation-1".to_string(),
        agent_turn_id: "turn-continuation-next".to_string(),
        decision_request_id: "request-continuation-next".to_string(),
        request_digest: Digest32::from(
            "blake3:7777777777777777777777777777777777777777777777777777777777777777",
        ),
        memory_snapshot: MemoryContextSnapshotV1::empty("continuation-test"),
        goal_snapshot: GoalSnapshotV1::empty(),
        continuation: None,
    };

    let error = runner
        .resume_consumed_continuation(
            AGENT_ID,
            current.observation,
            turn_context,
            &current.authority,
            runtime,
        )
        .expect_err("busy actor rejects the next turn dispatch");
    assert_eq!(error.code(), "agent_busy");
    assert_eq!(
        runner.active_continuation_proposal_id(AGENT_ID),
        Some("proposal-1"),
        "failed dispatch must roll back Harness retirement"
    );
}

#[test]
fn legacy_runtime_projection_is_fenced_from_authoritative_state() {
    let proposal = proposal();
    let runtime = runtime_projection(&proposal, ContinuationStatusV1::Pending, None);
    let mut runner = AsyncAgentRunner::builtin_fixture(AGENT_ID);
    runner
        .submit_continuation_proposal(AGENT_ID, proposal)
        .expect("legacy proposal remains proposal-only");
    let error = runner
        .apply_runtime_continuation_projection(AGENT_ID, runtime)
        .expect_err("legacy projection cannot cross the Runtime authority fence");
    assert!(error.to_string().contains("strict continuation"));
}
