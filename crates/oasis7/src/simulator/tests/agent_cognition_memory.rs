//! Harness-policy RED fixtures for memory retrieval and write intents.
//!
//! This file deliberately stays inside the P0 Agent-private policy: shared
//! world/player-visible memory, belief/preference correction and cognition
//! billing are not part of this contract.

use crate::simulator::{
    AgentBehavior, AgentDecision, AsyncAgentRunner, AsyncAgentTurnOutcome,
    ContinuousAgentTurnContextV1, MemoryContextSnapshotV1, MemoryCorrectionV1, MemoryWriteIntent,
    MemoryWriteIntentPolicyV1, MemoryWriteIntentV1, MemoryWritePolicyContextV1,
    MemoryWritePolicyOutcome, MemoryWriteStore,
};
use oasis7_simulator::Observation;
use serde_json::json;

const AGENT_ID: &str = "agent-memory-1";
const SESSION_ID: &str = "session-memory-1";
const TURN_ID: &str = "turn-memory-1";
const REQUEST_DIGEST: &str =
    "blake3:1111111111111111111111111111111111111111111111111111111111111111";

fn context() -> MemoryWritePolicyContextV1 {
    serde_json::from_value(json!({
        "agent_id": AGENT_ID,
        "agent_session_id": SESSION_ID,
        "agent_turn_id": TURN_ID,
        "request_digest": REQUEST_DIGEST,
        "source": "provider",
        "provenance": "provider_unverified"
    }))
    .expect("decode memory policy context")
}

fn intent(scope: &str, summary: Option<&str>, tags: &[&str]) -> MemoryWriteIntentV1 {
    serde_json::from_value(json!({
        "schema_version": 1,
        "scope": scope,
        "summary": summary,
        "tags": tags
    }))
    .expect("decode MemoryWriteIntentV1")
}

#[test]
fn empty_memory_snapshot_is_explicit_and_digest_stable() {
    let first = MemoryContextSnapshotV1::empty("turn_private");
    let second = MemoryContextSnapshotV1::empty("turn_private");
    assert_eq!(first.revision, 0);
    assert!(first.entries.is_empty());
    assert_eq!(first.scope, "turn_private");
    assert!(!first.digest.is_empty());
    assert_eq!(
        first.digest,
        "blake3:e6c8320449505fd5dc2fe87f7081a41be6e2264645190f86fb716006bb93879d"
    );
    assert_eq!(first.digest, second.digest);
    assert_eq!(
        serde_json::to_value(&first).expect("encode empty memory snapshot")["digest"],
        serde_json::to_value(&second).expect("encode empty memory snapshot")["digest"]
    );

    let encoded = serde_json::to_value(&first).expect("encode explicit empty snapshot");
    assert!(encoded.get("entries").is_some());
    assert!(encoded.get("scope").is_some());
    assert!(encoded.get("digest").is_some());
    assert!(
        MemoryContextSnapshotV1::from_value(json!({
            "revision": 0,
            "entries": null,
            "scope": "turn_private",
            "digest": first.digest
        }))
        .is_err(),
        "null must not stand in for the canonical empty entries list"
    );
}

#[test]
fn memory_intent_normalizes_nfc_trim_and_only_dedupes_sorted_tags() {
    let policy = MemoryWriteIntentPolicyV1::default();
    let normalized = policy
        .normalize(
            intent("session_private", Some(" e\u{301} "), &[" z ", "a", "a"]),
            &context(),
        )
        .expect("valid private memory intent");
    assert_eq!(normalized.summary.as_deref(), Some("é"));
    assert_eq!(normalized.tags, ["a", "z"]);
    let digest = policy
        .intent_digest(&normalized, &context())
        .expect("memory intent digest");
    assert!(digest.as_str().starts_with("blake3:"));

    let reordered = policy
        .normalize(
            intent("session_private", Some("é"), &["a", "z"]),
            &context(),
        )
        .expect("equivalent normalized intent");
    assert_eq!(
        policy
            .intent_digest(&reordered, &context())
            .expect("reordered digest"),
        digest
    );
}

