use super::*;
use crate::{
    incremental_field_read::WorldFieldReadContextV1, incremental_semantic_key::SemanticKeyBudget,
};
use oasis7_formal_protocol::{
    incremental_execution::{ExecutionWriteV1, RootSetV1},
    incremental_state::*,
    incremental_value::*,
};
use oasis7_proto::memory_budget::{MemoryBudget, MemoryClass, MemoryReservation, ReservedValue};
use std::{cell::RefCell, collections::BTreeMap, sync::Arc};
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
            .ok_or(ValueTreeError::Invalid("business test missing page"))?;
        if !value.uses_budget(memory) || class != MemoryClass::Ordinary || value.len() > max {
            return Err(ValueTreeError::Invalid("grant test admission"));
        }
        Ok(value)
    }
}
struct Node(Arc<ReservedValue<Vec<u8>>>);
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
                    Err("business state bound".into())
                } else {
                    Ok(Node(n.clone()))
                }
            })
            .transpose()
    }
}
fn io() -> ValueIoBudget {
    ValueIoBudget {
        nodes_remaining: 100_000,
        read_bytes_remaining: 64 << 20,
        write_bytes_remaining: 64 << 20,
    }
}

impl StatePageWriter for Pages {}
fn install(
    memory: &MemoryBudget,
    pages: &mut Pages,
    roots: &mut RootSetV1,
    rows: &[ReservedValue<ExecutionWriteV1>],
) {
    let mut writes: Vec<_> = rows.iter().map(|r| r.mutation.clone()).collect();
    writes.sort_by(|a, b| a.key.cmp(&b.key));
    let charge = memory
        .reserve((writes.len() + 1) * (512 << 10), MemoryClass::Ordinary)
        .unwrap();
    let delta = charge.retain(
        StateDeltaV1::build(
            StateRootV1::empty(StateNamespaceV1::Authorization).unwrap(),
            writes,
            pages,
            8 << 20,
        )
        .unwrap(),
    );
    for node in &delta.new_nodes {
        let id = *blake3::Hash::from_hex(&node.digest).unwrap().as_bytes();
        let charge = memory
            .reserve(node.bytes.len(), MemoryClass::Ordinary)
            .unwrap();
        pages
            .put_node(id, charge.retain(node.bytes.clone()))
            .unwrap();
    }
    *roots
        .state
        .iter_mut()
        .find(|r| r.namespace == StateNamespaceV1::Authorization)
        .unwrap() = delta.after.clone();
}
fn context<'a>(
    pages: &'a Pages,
    roots: &'a RootSetV1,
    memory: &'a MemoryBudget,
    io: &'a mut ValueIoBudget,
    key_work: &'a mut SemanticKeyBudget,
) -> WorldFieldReadContextV1<
    'a,
    Pages,
    Pages,
    impl FnMut() -> Option<ValueTreeError> + use<>,
    impl FnMut() -> bool + use<>,
> {
    WorldFieldReadContextV1 {
        root: roots.state_root(StateNamespaceV1::Authorization),
        state: pages,
        values: ValueReadContext {
            source: pages,
            memory,
            class: MemoryClass::Ordinary,
            io,
        },
        key_work,
        semantic_bytes: 1024,
        record_bytes: 4096,
        take_state_error: || None,
        cancelled: || false,
    }
}
fn scan() -> StateScanBudget {
    StateScanBudget {
        nodes_remaining: 100_000,
        read_bytes_remaining: 64 << 20,
        leaves_remaining: 100_000,
    }
}

#[test]
fn grants_original_graph_family_is_root_bound_and_requires_complete_metadata() {
    use crate::incremental_field_member::*;
    use crate::incremental_json_records::{JsonReadWorkV1, JsonWriteWorkV1};
    use crate::incremental_runtime_authorization_grants::*;
    let memory = MemoryBudget::new(96 << 20, 4 << 20).unwrap();
    let mut pages = Pages::new(&memory);
    let mut source = BTreeMap::new();
    source.insert("original-map-key".to_owned(),serde_json::json!({"grant_id":"different-inner-id","unknown_signed_field":[null,-1,0,18446744073709551615u64,{"":"raw"}]}));
    let mut rows = Vec::new();
    prepare_authorization_grants_genesis_v1(
        &source,
        &mut MemberRecordWriteContext {
            sink: &mut pages,
            memory: &memory,
            io: &mut io(),
            key_work: &mut SemanticKeyBudget {
                bytes_remaining: 8 << 20,
            },
            limits: MemberRecordLimitsV1 {
                semantic_bytes: 1024,
                payload_bytes: 1 << 20,
                record_bytes: 2 << 20,
            },
            cancelled: || false,
        },
        &mut JsonWriteWorkV1 {
            cells_remaining: 10000,
            json_nodes_remaining: 100,
            max_json_depth: 16,
            max_pending_node_bytes: 1 << 20,
        },
        &mut |row| {
            rows.push(row);
            Ok(())
        },
    )
    .unwrap();
    drop(source);
    let mut roots = RootSetV1::empty("authorization-test".into(), [94; 32]).unwrap();
    install(&memory, &mut pages, &mut roots, &rows);
    let mut value_io = io();
    let mut keys = SemanticKeyBudget {
        bytes_remaining: 8 << 20,
    };
    let mut ctx = context(&pages, &roots, &memory, &mut value_io, &mut keys);
    let mut work = JsonReadWorkV1 {
        cells_remaining: 10000,
        json_nodes_remaining: 100,
        max_depth: 16,
        max_payload_bytes: 1 << 20,
    };
    let proof =
        qualify_authorization_grants_family_v1(&roots, &mut ctx, &mut scan(), &mut work).unwrap();
    assert_eq!(proof.entries(), 1);
    proof.validate_binding(&roots).unwrap();
    let mut other = roots.clone();
    other.state.swap(0, 1);
    assert!(proof.validate_binding(&other).is_err());
    assert!(
        qualify_authorization_grant_member_v1("original-map-key", &mut ctx, &mut work)
            .unwrap()
            .is_some()
    );
    assert!(
        qualify_authorization_grant_member_v1("different-inner-id", &mut ctx, &mut work)
            .unwrap()
            .is_none()
    );
    let mut roots_without_metadata =
        RootSetV1::empty("authorization-test".into(), [94; 32]).unwrap();
    let entries: Vec<_> = rows
        .into_iter()
        .filter(|r| {
            crate::incremental_world_field_key::WorldFieldKeyV1::from_bytes(&r.mutation.key)
                .unwrap()
                .kind
                != WorldFieldRecordKindV1::Metadata
        })
        .collect();
    drop(ctx);
    install(&memory, &mut pages, &mut roots_without_metadata, &entries);
    let mut ctx = context(
        &pages,
        &roots_without_metadata,
        &memory,
        &mut value_io,
        &mut keys,
    );
    assert!(
        qualify_authorization_grants_family_v1(
            &roots_without_metadata,
            &mut ctx,
            &mut scan(),
            &mut work
        )
        .is_err()
    );
}
