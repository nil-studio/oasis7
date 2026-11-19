use super::*;
use crate::{
    incremental_field_overlay::WorldFieldOverlayContextV1, incremental_rollback_family_genesis::*,
    incremental_rollback_family_recovery::*, incremental_rollback_overlay::*,
};
use oasis7_formal_protocol::{
    incremental_execution::{
        AuthorityBinding, ExecutionParent, ExecutionTransaction, RootSetV1, TransactionLimits,
    },
    incremental_state::*,
};
struct Writer<'a>(&'a Pages);
impl ValueNodeWriter for Writer<'_> {
    fn put_node(
        &mut self,
        id: [u8; 32],
        bytes: ReservedValue<Vec<u8>>,
    ) -> Result<(), ValueTreeError> {
        if self.0.nodes.borrow().len() >= 65536 {
            return Err(ValueTreeError::ResourceLimited("rollback test pages"));
        }
        self.0.nodes.borrow_mut().insert(id, Arc::new(bytes));
        Ok(())
    }
}
impl StatePageWriter for Writer<'_> {}
pub(super) fn seed(nonce: &str) -> (MemoryBudget, Pages, RootSetV1) {
    let memory = MemoryBudget::new(128 << 20, 4 << 20).unwrap();
    let mut pages = Pages::new(&memory);
    let registry = RollbackAuthorityRegistry::default();
    let nonces = [String::new(), nonce.into()].into_iter().collect();
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
                semantic_bytes: 2 << 20,
                payload_bytes: 1 << 20,
                record_bytes: 4 << 20,
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
    (memory, pages, roots)
}
fn recover(
    memory: &MemoryBudget,
    pages: &Pages,
    roots: &RootSetV1,
) -> ReservedValue<RecoveredRollbackControlsV1> {
    let mut logical = io();
    let mut values = ValueReadContext {
        source: pages,
        memory,
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
    let mut cancel = || false;
    let mut ctx = RollbackDecodeContextV1 {
        values: &mut values,
        keys: &mut keys,
        work: &mut work,
        limits: RollbackDecodeLimitsV1 {
            max_output_bytes: 65536,
            max_text_bytes: 4096,
            max_record_bytes: 4 << 20,
        },
        cancelled: &mut cancel,
    };
    recover_rollback_controls_v1(
        roots,
        pages,
        &mut StateScanBudget {
            nodes_remaining: 100000,
            read_bytes_remaining: 64 << 20,
            leaves_remaining: 1000,
        },
        &mut ctx,
    )
    .unwrap()
}
#[test]
fn consumed_nonce_uses_overlay_and_rejects_foreign_rootset_with_same_auth_root() {
    let (memory, pages, roots) = seed(" nonce ");
    let qualification = recover(&memory, &pages, &roots);
    for foreign in [false, true] {
        let mut other = roots.clone();
        other.simulation_tick = 1;
        let foreign_qualification = recover(&memory, &pages, &other);
        let parent = ExecutionParent {
            commit: [4; 32],
            roots: roots.clone(),
            authority: AuthorityBinding {
                token: [5; 32],
                epoch: 7,
            },
        };
        let live = parent.fence().unwrap();
        let mut tx = ExecutionTransaction::begin(
            parent,
            [6; 32],
            &pages,
            &memory,
            TransactionLimits {
                max_reads: 64,
                max_writes: 16,
                max_facts: 8,
                max_pending_node_bytes: 8 << 20,
            },
        )
        .unwrap();
        let mut logical = io();
        let mut read_keys = SemanticKeyBudget {
            bytes_remaining: 8 << 20,
        };
        let mut writer = Writer(&pages);
        let mut physical = io();
        let mut write_keys = SemanticKeyBudget {
            bytes_remaining: 8 << 20,
        };
        let mut output = MemberRecordWriteContext {
            sink: &mut writer,
            memory: &memory,
            io: &mut physical,
            key_work: &mut write_keys,
            limits: MemberRecordLimitsV1 {
                semantic_bytes: 2 << 20,
                payload_bytes: 1 << 20,
                record_bytes: 4 << 20,
            },
            cancelled: || false,
        };
        let guard = tx.guard();
        let mut context = WorldFieldOverlayContextV1 {
            transaction: &mut tx,
            values: ValueReadContext {
                source: &pages,
                memory: &memory,
                class: MemoryClass::Ordinary,
                io: &mut logical,
            },
            key_work: &mut read_keys,
            semantic_bytes: 2 << 20,
            record_bytes: 4 << 20,
            take_state_error: || None,
            cancelled: || false,
        };
        let result = change_consumed_rollback_nonce_v1(
            RollbackNonceChangeV1 {
                nonce: "new",
                parent: if foreign { &other } else { &roots },
                family: if foreign {
                    &foreign_qualification
                } else {
                    &qualification
                },
                expected: false,
                after: true,
            },
            &mut context,
            &mut output,
            &guard,
            &live,
        );
        if foreign {
            assert!(result.is_err());
        } else {
            result.unwrap();
            let guard = context.transaction.guard();
            change_consumed_rollback_nonce_v1(
                RollbackNonceChangeV1 {
                    nonce: "new",
                    parent: &roots,
                    family: &qualification,
                    expected: true,
                    after: true,
                },
                &mut context,
                &mut output,
                &guard,
                &live,
            )
            .unwrap();
        }
        let guard = tx.guard();
        let sealed = tx.seal(&guard, &live, 1);
        if foreign {
            assert!(sealed.is_err());
        } else {
            assert_eq!(sealed.unwrap().record.writes.len(), 1);
        }
    }
}

#[test]
fn long_nonce_is_rejected_before_semantic_buffer_allocation() {
    let nonce = "z".repeat(1 << 20);
    let (memory, pages, roots) = seed(&nonce);
    // Leave enough for the documented scan and small header, less than one
    // semantic-key buffer. An attempted large allocation would report Memory.
    let filler_bytes = (128usize << 20) - (4usize << 20) - memory.used() - (512usize << 10);
    let _filler = memory.reserve(filler_bytes, MemoryClass::Ordinary).unwrap();
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
            max_text_bytes: 2 << 20,
            max_record_bytes: 4 << 20,
        },
        cancelled: &mut cancelled,
    };
    let result = recover_rollback_controls_v1(
        &roots,
        &pages,
        &mut StateScanBudget {
            nodes_remaining: 100000,
            read_bytes_remaining: 64 << 20,
            leaves_remaining: 1000,
        },
        &mut context,
    );
    assert!(
        matches!(
            result,
            Err(ValueTreeError::ResourceLimited(
                "rollback recovered key bytes"
            ))
        ),
        "wrong boundary: {:?}",
        result.err()
    );
}