#[test]
fn oasis_memory_policy_matches_harness_golden_contract() {
    let oasis_policy = MemoryWriteIntentPolicyV1::default();
    let harness_policy = oasis7_agent_harness::MemoryWriteIntentPolicyV1::default();
    let oasis_context = context();
    let harness_context: oasis7_agent_harness::MemoryWritePolicyContextV1 =
        serde_json::from_value(serde_json::to_value(&oasis_context).unwrap()).unwrap();
    let input = intent("session_private", Some(" e\u{301} "), &[" z ", "a", "a"]);
    let harness_input: oasis7_agent_api::MemoryWriteIntentV1 =
        serde_json::from_value(serde_json::to_value(&input).unwrap()).unwrap();

    let oasis_normalized = oasis_policy
        .normalize(input, &oasis_context)
        .expect("Oasis adapter normalizes the shared policy input");
    let harness_normalized = harness_policy
        .normalize(harness_input, &harness_context)
        .expect("Harness canonical policy normalizes the same input");
    assert_eq!(
        serde_json::to_value(&oasis_normalized).unwrap(),
        serde_json::to_value(&harness_normalized).unwrap()
    );
    let oasis_digest = oasis_policy
        .intent_digest(&oasis_normalized, &oasis_context)
        .expect("Oasis adapter digest");
    let harness_digest = harness_policy
        .intent_digest(&harness_normalized, &harness_context)
        .expect("Harness canonical digest");
    assert_eq!(
        oasis_digest.as_str(),
        "blake3:9b9025a8c606a75a074e5e1b1c747d4d42cd9ed037f9d30e708a2b93f4713690"
    );
    assert_eq!(oasis_digest, harness_digest);
}

#[test]
fn oasis_goal_snapshot_adapter_matches_harness_projection_golden() {
    let oasis_input = crate::simulator::GoalSnapshotInputV1 {
        revision: 7,
        short_term_summary: " short-term ".into(),
        long_term_summary: " e\u{301}-term ".into(),
        blocked_reason: Some("   ".into()),
        provenance: "harness_projection".into(),
    };
    let harness_input: oasis7_agent_harness::GoalSnapshotInputV1 =
        serde_json::from_value(serde_json::to_value(&oasis_input).unwrap()).unwrap();
    let oasis = crate::simulator::GoalSnapshotProjector::project(Some(oasis_input), None).unwrap();
    let harness =
        oasis7_agent_harness::GoalSnapshotProjector::project(Some(harness_input), None).unwrap();
    assert_eq!(
        serde_json::to_value(&oasis).unwrap(),
        serde_json::to_value(&harness).unwrap()
    );
    assert_eq!(oasis.short_term_summary, "short-term");
    assert_eq!(oasis.long_term_summary, "é-term");
    assert_eq!(oasis.blocked_reason, None);
}

#[test]
fn memory_store_reads_legacy_checkpoint_defaults_without_losing_correction_lineage() {
    let legacy = serde_json::json!({
        "entries": [{
            "schema_version": 1,
            "scope": "session_private",
            "summary": "kept private",
            "tags": [],
            "intent_digest": "blake3:entry",
            "receipt_id": "receipt:entry",
            "provenance": "runtime_authoritative",
            "agent_id": AGENT_ID,
            "agent_session_id": SESSION_ID
        }],
        "committed_by_digest": {"blake3:entry": "receipt:entry"},
        "corrections": [{
            "correction_id": "correction:legacy",
            "agent_id": AGENT_ID,
            "agent_session_id": SESSION_ID,
            "scope": "session_private",
            "target_memory_id": "blake3:entry",
            "expected_revision": 0,
            "replacement_summary": "corrected private fact",
            "status": "accepted",
            "reason": "next_retrieval_pending",
            "memory_revision": 1,
            "earliest_decision_request_id": "request:legacy",
            "earliest_request_digest": REQUEST_DIGEST,
            "runtime_receipt_id": null,
            "action_id": null
        }]
    });

    let store: MemoryWriteStore = serde_json::from_value(legacy).unwrap();
    let encoded = serde_json::to_value(&store).unwrap();
    assert_eq!(encoded["revision"], 0, "missing legacy revision defaults");
    assert_eq!(encoded["entries"][0]["receipt_id"], "receipt:entry");
    assert_eq!(
        encoded["committed_by_digest"]["blake3:entry"],
        "receipt:entry"
    );
    assert_eq!(
        encoded["corrections"][0]["active_decision_request_id"],
        serde_json::Value::Null
    );
    assert_eq!(
        encoded["corrections"][0]["active_request_digest"],
        serde_json::Value::Null
    );
    assert_eq!(
        encoded["corrections"][0]["committed_decision_request_id"],
        serde_json::Value::Null
    );
    assert_eq!(
        encoded["corrections"][0]["committed_request_digest"],
        serde_json::Value::Null
    );
    assert_eq!(
        encoded["referenced_context_by_agent"],
        serde_json::json!({})
    );
}

