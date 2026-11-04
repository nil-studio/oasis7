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

use crate::incremental_field_member::*;
use crate::incremental_field_presence::*;
use crate::incremental_semantic_key::{CanonicalSemanticKeyRefV1, SEMANTIC_KEY_DOMAIN};
fn deep_key(depth: usize, out: &mut Vec<u8>) {
    if depth == 0 {
        out.push(1);
        out.extend_from_slice(&0u64.to_be_bytes());
    } else {
        out.push(5);
        deep_key(depth - 1, out);
        deep_key(depth - 1, out);
    }
}
#[test]
fn wide_pair_tree_is_admitted_by_actual_box_and_string_metadata() {
    let memory = MemoryBudget::new(96 << 20, 4 << 20).unwrap();
    let _fixture = memory.reserve(1 << 20, MemoryClass::Ordinary).unwrap();
    let mut pages = Pages::new(&memory);
    let field = FieldDescriptor {
        owner: FieldOwner::Runtime,
        name: "deep.key.columns",
        shape: FieldShape::Map,
        destination: FieldDestination::State(StateNamespaceV1::Authorization),
    };
    let mut bytes = SEMANTIC_KEY_DOMAIN.to_vec();
    deep_key(8, &mut bytes);
    let key = CanonicalSemanticKeyRefV1::new(&bytes).unwrap();
    assert!(bytes.len() < 4096);
    let mut rows = Vec::new();
    {
        let mut value_io = io();
        let mut key_work = SemanticKeyBudget {
            bytes_remaining: 8 << 20,
        };
        let mut ctx = MemberRecordWriteContext {
            sink: &mut pages,
            memory: &memory,
            io: &mut value_io,
            key_work: &mut key_work,
            limits: MemberRecordLimitsV1 {
                semantic_bytes: 4096,
                payload_bytes: 4096,
                record_bytes: 8192,
            },
            cancelled: || false,
        };
        let metadata = write_field_metadata_v1(
            field,
            &mut PresenceRecordWriteContext {
                sink: &mut *ctx.sink,
                memory: ctx.memory,
                io: &mut *ctx.io,
                key_work: &mut *ctx.key_work,
                limits: PresenceRecordLimitsV1 {
                    semantic_bytes: 4096,
                    record_bytes: 8192,
                },
                cancelled: &mut ctx.cancelled,
            },
        )
        .unwrap();
        crate::incremental_field_write::emit(
            metadata.field_key,
            metadata.value_root,
            &mut ctx,
            &mut |row| {
                rows.push(row);
                Ok(())
            },
        )
        .unwrap();
        let row = write_member_record_v1(
            field,
            WorldFieldRecordKindV1::Entry,
            &key,
            MemberPayloadRefV1::Unit,
            &mut ctx,
        )
        .unwrap();
        crate::incremental_field_write::emit(row.field_key, row.value_root, &mut ctx, &mut |row| {
            rows.push(row);
            Ok(())
        })
        .unwrap();
    }
    let mut roots = RootSetV1::empty("deep-key".into(), [99; 32]).unwrap();
    install(&memory, &mut pages, &mut roots, &rows);
    let mut value_io = io();
    let mut keys = SemanticKeyBudget {
        bytes_remaining: 8 << 20,
    };
    let mut ctx = context(&pages, &roots, &memory, &mut value_io, &mut keys);
    ctx.semantic_bytes = 4096;
    ctx.record_bytes = 8192;
    assert!(matches!(
        read_columns_v1(
            ColumnReadProfileV1 {
                field,
                schema: [31; 32],
                groups: &[]
            },
            ctx.root,
            &mut ctx,
            &mut scan(),
            16 << 10
        ),
        Err(ValueTreeError::ResourceLimited(_))
    ));
}
