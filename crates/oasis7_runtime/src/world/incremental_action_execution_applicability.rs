//! Current-overlay prerequisites for the deliberately narrow bootstrap lane.
//! Each prerequisite is checked from the sole transaction, never a caller flag.
use oasis7_formal_protocol::{
    incremental_state::MerkleNodeReader,
    incremental_value::{ValueNodeReader, ValueTreeError},
};
use oasis7_proto::memory_budget::ReservedValue;
use oasis7_world_state::{
    incremental_field_overlay::{WorldFieldOverlayContextV1, WorldFieldOverlayErrorV1},
    incremental_runtime_module_authorization::{
        ModuleAuthorizationFamilyQualificationV1, read_module_policy_length_overlay_v1,
    },
};

/// A nonempty current policy list needs the full rule dispatcher. This function
/// authenticates metadata, membership, and the actual current length even when
/// the qualified parent's list was empty.
pub(super) fn require_empty_current_module_policy<M, V, E, C>(
    qualification: &ReservedValue<ModuleAuthorizationFamilyQualificationV1>,
    overlay: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, V, E, C>,
) -> Result<(), WorldFieldOverlayErrorV1>
where
    M: MerkleNodeReader,
    V: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
{
    let length = read_module_policy_length_overlay_v1(qualification, overlay)?;
    overlay.transaction.contribute(|_| {
        if length != 0 {
            return Err(ValueTreeError::Invalid(
                "unsupported bootstrap action: nonempty current module policy",
            ).into());
        }
        Ok(())
    })
}