#[test]
fn oasis_memory_correction_and_snapshot_adapters_match_harness_state_contract() {
    let checkpoint = serde_json::json!({
        "entries": [{
            "schema_version": 1,
            "scope": "session_private",
            "summary": "old private fact",
            "tags": ["verified"],
            "intent_digest": "blake3:memory-target",
            "receipt_id": "receipt:memory-target",
            "provenance": "runtime_authoritative",
            "agent_id": AGENT_ID,
            "agent_session_id": SESSION_ID
        }],
        "committed_by_digest": {"blake3:memory-target": "receipt:memory-target"},
        "corrections": [],
        "referenced_context_by_agent": {}
    });
    let mut oasis: MemoryWriteStore = serde_json::from_value(checkpoint.clone()).unwrap();
    let mut harness: oasis7_agent_harness::MemoryWriteStore =
        serde_json::from_value(checkpoint).unwrap();
    let correction = serde_json::json!({
        "correction_id": "correction:adapter-parity",
        "agent_id": AGENT_ID,
        "agent_session_id": SESSION_ID,
        "scope": "session_private",
        "target_memory_id": "blake3:memory-target",
        "expected_revision": 0,
        "replacement_summary": " corrected private fact ",
        "status": "pending",
        "reason": "provider_proposal",
        "memory_revision": 0,
        "earliest_decision_request_id": null,
        "earliest_request_digest": null,
        "active_decision_request_id": null,
        "active_request_digest": null,
        "committed_decision_request_id": null,
        "committed_request_digest": null,
        "runtime_receipt_id": null,
        "action_id": null
    });
    let oasis_correction: MemoryCorrectionV1 = serde_json::from_value(correction.clone()).unwrap();
    let harness_correction: oasis7_agent_harness::MemoryCorrectionV1 =
        serde_json::from_value(correction).unwrap();

    let oasis_result = oasis.correct_memory(oasis_correction).unwrap();
    let harness_result = harness.correct_memory(harness_correction).unwrap();
    assert_eq!(
        serde_json::to_value(oasis_result).unwrap(),
        serde_json::to_value(harness_result).unwrap()
    );
    assert_eq!(
        serde_json::to_value(oasis.context_snapshot(AGENT_ID, SESSION_ID, "session_private", 8))
            .unwrap(),
        serde_json::to_value(harness.context_snapshot(AGENT_ID, SESSION_ID, "session_private", 8))
            .unwrap()
    );
    assert_eq!(
        serde_json::to_value(oasis).unwrap(),
        serde_json::to_value(harness).unwrap()
    );
}

