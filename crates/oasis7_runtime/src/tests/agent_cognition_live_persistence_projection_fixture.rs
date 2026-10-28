// Test data only. CodeQL excludes this exact file; keep assertions and business logic outside.
use super::*;

pub(super) fn cognition_projection(prefix: &str) -> Value {
    let committed = matches!(prefix, "committed" | "committed_missing_projection");
    let conflict = prefix == "conflict";
    let disposition = if committed {
        "committed"
    } else {
        "recovery_pending"
    };
    let head_status = if committed {
        "canonical"
    } else {
        "recovery_pending"
    };
    let state_root = if committed { R_NEXT } else { R_PARENT };
    let receipt = if committed {
        json!({"receipt_id": RECEIPT_ID, "receipt_digest": RECEIPT_DIGEST})
    } else {
        Value::Null
    };
    let receipt_projection_present = committed && prefix != "committed_missing_projection";
    let idempotency_projection_present = committed && prefix != "committed_missing_projection";
    let effect_count = u64::from(committed && prefix == "committed");
    let debit_count = effect_count;
    let receipt_count = u64::from(committed);
    let world_receipt_linked_count = u64::from(committed && prefix == "committed");

    json!({
        "schema_version": COGNITION_SCHEMA,
        "cognition_journal": {
            "schema_version": JOURNAL_SCHEMA,
            "head_seq": 8,
            "head_digest": "blake3:cognition-journal-head-live-1",
            "events": [
                {"journal_seq": 1, "kind": "TurnStarted", "agent_session_id": "session-live-1", "agent_turn_id": "turn-live-1", "decision_request_id": "request-live-1", "envelope_idempotency_key": ENVELOPE_KEY},
                {"journal_seq": 2, "kind": "ContextCaptured", "agent_session_id": "session-live-1", "agent_turn_id": "turn-live-1", "decision_request_id": "request-live-1", "envelope_idempotency_key": ENVELOPE_KEY},
                {"journal_seq": 3, "kind": "RequestDispatched", "agent_session_id": "session-live-1", "agent_turn_id": "turn-live-1", "decision_request_id": "request-live-1", "envelope_idempotency_key": ENVELOPE_KEY},
                {"journal_seq": 4, "kind": "ResponseRecorded", "agent_session_id": "session-live-1", "agent_turn_id": "turn-live-1", "decision_request_id": "request-live-1", "envelope_idempotency_key": ENVELOPE_KEY},
                {"journal_seq": 5, "kind": "DecisionEnvelopeSubmitted", "agent_session_id": "session-live-1", "agent_turn_id": "turn-live-1", "decision_request_id": "request-live-1", "envelope_idempotency_key": ENVELOPE_KEY},
                {"journal_seq": 6, "kind": "DecisionValidated", "agent_session_id": "session-live-1", "agent_turn_id": "turn-live-1", "decision_request_id": "request-live-1", "envelope_idempotency_key": ENVELOPE_KEY},
                {"journal_seq": 7, "kind": "WorldReceiptLinked", "agent_session_id": "session-live-1", "agent_turn_id": "turn-live-1", "decision_request_id": "request-live-1", "envelope_idempotency_key": ENVELOPE_KEY},
                {"journal_seq": 8, "kind": "CognitionTurnCompleted", "status": disposition, "agent_session_id": "session-live-1", "agent_turn_id": "turn-live-1", "decision_request_id": "request-live-1", "envelope_idempotency_key": ENVELOPE_KEY}
            ]
        },
        "responses": [{
            "response_digest": "blake3:response-live-1",
            "response_artifact": {"decision": {"type": "move_agent", "agent_id": "agent-live-persistence"}},
            "envelope_digest": ENVELOPE_DIGEST,
            "journal_head": "blake3:cognition-journal-head-live-1"
        }],
        "commit_records": [{
            "schema_version": "world-commit-record.v1",
            "commit_id": "commit-live-1",
            "envelope_idempotency_key": ENVELOPE_KEY,
            "envelope_digest": ENVELOPE_DIGEST,
            "world_id": WORLD_ID,
            "branch_id": BRANCH_ID,
            "finality_epoch": 7,
            "finality_block_hash": "blake3:1111111111111111111111111111111111111111111111111111111111111111",
            "finality_status": "verified",
            "finality_binding_digest": "blake3:finality-binding-live-1",
            "runtime_manifest_hash": "blake3:runtime-manifest-live-1",
            "action_id": "action-live-1",
            "parent_tick": 1,
            "parent_world_hash": R_PARENT,
            "staged_event_root": "blake3:events-next-live-1",
            "staged_state_root": R_NEXT,
            "receipt_id": RECEIPT_ID,
            "receipt_digest": RECEIPT_DIGEST,
            "reorg_epoch": 3,
            "cognition_journal_seq": 6,
            "status": if committed {"committed"} else {"prepared"},
            "abort_reason": Value::Null
        }],
        "receipt_registry": if receipt_projection_present {json!([receipt])} else {json!([])},
        "idempotency_index": if idempotency_projection_present {
            json!({ENVELOPE_KEY: {"envelope_digest": ENVELOPE_DIGEST, "disposition": "committed", "receipt_id": RECEIPT_ID}})
        } else {
            json!({})
        },
        "scheduler_state": canonical_scheduler_state(),
        "continuations": [{
            "schema_version": "agent-continuation.v1",
            "continuation_id": "continuation-live-1",
            "wake_id": "wake-live-1",
            "world_id": WORLD_ID,
            "branch_id": BRANCH_ID,
            "finality_epoch": 7,
            "finality_block_hash": "blake3:1111111111111111111111111111111111111111111111111111111111111111",
            "finality_status": "verified",
            "reorg_epoch": 3,
            "runtime_manifest_hash": "blake3:runtime-manifest-live-1",
            "agent_id": "agent-live-persistence",
            "agent_session_id": "session-live-1",
            "agent_turn_id": "turn-live-1",
            "decision_request_id": "request-live-1",
            "origin_turn_id": "turn-live-1",
            "origin_request_digest": "blake3:request-live-1",
            "continuation_proposal_id": "proposal-live-1",
            "proposal_digest": "blake3:proposal-live-1",
            "remaining_budget": {"unit": "steps", "value": 3},
            "precondition_digest": "blake3:precondition-live-1",
            "wake_conditions": [{"schema_version": "wake-condition.v1", "kind": "receipt_linked", "receipt_id": RECEIPT_ID}],
            "wake_seq": 1,
            "status": "scheduled",
            "continuation_digest": "blake3:continuation-live-1"
        }],
        "recovery": {
            "crash_prefix": prefix,
            "disposition": disposition,
            "world_root": {
                "schema_version": "world-root-view.v1",
                "world_id": WORLD_ID,
                "branch_id": BRANCH_ID,
                "logical_tick": if committed {2} else {1},
                "state_root": state_root,
                "head_status": head_status,
                "commit_id": if committed {json!("commit-live-1")} else {Value::Null},
                "quarantine_id": if conflict {json!("quarantine-live-1")} else {Value::Null}
            },
            "candidate_root": if conflict {json!(R_NEXT)} else {Value::Null},
            "candidate_receipt": if conflict {json!(receipt)} else {Value::Null},
            "receipt": receipt,
            "provider_invocation_count": 0,
            "kernel_invocation_count": 0,
            "effect_count": effect_count,
            "debit_count": debit_count,
            "receipt_count": receipt_count,
            "world_receipt_linked_count": world_receipt_linked_count,
            "response_replayed": committed
        }
    })
}
