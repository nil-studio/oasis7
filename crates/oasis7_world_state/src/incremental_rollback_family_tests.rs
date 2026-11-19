use super::*;
use crate::incremental_rollback_genesis::{
    RollbackRecordRefV1, RollbackWorkV1, prepare_rollback_record_v1,
};
use crate::incremental_semantic_key::SemanticKeyBudget;
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

use crate::incremental_rollback_family_genesis::*;
use oasis7_formal_protocol::incremental_state::*;
pub(super) struct Node(Arc<ReservedValue<Vec<u8>>>);
impl AsRef<[u8]> for Node {
    fn as_ref(&self) -> &[u8] {
        self.0.as_slice()
    }
}
impl MerkleNodeReader for Pages {
    type Node = Node;
    fn allocation_budget(&self) -> Option<&MemoryBudget> {
        Some(&self.memory)
    }
    fn allocation_class(&self) -> Option<MemoryClass> {
        Some(MemoryClass::Ordinary)
    }
    fn read_node(&self, id: &str, max: usize) -> Result<Option<Node>, String> {
        let id = *blake3::Hash::from_hex(id)
            .map_err(|e| e.to_string())?
            .as_bytes();
        self.nodes
            .borrow()
            .get(&id)
            .map(|n| {
                if n.len() > max {
                    Err("control state bound".into())
                } else {
                    Ok(Node(n.clone()))
                }
            })
            .transpose()
    }
}

#[test]
fn rollback_family_streams_real_gen_and_requires_every_marker() {
    let memory = MemoryBudget::new(128 << 20, 4 << 20).unwrap();
    let mut pages = Pages::new(&memory);
    let registry = RollbackAuthorityRegistry::default();
    let nonces = [String::new(), " nonce ".into()].into_iter().collect();
    let outcomes = BTreeMap::new();
    let _rows_charge = memory.reserve(128 << 10, MemoryClass::Ordinary).unwrap();
    let mut rows = Vec::new();
    prepare_rollback_controls_genesis_v1(
        &RollbackControlsRefV1 {
            authority_registry: &registry,
            consumed_nonces: &nonces,
            nonce_outcomes: &outcomes,
        },
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
        &mut |row| {
            rows.push(row);
            Ok(())
        },
    )
    .unwrap();
    let mut writes: Vec<_> = rows.iter().map(|row| row.mutation.clone()).collect();
    writes.sort_by(|a, b| a.key.cmp(&b.key));
    let _delta_charge = memory.reserve(8 << 20, MemoryClass::Ordinary).unwrap();
    let delta = StateDeltaV1::build(
        StateRootV1::empty(StateNamespaceV1::Authorization).unwrap(),
        writes,
        &pages,
        8 << 20,
    )
    .unwrap();
    for node in &delta.new_nodes {
        let id = *blake3::Hash::from_hex(&node.digest).unwrap().as_bytes();
        let charge = memory
            .reserve(node.bytes.len(), MemoryClass::Ordinary)
            .unwrap();
        pages
            .put_node(id, charge.retain(node.bytes.clone()))
            .unwrap();
    }
    let mut roots = RootSetV1::empty("rollback-family".into(), [77; 32]).unwrap();
    *roots
        .state
        .iter_mut()
        .find(|root| root.namespace == StateNamespaceV1::Authorization)
        .unwrap() = delta.after.clone();
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
    let mut context = RollbackDecodeContextV1 {
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
    let recovered = qualify_rollback_family_v1(
        &roots,
        &pages,
        &mut StateScanBudget {
            nodes_remaining: 100000,
            read_bytes_remaining: 64 << 20,
            leaves_remaining: 1000,
        },
        &mut context,
        || None,
    )
    .unwrap();
    assert_eq!(recovered.consumed_nonce_count(), 2);
    assert_eq!(recovered.outcome_count(), 0);
    recovered.validate_binding(&roots).unwrap();
    let mut wrong = roots.clone();
    wrong.simulation_tick = 1;
    assert!(recovered.validate_binding(&wrong).is_err());
    drop(recovered);
    let empty = RootSetV1::empty("rollback-family".into(), [77; 32]).unwrap();
    assert!(
        qualify_rollback_family_v1(
            &empty,
            &pages,
            &mut StateScanBudget {
                nodes_remaining: 100000,
                read_bytes_remaining: 64 << 20,
                leaves_remaining: 1000
            },
            &mut context,
            || None,
        )
        .is_err()
    );
}

#[test]
fn rollback_stream_large_text_preserves_all_original_nested_shapes_without_dto_budget() {
    let memory = MemoryBudget::new(256 << 20, 4 << 20).unwrap();
    let mut pages = Pages::new(&memory);
    let mut source = RollbackNonceOutcome {
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
    let _input_charge = memory.reserve(8 << 20, MemoryClass::Ordinary).unwrap();
    source.canonical_intent_hash = "x".repeat(8 << 20);
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
                payload_bytes: 16 << 20,
                record_bytes: 32 << 20,
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
            max_output_bytes: 4096,
            max_text_bytes: 16 << 20,
            max_record_bytes: 32 << 20,
        },
        cancelled: &mut cancelled,
    };
    stream::qualify_outcome(&root, &mut ctx).unwrap();
    assert!(recover_rollback_outcome_v1(&root, &mut ctx).is_err());
    let mut malformed = root.clone();
    malformed.leaf_count += 1;
    assert!(stream::qualify_outcome(&malformed, &mut ctx).is_err());
}

#[path = "incremental_rollback_family_recovery_tests.rs"]
mod recovery;

#[path = "incremental_rollback_family_large_delta_tests.rs"]
mod large_delta;
#[path = "incremental_rollback_family_transition_tests.rs"]
mod transition;

#[path = "incremental_rollback_family_foreign_tests.rs"]
mod foreign_domain;