#[test]
fn memory_policy_rejects_scope_aliases_in_target_lane_and_all_bounds_fail_closed() {
    let policy = MemoryWriteIntentPolicyV1::default();
    assert_eq!(
        policy
            .normalize(intent("short_term", Some("legacy"), &[]), &context())
            .expect_err("target lane must not guess legacy scope")
            .code(),
        "memory_scope_denied"
    );
    let legacy = policy
        .normalize_legacy(intent("short_term", Some(" legacy "), &[]), &context())
        .expect("explicit legacy_v1 scope alias");
    let legacy = serde_json::to_value(legacy).expect("encode legacy projection");
    assert_eq!(legacy["scope"], "turn_private");
    assert_eq!(legacy["compatibility_reason"], "memory_scope_alias_used");

    assert_eq!(
        policy
            .normalize(intent("turn_private", Some("   "), &[]), &context())
            .expect_err("empty summary")
            .code(),
        "memory_summary_invalid"
    );
    assert_eq!(
        policy
            .normalize(
                intent("turn_private", Some(&"x".repeat(513)), &[]),
                &context()
            )
            .expect_err("summary bound")
            .code(),
        "memory_summary_too_large"
    );
    assert_eq!(
        policy
            .normalize(
                intent(
                    "turn_private",
                    Some("ok"),
                    &["1", "2", "3", "4", "5", "6", "7", "8", "9"]
                ),
                &context(),
            )
            .expect_err("tag count bound")
            .code(),
        "memory_tag_count_exceeded"
    );
    assert_eq!(
        policy
            .normalize(intent("turn_private", Some("ok"), &["\0"]), &context())
            .expect_err("control tag")
            .code(),
        "memory_tag_invalid"
    );
}

#[test]
fn memory_write_requires_runtime_authority_and_committed_receipt_for_every_scope() {
    let policy = MemoryWriteIntentPolicyV1::default();
    let normalized = policy
        .normalize(intent("turn_private", Some("fact"), &[]), &context())
        .expect("valid intent");
    let digest = policy
        .intent_digest(&normalized, &context())
        .expect("intent digest");
    let mut store = MemoryWriteStore::default();

    for outcome in [
        MemoryWritePolicyOutcome::Rejected,
        MemoryWritePolicyOutcome::Failed,
        MemoryWritePolicyOutcome::Pending,
    ] {
        let result = store
            .apply(normalized.clone(), digest.clone(), outcome)
            .expect_err("noncommitted outcome cannot write authoritative memory");
        assert_eq!(result.code(), "memory_no_committed_outcome");
        assert!(store.entries().is_empty());
    }

    let committed = MemoryWritePolicyOutcome::Committed {
        receipt_id: "receipt-memory-1".to_string(),
        provenance: "runtime_authoritative".to_string(),
    };
    store
        .apply(normalized.clone(), digest.clone(), committed.clone())
        .expect("committed receipt gates one memory write");
    assert_eq!(store.entries().len(), 1);
    store
        .apply(normalized, digest, committed)
        .expect("same intent digest is idempotent");
    assert_eq!(store.entries().len(), 1);
    assert_eq!(store.entries()[0]["provenance"], "runtime_authoritative");
    assert_eq!(store.entries()[0]["receipt_id"], "receipt-memory-1");
}

#[test]
fn memory_digest_context_is_tied_to_agent_turn_and_request_not_provider_provenance() {
    let policy = MemoryWriteIntentPolicyV1::default();
    let normalized = policy
        .normalize(intent("turn_private", Some("fact"), &[]), &context())
        .expect("valid intent");
    let first = policy
        .intent_digest(&normalized, &context())
        .expect("first digest");
    let mut other = context();
    other.agent_id = "agent-memory-2".to_string();
    other.provenance = "runtime_fact".to_string();
    let second = policy
        .intent_digest(&normalized, &other)
        .expect("second digest");
    assert_ne!(first, second);
    assert_ne!(other.provenance, "runtime_authoritative");
}

struct MemoryCandidateBehavior;

impl AgentBehavior for MemoryCandidateBehavior {
    fn agent_id(&self) -> &str {
        AGENT_ID
    }

    fn decide(&mut self, _observation: &Observation) -> AgentDecision {
        AgentDecision::Wait
    }

    fn take_memory_write_intents(&mut self) -> Vec<MemoryWriteIntent> {
        vec![MemoryWriteIntent {
            scope: "turn_private".to_string(),
            summary: "candidate awaiting a Runtime action receipt".to_string(),
            tags: vec!["candidate".to_string()],
        }]
    }
}

