//! Effects keeps its original fixed profile while sharing the JSON algorithm.
use super::*;
pub(super) fn body<W: StatePageWriter + ValueNodeReader, C: FnMut() -> bool>(
    value: &serde_json::Value,
    ctx: &mut MemberRecordWriteContext<'_, W, C>,
    work: &mut RuntimeEffectRecordsWorkV1,
) -> Result<StateRootV1, MemberRecordError> {
    let mut json = crate::incremental_json_records::JsonWriteWorkV1 {
        cells_remaining: work.cells_remaining,
        json_nodes_remaining: work.json_nodes_remaining,
        max_json_depth: work.max_json_depth,
        max_pending_node_bytes: work.max_pending_node_bytes,
    };
    let result = crate::incremental_json_records::prepare_json_subtree_v1(
        json_profile(),
        value,
        ctx,
        &mut json,
    );
    // Preserve consumed work on errors as well as successful subtree completion.
    work.cells_remaining = json.cells_remaining;
    work.json_nodes_remaining = json.json_nodes_remaining;
    result
}
