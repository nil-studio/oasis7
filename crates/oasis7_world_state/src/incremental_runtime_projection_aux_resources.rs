//! The resource family retains the original closed recursive record grammar.
use super::*;
pub(super) fn capture<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    selected: Selection<'_>,
    read: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
    work: &mut ReadWork<'_>,
    p: &Processing<'_>,
    build: &mut Build,
) -> Result<(), ValueTreeError> {
    crate::incremental_runtime_artifact_resources::project_actual_chain_resource_manifest_v1(
        selected, read, work, p, build,
    )
}
