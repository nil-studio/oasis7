use super::*;
use crate::{
    incremental_inventory_value_read::InventoryValueReader,
    incremental_semantic_key::SemanticKeyBudget,
};
use oasis7_formal_protocol::incremental_value::{ValueIoBudget, ValueNodeWriter, ValueReadContext};
use std::{cell::RefCell, collections::BTreeMap, sync::Arc};
struct Pages(RefCell<BTreeMap<[u8; 32], Arc<ReservedValue<Vec<u8>>>>>);
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
        let rows = self.0.borrow();
        let row = rows.get(id).ok_or_else(invalid)?;
        if row.len() > max {
            return Err(invalid());
        }
        Ok(Arc::new(
            memory.reserve(row.len(), class)?.retain(row.to_vec()),
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
fn read_work() -> JsonReadWorkV1 {
    JsonReadWorkV1 {
        cells_remaining: 100_000,
        json_nodes_remaining: 100_000,
        max_depth: 32,
        max_payload_bytes: 1 << 20,
    }
}
#[test]
fn retention_original_selected_paths_distinguish_missing_empty_and_wrong_container() {
    let memory = MemoryBudget::new(32 << 20, 4 << 20).unwrap();
    let mut pages = Pages(RefCell::new(BTreeMap::new()));
    let mut quota = io();
    let mut keys = SemanticKeyBudget {
        bytes_remaining: 64 << 20,
    };
    let mut stop = || false;
    let original = serde_json::json!({"horizon":7,"gc_floor":3,"records":{"original outer key":{"commit_record_id":"cognition-id-not-IWS-id","extra":[null,"kept"]}},"pins":{"empty":[],"ordered":["a","z"]},"response_artifacts":["artifact-a"]});
    let root = prepare_json_subtree_v1(
        profile(),
        &original,
        &mut MemberRecordWriteContext {
            sink: &mut pages,
            memory: &memory,
            io: &mut quota,
            key_work: &mut keys,
            limits: MemberRecordLimitsV1 {
                semantic_bytes: 1 << 20,
                payload_bytes: 1 << 20,
                record_bytes: 2 << 20,
            },
            cancelled: || false,
        },
        &mut JsonWriteWorkV1 {
            cells_remaining: 100_000,
            json_nodes_remaining: 100_000,
            max_json_depth: 32,
            max_pending_node_bytes: 1 << 20,
        },
    )
    .unwrap();
    let mut values = ValueReadContext {
        source: &pages,
        memory: &memory,
        class: MemoryClass::Ordinary,
        io: &mut quota,
    };
    let mut records = InventoryValueReader {
        values: &mut values,
        key_work: &mut keys,
        semantic_bytes: 1 << 20,
        record_bytes: 2 << 20,
        cancelled: &mut stop,
    };
    let q = crate::incremental_json_records::qualify_json_subtree_records_v1(
        profile(),
        &root,
        &mut records,
        &mut read_work(),
    )
    .unwrap();
    let absent = crate::incremental_json_records::qualify_optional_json_path_records_v1(
        profile(),
        &q,
        &[
            JsonPathSegmentV1::Name("pins"),
            JsonPathSegmentV1::Name("absent"),
        ],
        &mut records,
        &mut read_work(),
    )
    .unwrap();
    assert!(absent.is_none());
    let selected = crate::incremental_json_records::qualify_optional_json_path_records_v1(
        profile(),
        &q,
        &[
            JsonPathSegmentV1::Name("pins"),
            JsonPathSegmentV1::Name("empty"),
        ],
        &mut records,
        &mut read_work(),
    )
    .unwrap()
    .unwrap();
    let (empty, _fee) = crate::incremental_json_records::materialize_records(
        profile(),
        &selected,
        &mut records,
        &mut read_work(),
        1 << 20,
    )
    .unwrap();
    assert_eq!(empty, serde_json::json!([]));
    let wrong = crate::incremental_json_records::qualify_optional_json_path_records_v1(
        profile(),
        &q,
        &[
            JsonPathSegmentV1::Name("horizon"),
            JsonPathSegmentV1::Name("not-an-object"),
        ],
        &mut records,
        &mut read_work(),
    );
    assert!(wrong.is_err());
    let strict = crate::incremental_json_records::qualify_json_path_records_v1(
        profile(),
        &q,
        &[
            JsonPathSegmentV1::Name("pins"),
            JsonPathSegmentV1::Name("absent"),
        ],
        &mut records,
        &mut crate::incremental_json_records::JsonScalarPathReadWorkV1 {
            cells_remaining: 100_000,
            json_nodes_remaining: 100_000,
            max_depth: 32,
        },
    );
    assert!(matches!(
        strict,
        Err(MemberRecordError::Value(ValueTreeError::Invalid(
            "JSON point missing cell"
        )))
    ));
    let (roundtrip, _fee) = crate::incremental_json_records::materialize_records(
        profile(),
        &q,
        &mut records,
        &mut read_work(),
        1 << 20,
    )
    .unwrap();
    assert_eq!(roundtrip, original);
    let mut exhausted = read_work();
    exhausted.cells_remaining = 0;
    assert!(matches!(
        crate::incremental_json_records::qualify_optional_json_path_records_v1(
            profile(),
            &q,
            &[JsonPathSegmentV1::Name("pins")],
            &mut records,
            &mut exhausted
        ),
        Err(MemberRecordError::Value(ValueTreeError::ResourceLimited(_)))
    ));
}
