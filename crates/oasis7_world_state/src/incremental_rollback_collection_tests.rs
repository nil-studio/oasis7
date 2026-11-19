use super::*;
use crate::incremental_rollback_family_genesis::*;
use crate::incremental_rollback_family_recovery::*;
use oasis7_formal_protocol::{incremental_execution::RootSetV1, incremental_state::*};
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
fn cold_collection_keeps_empty_nonce_and_requires_all_markers() {
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
    let recovered = recover_rollback_controls_v1(
        &roots,
        &pages,
        &mut StateScanBudget {
            nodes_remaining: 100000,
            read_bytes_remaining: 64 << 20,
            leaves_remaining: 1000,
        },
        &mut context,
    )
    .unwrap();
    assert_eq!(recovered.authority_registry, registry);
    assert_eq!(recovered.consumed_nonces, nonces);
    assert!(recovered.nonce_outcomes.is_empty());
    recovered.validate_binding(&roots).unwrap();
    let mut wrong = roots.clone();
    wrong.simulation_tick = 1;
    assert!(recovered.validate_binding(&wrong).is_err());
    drop(recovered);
    let empty = RootSetV1::empty("rollback-family".into(), [77; 32]).unwrap();
    assert!(
        recover_rollback_controls_v1(
            &empty,
            &pages,
            &mut StateScanBudget {
                nodes_remaining: 100000,
                read_bytes_remaining: 64 << 20,
                leaves_remaining: 1000
            },
            &mut context
        )
        .is_err()
    );
}