fn turn_context() -> ContinuousAgentTurnContextV1 {
    ContinuousAgentTurnContextV1 {
        agent_id: AGENT_ID.to_string(),
        agent_session_id: SESSION_ID.to_string(),
        agent_turn_id: TURN_ID.to_string(),
        decision_request_id: "request-memory-1".to_string(),
        request_digest: serde_json::from_value(json!(REQUEST_DIGEST))
            .expect("decode request digest"),
        memory_snapshot: MemoryContextSnapshotV1::empty("turn_private"),
        goal_snapshot: crate::simulator::GoalSnapshotV1::empty(),
        continuation: None,
    }
}

fn completed_turn(runner: &mut AsyncAgentRunner) -> AsyncAgentTurnOutcome {
    let timeout = std::time::Duration::from_secs(2);
    let deadline = std::time::Instant::now() + timeout;
    loop {
        if let Some(outcome) = runner
            .poll_completed()
            .expect("poll memory candidate actor")
            .into_iter()
            .next()
        {
            return outcome;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "memory candidate actor did not complete before {timeout:?} deadline"
        );
        std::thread::sleep(std::time::Duration::from_millis(1));
    }
}

struct GatedMemoryCandidateBehavior {
    started: std::sync::mpsc::Sender<()>,
    release: std::sync::mpsc::Receiver<()>,
}

impl AgentBehavior for GatedMemoryCandidateBehavior {
    fn agent_id(&self) -> &str {
        AGENT_ID
    }

    fn decide(&mut self, _observation: &Observation) -> AgentDecision {
        self.started.send(()).expect("announce gated actor start");
        self.release.recv().expect("release gated actor");
        AgentDecision::Wait
    }

    fn take_memory_write_intents(&mut self) -> Vec<MemoryWriteIntent> {
        MemoryCandidateBehavior.take_memory_write_intents()
    }
}

#[test]
fn memory_candidate_completion_waits_for_pending_worker() {
    let (started_tx, started_rx) = std::sync::mpsc::channel();
    let (release_tx, release_rx) = std::sync::mpsc::channel();
    let mut runner = AsyncAgentRunner::new(1).expect("create gated memory runner");
    runner
        .register(GatedMemoryCandidateBehavior {
            started: started_tx,
            release: release_rx,
        })
        .expect("register gated memory actor");
    runner
        .start_turn_with_context(AGENT_ID, turn_context())
        .expect("open gated memory turn");
    started_rx
        .recv_timeout(std::time::Duration::from_secs(2))
        .expect("worker must start before completion polling");
    // Keep a real worker pending across repeated empty mailbox polls.
    let release = std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_millis(50));
        release_tx.send(()).expect("release pending memory worker");
    });
    let outcome = completed_turn(&mut runner);
    release.join().expect("join memory worker release");
    assert_eq!(outcome.agent_id, AGENT_ID);
    assert_eq!(outcome.memory_write_intents.len(), 1);
}

#[test]
fn memory_write_rejects_caller_signed_receipt_without_runtime_action_feedback_lineage() {
    let mut runner = AsyncAgentRunner::new(1).expect("create memory candidate runner");
    runner
        .register(MemoryCandidateBehavior)
        .expect("register memory candidate actor");
    runner
        .start_turn_with_context(AGENT_ID, turn_context())
        .expect("open memory candidate turn");
    let outcome = completed_turn(&mut runner);
    assert_eq!(outcome.memory_write_intents.len(), 1);

    // This envelope has the right local request/turn identity, but the
    // receipt and candidate action are caller-selected rather than issued and
    // linked by Runtime.  The write gate must reject it before persistence.
    let mut forged = outcome
        .feedback_for_runtime_status("committed", Some("caller-forged-receipt"))
        .expect("construct the forged feedback envelope");
    forged.candidate_action_id = Some(7_777);

    let mut store = MemoryWriteStore::default();
    runner
        .consume_runtime_feedback(AGENT_ID, forged, &mut store)
        .expect_err("caller-signed receipt without Runtime lineage must be rejected");
    assert!(store.entries().is_empty());
}
