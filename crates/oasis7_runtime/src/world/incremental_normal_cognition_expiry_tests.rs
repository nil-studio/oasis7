//! V1 compatibility checks, not an end-to-end Current/accepted-source claim.
use super::*;
use crate::{
    cognition_scheduler::{CognitionScheduler, SchedulerPolicyV1, SchedulerWakeV1},
    cognition_wake::{AgentContinuation, ContinuationStatusV1, ContinuationTransition},
};
use oasis7_formal_protocol::controlled_authority::formal::bounded_context::FormalBoundedWorkV1;
use oasis7_proto::memory_budget::MemoryBudget;
use serde_json::json;
fn canonical() -> oasis7_wasm_abi::CanonicalCborWorkV1 {
    oasis7_wasm_abi::CanonicalCborWorkV1 {
        visits_remaining: 10_000_000,
        bytes_remaining: 64 << 20,
        max_depth: 64,
        max_map_keys: 4096,
        max_key_bytes: 4096,
    }
}
fn json_work() -> JsonReadWorkV1 {
    JsonReadWorkV1 {
        cells_remaining: 1_000_000,
        json_nodes_remaining: 1_000_000,
        max_depth: 64,
        max_payload_bytes: 16 << 20,
    }
}
fn work() -> CurrentCognitionExpiryWorkV1 {
    CurrentCognitionExpiryWorkV1 {
        read: json_work(),
        write: JsonWriteWorkV1 {
            cells_remaining: 1_000_000,
            json_nodes_remaining: 1_000_000,
            max_json_depth: 64,
            max_pending_node_bytes: 16 << 20,
        },
        index: CognitionHistoryIndexWorkV1 {
            cells_remaining: 1_000_000,
            max_pending_node_bytes: 16 << 20,
        },
        cbor: CognitionHistoryCborWorkV1 {
            bytes_remaining: 64 << 20,
            nodes_remaining: 1_000_000,
            source_reads_remaining: 1_000_000,
            key_bytes_remaining: 16 << 20,
            max_depth: 64,
            max_key_bytes: 4096,
        },
        hash: CognitionHeadHashWorkV1 {
            input_bytes_remaining: 64 << 20,
            hash_nodes_remaining: 1_000_000,
        },
        original: canonical(),
        hashes: canonical(),
        ledger: ProofLedgerReadWorkV1 {
            json: json_work(),
            max_retained_trees: 64,
            max_tree_heap: 16 << 20,
        },
        ledger_scan: StateScanBudget {
            nodes_remaining: 1_000_000,
            leaves_remaining: 1_000_000,
            read_bytes_remaining: 64 << 20,
        },
        max_selected_heap: 16 << 20,
    }
}
fn policy() -> SchedulerPolicyV1 {
    SchedulerPolicyV1 {
        schema_version: SchedulerPolicyV1::SCHEMA_VERSION.into(),
        max_total_wakes_per_tick: 4,
        max_wakes_per_agent_per_tick: 2,
        aging_after_ticks: 1,
        max_starvation_ticks: 10,
        initial_priority: 0,
        comparator: SchedulerPolicyV1::COMPARATOR.into(),
        service_order: SchedulerPolicyV1::SERVICE_ORDER.into(),
    }
}
fn wake() -> SchedulerWakeV1 {
    serde_json::from_value(json!({"schema_version":"scheduler-wake.v1","wake_id":"wake","continuation_id":"continuation","world_id":"world","branch_id":"main","finality_epoch":0,"finality_block_hash":null,"finality_status":"pending","reorg_epoch":0,"runtime_manifest_hash":"manifest","agent_id":"agent","agent_session_id":"session","agent_turn_id":"turn","decision_request_id":"request","request_digest":"digest","next_wake_tick":0,"eligible_since_tick":0,"starvation_deadline_tick":10,"initial_priority":0,"wake_seq":1,"retry_seq":0,"status":"pending","pending_reason":"capacity_available"})).unwrap()
}
fn continuation() -> AgentContinuation {
    serde_json::from_value(json!({"schema_version":"agent-continuation.v1","continuation_id":"continuation","wake_id":"wake","world_id":"world","branch_id":"main","finality_epoch":0,"finality_block_hash":null,"finality_status":"pending","reorg_epoch":0,"runtime_manifest_hash":"manifest","agent_id":"agent","agent_session_id":"session","agent_turn_id":"turn","decision_request_id":"request","origin_turn_id":"turn","origin_request_digest":"digest","continuation_proposal_id":"proposal","proposal_digest":"proposal-digest","wake_conditions":[{"schema_version":"wake-condition.v1","kind":"at_or_after_tick","logical_tick":0}],"remaining_budget":{"unit":"steps","value":2},"valid_until_tick":5,"precondition_digest":"precondition","wake_seq":1,"logical_tick":0,"status":"scheduled","terminal_disposition":null})).unwrap()
}
#[test]
fn fixed_expiry_transition_and_journal_event_match_original_v1() {
    let memory = MemoryBudget::new(64 << 20, 0).unwrap();
    let _fixture = memory.reserve(1 << 20, MemoryClass::Ordinary).unwrap();
    let meter = FormalBoundedWorkV1::new(100_000_000, 512 << 20).unwrap();
    let mut stop = || false;
    let mut common = FormalBoundedContextV1::new(&memory, &meter, &mut stop);
    let mut scheduler = CognitionScheduler::try_new(policy(), 4).unwrap();
    scheduler.enqueue_for_test(wake());
    let snapshot = scheduler.snapshot_json();
    let mut paid = match owned::decode_scheduler_v1(&snapshot, 64, &mut common) {
        Ok(v) => v,
        Err(_) => panic!("valid original Scheduler fixture"),
    };
    let mut actual = continuation();
    let mut expected = actual.clone();
    ContinuationTransition::apply_at_tick(&mut expected, ContinuationStatusV1::Expired, 6).unwrap();
    let moved = match owned::expire_selected_continuation_v1(
        &mut actual,
        &mut paid,
        6,
        &mut canonical(),
        &mut common,
    ) {
        Ok(Some(v)) => v,
        _ => panic!("actual overdue scheduled continuation"),
    };
    assert_eq!(actual, expected);
    assert!(moved.is_some());
    let mut projection = json!({"runtime_binding":{"world_id":"actual-world","branch_id":"actual-branch","finality_epoch":7,"finality_block_hash":{"preserved":[null,7,"unknown"]},"finality_status":"verified","reorg_epoch":3},"cognition_journal":{"schema_version":"cognition-journal.v1","head_seq":0,"head_digest":"","events":[]}});
    for wake in [None, moved.as_ref()] {
        let details = json!({"continuation_id":actual.continuation_id,"wake_id":actual.wake_id,"world_id":actual.world_id,"agent_id":actual.agent_id,"agent_session_id":actual.agent_session_id,"agent_turn_id":actual.agent_turn_id,"decision_request_id":actual.decision_request_id,"status":actual.status,"continuation_status_digest":actual.continuation_status_digest,"scheduler_policy_digest":paid.value.policy_config_digest(),"cursor_seq":paid.value.cursor_seq()});
        let mut details = details;
        if let Some(wake) = wake {
            details["scheduler_action"] = "deactivated".into();
            details["wake"] = serde_json::to_value(wake).unwrap();
        }
        let seq = projection["cognition_journal"]["head_seq"]
            .as_u64()
            .unwrap();
        let previous = projection["cognition_journal"]["events"]
            .as_array()
            .unwrap()
            .last()
            .map(|v| {
                let text = v["event_digest"]
                    .as_str()
                    .unwrap()
                    .strip_prefix("blake3:")
                    .unwrap();
                *blake3::Hash::from_hex(text).unwrap().as_bytes()
            });
        let produced = event::prepare(
            &actual,
            &paid.value,
            wake,
            projection.get("runtime_binding"),
            seq,
            previous,
            &mut work(),
            &mut common,
        )
        .unwrap();
        crate::world::cognition_persistence_validation::append_cognition_event(
            &mut projection,
            "ContinuationExpired",
            details,
        )
        .unwrap();
        assert_eq!(
            &produced.value.value,
            projection["cognition_journal"]["events"]
                .as_array()
                .unwrap()
                .last()
                .unwrap()
        );
    }
}

