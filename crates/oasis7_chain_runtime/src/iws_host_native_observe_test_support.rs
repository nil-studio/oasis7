//! Original native-fixture allowances for the production accepted reader.
//! Only account construction is test-only; authentication and every read run
//! through the same shipping implementation.
use super::*;
use oasis7_distfs::incremental_page_store::PageIoBudget;
use oasis7_formal_protocol::{controlled_authority::formal::bounded_context::FormalBoundedContextV1, incremental_value::ValueIoBudget};
use oasis7_proto::cbor_decode_preflight::CborPreflightWorkV1;
use oasis7_world_state::{incremental_semantic_key::SemanticKeyBudget, incremental_world_event_decode::WorldEventDecodeWorkV1};
pub(crate) use super::read::{HostObservationV1, HostObservationWorkV1, observe_current_bounded};
use super::read::HostObservationPartsV1;
fn physical_allowance() -> PageIoBudget {
    PageIoBudget {
        reads_remaining: 65_536,
        read_bytes_remaining: 64 << 20,
        writes_remaining: 0,
        write_bytes_remaining: 0,
    }
}
fn logical_allowance() -> ValueIoBudget {
    // Range reads charge in-memory output to the logical write lane. Split
    // the original 64 MiB allowance; physical writes remain forbidden.
    ValueIoBudget {
        nodes_remaining: 65_536,
        read_bytes_remaining: 32 << 20,
        write_bytes_remaining: 32 << 20,
    }
}

impl HostObservationWorkV1 {
    pub(crate) fn new_bounded(common: &mut FormalBoundedContextV1<'_>) -> Result<Self,IwsHostRuntimeErrorV1> {
        Self::from_parts_bounded(HostObservationPartsV1 {
            state_pages: physical_allowance(),
            value_pages: physical_allowance(),
            original_pages: physical_allowance(),
            logical: logical_allowance(),
            original_values: logical_allowance(),
            keys: SemanticKeyBudget {
                bytes_remaining: 1 << 20,
            },
            event_decode: WorldEventDecodeWorkV1 {
                structure: CborPreflightWorkV1 {
                    bytes_remaining: 64 << 20,
                    values_remaining: 1_000_000,
                    max_depth: 64,
                },
                canonical: oasis7_wasm_abi::CanonicalCborWorkV1 {
                    visits_remaining: 1_000_000,
                    bytes_remaining: 64 << 20,
                    max_depth: 64,
                    max_map_keys: 4096,
                    max_key_bytes: 4096,
                },
                max_original_bytes: 16 << 20,
                max_heap_bytes: 32 << 20,
            },
            points_remaining: 128,
            events_remaining: 65_536,
            semantic_bytes: 4096,
            record_bytes: 4096,
            max_scalar_bytes: 64,
        }, common)
    }
}
