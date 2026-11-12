//! Complete original pre-output industrial terminal checks. Only the fixed due
//! driver consumes this private result; no supplied body/root/boolean grants it.
use super::*;
use crate::world::{
    incremental_factory_directory_records::CurrentFactoryRecipeDirectoryV1,
    incremental_industrial_eligibility::{CurrentStateOwnerSourcesV1, IndustrialEligibilityWorkV1},
    incremental_normal_action_factory_schedule::{
        DueTerminalCurrentErrorV1, FactoryScheduleReadErrorV1, FactoryScheduleSourcesV1,
        FactoryScheduleWorkV1, check_due_ledger_destination_v1, with_factory_schedule_read_io_v1,
    },
};
use oasis7_formal_protocol::{
    controlled_authority::formal::bounded_context::FormalBoundedContextV1,
    incremental_value::{ValueNodeReader, ValueTreeError},
};
use oasis7_proto::memory_budget::MemoryClass;
use oasis7_world_state::{
    incremental_field_overlay::WorldFieldOverlayContextV1,
    incremental_field_read::WorldFieldReadContextV1,
    incremental_world_rules_family::WorldRulesQualificationV1,
};
use std::mem::size_of;
fn limit() -> BodyMutationError {
    ValueTreeError::ResourceLimited("industrial due terminal").into()
}
#[derive(Debug)]
struct DueTerminalCause {
    error: crate::world::incremental_industrial_eligibility::IndustrialEligibilityErrorV1,
    _fee: MemoryReservation,
}
impl std::fmt::Display for DueTerminalCause {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}", self.error)
    }
}
impl std::error::Error for DueTerminalCause {}
#[derive(Debug)]
struct DueReadCause(crate::WorldError);
impl std::fmt::Display for DueReadCause {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}", self.0)
    }
}
impl std::error::Error for DueReadCause {}
pub(super) enum IndustrialTerminalDecisionV1 {
    Ready(MemoryReservation),
    Blocked(ReservedValue<crate::WorldError>),
}
fn charge(ctx: &mut FormalBoundedContextV1<'_>, n: usize) -> Result<(), BodyMutationError> {
    ctx.charge(1, u64::try_from(n).map_err(|_| limit())?)
        .map_err(|e| BodyMutationError::Original(Box::new(e)))
}
fn ledger_bytes(ledger: &crate::MaterialLedgerId) -> usize {
    match ledger {
        crate::MaterialLedgerId::World => 0,
        crate::MaterialLedgerId::Agent(id)
        | crate::MaterialLedgerId::Site(id)
        | crate::MaterialLedgerId::Factory(id) => id.len(),
    }
}
fn blocked(phase: MemoryReservation, reason: &str) -> IndustrialTerminalDecisionV1 {
    IndustrialTerminalDecisionV1::Blocked(phase.retain(crate::WorldError::ResourceBalanceInvalid {
        reason: reason.into(),
    }))
}
#[derive(Debug)]
enum CapacityErrorV1 {
    World(crate::WorldError),
    Body(BodyMutationError),
}
impl From<crate::WorldError> for CapacityErrorV1 {
    fn from(e: crate::WorldError) -> Self {
        Self::World(e)
    }
}
impl From<BodyMutationError> for CapacityErrorV1 {
    fn from(e: BodyMutationError) -> Self {
        Self::Body(e)
    }
}
#[allow(clippy::too_many_arguments)]
pub(super) fn check_current_terminal_v1<M, P, R, V, E, C, PE, PC>(
    job: &ReservedValue<crate::RecipeJobState>,
    rules: &ReservedValue<WorldRulesQualificationV1>,
    orders: &CurrentStateOwnerSourcesV1<'_, '_>,
    reservations: &ReservedValue<CurrentFactoryRecipeDirectoryV1<'_>>,
    sources: &FactoryScheduleSourcesV1<'_>,
    ctx: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    parent: &mut WorldFieldReadContextV1<'_, P, V, PE, PC>,
    factory: &mut FactoryScheduleWorkV1,
    work: &mut IndustrialEligibilityWorkV1,
    common: &mut FormalBoundedContextV1<'_>,
    now: u64,
    live: &ExecutionFence,
) -> Result<IndustrialTerminalDecisionV1, BodyMutationError>
where
    M: MerkleNodeReader,
    P: MerkleNodeReader,
    R: ValueNodeReader,
    V: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    PE: FnMut() -> Option<ValueTreeError>,
    PC: FnMut() -> bool,
{
    if !job.uses_budget(common.memory()) || !reservations.uses_budget(common.memory()) {
        return Err(BodyMutationError::Invalid(
            "industrial terminal actual job/directory pool",
        ));
    }
    let phase_size = 256
        + size_of::<ReservedValue<crate::WorldError>>()
        + size_of::<ValidatedDueRecipeTerminalV1>()
        + size_of::<MemoryReservation>();
    charge(common, phase_size)?;
    let phase = common
        .memory()
        .reserve(phase_size, MemoryClass::Ordinary)
        .map_err(ValueTreeError::Memory)?;
    let selected = graph::select_industrial_root_v1(
        job,
        rules,
        sources.roots,
        parent.state,
        ctx,
        &mut work.scan,
        &mut work.typed,
        common,
        live,
    )?;
    let (source, root) = match selected {
        graph::SelectedIndustrialRootV1::Unlinked => {
            return Ok(IndustrialTerminalDecisionV1::Ready(phase));
        }
        graph::SelectedIndustrialRootV1::Rejected(error, fee) => {
            return Ok(IndustrialTerminalDecisionV1::Blocked(fee.retain(error)));
        }
        graph::SelectedIndustrialRootV1::Linked { source, root } => (source, root),
    };
    let root = root.value();
    let binding = owner::read_current_due_owner_binding_v1(
        &job.requester_agent_id,
        orders,
        ctx,
        parent,
        work,
        common,
        live,
    )
    .map_err(|e| match e {
        owner::DueOwnerReadErrorV1::Bounded(e) => e,
        owner::DueOwnerReadErrorV1::Source { error, _fee } => {
            BodyMutationError::Original(Box::new(DueTerminalCause { error, _fee }))
        }
    })?;
    if let Some(error) = disposition::check_current_disposition_v1(
        &source,
        root,
        binding.as_ref(),
        parent.state,
        ctx,
        &mut work.scan,
        &mut work.typed,
        common,
        live,
    )? {
        return Ok(IndustrialTerminalDecisionV1::Blocked(error));
    }
    match check_due_ledger_destination_v1(root, sources, ctx, parent, factory, common, now, live) {
        Ok(()) => {}
        Err(DueTerminalCurrentErrorV1::Body(e)) => return Err(e),
        Err(DueTerminalCurrentErrorV1::World {
            cause,
            _fees,
            mut _slots,
        }) => {
            // The original business cause already owns a paid diagnostic phase; move
            // all its true reservations into the returned error owner without cloning.
            for mut fee in _fees {
                _slots.merge(&mut fee).map_err(|_| limit())?;
            }
            let extra = size_of::<ReservedValue<crate::WorldError>>()
                .checked_sub(size_of::<crate::WorldError>())
                .ok_or_else(limit)?;
            charge(common, extra)?;
            let mut carrier = common
                .memory()
                .reserve(extra, MemoryClass::Ordinary)
                .map_err(ValueTreeError::Memory)?;
            _slots.merge(&mut carrier).map_err(|_| limit())?;
            return Ok(IndustrialTerminalDecisionV1::Blocked(_slots.retain(cause)));
        }
    }
    let Some(plan) = plan::original_builtin_plan_v1(root, common)? else {
        return Ok(blocked(phase, "industrial_completion_recipe_unavailable"));
    };
    let mut compare = 0usize;
    for stack in plan
        .produce
        .iter()
        .chain(&plan.byproducts)
        .chain(&job.produce)
        .chain(&job.byproducts)
    {
        charge(common, stack.kind.len())?;
        compare = compare.checked_add(stack.kind.len()).ok_or_else(limit)?;
    }
    charge(common, compare)?;
    if plan.produce != job.produce || plan.byproducts != job.byproducts {
        return Ok(blocked(phase, "industrial_completion_plan_conflict"));
    }
    let ledger = with_factory_schedule_read_io_v1(
        sources.roots,
        ctx,
        parent,
        factory,
        common,
        |ctx, parent, work| {
            let guard = ctx.transaction.guard();
            Ok(
                oasis7_world_state::incremental_inventory::recover_current_inventory_ledger_v1(
                    &root.output_ledger,
                    sources.inventory_directory,
                    sources.lifecycle.inventory,
                    sources.roots,
                    ctx,
                    parent,
                    &mut work.inventory_family,
                    &mut work.inventory_directory,
                    &guard,
                    live,
                )?,
            )
        },
    )
    .map_err(|e| match e {
        FactoryScheduleReadErrorV1::Body(e) => e,
        FactoryScheduleReadErrorV1::World(e) => {
            BodyMutationError::Original(Box::new(DueReadCause(e)))
        }
    })?;
    // The finite builtin output list bounds every original repeated scan. Debit
    // actual resident reservation text/slots and selected-ledger search work;
    // no pending job is reconstructed and no unrelated Cold branch is scanned.
    let repetitions = plan
        .produce
        .len()
        .checked_add(plan.byproducts.len())
        .ok_or_else(limit)?;
    for entry in reservations.iter_recipe_reservations_v1() {
        let mut bytes = ledger_bytes(entry.output_ledger)
            .checked_add(ledger_bytes(&root.output_ledger))
            .ok_or_else(limit)?;
        for stack in entry.produce.iter().chain(entry.byproducts) {
            bytes = bytes.checked_add(stack.kind.len()).ok_or_else(limit)?;
        }
        for _ in 0..repetitions {
            charge(common, bytes)?;
            for _ in entry.produce.iter().chain(entry.byproducts) {
                common
                    .consume_steps(1)
                    .map_err(|e| BodyMutationError::Original(Box::new(e)))?;
            }
        }
    }
    if let Some(ledger) = &*ledger {
        for (kind, _) in ledger {
            for _ in 0..repetitions {
                charge(common, kind.len())?;
            }
        }
    }
    for (kind, _) in &root.terminal_capacities {
        charge(common, kind.len())?;
    }
    let mut output_fee = None;
    let checked:Result<(),CapacityErrorV1>=oasis7_world_state::world_rules::validate_industrial_output_capacity_from_reservations_with_work_v1(root,&plan,Some(job.job_id),ledger.as_ref(),reservations.iter_recipe_reservations_v1(),&mut |plan|{
  let n=oasis7_world_state::world_rules::industrial_completion_output_map_heap_v1(plan).map_err(BodyMutationError::from)?;
  charge(common,n)?;output_fee=Some(common.memory().reserve(n,MemoryClass::Ordinary).map_err(ValueTreeError::Memory).map_err(BodyMutationError::from)?);Ok(())
 });
    match checked {
        Ok(()) => Ok(IndustrialTerminalDecisionV1::Ready(phase)),
        Err(CapacityErrorV1::Body(e)) => Err(e),
        Err(CapacityErrorV1::World(error)) => {
            Ok(IndustrialTerminalDecisionV1::Blocked(phase.retain(error)))
        }
    }
}
