use super::*;
use crate::incremental_inventory_value_read::InventoryValueReader;
use crate::incremental_semantic_key::SemanticKeyBudget;
use oasis7_formal_protocol::incremental_value::{ValueIoBudget, ValueReadContext};
use oasis7_proto::memory_budget::MemoryBudget;
use std::{cell::RefCell, collections::BTreeMap, sync::Arc};
type PageMap = BTreeMap<[u8; 32], Arc<ReservedValue<Vec<u8>>>>;
struct Pages(RefCell<PageMap>);
impl ValueNodeWriter for Pages {
    fn put_node(
        &mut self,
        id: [u8; 32],
        bytes: ReservedValue<Vec<u8>>,
    ) -> Result<(), ValueTreeError> {
        self.0.borrow_mut().insert(id, Arc::new(bytes));
        Ok(())
    }
}
impl StatePageWriter for Pages {}
impl ValueNodeReader for Pages {
    fn read_node(
        &self,
        id: &[u8; 32],
        max: usize,
        memory: &MemoryBudget,
        class: MemoryClass,
    ) -> Result<Arc<ReservedValue<Vec<u8>>>, ValueTreeError> {
        let pages = self.0.borrow();
        let bytes = pages
            .get(id)
            .ok_or(ValueTreeError::Invalid("selected fixture missing"))?;
        if bytes.len() > max {
            return Err(invalid());
        }
        Ok(Arc::new(
            memory.reserve(bytes.len(), class)?.retain(bytes.to_vec()),
        ))
    }
}
fn io() -> ValueIoBudget {
    ValueIoBudget {
        nodes_remaining: 1_000_000,
        read_bytes_remaining: 128 << 20,
        write_bytes_remaining: 128 << 20,
    }
}
fn work() -> JsonReadWorkV1 {
    JsonReadWorkV1 {
        cells_remaining: 100_000,
        json_nodes_remaining: 1000,
        max_depth: 16,
        max_payload_bytes: 1 << 20,
    }
}
#[test]
fn selected_json_materialization_matches_original_and_preserves_sha_json_bytes() {
    let memory = MemoryBudget::new(32 << 20, 4 << 20).unwrap();
    let mut pages = Pages(RefCell::new(BTreeMap::new()));
    let profile = JsonRecordProfileV1 {
        field: FieldDescriptor {
            owner: FieldOwner::Runtime,
            name: "selected.fixture",
            shape: FieldShape::Map,
            destination: FieldDestination::State(StateNamespaceV1::Artifacts),
        },
        schema: [27; 32],
    };
    let original = serde_json::json!({"": [null, true, false, -7, u64::MAX, 1.5, -0.0, "原🙂\u{0000}"], "nested": {"b": [], "a": {}}});
    let root = prepare_json_subtree_v1(
        profile,
        &original,
        &mut MemberRecordWriteContext {
            sink: &mut pages,
            memory: &memory,
            io: &mut io(),
            key_work: &mut SemanticKeyBudget {
                bytes_remaining: 8 << 20,
            },
            limits: MemberRecordLimitsV1 {
                semantic_bytes: 1 << 20,
                payload_bytes: 1 << 20,
                record_bytes: 2 << 20,
            },
            cancelled: || false,
        },
        &mut JsonWriteWorkV1 {
            cells_remaining: 100_000,
            json_nodes_remaining: 1000,
            max_json_depth: 16,
            max_pending_node_bytes: 1 << 20,
        },
    )
    .unwrap();
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
        semantic_bytes: 1 << 20,
        record_bytes: 2 << 20,
        cancelled: &mut stop,
    };
    let proof = qualify_json_subtree_records_v1(profile, &root, &mut records, &mut work()).unwrap();
    let (value, _charge) =
        materialize_records(profile, &proof, &mut records, &mut work(), 1 << 20).unwrap();
    assert_eq!(value, original);
    assert_eq!(
        serde_json::to_vec(&value).unwrap(),
        serde_json::to_vec(&original).unwrap()
    );
    assert!(matches!(
        materialize_records(profile, &proof, &mut records, &mut work(), 0),
        Err(ValueTreeError::ResourceLimited(_))
    ));
    let mut short = work();
    short.json_nodes_remaining = 0;
    assert!(matches!(
        materialize_records(profile, &proof, &mut records, &mut short, 1 << 20),
        Err(ValueTreeError::ResourceLimited(_))
    ));
}

#[path = "incremental_json_records_retention_tests.rs"]
mod retention;

fn value_profile() -> JsonRecordProfileV1 {
    JsonRecordProfileV1 {
        field: FieldDescriptor {
            owner: FieldOwner::Cognition,
            name: "json.value.fixture",
            shape: FieldShape::Value,
            destination: FieldDestination::State(StateNamespaceV1::Artifacts),
        },
        schema: [41; 32],
    }
}

