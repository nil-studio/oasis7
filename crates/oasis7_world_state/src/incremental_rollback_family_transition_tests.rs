use super::*;
use oasis7_formal_protocol::incremental_execution::{
    AuthorityBinding, ExecutionParent, ExecutionTransaction, TransactionLimits, TransitionVerifier,
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
impl ValueNodeReader for Writer<'_> {
    fn read_node(
        &self,
        id: &[u8; 32],
        max: usize,
        memory: &MemoryBudget,
        class: MemoryClass,
    ) -> Result<Arc<ReservedValue<Vec<u8>>>, ValueTreeError> {
        ValueNodeReader::read_node(self.0, id, max, memory, class)
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

fn proof(
    memory: &MemoryBudget,
    pages: &Pages,
    roots: &RootSetV1,
) -> ReservedValue<RollbackFamilyQualificationV1> {
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
    qualify_rollback_family_v1(
        roots,
        pages,
        &mut StateScanBudget {
            nodes_remaining: 100000,
            read_bytes_remaining: 64 << 20,
            leaves_remaining: 1000,
        },
        &mut RollbackDecodeContextV1 {
            values: &mut values,
            keys: &mut keys,
            work: &mut work,
            limits: RollbackDecodeLimitsV1 {
                max_output_bytes: 65536,
                max_text_bytes: 4096,
                max_record_bytes: 2 << 20,
            },
            cancelled: &mut cancel,
        },
        || None,
    )
    .unwrap()
}
#[test]
fn rollback_family_receiver_inherits_exact_parent_and_counts_real_nonce_delta() {
    let (memory, pages, roots) = seed("prior");
    let qualified = proof(&memory, &pages, &roots);
    let registry = RollbackAuthorityRegistry::default();
    let nonces = ["new".to_owned()].into_iter().collect();
    let outcomes = BTreeMap::new();
    let mut newrow = None;
    let target = crate::incremental_semantic_key::hash_semantic_key_v1(
        &"new",
        &memory,
        MemoryClass::Ordinary,
        &mut SemanticKeyBudget {
            bytes_remaining: 4096,
        },
        &mut || false,
    )
    .unwrap()
    .digest;
    prepare_rollback_controls_genesis_v1(
        &RollbackControlsRefV1 {
            authority_registry: &registry,
            consumed_nonces: &nonces,
            nonce_outcomes: &outcomes,
        },
        &mut MemberRecordWriteContext {
            sink: &mut Writer(&pages),
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
            let key =
                WorldFieldKeyV1::from_bytes(&row.mutation.key).map_err(ValueTreeError::Invalid)?;
            if key.name == "consumed_rollback_nonces" && key.semantic_key == Some(target) {
                newrow = Some(row);
            }
            Ok(())
        },
    )
    .unwrap();
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
        parent.clone(),
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
    let row = newrow.unwrap();
    tx.write(row.namespace, &row.mutation.key, row.mutation.after)
        .unwrap();
    let guard = tx.guard();
    let prepared = tx.seal(&guard, &live, 1).unwrap();
    for node in &prepared.new_nodes {
        Writer(&pages)
            .put_node(
                *blake3::Hash::from_hex(&node.digest).unwrap().as_bytes(),
                memory
                    .reserve(node.bytes.len(), MemoryClass::Ordinary)
                    .unwrap()
                    .retain(node.bytes.clone()),
            )
            .unwrap();
    }
    let verifier = TransitionVerifier {
        parent: &parent,
        operation: [6; 32],
        source: &pages,
        memory: &memory,
        class: MemoryClass::Ordinary,
        max_pending_node_bytes: 8 << 20,
    };
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
    let mut cancel = || false;
    let mut ctx = RollbackDecodeContextV1 {
        values: &mut values,
        keys: &mut keys,
        work: &mut work,
        limits: RollbackDecodeLimitsV1 {
            max_output_bytes: 65536,
            max_text_bytes: 4096,
            max_record_bytes: 2 << 20,
        },
        cancelled: &mut cancel,
    };
    let after = accept_rollback_family_transition_v1(
        &qualified,
        &verifier,
        &prepared.record,
        &pages,
        &mut StateScanBudget {
            nodes_remaining: 100000,
            read_bytes_remaining: 64 << 20,
            leaves_remaining: 1000,
        },
        &mut ctx,
        &mut 100000,
        || None,
    )
    .unwrap();
    assert_eq!(after.consumed_nonce_count(), 3);
    after.validate_binding(&prepared.record.after).unwrap();
    let mut wrong = roots.clone();
    wrong.simulation_tick += 1;
    let wrongproof = proof(&memory, &pages, &wrong);
    assert!(
        accept_rollback_family_transition_v1(
            &wrongproof,
            &verifier,
            &prepared.record,
            &pages,
            &mut StateScanBudget {
                nodes_remaining: 100000,
                read_bytes_remaining: 64 << 20,
                leaves_remaining: 1000
            },
            &mut ctx,
            &mut 100000,
            || None
        )
        .is_err()
    );
}