#[test]
fn fixed_scheduler_journal_details_match_original_v1() {
    let memory = MemoryBudget::new(64 << 20, 0).unwrap();
    let meter = FormalBoundedWorkV1::new(100_000_000, 512 << 20).unwrap();
    let mut stop = || false;
    let mut common = FormalBoundedContextV1::new(&memory, &meter, &mut stop);
    let scheduler = CognitionScheduler::try_new(policy(), 4).unwrap();
    let wake = wake();
    let mut projection = json!({"runtime_binding":{"world_id":"actual-world","branch_id":"actual-branch","finality_epoch":7,"finality_block_hash":{"preserved":[null,7,"unknown"]},"finality_status":"verified","reorg_epoch":3},"cognition_journal":{"schema_version":"cognition-journal.v1","head_seq":0,"head_digest":"","events":[]}});
    for (kind, wake) in [
        ("SchedulerWakeDeactivated", Some(&wake)),
        ("SchedulerWakeSelected", None),
        ("SchedulerWakeSelected", Some(&wake)),
        ("ContinuationWoken", Some(&wake)),
    ] {
        // The original scheduler transaction's exact details construction.
        let mut details = json!({"scheduler_policy_digest":scheduler.policy_config_digest(),"cursor_seq":scheduler.cursor_seq()});
        if let Some(wake) = wake {
            details.as_object_mut().unwrap().extend(
                serde_json::to_value(wake)
                    .unwrap()
                    .as_object()
                    .unwrap()
                    .clone(),
            );
        }
        let seq = projection["cognition_journal"]["head_seq"]
            .as_u64()
            .unwrap();
        let previous = projection["cognition_journal"]["events"]
            .as_array()
            .unwrap()
            .last()
            .map(|v| {
                *blake3::Hash::from_hex(
                    v["event_digest"]
                        .as_str()
                        .unwrap()
                        .strip_prefix("blake3:")
                        .unwrap(),
                )
                .unwrap()
                .as_bytes()
            });
        let produced = event::prepare_scheduler_v2(
            kind,
            &scheduler,
            wake,
            projection.get("runtime_binding"),
            seq,
            previous,
            &mut work(),
            &mut common,
        )
        .unwrap();
        crate::world::cognition_persistence_validation::append_cognition_event(
            &mut projection,
            kind,
            details,
        )
        .unwrap();
        assert_eq!(
            &produced.value.value,
            projection["cognition_journal"]["events"]
                .as_array()
                .unwrap()
                .last()
                .unwrap()
        );
    }
}