fn prepare_profile_fixture(
    profile: JsonRecordProfileV1,
    value: &Value,
    pages: &mut Pages,
    memory: &MemoryBudget,
) -> Result<StateRootV1, MemberRecordError> {
    prepare_json_subtree_v1(
        profile,
        value,
        &mut MemberRecordWriteContext {
            sink: pages,
            memory,
            io: &mut io(),
            key_work: &mut SemanticKeyBudget {
                bytes_remaining: 8 << 20,
            },
            limits: MemberRecordLimitsV1 {
                semantic_bytes: 1 << 20,
                payload_bytes: 1 << 20,
                record_bytes: 2 << 20,
            },
            cancelled: || false,
        },
        &mut JsonWriteWorkV1 {
            cells_remaining: 100_000,
            json_nodes_remaining: 1000,
            max_json_depth: 16,
            max_pending_node_bytes: 1 << 20,
        },
    )
}

#[test]
fn value_profile_roundtrip_and_readmission_keep_exact_owner_binding() {
    let memory = MemoryBudget::new(32 << 20, 4 << 20).unwrap();
    let mut pages = Pages(RefCell::new(BTreeMap::new()));
    let profile = value_profile();
    // An original Value can contain every JSON kind. Its descriptor remains
    // Value even when the actual root happens to be an object.
    let original = serde_json::json!({"": [null, true, false, -7, u64::MAX, 1.5, "原🙂\u{0000}"], "empty": {}, "array": []});
    let root = prepare_profile_fixture(profile, &original, &mut pages, &memory).unwrap();
    let mut quota = io();
    let mut keys = SemanticKeyBudget {
        bytes_remaining: 8 << 20,
    };
    let mut stop = || false;
    let mut records = InventoryValueReader {
        values: &mut ValueReadContext {
            source: &pages,
            memory: &memory,
            class: MemoryClass::Recovery,
            io: &mut quota,
        },
        key_work: &mut keys,
        semantic_bytes: 1 << 20,
        record_bytes: 2 << 20,
        cancelled: &mut stop,
    };
    let proof = qualify_json_subtree_records_v1(profile, &root, &mut records, &mut work()).unwrap();
    let (value, _charge) =
        materialize_records(profile, &proof, &mut records, &mut work(), 1 << 20).unwrap();
    assert_eq!(value, original);
    assert_eq!(
        serde_json::to_vec(&value).unwrap(),
        serde_json::to_vec(&original).unwrap()
    );
    let ordinary =
        copy_json_subtree_metadata_ordinary_v1(&proof, profile, &memory, || false).unwrap();
    ordinary.validate_binding(profile, &root).unwrap();
    assert!(ordinary.uses_budget(&memory));
    assert_eq!(ordinary.exact_node_count(), proof.exact_node_count());

    // Both supported shapes retain distinct authenticated keys; accepting
    // Value must not let a Map profile or another owner reuse its proof.
    let mut wrong = [profile; 6];
    wrong[0].field.shape = FieldShape::Map;
    wrong[1].field.shape = FieldShape::NestedMap;
    wrong[2].field.owner = FieldOwner::Runtime;
    wrong[3].field.name = "json.other.fixture";
    wrong[4].field.destination = FieldDestination::State(StateNamespaceV1::Business);
    wrong[5].schema = [42; 32];
    for foreign in wrong {
        assert!(matches!(
            qualify_json_subtree_records_v1(foreign, &root, &mut records, &mut work()),
            Err(ValueTreeError::Invalid(_))
        ));
        assert!(matches!(
            materialize_records(foreign, &proof, &mut records, &mut work(), 1 << 20),
            Err(ValueTreeError::Invalid(_))
        ));
        assert!(matches!(
            copy_json_subtree_metadata_ordinary_v1(&proof, foreign, &memory, || false),
            Err(ValueTreeError::Invalid(_))
        ));
    }
    assert!(matches!(
        materialize_records(profile, &proof, &mut records, &mut work(), 0),
        Err(ValueTreeError::ResourceLimited(_))
    ));
    let mut short = work();
    short.json_nodes_remaining = 0;
    assert!(matches!(
        qualify_json_subtree_records_v1(profile, &root, &mut records, &mut short),
        Err(ValueTreeError::ResourceLimited(_))
    ));
}

#[test]
fn json_profile_rejects_unsupported_shape_or_destination_before_writing() {
    let memory = MemoryBudget::new(32 << 20, 4 << 20).unwrap();
    let mut pages = Pages(RefCell::new(BTreeMap::new()));
    let state = FieldDestination::State(StateNamespaceV1::Artifacts);
    for (shape, destination) in [
        (FieldShape::NestedMap, state),
        (FieldShape::Set, state),
        (FieldShape::MapSet, state),
        (FieldShape::Ordered, state),
        (FieldShape::ByteMap, state),
        (FieldShape::History, state),
        (FieldShape::Process, state),
        (FieldShape::Children, state),
        (FieldShape::Value, FieldDestination::History),
        (FieldShape::Value, FieldDestination::ProofLedger),
        (FieldShape::Value, FieldDestination::RetentionLedger),
        (FieldShape::Value, FieldDestination::Process),
        (FieldShape::Value, FieldDestination::Children),
    ] {
        let mut profile = value_profile();
        profile.field.shape = shape;
        profile.field.destination = destination;
        assert!(matches!(
            prepare_profile_fixture(profile, &Value::Null, &mut pages, &memory),
            Err(MemberRecordError::Value(ValueTreeError::Invalid(
                "JSON node profile"
            )))
        ));
        assert!(pages.0.borrow().is_empty());
    }
    assert_eq!(memory.used(), 0);
}
