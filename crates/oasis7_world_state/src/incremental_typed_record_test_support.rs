//! Bounded, admitted page fixture shared by typed-record physical regressions.
use oasis7_formal_protocol::{
    incremental_state::{MerkleNodeReader, StatePageWriter},
    incremental_value::*,
};
use oasis7_proto::memory_budget::{MemoryBudget, MemoryClass, MemoryReservation, ReservedValue};
use std::{cell::RefCell, collections::BTreeMap, sync::Arc};
type PageMap = BTreeMap<[u8; 32], Arc<ReservedValue<Vec<u8>>>>;
pub(crate) struct Pages {
    memory: MemoryBudget,
    nodes: RefCell<PageMap>,
    max_pages: usize,
    _metadata: MemoryReservation,
}
pub(crate) struct Node(Arc<ReservedValue<Vec<u8>>>);
impl AsRef<[u8]> for Node {
    fn as_ref(&self) -> &[u8] {
        self.0.as_slice()
    }
}
impl Pages {
    pub(crate) fn new(memory: &MemoryBudget) -> Self {
        Self::with_limit(memory, 16_384)
    }
    pub(crate) fn with_limit(memory: &MemoryBudget, max_pages: usize) -> Self {
        let bytes = max_pages
            .checked_mul(256)
            .and_then(|n| n.checked_add(512))
            .expect("fixture metadata overflow");
        Self {
            memory: memory.clone(),
            nodes: RefCell::new(BTreeMap::new()),
            max_pages,
            _metadata: memory
                .reserve(bytes, MemoryClass::Ordinary)
                .expect("fixture metadata quota"),
        }
    }
    pub(crate) fn remove(&self, id: &[u8; 32]) {
        self.nodes.borrow_mut().remove(id);
    }
    pub(crate) fn len(&self) -> usize {
        self.nodes.borrow().len()
    }
}
impl Pages {
    fn put_shared(
        &self,
        id: [u8; 32],
        value: ReservedValue<Vec<u8>>,
    ) -> Result<(), ValueTreeError> {
        if !value.uses_budget(&self.memory) {
            return Err(ValueTreeError::Invalid("typed fixture foreign pool"));
        }
        let mut nodes = self.nodes.borrow_mut();
        if !nodes.contains_key(&id) && nodes.len() >= self.max_pages {
            return Err(ValueTreeError::ResourceLimited("typed fixture pages"));
        }
        nodes.insert(id, Arc::new(value));
        Ok(())
    }
}
impl ValueNodeWriter for Pages {
    fn put_node(
        &mut self,
        id: [u8; 32],
        value: ReservedValue<Vec<u8>>,
    ) -> Result<(), ValueTreeError> {
        self.put_shared(id, value)
    }
}
/// A scoped write handle permits authentic readers and writers to share the
/// same admitted physical store without cloning or duplicating cached pages.
pub(crate) struct Writer<'a>(pub(crate) &'a Pages);
impl ValueNodeWriter for Writer<'_> {
    fn put_node(
        &mut self,
        id: [u8; 32],
        value: ReservedValue<Vec<u8>>,
    ) -> Result<(), ValueTreeError> {
        self.0.put_shared(id, value)
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
impl ValueNodeReader for Pages {
    fn read_node(
        &self,
        id: &[u8; 32],
        max: usize,
        memory: &MemoryBudget,
        class: MemoryClass,
    ) -> Result<Arc<ReservedValue<Vec<u8>>>, ValueTreeError> {
        if class != MemoryClass::Ordinary || !memory.same_pool(&self.memory) {
            return Err(ValueTreeError::Invalid("typed fixture class/pool"));
        }
        let value = self
            .nodes
            .borrow()
            .get(id)
            .cloned()
            .ok_or(ValueTreeError::Invalid("typed fixture missing page"))?;
        if value.len() > max {
            return Err(ValueTreeError::Invalid("typed fixture page bound"));
        }
        Ok(value)
    }
}
impl StatePageWriter for Pages {}
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
                    Err("typed fixture state page bound".into())
                } else {
                    Ok(Node(n.clone()))
                }
            })
            .transpose()
    }
}
pub(crate) fn io() -> ValueIoBudget {
    ValueIoBudget {
        nodes_remaining: 10_000_000,
        read_bytes_remaining: 256 << 20,
        write_bytes_remaining: 256 << 20,
    }
}

