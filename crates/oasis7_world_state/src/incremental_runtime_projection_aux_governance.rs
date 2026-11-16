//! Actual changed governance records, decoded by the original closed codec.
use super::*;
use crate::incremental_runtime_governance::{
    RecoveredRuntimeGovernanceRecordV1 as Record, RuntimeGovernanceKeyV1 as Key,
    RuntimeGovernanceKindV1 as Kind,
};
pub(super) fn capture<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    selected: Selection<'_>,
    index: usize,
    parent: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
    work: &mut ReadWork<'_>,
    p: &Processing<'_>,
    build: &mut Build,
) -> Result<(), ValueTreeError> {
    parent.check()?;
    let parsed = WorldFieldKeyV1::from_bytes(selected.key()).map_err(ValueTreeError::Invalid)?;
    let kind = match index {
        9 => Kind::Policy,
        10 => Kind::EpochSnapshot,
        11 => Kind::IdentityPenalty,
        _ => return Err(bad()),
    };
    if selected.namespace()
        != oasis7_formal_protocol::incremental_state::StateNamespaceV1::Authorization
    {
        return Err(bad());
    }
    let key = WorldFieldKeyV1::new(kind.descriptor(), parsed.kind, parsed.semantic_key)
        .map_err(ValueTreeError::Invalid)?;
    if key.as_bytes() != selected.key() {
        return Err(bad());
    }
    if parsed.kind == WorldFieldRecordKindV1::Metadata {
        return Ok(());
    }
    if parsed.kind != WorldFieldRecordKindV1::Entry {
        return Err(bad());
    }
    work.governance.cells_remaining = work
        .governance
        .cells_remaining
        .checked_sub(1)
        .ok_or_else(limit)?;
    // A deletion still decodes the real old outer row to recover its semantic key.
    let id = selected.after().or(selected.before()).ok_or_else(bad)?;
    let (identity, root) = crate::incremental_runtime_governance::projection::selected_root_v1(
        kind, &key, &id, parent,
    )?;
    let field = FIELDS[index];
    let (names, _names_fee) = match identity {
        Key::Policy => owned_path(&[], p, &build.memory, &mut parent.cancelled)?,
        Key::Epoch(id) | Key::Penalty(id) => {
            // JSON object keys are the original u64 serializer's decimal keys, not
            // reconstructed hashes or an inner record identifier.
            spend(p, 20, &mut parent.cancelled)?;
            let fee = build.memory.reserve(20, MemoryClass::Ordinary)?;
            let text = id.to_string();
            let result = owned_path(&[&text], p, &build.memory, &mut parent.cancelled)?;
            drop(fee);
            result
        }
    };
    if selected.after().is_none() {
        return build.value::<u8, _>(field, &names, None, p, &mut parent.cancelled);
    }
    let value = crate::incremental_runtime_governance::recover_runtime_governance_record_v1(
        kind,
        &root,
        parent,
        work.inner_scan,
        work.selected_heap,
    )?;
    match value.value() {
        Record::Policy(v) => build.value(field, &names, Some(v), p, &mut parent.cancelled),
        Record::EpochSnapshot(v) => build.value(field, &names, Some(v), p, &mut parent.cancelled),
        Record::IdentityPenalty(v) => build.value(field, &names, Some(v), p, &mut parent.cancelled),
    }
}
