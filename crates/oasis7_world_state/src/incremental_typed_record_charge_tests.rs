use super::*;
use crate::incremental_typed_record_test_support::{Pages, io};
fn profile() -> TypedRecordProfileV1 {
    TypedRecordProfileV1 {
        field: FieldDescriptor {
            owner: FieldOwner::Runtime,
            name: "typed_charge_test.columns",
            shape: FieldShape::Map,
            destination: FieldDestination::State(StateNamespaceV1::Business),
        },
        schema: [7; 32],
        reference_schema: None,
        labels: RecordTreeLabelsV1 {
            read: "charge test read",
            pending: "charge test pending",
            tree: "charge test tree",
            digest: "charge test digest",
            read_allowance: "charge test read allowance",
            admission: "charge test admission",
            bound: "charge test bound",
            producer_pool: "charge test pool",
            page: "charge test page",
            write_allowance: "charge test write allowance",
        },
    }
}
fn work() -> TypedRecordWriteWorkV1 {
    TypedRecordWriteWorkV1 {
        cells_remaining: 10000,
        max_pending_node_bytes: 4 << 20,
        json: crate::incremental_json_records::JsonWriteWorkV1 {
            cells_remaining: 10000,
            json_nodes_remaining: 100,
            max_json_depth: 16,
            max_pending_node_bytes: 4 << 20,
        },
    }
}
#[test]
fn root_carrier_retains_admission_and_foreign_builder_is_rejected_before_io() {
    let memory = MemoryBudget::new(32 << 20, 1 << 20).unwrap();
    let mut pages = Pages::new(&memory);
    let root = prepare_typed_record_body_v1(
        &BTreeMap::<String, u64>::new(),
        &profile(),
        &mut MemberRecordWriteContext {
            sink: &mut pages,
            memory: &memory,
            io: &mut io(),
            key_work: &mut crate::incremental_semantic_key::SemanticKeyBudget {
                bytes_remaining: 1 << 20,
            },
            limits: MemberRecordLimitsV1 {
                semantic_bytes: 1024,
                payload_bytes: 4096,
                record_bytes: 4096,
            },
            cancelled: || false,
        },
        &mut work(),
    )
    .unwrap();
    assert!(
        root.reserved_bytes() >= std::mem::size_of::<StateRootV1>() + root.node_root.capacity()
    );
    let paid = memory.used();
    let held = root.reserved_bytes();
    drop(root);
    assert_eq!(memory.used(), paid - held);
    let foreign = MemoryBudget::new(4096, 0).unwrap();
    let mut builder = TypedTreeBuilderV1::empty(StateNamespaceV1::Business, &foreign).unwrap();
    let n = pages.len();
    assert!(
        put(
            &profile(),
            &mut builder,
            "field",
            Encoded::Scalar(MemberPayloadRefV1::U64(1)),
            &mut MemberRecordWriteContext {
                sink: &mut pages,
                memory: &memory,
                io: &mut io(),
                key_work: &mut crate::incremental_semantic_key::SemanticKeyBudget {
                    bytes_remaining: 1 << 20
                },
                limits: MemberRecordLimitsV1 {
                    semantic_bytes: 1024,
                    payload_bytes: 4096,
                    record_bytes: 4096
                },
                cancelled: || false
            },
            &mut work()
        )
        .is_err()
    );
    assert_eq!(pages.len(), n);
    drop(builder);
    assert_eq!(foreign.used(), 0);
    let tiny = MemoryBudget::new(256, 0).unwrap();
    assert!(TypedTreeBuilderV1::empty(StateNamespaceV1::Business, &tiny).is_err());
    assert_eq!(tiny.used(), 0);
}
