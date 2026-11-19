//! Selected result sources retain their exact authenticated outer entries.
use super::*;
use crate::{
    incremental_field_overlay::{WorldFieldOverlayContextV1, WorldFieldOverlayErrorV1, lookup},
    incremental_inventory_value_read::InventoryValueReader,
    incremental_typed_record_diff::{TypedCellRefV1, point_typed_entry_v1, root},
    incremental_world_rule_composite_overlay::{
        WorldRuleCompositeOverlayV1, bind_current_world_rule_branch_v1,
    },
};
use oasis7_formal_protocol::incremental_execution::{
    ExecutionError, ExecutionFence, ExecutionTransaction, PreparedGuard,
};
use oasis7_formal_protocol::incremental_value::ValueReadContext;
type Continuity = std::collections::BTreeMap<String, crate::world_rules::WorldRuleReceiptV1>;
pub struct CurrentRequestsResultSourcesV1 {
    selected: ReservedValue<Vec<requests_result::index::SelectedRequestV1>>,
    _index: requests_result::index::RequestsResultIndexV1,
    _proof: Option<ReservedValue<WorldRuleCompositeOverlayV1<Continuity>>>,
    guard: PreparedGuard,
    memory: MemoryBudget,
}
/// A borrowed selection from a paid, complete Current result directory. This
/// token exposes data only and cannot be used as an ordinary request insert.
pub struct CurrentRequestsResultSourceV1<'a> {
    selection: &'a CurrentRequestsResultSourcesV1,
    row: &'a requests_result::index::SelectedRequestV1,
}
impl CurrentRequestsResultSourceV1<'_> {
    pub fn outer_key(&self) -> &str {
        self.row.outer()
    }
}
pub struct CurrentRequestsResultReceiptV1 {
    value: RecoveredTypedRecordV1<crate::world_rules::WorldRuleReceiptV1>,
    _proof: ReservedValue<WorldRuleCompositeOverlayV1<Continuity>>,
}
impl CurrentRequestsResultReceiptV1 {
    pub fn value(&self) -> &crate::world_rules::WorldRuleReceiptV1 {
        self.value.value()
    }
    pub fn retained_allocation_bytes_v1(
        &self,
        steps: &mut u64,
        cancelled: impl FnMut() -> bool,
    ) -> Result<usize, ValueTreeError> {
        self.value.retained_allocation_bytes_v1(steps, cancelled)
    }
}
impl CurrentRequestsResultSourcesV1 {
    pub fn requests(&self) -> impl Iterator<Item = CurrentRequestsResultSourceV1<'_>> {
        self.selected
            .iter()
            .map(move |row| CurrentRequestsResultSourceV1 {
                selection: self,
                row,
            })
    }
    pub fn validate<M: MerkleNodeReader>(
        &self,
        tx: &ExecutionTransaction<'_, M>,
        memory: &MemoryBudget,
        live: &ExecutionFence,
    ) -> Result<(), WorldFieldOverlayErrorV1> {
        tx.validate_guard(&self.guard, live)?;
        if !self.memory.same_pool(memory) || !tx.uses_budget(memory) {
            return Err(ValueTreeError::Invalid("Current continuity risk pool").into());
        }
        Ok(())
    }
}
/// Reopen the selected original entry under the actual Current branch. Reads
/// may extend the read-set; any intervening mutation invalidates the original
/// selection. Normal request and V2 registry write gates remain separate.
#[allow(clippy::too_many_arguments)]
pub fn read_current_requests_result_receipt_v1<
    M: MerkleNodeReader,
    S: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    source: &CurrentRequestsResultSourceV1<'_>,
    parent: &ReservedValue<WorldRuleCompositeQualificationV1<Continuity>>,
    roots: &RootSetV1,
    state: &S,
    ctx: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    scan: &mut StateScanBudget,
    work: &mut TypedRecordReadWorkV1,
    guard: &PreparedGuard,
    live: &ExecutionFence,
) -> Result<ReservedValue<CurrentRequestsResultReceiptV1>, WorldFieldOverlayErrorV1> {
    let result = (|| {
        ctx.transaction.validate_guard(guard, live)?;
        ctx.transaction
            .validate_read_source_guard(&source.selection.guard, live)?;
        if !source.selection.memory.same_pool(ctx.values.memory)
            || !source.selection.selected.uses_budget(ctx.values.memory)
            || !ctx.transaction.uses_budget(ctx.values.memory)
            || ctx.values.class != MemoryClass::Ordinary
        {
            return Err(invalid().into());
        }
        let selected_body = source.selection._proof.as_ref().ok_or_else(invalid)?.body();
        let proof =
            bind_current_world_rule_branch_v1(parent, roots, state, ctx, scan, work, guard, live)?;
        if proof.body() != selected_body {
            return Err(ValueTreeError::Invalid("Current request result selection changed").into());
        }
        let fee = ctx
            .values
            .memory
            .reserve(
                std::mem::size_of::<CurrentRequestsResultReceiptV1>(),
                MemoryClass::Ordinary,
            )
            .map_err(ValueTreeError::Memory)?;
        let value = {
            let mut read = WorldFieldReadContextV1 {
                root: proof.body(),
                state,
                values: ValueReadContext {
                    source: ctx.values.source,
                    memory: ctx.values.memory,
                    class: ctx.values.class,
                    io: &mut *ctx.values.io,
                },
                key_work: &mut *ctx.key_work,
                semantic_bytes: ctx.semantic_bytes,
                record_bytes: ctx.record_bytes,
                take_state_error: &mut ctx.take_state_error,
                cancelled: &mut ctx.cancelled,
            };
            let p = WorldRuleCompositeBranchV1::Requests.profile();
            let trees_remaining = work.max_retained_trees;
            let mut d = ChangeDecoder {
                ctx: &mut read,
                scan,
                work,
                profile: p,
                trees_remaining,
                tree_depth: 0,
            };
            let selected =
                point_typed_entry_v1(&p, proof.body(), &("entry", source.row.outer()), &mut d)?;
            if selected.entry.value != source.row.value() {
                return Err(
                    ValueTreeError::Invalid("Current request result original changed").into(),
                );
            }
            let body = root(&p, &TypedCellRefV1::Entry(&selected.entry), &mut d)?;
            let value =
                recover_typed_record_body_v1::<crate::world_rules::WorldRuleReceiptV1, _, _, _, _>(
                    &body, &p, d.ctx, d.scan, d.work,
                )?;
            source.row.validate_receipt(value.value(), &mut |s, b| {
                requests_result::spend(d.scan, &mut d.ctx.cancelled, s, b)
            })?;
            d.check()?;
            value
        };
        ctx.transaction
            .validate_read_source_guard(&source.selection.guard, live)?;
        ctx.transaction
            .validate_guard(&ctx.transaction.guard(), live)?;
        Ok(fee.retain(CurrentRequestsResultReceiptV1 {
            value,
            _proof: proof,
        }))
    })();
    if result.is_err() {
        let _: Result<(), ExecutionError> =
            ctx.transaction.contribute(|_| Err(ExecutionError::Aborted));
    }
    result
}
#[allow(clippy::too_many_arguments)]
pub fn observe_current_requests_result_sources_v1<
    M: MerkleNodeReader,
    S: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    contract: &str,
    parent: &ReservedValue<WorldRuleCompositeQualificationV1<Continuity>>,
    roots: &RootSetV1,
    state: &S,
    ctx: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    scan: &mut StateScanBudget,
    work: &mut TypedRecordReadWorkV1,
    guard: &PreparedGuard,
    live: &ExecutionFence,
) -> Result<ReservedValue<CurrentRequestsResultSourcesV1>, WorldFieldOverlayErrorV1> {
    let result = (|| {
        ctx.transaction.validate_guard(guard, live)?;
        parent.check_budget(ctx.values.memory, MemoryClass::Ordinary)?;
        if !parent.uses_budget(ctx.values.memory)
            || parent.reserved_bytes() < 512
            || ctx.values.class != MemoryClass::Ordinary
            || !ctx.transaction.uses_budget(ctx.values.memory)
        {
            return Err(invalid().into());
        }
        let fee = ctx
            .values
            .memory
            .reserve(8192, MemoryClass::Ordinary)
            .map_err(ValueTreeError::Memory)?;
        parent.validate_binding(roots)?;
        if roots.digest().map_err(|_| invalid())? != ctx.transaction.parent_fence().parent_roots {
            return Err(invalid().into());
        }
        let present = {
            let mut records = InventoryValueReader {
                values: &mut ctx.values,
                key_work: &mut *ctx.key_work,
                semantic_bytes: ctx.semantic_bytes,
                record_bytes: ctx.record_bytes,
                cancelled: &mut ctx.cancelled,
            };
            let (field, present) = crate::incremental_world_rule_parent::read_parent_overlay(
                ctx.transaction,
                &mut records,
                &mut ctx.take_state_error,
            )?;
            if !present {
                let key = records
                    .key(
                        field,
                        WorldFieldRecordKindV1::Entry,
                        &(Continuity::BRANCH.name(), ()),
                    )?
                    .0;
                if lookup(
                    ctx.transaction,
                    &key,
                    &mut records,
                    &mut ctx.take_state_error,
                )?
                .is_some()
                {
                    return Err(invalid().into());
                }
            }
            present
        };
        let proof = if present {
            let guard = ctx.transaction.guard();
            Some(bind_current_world_rule_branch_v1(
                parent, roots, state, ctx, scan, work, &guard, live,
            )?)
        } else {
            None
        };
        let anchor = proof
            .as_ref()
            .map(|proof| proof.body())
            .or_else(|| parent.body())
            .unwrap_or(roots.state_root(StateNamespaceV1::Authorization));
        let mut read = WorldFieldReadContextV1 {
            root: anchor,
            state,
            values: ValueReadContext {
                source: ctx.values.source,
                memory: ctx.values.memory,
                class: ctx.values.class,
                io: &mut *ctx.values.io,
            },
            key_work: &mut *ctx.key_work,
            semantic_bytes: ctx.semantic_bytes,
            record_bytes: ctx.record_bytes,
            take_state_error: &mut ctx.take_state_error,
            cancelled: &mut ctx.cancelled,
        };
        let index = requests_result::update::<Continuity, _, _, _, _>(
            parent.requests_result.as_ref(),
            parent.body(),
            proof.as_ref().map(|proof| proof.body()),
            &mut read,
            scan,
            work,
        )?
        .ok_or_else(invalid)?;
        let selected = index.matching(
            contract,
            read.values.memory,
            MemoryClass::Ordinary,
            &mut |s, b| requests_result::spend(scan, &mut read.cancelled, s, b),
        )?;
        Ok(fee.retain(CurrentRequestsResultSourcesV1 {
            selected,
            _index: index,
            _proof: proof,
            guard: ctx.transaction.guard(),
            memory: ctx.values.memory.clone(),
        }))
    })();
    if result.is_err() {
        let _: Result<(), ExecutionError> =
            ctx.transaction.contribute(|_| Err(ExecutionError::Aborted));
    }
    result
}

#[cfg(test)]
#[path = "incremental_world_rule_requests_result_current_tests.rs"]
mod tests;
