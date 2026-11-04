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
