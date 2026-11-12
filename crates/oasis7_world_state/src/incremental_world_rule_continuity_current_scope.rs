//! Selected organization predicate over resident metadata plus changed cases.
use super::*;
use crate::{
    incremental_field_overlay::{WorldFieldOverlayContextV1, WorldFieldOverlayErrorV1, lookup},
    incremental_inventory_value_read::InventoryValueReader,
    incremental_organization_scope_index::{OrganizationScopeIndexV1, OrganizationScopeKindV1},
    incremental_world_rule_composite_overlay::{
        WorldRuleCompositeOverlayV1, bind_current_world_rule_branch_v1,
    },
};
use oasis7_formal_protocol::incremental_execution::{
    ExecutionError, ExecutionFence, ExecutionTransaction, PreparedGuard,
};
use oasis7_formal_protocol::incremental_value::ValueReadContext;
type Continuity = crate::world_rules::OrganizationContinuityStateV1;
pub struct CurrentContinuityRiskV1 {
    risk: bool,
    _index: OrganizationScopeIndexV1,
    _proof: Option<ReservedValue<WorldRuleCompositeOverlayV1<Continuity>>>,
    guard: PreparedGuard,
    memory: MemoryBudget,
}
impl CurrentContinuityRiskV1 {
    pub fn risk_frozen(&self) -> bool {
        self.risk
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
#[allow(clippy::too_many_arguments)]
pub fn observe_current_continuity_risk_v1<
    M: MerkleNodeReader,
    S: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    organization: &str,
    parent: &ReservedValue<WorldRuleCompositeQualificationV1<Continuity>>,
    roots: &RootSetV1,
    state: &S,
    ctx: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    scan: &mut StateScanBudget,
    work: &mut TypedRecordReadWorkV1,
    guard: &PreparedGuard,
    live: &ExecutionFence,
) -> Result<ReservedValue<CurrentContinuityRiskV1>, WorldFieldOverlayErrorV1> {
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
        let index = continuity_scope::update::<Continuity, _, _, _, _>(
            parent.continuity_scope.as_ref(),
            parent.body(),
            proof.as_ref().map(|proof| proof.body()),
            &mut read,
            scan,
            work,
        )?
        .ok_or_else(invalid)?;
        let flags = index.observe(
            OrganizationScopeKindV1::Continuity,
            organization,
            read.values.memory,
            MemoryClass::Ordinary,
            &mut |s, b| continuity_scope::spend(scan, &mut read.cancelled, s, b),
        )?;
        drop(read);
        Ok(fee.retain(CurrentContinuityRiskV1 {
            risk: flags.risk_frozen,
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
