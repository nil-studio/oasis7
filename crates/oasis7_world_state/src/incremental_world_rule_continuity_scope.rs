//! Only changed continuity cases feed the paid resident organization directory.
use super::*;
use crate::{
    incremental_organization_scope_index::{
        OrganizationScopeFlagsV1, OrganizationScopeIndexBuilderV1, OrganizationScopeIndexV1,
        OrganizationScopeKindV1,
    },
    incremental_typed_record_diff::point_typed_column_v1,
    incremental_typed_record_diff::{TypedCellRefV1, key, root, visit_entries},
    world_rules::{OrganizationContinuityCaseV1, OrganizationContinuityPhaseV1},
};
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
fn cases<
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
    let row = point_typed_column_v1(&p, body, "cases", d, cap)?;
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
    parent: Option<&OrganizationScopeIndexV1>,
    before: Option<&StateRootV1>,
    after: Option<&StateRootV1>,
    ctx: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
    scan: &mut StateScanBudget,
    work: &mut TypedRecordReadWorkV1,
) -> Result<Option<OrganizationScopeIndexV1>, ValueTreeError> {
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
    let old = cases(before, &mut d)?;
    let new = cases(after, &mut d)?;
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
        OrganizationScopeIndexBuilderV1::new(d.ctx.values.memory, d.ctx.values.class)?
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
                let value = recover_typed_record_body_v1::<OrganizationContinuityCaseV1, _, _, _, _>(
                    &body, &p, d.ctx, d.scan, d.work,
                )?;
                let value = value.value();
                let active = !matches!(
                    value.phase,
                    OrganizationContinuityPhaseV1::Resolved
                        | OrganizationContinuityPhaseV1::Cancelled
                );
                builder.put(
                    OrganizationScopeKindV1::Continuity,
                    id,
                    &value.organization_id,
                    OrganizationScopeFlagsV1 {
                        active_case: active,
                        risk_frozen: active,
                        undisposed_estate: false,
                    },
                    &mut |s, b| spend(d.scan, &mut d.ctx.cancelled, s, b),
                )?;
            } else {
                builder.remove(OrganizationScopeKindV1::Continuity, id, &mut |s, b| {
                    spend(d.scan, &mut d.ctx.cancelled, s, b)
                })?;
            }
            Ok(())
        },
    )?;
    Ok(Some(builder.finish()?))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::incremental_semantic_key::SemanticKeyBudget;
    use crate::incremental_typed_record_test_support::{continuity_data, io, source_admission};
    use crate::incremental_world_rule_composite_family::test_support::{
        context, read_work, scan, seed,
    };
    type Continuity = crate::world_rules::OrganizationContinuityStateV1;
    #[test]
    fn some_empty_continuity_cases_authenticate_metadata_before_column_keys() {
        let _source = source_admission();
        let (memory, pages, roots) = seed(&Continuity::default());
        let mut io = io();
        let mut keys = SemanticKeyBudget {
            bytes_remaining: 32 << 20,
        };
        let mut read = context(&pages, &roots, &memory, &mut io, &mut keys);
        let qualification = qualify_world_rule_branch_v1::<Continuity, _, _, _, _>(
            &roots,
            &mut read,
            &mut scan(),
            &mut read_work(),
        )
        .unwrap();
        assert!(qualification.parent_present());
        let index = qualification
            .continuity_scope
            .as_ref()
            .expect("Some branch owns complete empty resident metadata");
        let mut budget = scan();
        let flags = index
            .observe(
                OrganizationScopeKindV1::Continuity,
                "not-an-organization",
                &memory,
                MemoryClass::Ordinary,
                &mut |s, b| spend(&mut budget, &mut || false, s, b),
            )
            .unwrap();
        assert!(!flags.active_case && !flags.risk_frozen && !flags.undisposed_estate);
    }
    #[test]
    fn some_nonempty_continuity_cases_preserve_outer_keys_and_embedded_organization() {
        let _source = source_admission();
        let mut state = Continuity::default();
        let mut active = continuity_data::fixture_OrganizationContinuityCaseV1();
        active.organization_id = "shared-organization".into();
        active.case_id = "different-embedded-case".into();
        let mut resolved = active.clone();
        resolved.phase = OrganizationContinuityPhaseV1::Resolved;
        state.cases.insert("active-outer-key".into(), active);
        state.cases.insert("resolved-outer-key".into(), resolved);
        let (memory, pages, roots) = seed(&state);
        let mut io = io();
        let mut keys = SemanticKeyBudget {
            bytes_remaining: 32 << 20,
        };
        let mut read = context(&pages, &roots, &memory, &mut io, &mut keys);
        let mut work = read_work();
        work.max_tree_heap = 1 << 20;
        let qualification = qualify_world_rule_branch_v1::<Continuity, _, _, _, _>(
            &roots,
            &mut read,
            &mut scan(),
            &mut work,
        )
        .unwrap();
        let index = qualification.continuity_scope.as_ref().unwrap();
        let mut budget = scan();
        let flags = index
            .observe(
                OrganizationScopeKindV1::Continuity,
                "shared-organization",
                &memory,
                MemoryClass::Ordinary,
                &mut |s, b| spend(&mut budget, &mut || false, s, b),
            )
            .unwrap();
        assert!(flags.active_case && flags.risk_frozen);
        let absent = index
            .observe(
                OrganizationScopeKindV1::Continuity,
                "active-outer-key",
                &memory,
                MemoryClass::Ordinary,
                &mut |s, b| spend(&mut budget, &mut || false, s, b),
            )
            .unwrap();
        assert!(!absent.active_case && !absent.risk_frozen);
        let mut changed = index
            .copy_for_changes(&memory, MemoryClass::Ordinary, &mut |s, b| {
                spend(&mut budget, &mut || false, s, b)
            })
            .unwrap();
        changed
            .remove(
                OrganizationScopeKindV1::Continuity,
                "active-outer-key",
                &mut |s, b| spend(&mut budget, &mut || false, s, b),
            )
            .unwrap();
        let changed = changed.finish().unwrap();
        let flags = changed
            .observe(
                OrganizationScopeKindV1::Continuity,
                "shared-organization",
                &memory,
                MemoryClass::Ordinary,
                &mut |s, b| spend(&mut budget, &mut || false, s, b),
            )
            .unwrap();
        assert!(
            !flags.active_case && !flags.risk_frozen,
            "resolved duplicate embedded organization remains after the active OUTER row is removed"
        );
    }
}
