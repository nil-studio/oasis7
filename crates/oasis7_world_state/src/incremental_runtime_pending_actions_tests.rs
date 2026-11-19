use super::*;
use crate::{
    Action,
    incremental_semantic_key::SemanticKeyBudget,
    incremental_typed_record_test_support::{Pages, io},
};
use oasis7_proto::memory_budget::MemoryBudget;
use std::cell::Cell;
fn work() -> crate::incremental_runtime_pending_action_record::PendingActionWriteWorkV1 {
    crate::incremental_runtime_pending_action_record::PendingActionWriteWorkV1 {
        cells_remaining: 100_000,
        max_pending_node_bytes: 8 << 20,
        json: crate::incremental_json_records::JsonWriteWorkV1 {
            cells_remaining: 100_000,
            json_nodes_remaining: 100_000,
            max_json_depth: 128,
            max_pending_node_bytes: 8 << 20,
        },
    }
}
fn source() -> VecDeque<ActionEnvelope> {
    (0..2)
        .map(|n| ActionEnvelope {
            id: 77,
            action: Action::QueryObservation {
                agent_id: format!("original-{n}"),
            },
            governance_authorization: None,
            committed_recipe_origin: None,
        })
        .collect()
}
fn limits() -> MemberRecordLimitsV1 {
    MemberRecordLimitsV1 {
        semantic_bytes: 4096,
        payload_bytes: 4096,
        record_bytes: 8192,
    }
}
#[test]
fn pending_action_genesis_keeps_original_envelopes_and_queue_positions() {
    let memory = MemoryBudget::new(64 << 20, 1 << 20).unwrap();
    let mut pages = Pages::with_limit(&memory, 16384);
    let source = source();
    let mut rows = Vec::new();
    let mut seen = 0;
    let mut remaining = 3;
    prepare_runtime_pending_actions_genesis_v1(
        &source,
        &mut MemberRecordWriteContext {
            sink: &mut pages,
            memory: &memory,
            io: &mut io(),
            key_work: &mut SemanticKeyBudget {
                bytes_remaining: 1 << 20,
            },
            limits: limits(),
            cancelled: || false,
        },
        &mut remaining,
        &mut work(),
        &mut |ordinal, envelope| {
            assert!(std::ptr::eq(envelope, &source[ordinal as usize]));
            assert_eq!(envelope.id, 77);
            seen += 1;
            Ok(OriginalArtifactRefV1 {
                id: [ordinal as u8; 32],
                byte_len: 100,
            })
        },
        &mut |row| {
            assert!(row.uses_budget(&memory));
            rows.push(row);
            Ok(())
        },
    )
    .unwrap();
    assert_eq!(seen, 2);
    assert_eq!(remaining, 0);
    assert_eq!(rows.len(), 3);
    let a = WorldFieldKeyV1::from_bytes(&rows[1].mutation.key).unwrap();
    let b = WorldFieldKeyV1::from_bytes(&rows[2].mutation.key).unwrap();
    assert_eq!(a.name, "pending_actions");
    assert_eq!(a.kind, WorldFieldRecordKindV1::Entry);
    assert_ne!(a.semantic_key, b.semantic_key);
    assert_ne!(rows[1].mutation.after, rows[2].mutation.after);
    for (ordinal, row) in rows.iter().skip(1).enumerate() {
        let parsed = WorldFieldKeyV1::from_bytes(&row.mutation.key).unwrap();
        let key = WorldFieldKeyV1::new(PENDING_ACTIONS_FIELD_V1, parsed.kind, parsed.semantic_key)
            .unwrap();
        let mut quota = io();
        let mut values = oasis7_formal_protocol::incremental_value::ValueReadContext {
            source: &pages,
            memory: &memory,
            class: oasis7_proto::memory_budget::MemoryClass::Ordinary,
            io: &mut quota,
        };
        let mut payload = Vec::new();
        let mut semantic = Vec::new();
        let info = read_member_record_v1(
            &row.mutation.after.unwrap(),
            &key,
            &mut values,
            limits(),
            &mut semantic,
            &mut payload,
            || false,
        )
        .unwrap();
        assert_eq!(info.codec, MemberPayloadCodecV1::RecordRoot);
        let mut expected = Vec::new();
        crate::incremental_semantic_key::write_semantic_key_v1(
            &(0u64, ordinal as u64),
            &mut expected,
        )
        .unwrap();
        assert_eq!(semantic, expected);
        let reference =
            oasis7_formal_protocol::incremental_record_root::RecordRootRefV1::from_canonical_bytes(
                &payload,
            )
            .unwrap();
        reference
            .validate_binding(
                &crate::incremental_pending_action_codec::schema(),
                &parsed.semantic_key.unwrap(),
                StateNamespaceV1::Scheduler,
            )
            .unwrap();
    }
}
#[test]
fn pending_action_genesis_stops_before_later_original_after_sink_failure() {
    let memory = MemoryBudget::new(64 << 20, 1 << 20).unwrap();
    let mut pages = Pages::with_limit(&memory, 16384);
    let mut persisted = 0;
    let mut emitted = 0;
    let mut remaining = 3;
    let result = prepare_runtime_pending_actions_genesis_v1(
        &source(),
        &mut MemberRecordWriteContext {
            sink: &mut pages,
            memory: &memory,
            io: &mut io(),
            key_work: &mut SemanticKeyBudget {
                bytes_remaining: 1 << 20,
            },
            limits: limits(),
            cancelled: || false,
        },
        &mut remaining,
        &mut work(),
        &mut |_, _| {
            persisted += 1;
            Ok(OriginalArtifactRefV1 {
                id: [1; 32],
                byte_len: 100,
            })
        },
        &mut |_| {
            emitted += 1;
            if emitted == 2 {
                Err(ValueTreeError::Invalid("sink failure"))
            } else {
                Ok(())
            }
        },
    );
    assert!(result.is_err());
    assert_eq!(persisted, 1);
    assert_eq!(emitted, 2);
    assert_eq!(remaining, 1);
}
#[test]
fn pending_action_genesis_checks_cancellation_after_original_persistence() {
    let memory = MemoryBudget::new(64 << 20, 1 << 20).unwrap();
    let mut pages = Pages::with_limit(&memory, 16384);
    let cancelled = Cell::new(false);
    let mut emitted = 0;
    let result = prepare_runtime_pending_actions_genesis_v1(
        &source(),
        &mut MemberRecordWriteContext {
            sink: &mut pages,
            memory: &memory,
            io: &mut io(),
            key_work: &mut SemanticKeyBudget {
                bytes_remaining: 1 << 20,
            },
            limits: limits(),
            cancelled: || cancelled.get(),
        },
        &mut 3,
        &mut work(),
        &mut |_, _| {
            cancelled.set(true);
            Ok(OriginalArtifactRefV1 {
                id: [1; 32],
                byte_len: 100,
            })
        },
        &mut |_| {
            emitted += 1;
            Ok(())
        },
    );
    assert!(matches!(
        result,
        Err(MemberRecordError::Value(ValueTreeError::Cancelled))
    ));
    assert_eq!(emitted, 1);
}
