//! Fixed selected governance decoder. The original outer-key and closed-row
//! kernels remain the only schema and identity interpreters.
use super::*;
use crate::incremental_field_read::WorldFieldReadContextV1;
use crate::incremental_world_field_key::WorldFieldKeyV1;
use oasis7_formal_protocol::incremental_state::{MerkleNodeReader, StateScanBudget};
pub(crate) fn recover_selected_v1<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    kind: RuntimeGovernanceKindV1,
    key: &WorldFieldKeyV1,
    id: &[u8; 32],
    ctx: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
    scan: &mut StateScanBudget,
    max_heap: usize,
) -> Result<
    (
        RuntimeGovernanceKeyV1,
        RecoveredColumnsV1<RecoveredRuntimeGovernanceRecordV1>,
    ),
    ValueTreeError,
> {
    let (identity, root) = family::child(kind, key, id, ctx)?;
    let value = recover_runtime_governance_record_v1(kind, &root, ctx, scan, max_heap)?;
    Ok((identity, value))
}

pub(crate) fn selected_root_v1<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    kind: RuntimeGovernanceKindV1,
    key: &WorldFieldKeyV1,
    id: &[u8; 32],
    ctx: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
) -> Result<(RuntimeGovernanceKeyV1, StateRootV1), ValueTreeError> {
    family::child(kind, key, id, ctx)
}
