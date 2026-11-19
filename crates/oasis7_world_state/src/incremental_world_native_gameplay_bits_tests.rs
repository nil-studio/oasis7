//! The sole raw JSON leaf family remains raw JSON, including signed float zero.
use super::*;
use crate::incremental_typed_record_test_support::{Pages, io};
use oasis7_formal_protocol::{
    incremental_state::StateScanBudget, incremental_value::ValueReadContext,
};
use oasis7_proto::memory_budget::{MemoryBudget, MemoryClass};
#[test]
fn native_module_subscription_cold_json_preserves_negative_zero_and_unsigned_width() {
    let _source = source_admission();
    let manifest = fixture::manifest();
    let source = &manifest.subscriptions[1];
    let profile = profile::<ModuleReleaseRequestsFieldV1>();
    let memory = MemoryBudget::new(96 << 20, 2 << 20).unwrap();
    let mut pages = Pages::with_limit(&memory, 32768);
    let root = prepare_typed_record_body_v1(
        source,
        &profile,
        &mut MemberRecordWriteContext {
            sink: &mut pages,
            memory: &memory,
            io: &mut io(),
            key_work: &mut SemanticKeyBudget {
                bytes_remaining: 4 << 20,
            },
            limits: MemberRecordLimitsV1 {
                semantic_bytes: 4096,
                payload_bytes: 1 << 20,
                record_bytes: 2 << 20,
            },
            cancelled: || false,
        },
        &mut TypedRecordWriteWorkV1 {
            cells_remaining: 100000,
            max_pending_node_bytes: 4 << 20,
            json: crate::incremental_json_records::JsonWriteWorkV1 {
                cells_remaining: 100000,
                json_nodes_remaining: 1000,
                max_json_depth: 32,
                max_pending_node_bytes: 4 << 20,
            },
        },
    )
    .unwrap();
    let recovered =
        recover_typed_record_body_v1::<oasis7_wasm_abi::ModuleSubscription, _, _, _, _>(
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
                    bytes_remaining: 4 << 20,
                },
                semantic_bytes: 4096,
                record_bytes: 2 << 20,
                take_state_error: || None,
                cancelled: || false,
            },
            &mut StateScanBudget {
                nodes_remaining: 1000000,
                read_bytes_remaining: 64 << 20,
                leaves_remaining: 10000,
            },
            &mut TypedRecordReadWorkV1 {
                json: crate::incremental_json_records::JsonReadWorkV1 {
                    cells_remaining: 100000,
                    json_nodes_remaining: 1000,
                    max_depth: 32,
                    max_payload_bytes: 1 << 20,
                },
                max_retained_trees: 128,
                max_tree_heap: 1 << 20,
            },
        )
        .unwrap();
    let json = recovered.value().filters.as_ref().unwrap();
    assert_eq!(
        json["negative_zero"].as_f64().unwrap().to_bits(),
        (-0.0f64).to_bits()
    );
    assert_eq!(json["unsigned"].as_u64(), Some(u64::MAX));
    assert_eq!(recovered.value().stage, source.stage);
}
