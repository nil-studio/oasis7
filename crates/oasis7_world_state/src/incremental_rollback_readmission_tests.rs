use super::*;
use crate::{
    incremental_field_overlay::WorldFieldOverlayContextV1, incremental_rollback_family_recovery::*,
    incremental_rollback_overlay::*,
};
use oasis7_formal_protocol::{
    incremental_execution::{
        AuthorityBinding, ExecutionParent, ExecutionTransaction, TransactionLimits,
    },
    incremental_state::*,
};
struct ClassView<'a> {
    pages: &'a Pages,
    class: MemoryClass,
}
impl MerkleNodeReader for ClassView<'_> {
    type Node = <Pages as MerkleNodeReader>::Node;
    fn allocation_budget(&self) -> Option<&MemoryBudget> {
        Some(&self.pages.memory)
    }
    fn allocation_class(&self) -> Option<MemoryClass> {
        Some(self.class)
    }
    fn read_node(&self, id: &str, max: usize) -> Result<Option<Self::Node>, String> {
        MerkleNodeReader::read_node(self.pages, id, max)
    }
}
impl ValueNodeReader for ClassView<'_> {
    fn read_node(
        &self,
        id: &[u8; 32],
        max: usize,
        memory: &MemoryBudget,
        class: MemoryClass,
    ) -> Result<Arc<ReservedValue<Vec<u8>>>, ValueTreeError> {
        if class != self.class {
            return Err(invalid());
        } /* Reuse charged immutable physical cache pages; only DTO construction is new. */
        ValueNodeReader::read_node(self.pages, id, max, memory, MemoryClass::Ordinary)
    }
}
struct Writer<'a>(&'a Pages);
impl ValueNodeWriter for Writer<'_> {
    fn put_node(
        &mut self,
        id: [u8; 32],
        bytes: ReservedValue<Vec<u8>>,
    ) -> Result<(), ValueTreeError> {
        self.0.nodes.borrow_mut().insert(id, Arc::new(bytes));
        Ok(())
    }
}
impl StatePageWriter for Writer<'_> {}
#[test]
fn recovery_carrier_needs_ordinary_readmission_and_shared_charges_stay_live() {
    let (memory, pages, roots) = super::overlay::seed(" nonce ");
    let before = memory.used();
    let source = ClassView {
        pages: &pages,
        class: MemoryClass::Recovery,
    };
    let mut logical = io();
    let mut values = ValueReadContext {
        source: &source,
        memory: &memory,
        class: MemoryClass::Recovery,
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
            max_record_bytes: 4 << 20,
        },
        cancelled: &mut cancelled,
    };
    let recovered = recover_rollback_controls_v1(
        &roots,
        &source,
        &mut StateScanBudget {
            nodes_remaining: 100000,
            read_bytes_remaining: 64 << 20,
            leaves_remaining: 1000,
        },
        &mut ctx,
    )
    .unwrap();
    assert_eq!(recovered.allocation_class(), MemoryClass::Recovery);
    let recovery_bytes = recovered.reserved_bytes();
    let recovery = Arc::new(recovered);
    let filler_bytes = (128usize << 20) - (4usize << 20) - memory.used() - 512;
    let filler = memory.reserve(filler_bytes, MemoryClass::Ordinary).unwrap();
    assert!(matches!(
        readmit_rollback_controls_ordinary_v1(&recovery, &memory),
        Err(ValueTreeError::Memory(_))
    ));
    assert!(recovery.consumed_nonces.contains(" nonce "));
    drop(filler);
    let ordinary = readmit_rollback_controls_ordinary_v1(&recovery, &memory).unwrap();
    assert_eq!(ordinary.allocation_class(), MemoryClass::Ordinary);
    assert_eq!(ordinary.consumed_nonces, recovery.consumed_nonces);
    for (family, accepted) in [(&*recovery, false), (&ordinary, true)] {
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
        let guard = tx.guard();
        let mut read_io = io();
        let mut read_keys = SemanticKeyBudget {
            bytes_remaining: 8 << 20,
        };
        let mut context = WorldFieldOverlayContextV1 {
            transaction: &mut tx,
            values: ValueReadContext {
                source: &pages,
                memory: &memory,
                class: MemoryClass::Ordinary,
                io: &mut read_io,
            },
            key_work: &mut read_keys,
            semantic_bytes: 4096,
            record_bytes: 4 << 20,
            take_state_error: || None,
            cancelled: || false,
        };
        let mut writer = Writer(&pages);
        let mut write_io = io();
        let mut write_keys = SemanticKeyBudget {
            bytes_remaining: 8 << 20,
        };
        let mut write = MemberRecordWriteContext {
            sink: &mut writer,
            memory: &memory,
            io: &mut write_io,
            key_work: &mut write_keys,
            limits: MemberRecordLimitsV1 {
                semantic_bytes: 4096,
                payload_bytes: 1 << 20,
                record_bytes: 4 << 20,
            },
            cancelled: || false,
        };
        let result = change_consumed_rollback_nonce_v1(
            RollbackNonceChangeV1 {
                nonce: "new",
                parent: &roots,
                family,
                expected: false,
                after: true,
            },
            &mut context,
            &mut write,
            &guard,
            &live,
        );
        assert_eq!(result.is_ok(), accepted);
        let guard = tx.guard();
        assert_eq!(tx.seal(&guard, &live, 1).is_ok(), accepted);
    }
    let first_bytes = ordinary.reserved_bytes();
    let first = Arc::new(ordinary);
    let ordinary = readmit_rollback_controls_ordinary_v1(&first, &memory).unwrap();
    let before_drop = memory.used();
    drop(first);
    assert_eq!(
        memory.used(),
        before_drop - first_bytes,
        "readmission must flatten the source carrier chain"
    );
    let ordinary_bytes = ordinary.reserved_bytes();
    drop(recovery);
    assert!(memory.used() >= before + recovery_bytes + ordinary_bytes);
    assert!(ordinary.consumed_nonces.contains(" nonce "));
    drop(ordinary);
    assert!(memory.used() < before + recovery_bytes + ordinary_bytes);
}