use crate::{incremental_semantic_key::SemanticKeyBudget, incremental_typed_record_codec::*};
fn write_work() -> TypedRecordWriteWorkV1 {
    TypedRecordWriteWorkV1 {
        cells_remaining: 100_000,
        max_pending_node_bytes: 8 << 20,
        json: crate::incremental_json_records::JsonWriteWorkV1 {
            cells_remaining: 100_000,
            json_nodes_remaining: 100_000,
            max_json_depth: 32,
            max_pending_node_bytes: 8 << 20,
        },
    }
}
fn read_work() -> TypedRecordReadWorkV1 {
    TypedRecordReadWorkV1 {
        json: crate::incremental_json_records::JsonReadWorkV1 {
            cells_remaining: 100_000,
            json_nodes_remaining: 100_000,
            max_depth: 32,
            max_payload_bytes: 2 << 20,
        },
        max_retained_trees: 4096,
        max_tree_heap: 2 << 20,
    }
}
pub(crate) fn roundtrip<T: Codec + PartialEq + std::fmt::Debug>(
    source: &T,
    profile: TypedRecordProfileV1,
) {
    let memory = MemoryBudget::new(512 << 20, 8 << 20).unwrap();
    let mut pages = Pages::with_limit(&memory, 131_072);
    let root = prepare_typed_record_body_v1(
        source,
        &profile,
        &mut MemberRecordWriteContext {
            sink: &mut pages,
            memory: &memory,
            io: &mut io(),
            key_work: &mut SemanticKeyBudget {
                bytes_remaining: 32 << 20,
            },
            limits: MemberRecordLimitsV1 {
                semantic_bytes: 16384,
                payload_bytes: 2 << 20,
                record_bytes: 3 << 20,
            },
            cancelled: || false,
        },
        &mut write_work(),
    )
    .unwrap();
    let recovered = recover_typed_record_body_v1::<T, _, _, _, _>(
        &root,
        &profile,
        &mut WorldFieldReadContextV1 {
            root: &root,
            state: &pages,
            values: ValueReadContext {
                source: &pages,
                memory: &memory,
                class: MemoryClass::Ordinary,
                io: &mut io(),
            },
            key_work: &mut SemanticKeyBudget {
                bytes_remaining: 32 << 20,
            },
            semantic_bytes: 16384,
            record_bytes: 3 << 20,
            take_state_error: || None,
            cancelled: || false,
        },
        &mut StateScanBudget {
            nodes_remaining: 10_000_000,
            read_bytes_remaining: 512 << 20,
            leaves_remaining: 100_000,
        },
        &mut read_work(),
    )
    .unwrap();
    assert_eq!(recovered.value(), source);
    drop(recovered);
    // Full qualification has a small per-column workspace and never builds T.
    // Literal callers include text values larger than this entire workspace.
    let mut streaming_work = read_work();
    streaming_work.max_tree_heap = 64 << 10;
    let trees_remaining = streaming_work.max_retained_trees;
    crate::incremental_typed_record_diff::accept_body::<T, _, _, _, _>(
        &profile,
        None,
        Some(&root),
        &mut crate::incremental_typed_record_diff::ChangeDecoder {
            ctx: &mut WorldFieldReadContextV1 {
                root: &root,
                state: &pages,
                values: ValueReadContext {
                    source: &pages,
                    memory: &memory,
                    class: MemoryClass::Ordinary,
                    io: &mut io(),
                },
                key_work: &mut SemanticKeyBudget {
                    bytes_remaining: 32 << 20,
                },
                semantic_bytes: 16384,
                record_bytes: 3 << 20,
                take_state_error: || None,
                cancelled: || false,
            },
            scan: &mut StateScanBudget {
                nodes_remaining: 10_000_000,
                read_bytes_remaining: 512 << 20,
                leaves_remaining: 100_000,
            },
            work: &mut streaming_work,
            profile,
            trees_remaining,
            tree_depth: 0,
        },
    )
    .unwrap();
    // A complete new body needs explicit finite DTO admission, even though all
    // immutable pages already exist in the same admitted physical store.
    let mut tiny = read_work();
    tiny.max_tree_heap = 1;
    assert!(
        recover_typed_record_body_v1::<T, _, _, _, _>(
            &root,
            &profile,
            &mut WorldFieldReadContextV1 {
                root: &root,
                state: &pages,
                values: ValueReadContext {
                    source: &pages,
                    memory: &memory,
                    class: MemoryClass::Ordinary,
                    io: &mut io()
                },
                key_work: &mut SemanticKeyBudget {
                    bytes_remaining: 32 << 20
                },
                semantic_bytes: 16384,
                record_bytes: 3 << 20,
                take_state_error: || None,
                cancelled: || false
            },
            &mut StateScanBudget {
                nodes_remaining: 10_000_000,
                read_bytes_remaining: 512 << 20,
                leaves_remaining: 100_000
            },
            &mut tiny
        )
        .is_err()
    );
}

/// Before constructing bounded literal fixtures, retain this separate source
/// admission. These tests do not claim complete live-World source admission.
pub(crate) fn source_admission() -> MemoryReservation {
    MemoryBudget::new(8 << 20, 1 << 20)
        .unwrap()
        .reserve(4 << 20, MemoryClass::Ordinary)
        .unwrap()
}

#[path = "incremental_world_rule_continuity_test_data.rs"]
pub(crate) mod continuity_data;
