//! Fixed Current continuity observation. This is data, never organization authority.
use super::*;
use crate::{
    incremental_field_overlay::{WorldFieldOverlayContextV1, WorldFieldOverlayErrorV1, lookup},
    incremental_inventory_value_read::InventoryValueReader,
    incremental_world_rule_composite_family::CompositeBranchCodecV1,
    incremental_world_rule_composite_overlay::{
        WorldRuleCompositeOverlayV1, bind_current_world_rule_branch_v1,
    },
};
use oasis7_formal_protocol::incremental_execution::{
    ExecutionError, ExecutionFence, ExecutionTransaction, PreparedGuard, RootSetV1,
};
use oasis7_proto::memory_budget::{MemoryBudget, MemoryClass, MemoryReservation};
use std::mem::size_of;
type State = crate::world_rules::OrganizationContinuityStateV1;
/// All value/closure charges and the actual Current body remain attached.
pub struct CurrentOrganizationContinuityV1 {
    proof: Option<ReservedValue<WorldRuleCompositeOverlayV1<State>>>,
    value: Option<RecoveredWorldRuleCompositeV1<State>>,
    identity: PreparedGuard,
    memory: MemoryBudget,
    _header: MemoryReservation,
}
impl CurrentOrganizationContinuityV1 {
    pub fn value(&self) -> Option<&State> {
        self.value.as_ref().map(|value| value.value())
    }
    pub(crate) fn proof(&self) -> Option<&ReservedValue<WorldRuleCompositeOverlayV1<State>>> {
        self.proof.as_ref()
    }
    pub fn validate<M: MerkleNodeReader>(
        &self,
        tx: &mut ExecutionTransaction<'_, M>,
        memory: &MemoryBudget,
        live: &ExecutionFence,
    ) -> Result<(), WorldFieldOverlayErrorV1> {
        tx.contribute(|tx| {
            tx.validate_guard(&self.identity, live)?;
            if !self.memory.same_pool(memory)
                || !tx.uses_budget(memory)
                || self.proof.is_some() != self.value.is_some()
                || self
                    .proof
                    .as_ref()
                    .is_some_and(|proof| !proof.uses_budget(memory))
            {
                return Err(ValueTreeError::Invalid("Current continuity source").into());
            }
            Ok(())
        })
    }
}
/// Read the actual parent marker before choosing a branch. In particular an
/// absent parent qualification is never substituted for a newly created
/// Current branch, and an actual absent branch has no fabricated overlay token.
#[allow(clippy::too_many_arguments)]
pub fn read_current_organization_continuity_v1<M, S, R, E, C>(
    parent: &ReservedValue<OrganizationContinuityQualificationV1>,
    roots: &RootSetV1,
    state: &S,
    context: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    scan: &mut StateScanBudget,
    work: &mut WorldRuleCompositeReadWorkV1,
    guard: &PreparedGuard,
    live: &ExecutionFence,
) -> Result<CurrentOrganizationContinuityV1, WorldFieldOverlayErrorV1>
where
    M: MerkleNodeReader,
    S: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
{
    let result = (|| {
        context.transaction.validate_guard(guard, live)?;
        parent.check_budget(context.values.memory, MemoryClass::Ordinary)?;
        if context.values.class != MemoryClass::Ordinary
            || !context.transaction.uses_budget(context.values.memory)
            || !parent.uses_budget(context.values.memory)
            || parent.reserved_bytes() < 512
        {
            return Err(ValueTreeError::Invalid("Current continuity parent source").into());
        }
        let _phase = context
            .values
            .memory
            .reserve(4096, MemoryClass::Ordinary)
            .map_err(ValueTreeError::Memory)?;
        roots.validate()?;
        parent.validate_binding(roots)?;
        if context.transaction.parent_fence().parent_roots != roots.digest()? {
            return Err(ValueTreeError::Invalid("Current continuity parent source").into());
        }
        let header = context
            .values
            .memory
            .reserve(
                size_of::<CurrentOrganizationContinuityV1>(),
                MemoryClass::Ordinary,
            )
            .map_err(ValueTreeError::Memory)?;
        let present = {
            let mut records = InventoryValueReader {
                values: &mut context.values,
                key_work: &mut *context.key_work,
                semantic_bytes: context.semantic_bytes,
                record_bytes: context.record_bytes,
                cancelled: &mut context.cancelled,
            };
            let (field, present) = crate::incremental_world_rule_parent::read_parent_overlay(
                context.transaction,
                &mut records,
                &mut context.take_state_error,
            )?;
            if !present {
                let key = records
                    .key(
                        field,
                        WorldFieldRecordKindV1::Entry,
                        &(State::BRANCH.name(), ()),
                    )?
                    .0;
                if lookup(
                    context.transaction,
                    &key,
                    &mut records,
                    &mut context.take_state_error,
                )?
                .is_some()
                {
                    return Err(ValueTreeError::Invalid("absent Current continuity branch").into());
                }
            }
            present
        };
        let (proof, value) = if present {
            // Only our own authenticated reads advanced the revision. The
            // binder opens and validates the actual Current edge again.
            let guard = context.transaction.guard();
            let proof = bind_current_world_rule_branch_v1(
                parent, roots, state, context, scan, work, &guard, live,
            )?;
            let mut read = WorldFieldReadContextV1 {
                root: proof.body(),
                state,
                values: oasis7_formal_protocol::incremental_value::ValueReadContext {
                    source: context.values.source,
                    memory: context.values.memory,
                    class: context.values.class,
                    io: &mut *context.values.io,
                },
                key_work: &mut *context.key_work,
                semantic_bytes: context.semantic_bytes,
                record_bytes: context.record_bytes,
                take_state_error: &mut context.take_state_error,
                cancelled: &mut context.cancelled,
            };
            let value = recover_typed_record_body_v1::<State, _, _, _, _>(
                proof.body(),
                &State::BRANCH.profile(),
                &mut read,
                scan,
                work,
            )?;
            read.check()?;
            drop(read);
            (Some(proof), Some(value))
        } else {
            (None, None)
        };
        if (context.cancelled)() {
            return Err(ValueTreeError::Cancelled.into());
        }
        Ok(CurrentOrganizationContinuityV1 {
            proof,
            value,
            identity: context.transaction.guard(),
            memory: context.values.memory.clone(),
            _header: header,
        })
    })();
    if result.is_err() {
        let _: Result<(), ExecutionError> = context
            .transaction
            .contribute(|_| Err(ExecutionError::Aborted));
    }
    result
}
