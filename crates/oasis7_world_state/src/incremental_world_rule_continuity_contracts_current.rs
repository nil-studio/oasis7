//! Selected organization predicate over resident metadata plus changed cases.
use super::*;
use crate::{
    incremental_field_overlay::{WorldFieldOverlayContextV1, WorldFieldOverlayErrorV1, lookup},
    incremental_inventory_value_read::InventoryValueReader,
    incremental_world_rule_composite_overlay::{
        WorldRuleCompositeOverlayV1, bind_current_world_rule_branch_v1,
    },
};
use oasis7_formal_protocol::incremental_execution::{
    ExecutionError, ExecutionFence, ExecutionTransaction, PreparedGuard,
};
use oasis7_formal_protocol::incremental_value::ValueReadContext;
type Continuity = crate::world_rules::OrganizationContinuityStateV1;
pub struct CurrentContinuityContractSourcesV1 {
    selected: ReservedValue<Vec<(String, String)>>,
    _index: continuity_contracts::index::ContinuityContractIndexV1,
    _proof: Option<ReservedValue<WorldRuleCompositeOverlayV1<Continuity>>>,
    guard: PreparedGuard,
    memory: MemoryBudget,
}
impl CurrentContinuityContractSourcesV1 {
    pub fn registrations(&self) -> impl Iterator<Item = (&str, &str)> {
        self.selected
            .iter()
            .map(|(outer, embedded)| (outer.as_str(), embedded.as_str()))
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
pub fn observe_current_continuity_contract_sources_v1<
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
) -> Result<ReservedValue<CurrentContinuityContractSourcesV1>, WorldFieldOverlayErrorV1> {
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
        let index = continuity_contracts::update::<Continuity, _, _, _, _>(
            parent.continuity_contracts.as_ref(),
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
            &mut |s, b| continuity_contracts::spend(scan, &mut read.cancelled, s, b),
        )?;
        drop(read);
        Ok(fee.retain(CurrentContinuityContractSourcesV1 {
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
