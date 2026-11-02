//! Effects selects its own immutable JSON address/profile, never caller labels.
use super::*;
use crate::incremental_field_read::WorldFieldReadContextV1;
pub use crate::incremental_json_records::{
    JsonReadWorkV1 as RuntimeEffectJsonWorkV1,
    JsonSubtreeQualificationV1 as RuntimeEffectJsonQualificationV1,
};
use oasis7_formal_protocol::incremental_state::MerkleNodeReader;
use oasis7_proto::memory_budget::ReservedValue;
pub(super) mod cursor {
    pub(crate) use crate::incremental_json_records::cursor::Cursor;
}
pub fn inspect_runtime_effect_json_body_v1<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    root: &StateRootV1,
    ctx: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
    work: &mut RuntimeEffectJsonWorkV1,
) -> Result<ReservedValue<RuntimeEffectJsonQualificationV1>, ValueTreeError> {
    crate::incremental_json_records::qualify_json_subtree_v1(json_profile(), root, ctx, work)
}
