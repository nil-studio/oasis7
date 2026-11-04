//! Recovery proof admission uses real separately charged page copies.
use super::*;
struct RecoveryPages<'a>(&'a Pages);
impl ValueNodeReader for RecoveryPages<'_> {
    fn read_node(
        &self,
        id: &[u8; 32],
        max: usize,
        memory: &MemoryBudget,
        class: MemoryClass,
    ) -> Result<Arc<ReservedValue<Vec<u8>>>, ValueTreeError> {
        if class != MemoryClass::Recovery || !memory.same_pool(&self.0.memory) {
            return Err(invalid());
        }
        let pages = self.0.nodes.borrow();
        let value = pages.get(id).ok_or_else(invalid)?;
        if value.len() > max {
            return Err(invalid());
        }
        let charge = memory.reserve(value.len().checked_add(128).ok_or_else(invalid)?, class)?;
        Ok(Arc::new(charge.retain(value.as_slice().to_vec())))
    }
}
impl MerkleNodeReader for RecoveryPages<'_> {
    type Node = Node;
    fn allocation_budget(&self) -> Option<&MemoryBudget> {
        Some(&self.0.memory)
    }
    fn allocation_class(&self) -> Option<MemoryClass> {
        Some(MemoryClass::Recovery)
    }
    fn read_node(&self, id: &str, max: usize) -> Result<Option<Node>, String> {
        let hash = *blake3::Hash::from_hex(id)
            .map_err(|e| e.to_string())?
            .as_bytes();
        if !self.0.nodes.borrow().contains_key(&hash) {
            return Ok(None);
        }
        ValueNodeReader::read_node(self, &hash, max, &self.0.memory, MemoryClass::Recovery)
            .map(|v| Some(Node(v)))
            .map_err(|e| e.to_string())
    }
}

#[test]
fn rollback_family_requires_explicit_same_root_same_pool_recovery_readmission() {
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

    let recovery = RecoveryPages(&pages);
    let mut logical = io();
    let mut values = ValueReadContext {
        source: &recovery,
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
            max_record_bytes: 2 << 20,
        },
        cancelled: &mut cancelled,
    };
    let proof = qualify_rollback_family_v1(
        &roots,
        &recovery,
        &mut StateScanBudget {
            nodes_remaining: 100000,
            read_bytes_remaining: 64 << 20,
            leaves_remaining: 1000,
        },
        &mut ctx,
        || None,
    )
    .unwrap();
    assert_eq!(proof.allocation_class(), MemoryClass::Recovery);
    let baseline = memory.used();
    let admitted = readmit_rollback_family_ordinary_v1(&proof, &roots, &memory, || false).unwrap();
    assert_eq!(admitted.allocation_class(), MemoryClass::Ordinary);
    admitted.validate_binding(&roots).unwrap();
    assert!(readmit_rollback_family_ordinary_v1(&admitted, &roots, &memory, || false).is_err());
    let other = MemoryBudget::new(16 << 20, 4 << 20).unwrap();
    assert!(readmit_rollback_family_ordinary_v1(&proof, &roots, &other, || false).is_err());
    let mut wrong = roots.clone();
    wrong.simulation_tick += 1;
    assert!(readmit_rollback_family_ordinary_v1(&proof, &wrong, &memory, || false).is_err());
    assert!(matches!(
        readmit_rollback_family_ordinary_v1(&proof, &roots, &memory, || true),
        Err(ValueTreeError::Cancelled)
    ));
    drop(admitted);
    assert_eq!(memory.used(), baseline);
}
