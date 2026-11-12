//! Only authenticated changed registrations feed the resident contract directory.
use super::*;
use crate::{
    incremental_typed_record_diff::point_typed_column_v1,
    incremental_typed_record_diff::{TypedCellRefV1, key, root, visit_entries},
    world_rules::OrganizationRegistrationV1,
};
#[path = "incremental_world_rule_continuity_contracts_index.rs"]
pub(crate) mod index;
use index::{Builder, ContinuityContractIndexV1};

pub(super) fn spend(
    scan: &mut StateScanBudget,
    stop: &mut impl FnMut() -> bool,
    steps: u64,
    bytes: u64,
) -> Result<(), ValueTreeError> {
    if stop() {
        return Err(ValueTreeError::Cancelled);
    }
    scan.nodes_remaining =
        scan.nodes_remaining
            .checked_sub(steps)
            .ok_or(ValueTreeError::ResourceLimited(
                "organization resident metadata work",
            ))?;
    scan.read_bytes_remaining =
        scan.read_bytes_remaining
            .checked_sub(bytes)
            .ok_or(ValueTreeError::ResourceLimited(
                "organization resident metadata bytes",
            ))?;
    Ok(())
}
fn registrations<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    body: Option<&StateRootV1>,
    d: &mut ChangeDecoder<'_, '_, M, R, E, C>,
) -> Result<Option<ReservedValue<StateRootV1>>, ValueTreeError> {
    let Some(body) = body else {
        return Ok(None);
    };
    let p = d.profile;
    let cap = d.work.max_tree_heap;
    let row = point_typed_column_v1(&p, body, "registrations", d, cap)?;
    let ColumnCellV1::Root(root) = &row.cell else {
        return Err(invalid());
    };
    Ok(Some(
        d.ctx
            .values
            .memory
            .reserve(512, d.ctx.values.class)?
            .retain(root.clone()),
    ))
}
pub(super) fn update<
    T: CompositeBranchCodecV1,
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    parent: Option<&ContinuityContractIndexV1>,
    before: Option<&StateRootV1>,
    after: Option<&StateRootV1>,
    ctx: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
    scan: &mut StateScanBudget,
    work: &mut TypedRecordReadWorkV1,
) -> Result<Option<ContinuityContractIndexV1>, ValueTreeError> {
    if T::BRANCH != WorldRuleCompositeBranchV1::OrganizationContinuity {
        return Ok(None);
    }
    if before == after {
        if let Some(index) = parent {
            return Ok(Some(index.share(ctx.values.memory, ctx.values.class)?));
        }
    }
    let trees_remaining = work.max_retained_trees;
    let mut d = ChangeDecoder {
        ctx,
        scan,
        work,
        profile: T::BRANCH.profile(),
        trees_remaining,
        tree_depth: 0,
    };
    let old = registrations(before, &mut d)?;
    let new = registrations(after, &mut d)?;
    if old.as_deref() == new.as_deref() {
        if let Some(index) = parent {
            return Ok(Some(index.share(d.ctx.values.memory, d.ctx.values.class)?));
        }
    }
    let builder = if let Some(index) = parent {
        index.copy_for_changes(d.ctx.values.memory, d.ctx.values.class, &mut |s, b| {
            spend(d.scan, &mut d.ctx.cancelled, s, b)
        })?
    } else {
        Builder::new(d.ctx.values.memory, d.ctx.values.class)?
    };
    let empty = d
        .ctx
        .values
        .memory
        .reserve(512, d.ctx.values.class)?
        .retain(StateRootV1::empty(StateNamespaceV1::Authorization).map_err(|_| invalid())?);
    let mut builder = builder;
    visit_entries(
        old.as_deref().unwrap_or(&empty),
        new.as_deref().unwrap_or(&empty),
        &mut d,
        |old, new, d| {
            let Some(cell) = new.or(old) else {
                return Err(invalid());
            };
            let p = d.profile;
            // Full bootstrap includes the container's Metadata leaf. The
            // original column-key reader accepts Entry only; authenticate the
            // metadata with the original presence codec before visiting keys,
            // as the original full typed-container visitor does.
            let TypedCellRefV1::Entry(entry) = cell else {
                return Err(invalid());
            };
            let parsed =
                WorldFieldKeyV1::from_bytes(&entry.key).map_err(ValueTreeError::Invalid)?;
            if parsed.kind == WorldFieldRecordKindV1::Metadata {
                let expected =
                    WorldFieldKeyV1::new(p.field, WorldFieldRecordKindV1::Metadata, None)
                        .map_err(ValueTreeError::Invalid)?;
                for edge in [old, new].into_iter().flatten() {
                    let TypedCellRefV1::Entry(entry) = edge else {
                        return Err(invalid());
                    };
                    let binding =
                        WorldFieldKeyV1::from_bytes(&entry.key).map_err(ValueTreeError::Invalid)?;
                    if binding.semantic_key.is_some()
                        || entry.key != expected.as_bytes()
                        || entry.namespace != p.namespace()?
                    {
                        return Err(invalid());
                    }
                    d.ctx.records().presence(&entry.value, &expected, 0)?;
                }
                return d.check();
            }
            let name = key(&p, cell, d)?;
            let id = match &*name {
                ColumnKeyV1::Text(s) if s == "_length" => return Ok(()),
                ColumnKeyV1::Pair(kind, id) if matches!(&**kind,ColumnKeyV1::Text(s)if s=="entry") => {
                    match &**id {
                        ColumnKeyV1::Text(s) => s.as_str(),
                        _ => return Err(invalid()),
                    }
                }
                _ => return Err(invalid()),
            };
            if let Some(new) = new {
                let body = root(&p, new, d)?;
                let value = recover_typed_record_body_v1::<OrganizationRegistrationV1, _, _, _, _>(
                    &body, &p, d.ctx, d.scan, d.work,
                )?;
                builder.put(id, value.value(), &mut |s, b| {
                    spend(d.scan, &mut d.ctx.cancelled, s, b)
                })?;
            } else {
                builder.remove(id, &mut |s, b| spend(d.scan, &mut d.ctx.cancelled, s, b))?;
            }
            Ok(())
        },
    )?;
    Ok(Some(builder.finish()?))
}
