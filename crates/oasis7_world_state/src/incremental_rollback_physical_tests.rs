use super::*;
use crate::{
    incremental_rollback_recovery::*, incremental_semantic_key::SemanticKeyBudget, snapshot::*,
};
use oasis7_formal_protocol::{incremental_state::StatePageWriter, incremental_value::*};
use oasis7_proto::memory_budget::{MemoryBudget, MemoryClass, MemoryReservation, ReservedValue};
use std::{cell::RefCell, collections::BTreeMap, sync::Arc};
fn invalid() -> ValueTreeError {
    ValueTreeError::Invalid("rollback test")
}
type PageMap = BTreeMap<[u8; 32], Arc<ReservedValue<Vec<u8>>>>;
struct Pages {
    memory: MemoryBudget,
    nodes: RefCell<PageMap>,
    _metadata: MemoryReservation,
}
impl Pages {
    fn new(memory: &MemoryBudget) -> Self {
        Self {
            memory: memory.clone(),
            nodes: RefCell::new(BTreeMap::new()),
            _metadata: memory.reserve(2 << 20, MemoryClass::Ordinary).unwrap(),
        }
    }
}
impl ValueNodeWriter for Pages {
    fn put_node(
        &mut self,
        id: [u8; 32],
        value: ReservedValue<Vec<u8>>,
    ) -> Result<(), ValueTreeError> {
        assert!(self.nodes.borrow().len() < 16_384);
        self.nodes.borrow_mut().insert(id, Arc::new(value));
        Ok(())
    }
}
impl ValueNodeReader for Pages {
    fn read_node(
        &self,
        id: &[u8; 32],
        max: usize,
        memory: &MemoryBudget,
        class: MemoryClass,
    ) -> Result<Arc<ReservedValue<Vec<u8>>>, ValueTreeError> {
        let value = self
            .nodes
            .borrow()
            .get(id)
            .cloned()
            .ok_or(ValueTreeError::Invalid("control test missing page"))?;
        if !value.uses_budget(memory) || class != MemoryClass::Ordinary || value.len() > max {
            return Err(invalid());
        }
        Ok(value)
    }
}
impl StatePageWriter for Pages {}
fn io() -> ValueIoBudget {
    ValueIoBudget {
        nodes_remaining: 10000000,
        read_bytes_remaining: 256 << 20,
        write_bytes_remaining: 256 << 20,
    }
}
#[test]
fn cold_outcome_preserves_nested_original_and_rejects_missing_pages() {
    let memory = MemoryBudget::new(256 << 20, 4 << 20).unwrap();
    let mut pages = Pages::new(&memory);
    let source = RollbackNonceOutcome {
        canonical_intent_hash: "original".into(),
        rollback_checkpoint_batch_id: "c".into(),
        rollback_checkpoint_snapshot_hash: "h".into(),
        rollback_checkpoint_journal_len: 3,
        rollback_ticket: "ticket".into(),
        target_journal_commitment: "j".into(),
        target_state_root: "s".into(),
        target_journal_len: 5,
        affected_census_digest: "d".into(),
        affected_census_count: 1,
        readiness_blockers: vec!["a".into(), "".into()],
        rollback_event_id: u64::MAX,
        target_batch_id: "b".into(),
        prior_reorg_epoch: 7,
        committed_reorg_epoch: 9,
        invalidated_batch_ids: vec!["b1".into(), "b0".into()],
        dispositions: vec![RollbackEventDisposition {
            source_batch_id: "sb".into(),
            source_event_id: 11,
            status: RollbackDispositionStatus::CompensationRequired,
            compensation: Some(RollbackCompensationCaseRef {
                owner_id: "owner".into(),
                ticket_id: "t".into(),
                state: RollbackCompensationState::InProgress,
            }),
            player_id: Some(String::new()),
            action_id: None,
            system_authored: false,
        }],
        receipt: Some(RollbackReceiptProjection {
            receipt_id: "r".into(),
            canonical_intent_digest: "ci".into(),
            rollback_checkpoint_batch_id: "cb".into(),
            rollback_checkpoint_snapshot_hash: "cs".into(),
            rollback_checkpoint_journal_len: 12,
            replay_target_batch_id: "rt".into(),
            replay_target_journal_len: 13,
            replay_target_state_root: "rs".into(),
            replay_target_journal_commitment: "rj".into(),
            affected_census_digest: "ad".into(),
            affected_census_count: 14,
            readiness_blockers: vec!["blocked".into()],
            snapshot_height: 15,
            snapshot_hash: "sh".into(),
            log_cursor: 16,
            acknowledged_at_tick: 17,
        }),
    };
    let root = prepare_rollback_record_v1(
        RollbackRecordRefV1::Outcome(&source),
        &mut MemberRecordWriteContext {
            sink: &mut pages,
            memory: &memory,
            io: &mut io(),
            key_work: &mut SemanticKeyBudget {
                bytes_remaining: 8 << 20,
            },
            limits: MemberRecordLimitsV1 {
                semantic_bytes: 4096,
                payload_bytes: 1 << 20,
                record_bytes: 2 << 20,
            },
            cancelled: || false,
        },
        &mut RollbackWorkV1 {
            cells_remaining: 1000,
            max_pending_node_bytes: 8 << 20,
        },
    )
    .unwrap();
    let mut logical = io();
    let mut values = ValueReadContext {
        source: &pages,
        memory: &memory,
        class: MemoryClass::Ordinary,
        io: &mut logical,
    };
    let mut keys = SemanticKeyBudget {
        bytes_remaining: 8 << 20,
    };
    let mut work = RollbackWorkV1 {
        cells_remaining: 10000,
        max_pending_node_bytes: 8 << 20,
    };
    let mut cancelled = || false;
    let mut ctx = RollbackDecodeContextV1 {
        values: &mut values,
        keys: &mut keys,
        work: &mut work,
        limits: RollbackDecodeLimitsV1 {
            max_output_bytes: 65536,
            max_text_bytes: 4096,
            max_record_bytes: 2 << 20,
        },
        cancelled: &mut cancelled,
    };
    let recovered = recover_rollback_outcome_v1(&root, &mut ctx).unwrap();
    assert_eq!(*recovered, source);
    drop(recovered);
    ctx.limits.max_output_bytes = 1;
    assert!(recover_rollback_outcome_v1(&root, &mut ctx).is_err());
    pages.nodes.borrow_mut().clear();
    let mut logical = io();
    let mut values = ValueReadContext {
        source: &pages,
        memory: &memory,
        class: MemoryClass::Ordinary,
        io: &mut logical,
    };
    let mut ctx = RollbackDecodeContextV1 {
        values: &mut values,
        keys: &mut keys,
        work: &mut work,
        limits: RollbackDecodeLimitsV1 {
            max_output_bytes: 65536,
            max_text_bytes: 4096,
            max_record_bytes: 2 << 20,
        },
        cancelled: &mut cancelled,
    };
    assert!(recover_rollback_outcome_v1(&root, &mut ctx).is_err());
}

#[path = "incremental_rollback_collection_tests.rs"]
mod collection;
#[path = "incremental_rollback_delta_tests.rs"]
mod delta;

#[path = "incremental_rollback_overlay_tests.rs"]
mod overlay;

#[path = "incremental_rollback_readmission_tests.rs"]
mod readmission;
