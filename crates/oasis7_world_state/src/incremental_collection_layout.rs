//! Fixed collection layout bounds shared by actual bounded producers.
//! Returned sizes are data; they do not prove a caller's entry count or grant
//! any state/permission capability. Consumers count their actual inputs first.
use oasis7_formal_protocol::incremental_value::ValueTreeError;

/// Node storage for the locked Rust BTreeSet<String> implementation, excluding
/// String payloads (which the concrete producer must add from actual lengths).
pub fn string_set_output_heap_v1(entries: usize) -> Result<usize, ValueTreeError> {
    crate::incremental_typed_record_codec::btree_output_heap::<String, ()>(entries)
}

/// Node storage for a JSON object, excluding owned key and value payloads.
/// Concrete producers separately account for the recursively observed graph.
pub fn json_object_output_heap_v1(entries: usize) -> Result<usize, ValueTreeError> {
    crate::incremental_typed_record_codec::btree_output_heap::<String, serde_json::Value>(entries)
}
