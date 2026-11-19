//! Fixed-profile branch assembly reuses the admitted physical page fixture.
use crate::{
    incremental_semantic_key::SemanticKeyBudget,
    incremental_typed_record_codec::*,
    incremental_typed_record_test_support::{Pages, Writer, io},
    incremental_world_rule_composite_family::CompositeBranchCodecV1,
};
use oasis7_formal_protocol::{
    incremental_execution::{AuthorityBinding, ExecutionParent, RootSetV1, TransactionLimits},
    incremental_value::{ValueIoBudget, ValueReadContext},
};
use oasis7_proto::memory_budget::{MemoryBudget, MemoryClass};
pub(crate) fn write_work() -> TypedRecordWriteWorkV1 {
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
pub(crate) fn read_work() -> TypedRecordReadWorkV1 {
    TypedRecordReadWorkV1 {
        json: crate::incremental_json_records::JsonReadWorkV1 {
            cells_remaining: 100_000,
            json_nodes_remaining: 100_000,
            max_depth: 32,
            max_payload_bytes: 3 << 20,
        },
        max_retained_trees: 4096,
        max_tree_heap: 64 << 10,
    }
}
pub(crate) fn scan() -> StateScanBudget {
    StateScanBudget {
        nodes_remaining: 10_000_000,
        read_bytes_remaining: 512 << 20,
        leaves_remaining: 100_000,
    }
}
pub(crate) fn limits() -> TransactionLimits {
    TransactionLimits {
        max_reads: 64,
        max_writes: 16,
        max_facts: 8,
        max_pending_node_bytes: 8 << 20,
    }
}
pub(crate) fn parent(roots: &RootSetV1) -> ExecutionParent {
    ExecutionParent {
        commit: [1; 32],
        roots: roots.clone(),
        authority: AuthorityBinding {
            token: [2; 32],
            epoch: 3,
        },
    }
}
fn no_error() -> Option<ValueTreeError> {
    None
}
fn never() -> bool {
    false
}
pub(crate) fn context<'a>(
    pages: &'a Pages,
    roots: &'a RootSetV1,
    memory: &'a MemoryBudget,
    io: &'a mut ValueIoBudget,
    keys: &'a mut SemanticKeyBudget,
) -> WorldFieldReadContextV1<'a, Pages, Pages, fn() -> Option<ValueTreeError>, fn() -> bool> {
    WorldFieldReadContextV1 {
        root: roots.state_root(StateNamespaceV1::Authorization),
        state: pages,
        values: ValueReadContext {
            source: pages,
            memory,
            class: MemoryClass::Ordinary,
            io,
        },
        key_work: keys,
        semantic_bytes: 4096,
        record_bytes: 3 << 20,
        take_state_error: no_error,
        cancelled: never,
    }
}
pub(crate) fn seed<T: CompositeBranchCodecV1>(source: &T) -> (MemoryBudget, Pages, RootSetV1) {
    let memory = MemoryBudget::new(512 << 20, 8 << 20).unwrap();
    let mut pages = Pages::with_limit(&memory, 131_072);
    let profile = T::BRANCH.profile();
    let mut write_io = io();
    let mut keys = SemanticKeyBudget {
        bytes_remaining: 32 << 20,
    };
    let mut work = write_work();
    let mut output = MemberRecordWriteContext {
        sink: &mut pages,
        memory: &memory,
        io: &mut write_io,
        key_work: &mut keys,
        limits: MemberRecordLimitsV1 {
            semantic_bytes: 4096,
            payload_bytes: 2 << 20,
            record_bytes: 3 << 20,
        },
        cancelled: || false,
    };
    let field = crate::incremental_world_rule_parent::descriptor().unwrap();
    let mut outer = TypedTreeBuilderV1::empty(StateNamespaceV1::Authorization, &memory).unwrap();
    let metadata = write_field_metadata_v1(
        field,
        &mut PresenceRecordWriteContext {
            sink: &mut *output.sink,
            memory: &memory,
            io: &mut *output.io,
            key_work: &mut *output.key_work,
            limits: PresenceRecordLimitsV1 {
                semantic_bytes: 4096,
                record_bytes: 3 << 20,
            },
            cancelled: || false,
        },
    )
    .unwrap();
    append(
        &profile,
        &mut outer,
        &metadata.field_key,
        metadata.value_root,
        &mut output,
        &work,
    )
    .unwrap();
    let marker = write_member_record_v1(
        field,
        WorldFieldRecordKindV1::Entry,
        &(),
        MemberPayloadRefV1::Bool(true),
        &mut output,
    )
    .unwrap();
    append(
        &profile,
        &mut outer,
        &marker.field_key,
        marker.value_root,
        &mut output,
        &work,
    )
    .unwrap();
    let mut row = None;
    crate::incremental_world_rule_composite::prepare_branch_v1(
        T::BRANCH,
        source,
        &mut output,
        &mut work,
        &mut |next| {
            assert!(row.is_none());
            row = Some(next);
            Ok(())
        },
    )
    .unwrap();
    let row = row.unwrap();
    let parsed = WorldFieldKeyV1::from_bytes(&row.mutation.key).unwrap();
    let key = WorldFieldKeyV1::new(field, parsed.kind, parsed.semantic_key).unwrap();
    append(
        &profile,
        &mut outer,
        &key,
        row.mutation.after.unwrap(),
        &mut output,
        &work,
    )
    .unwrap();
    let mut roots = RootSetV1::empty("composite-family".into(), [9; 32]).unwrap();
    *roots
        .state
        .iter_mut()
        .find(|r| r.namespace == StateNamespaceV1::Authorization)
        .unwrap() = outer.root().clone();
    drop(output);
    (memory, pages, roots)
}
pub(crate) enum Key<'a> {
    Field(&'static str),
    Entry(&'a str),
}
pub(crate) fn selected<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    body: &StateRootV1,
    profile: &TypedRecordProfileV1,
    path: &[Key<'_>],
    ctx: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
) -> [u8; 32] {
    let _phase = ctx.values.memory.reserve(16384, ctx.values.class).unwrap();
    let mut current = ctx
        .values
        .memory
        .reserve(512, ctx.values.class)
        .unwrap()
        .retain(body.clone());
    for (index, key) in path.iter().enumerate() {
        let (key, _) = match key {
            Key::Field(name) => {
                ctx.records()
                    .key(profile.field, WorldFieldRecordKindV1::Entry, *name)
            }
            Key::Entry(id) => ctx.records().key(
                profile.field,
                WorldFieldRecordKindV1::Entry,
                &("entry", *id),
            ),
        }
        .unwrap();
        let id = ctx.lookup_raw(&current, key.as_bytes()).unwrap().unwrap();
        if index + 1 == path.len() {
            return id;
        }
        let entry = oasis7_formal_protocol::incremental_state::StateEntryV1 {
            namespace: StateNamespaceV1::Authorization,
            key: key.as_bytes().to_vec(),
            value: id,
        };
        let row = crate::incremental_record_columns_read::read_column_row_v1(
            ColumnReadProfileV1 {
                field: profile.field,
                schema: profile.schema,
                groups: &[],
                reference_schema: profile.reference_schema,
            },
            &entry,
            &mut ctx.records(),
            64 << 10,
        )
        .unwrap();
        let ColumnCellV1::Root(root) = &row.cell else {
            panic!("non-root path ancestor")
        };
        current = ctx
            .values
            .memory
            .reserve(512, ctx.values.class)
            .unwrap()
            .retain(root.clone());
    }
    panic!("empty selected path")
}
pub(crate) fn persist(
    pages: &Pages,
    memory: &MemoryBudget,
    nodes: &[oasis7_formal_protocol::incremental_state::StateNodeWriteV1],
) {
    for node in nodes {
        ValueNodeWriter::put_node(
            &mut Writer(pages),
            *blake3::Hash::from_hex(&node.digest).unwrap().as_bytes(),
            memory
                .reserve(node.bytes.len(), MemoryClass::Ordinary)
                .unwrap()
                .retain(node.bytes.clone()),
        )
        .unwrap();
    }
}

/// Recovery reads allocate fresh charged pages in the requested class, rather
/// than labelling the Ordinary cache's retained allocations as Recovery.
pub(crate) struct ClassedReader<'a> {
    pub(crate) pages: &'a Pages,
    pub(crate) memory: &'a MemoryBudget,
    pub(crate) class: MemoryClass,
    pub(crate) error: std::cell::RefCell<Option<ValueTreeError>>,
}
pub(crate) struct ClassedNode(std::sync::Arc<oasis7_proto::memory_budget::ReservedValue<Vec<u8>>>);
impl AsRef<[u8]> for ClassedNode {
    fn as_ref(&self) -> &[u8] {
        self.0.as_slice()
    }
}
impl ValueNodeReader for ClassedReader<'_> {
    fn read_node(
        &self,
        id: &[u8; 32],
        max: usize,
        memory: &MemoryBudget,
        class: MemoryClass,
    ) -> Result<std::sync::Arc<oasis7_proto::memory_budget::ReservedValue<Vec<u8>>>, ValueTreeError>
    {
        if class != self.class || !memory.same_pool(self.memory) {
            return Err(ValueTreeError::Invalid("classed fixture class/pool"));
        }
        let old = ValueNodeReader::read_node(self.pages, id, max, memory, MemoryClass::Ordinary)?;
        let size = old
            .len()
            .checked_add(256)
            .ok_or(ValueTreeError::Invalid("classed fixture bytes"))?;
        let charge = memory.reserve(size, class)?;
        let bytes = old.as_slice().to_vec();
        if bytes.capacity() > size {
            return Err(ValueTreeError::Invalid("classed fixture capacity"));
        }
        Ok(std::sync::Arc::new(charge.retain(bytes)))
    }
}
impl MerkleNodeReader for ClassedReader<'_> {
    type Node = ClassedNode;
    fn allocation_budget(&self) -> Option<&MemoryBudget> {
        Some(self.memory)
    }
    fn allocation_class(&self) -> Option<MemoryClass> {
        Some(self.class)
    }
    fn read_node(&self, id: &str, max: usize) -> Result<Option<Self::Node>, String> {
        let id = *blake3::Hash::from_hex(id)
            .map_err(|e| e.to_string())?
            .as_bytes();
        match ValueNodeReader::read_node(self, &id, max, self.memory, self.class) {
            Ok(node) => Ok(Some(ClassedNode(node))),
            Err(error) => {
                if self.error.borrow().is_none() {
                    *self.error.borrow_mut() = Some(error);
                }
                Err("classed fixture state failure".into())
            }
        }
    }
}
