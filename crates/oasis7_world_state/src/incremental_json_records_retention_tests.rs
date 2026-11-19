//! Small selected JSONs must fit together under the same Ordinary budget.
use super::*;
#[test]
fn many_live_small_jsons_keep_only_consumed_output_admission() {
    let memory = MemoryBudget::new(8 << 20, 1 << 20).unwrap();
    let _index = memory.reserve(1 << 20, MemoryClass::Ordinary).unwrap();
    let _vector = memory.reserve(4096, MemoryClass::Ordinary).unwrap();
    let mut pages = Pages(RefCell::new(BTreeMap::new()));
    let profile = JsonRecordProfileV1 {
        field: FieldDescriptor {
            owner: FieldOwner::Runtime,
            name: "json.retention",
            shape: FieldShape::Map,
            destination: FieldDestination::State(StateNamespaceV1::Authorization),
        },
        schema: [39; 32],
    };
    let root = prepare_json_subtree_v1(
        profile,
        &Value::Null,
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
        &mut JsonWriteWorkV1 {
            cells_remaining: 100000,
            json_nodes_remaining: 1000,
            max_json_depth: 16,
            max_pending_node_bytes: 1 << 20,
        },
    )
    .unwrap();
    assert!(pages.0.borrow().len() < 4096);
    let mut quota = io();
    let mut keys = SemanticKeyBudget {
        bytes_remaining: 8 << 20,
    };
    let mut stop = || false;
    let mut records = InventoryValueReader {
        values: &mut ValueReadContext {
            source: &pages,
            memory: &memory,
            class: MemoryClass::Ordinary,
            io: &mut quota,
        },
        key_work: &mut keys,
        semantic_bytes: 4096,
        record_bytes: 2 << 20,
        cancelled: &mut stop,
    };
    let mut read_work = work();
    let proof =
        qualify_json_subtree_records_v1(profile, &root, &mut records, &mut read_work).unwrap();
    let before = memory.used();
    let mut held = Vec::with_capacity(8);
    for _ in 0..8 {
        let (value, charge) =
            materialize_records(profile, &proof, &mut records, &mut read_work, 1 << 20).unwrap();
        assert_eq!(value, Value::Null);
        assert!(charge.bytes() < 16 << 10);
        held.push((value, charge));
        assert_eq!(
            memory.used(),
            before + held.iter().map(|(_, c)| c.bytes()).sum::<usize>()
        );
    }
    while let Some((value, charge)) = held.pop() {
        drop(value);
        drop(charge);
        assert_eq!(
            memory.used(),
            before + held.iter().map(|(_, c)| c.bytes()).sum::<usize>()
        );
    }
    assert_eq!(memory.used(), before);
}
