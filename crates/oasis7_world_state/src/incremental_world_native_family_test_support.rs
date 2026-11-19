use super::*;
pub(super) use crate::incremental_typed_record_test_support::{Pages, Writer, io};
use oasis7_formal_protocol::incremental_state::{StateDeltaV1, StateWriteV1};
use oasis7_formal_protocol::incremental_value::ValueReadContext;
pub(super) fn scan() -> StateScanBudget {
    StateScanBudget {
        nodes_remaining: 10000000,
        read_bytes_remaining: 256 << 20,
        leaves_remaining: 100000,
    }
}
pub(super) fn work() -> WorldNativeFieldReadWorkV1 {
    WorldNativeFieldReadWorkV1 {
        records_remaining: 100000,
        trees_remaining: 100000,
        max_key_heap: 1 << 20,
        max_pending_records: 4,
        typed: TypedRecordReadWorkV1 {
            json: crate::incremental_json_records::JsonReadWorkV1 {
                cells_remaining: 100000,
                json_nodes_remaining: 100000,
                max_depth: 64,
                max_payload_bytes: 2 << 20,
            },
            max_retained_trees: 4096,
            max_tree_heap: 64 << 10,
        },
    }
}
pub(super) fn write_work() -> TypedRecordWriteWorkV1 {
    TypedRecordWriteWorkV1 {
        cells_remaining: 100000,
        max_pending_node_bytes: 8 << 20,
        json: crate::incremental_json_records::JsonWriteWorkV1 {
            cells_remaining: 100000,
            json_nodes_remaining: 100000,
            max_json_depth: 64,
            max_pending_node_bytes: 8 << 20,
        },
    }
}
pub(super) fn rows<F: WorldNativeFieldSpecV1>(
    memory: &MemoryBudget,
    pages: &Pages,
    entries: &[(F::Key, F::Value)],
) -> Vec<ReservedValue<ExecutionWriteV1>> {
    let mut sink = Writer(pages);
    let mut quota = io();
    let mut keys = SemanticKeyBudget {
        bytes_remaining: 32 << 20,
    };
    let mut output = Vec::with_capacity(entries.len() + 1);
    let mut context = MemberRecordWriteContext {
        sink: &mut sink,
        memory,
        io: &mut quota,
        key_work: &mut keys,
        limits: MemberRecordLimitsV1 {
            semantic_bytes: 1 << 20,
            payload_bytes: 2 << 20,
            record_bytes: 3 << 20,
        },
        cancelled: || false,
    };
    let mut emit = |row| {
        output.push(row);
        Ok(())
    };
    let mut records = 100000;
    prepare_world_native_metadata_v1::<F>(&mut context, &mut records, &mut emit).unwrap();
    let mut work = write_work();
    for (key, value) in entries {
        prepare_world_native_record_v1::<F>(
            key,
            value,
            &mut context,
            &mut records,
            &mut work,
            &mut emit,
        )
        .unwrap();
    }
    output
}
pub(super) fn install(
    memory: &MemoryBudget,
    pages: &Pages,
    ns: StateNamespaceV1,
    rows: &[ReservedValue<ExecutionWriteV1>],
) -> RootSetV1 {
    let _phase = memory
        .reserve((rows.len() + 1) * (512 << 10), MemoryClass::Ordinary)
        .unwrap();
    let mut writes: Vec<StateWriteV1> = rows.iter().map(|r| r.mutation.clone()).collect();
    writes.sort_by(|a, b| a.key.cmp(&b.key));
    let delta =
        StateDeltaV1::build(StateRootV1::empty(ns).unwrap(), writes, pages, 8 << 20).unwrap();
    for node in &delta.new_nodes {
        Writer(pages)
            .put_node(
                *blake3::Hash::from_hex(&node.digest).unwrap().as_bytes(),
                memory
                    .reserve(node.bytes.len(), MemoryClass::Ordinary)
                    .unwrap()
                    .retain(node.bytes.clone()),
            )
            .unwrap();
    }
    let mut roots = RootSetV1::empty("native-field-component".into(), [73; 32]).unwrap();
    *roots.state.iter_mut().find(|r| r.namespace == ns).unwrap() = delta.after;
    roots
}
pub(super) fn context<'a>(
    memory: &'a MemoryBudget,
    pages: &'a Pages,
    root: &'a StateRootV1,
    quota: &'a mut oasis7_formal_protocol::incremental_value::ValueIoBudget,
    keys: &'a mut SemanticKeyBudget,
) -> WorldFieldReadContextV1<
    'a,
    Pages,
    Pages,
    impl FnMut() -> Option<ValueTreeError>,
    impl FnMut() -> bool,
> {
    WorldFieldReadContextV1 {
        root,
        state: pages,
        values: ValueReadContext {
            source: pages,
            memory,
            class: MemoryClass::Ordinary,
            io: quota,
        },
        key_work: keys,
        semantic_bytes: 1 << 20,
        record_bytes: 3 << 20,
        take_state_error: || None,
        cancelled: || false,
    }
}
