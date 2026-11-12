//! Whole Current failure commit; caps are minted only after all original writes.
use super::super::recipe_commit::{self, *};
use super::*;
use crate::world::{
    incremental_normal_action_factory_schedule::{FactoryScheduleSourcesV1, FactoryScheduleWorkV1},
    incremental_step::IwsStepContext,
};
use oasis7_formal_protocol::{
    incremental_state::StatePageWriter,
    incremental_value::{ValueNodeReader, ValueNodeWriter, ValueReadContext, ValueTreeError},
};
use oasis7_proto::memory_budget::MemoryClass;
use oasis7_world_state::{
    incremental_field_member::MemberRecordWriteContext,
    incremental_field_overlay::WorldFieldOverlayContextV1,
    incremental_field_read::WorldFieldReadContextV1, incremental_industry_jobs::*,
};
fn invalid() -> BodyMutationError {
    BodyMutationError::Invalid("industrial failure Current commit")
}
macro_rules! context {
    ($step:expr,$read:expr) => {
        WorldFieldOverlayContextV1 {
            transaction: &mut *$step.transaction,
            values: ValueReadContext {
                source: $read.values.source,
                memory: $read.values.memory,
                class: $read.values.class,
                io: &mut *$read.values.io,
            },
            key_work: &mut *$read.key_work,
            semantic_bytes: $read.semantic_bytes,
            record_bytes: $read.record_bytes,
            take_state_error: &mut $read.take_state_error,
            cancelled: &mut $read.cancelled,
        }
    };
}
fn rejection_receipt<'a>(
    cause: &'a DueRecipeFailureCauseV1<'_>,
) -> Option<&'a crate::ProductValidationReceiptV1> {
    match cause {
        DueRecipeFailureCauseV1::IndustrialTerminal { .. } => None,
        DueRecipeFailureCauseV1::ProductValidation { receipt, .. } => Some(receipt),
        DueRecipeFailureCauseV1::PersistedProductValidation { history } => history.receipt(),
    }
}
struct CountText<'a,'b> { n:usize, common:&'a mut oasis7_formal_protocol::controlled_authority::formal::bounded_context::FormalBoundedContextV1<'b>, error:Option<BodyMutationError> }
impl std::fmt::Write for CountText<'_, '_> {
    fn write_str(&mut self, s: &str) -> std::fmt::Result {
        if let Err(error) = self.common.charge(1, s.len() as u64) {
            self.error = Some(BodyMutationError::Original(Box::new(error)));
            return Err(std::fmt::Error);
        }
        self.n = self.n.checked_add(s.len()).ok_or(std::fmt::Error)?;
        Ok(())
    }
}
pub(in crate::world::incremental_normal_industry_due) fn commit_current_due_recipe_failure_v1<
    M,
    P,
    V,
    W,
    D,
    E,
    C,
    PE,
    PC,
    CW,
    ME,
>(
    source: &PreparedDueRecipeFailureV1<'_>,
    step: &mut IwsStepContext<'_, '_, '_, '_, M, V, W, D>,
    materials: &mut crate::world::incremental_body_event::materials::CorePolicyMaterialContextV1<
        '_,
        M,
        ME,
    >,
    sources: &FactoryScheduleSourcesV1<'_>,
    read: &mut WorldFieldReadContextV1<'_, P, V, E, C>,
    parent: &mut WorldFieldReadContextV1<'_, P, V, PE, PC>,
    write: &mut MemberRecordWriteContext<'_, W, CW>,
    work: &mut FactoryScheduleWorkV1,
    common: &mut oasis7_formal_protocol::controlled_authority::formal::bounded_context::FormalBoundedContextV1<'_>,
    live: &ExecutionFence,
) -> Result<Option<PreparedNormalRecipeFailureV1>, BodyMutationError>
where
    M: MerkleNodeReader,
    P: MerkleNodeReader,
    V: ValueNodeReader,
    W: ValueNodeReader + ValueNodeWriter + StatePageWriter,
    D: FnMut() -> bool,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    PE: FnMut() -> Option<ValueTreeError>,
    PC: FnMut() -> bool,
    CW: FnMut() -> bool,
    ME: FnMut() -> Option<ValueTreeError>,
{
    use std::fmt::Write as _;
    let now = step.time();
    let identity = common.work().identity();
    let mut binding_bytes = source.pending.requester_agent_id.len();
    if let Some(receipt) = rejection_receipt(&source.cause) {
        for text in [
            &receipt.requester_agent_id,
            &receipt.stack.kind,
            &receipt.decision.product_id,
        ] {
            binding_bytes = binding_bytes.checked_add(text.len()).ok_or_else(invalid)?;
        }
        if let Some(index) = receipt.validation_index.map(|index| index as usize) {
            let stack = if index < source.pending.produce.len() {
                source.pending.produce.get(index)
            } else {
                index
                    .checked_sub(source.pending.produce.len())
                    .and_then(|index| source.pending.byproducts.get(index))
            };
            if let Some(stack) = stack {
                binding_bytes = binding_bytes
                    .checked_add(stack.kind.len())
                    .ok_or_else(invalid)?;
            }
        }
    }
    common
        .charge(1, binding_bytes as u64)
        .map_err(|error| BodyMutationError::Original(Box::new(error)))?;
    source.validate(step.transaction, common.memory(), now, identity, live)?;
    let job = &**source.pending;
    let (kind, detail_len) = match &source.cause {
        DueRecipeFailureCauseV1::IndustrialTerminal { error } => {
            let mut count = CountText {
                n: 0,
                common,
                error: None,
            };
            if write!(&mut count, "{:?}", &**error).is_err() {
                return Err(count.error.take().unwrap_or_else(invalid));
            }
            ("industrial_terminal", count.n)
        }
        _ => {
            let receipt = rejection_receipt(&source.cause).ok_or_else(invalid)?;
            (
                "product_validation",
                receipt.failure_detail.as_ref().map_or_else(
                    || {
                        "product validation rejected for  before production settlement".len()
                            + receipt.stack.kind.len()
                    },
                    String::len,
                ),
            )
        }
    };
    let job_layout = recipe_commit::job_copy_layout(job, common)?;
    let layout = job_layout
        .checked_mul(4)
        .and_then(|n| n.checked_add(detail_len.checked_mul(4)?))
        .and_then(|n| n.checked_add(4096 + std::mem::size_of::<PreparedNormalRecipeFailureV1>()))
        .ok_or_else(invalid)?;
    common
        .charge(layout as u64, layout as u64)
        .map_err(|e| BodyMutationError::Original(Box::new(e)))?;
    let phase = common
        .memory()
        .reserve(layout, MemoryClass::Ordinary)
        .map_err(ValueTreeError::Memory)?;
    let mut detail = String::with_capacity(detail_len);
    match &source.cause {
        DueRecipeFailureCauseV1::IndustrialTerminal { error } => {
            write!(&mut detail, "{:?}", &**error).map_err(|_| invalid())?
        }
        _ => {
            let receipt = rejection_receipt(&source.cause).ok_or_else(invalid)?;
            if let Some(value) = &receipt.failure_detail {
                detail.push_str(value)
            } else {
                write!(
                    &mut detail,
                    "product validation rejected for {} before production settlement",
                    receipt.stack.kind
                )
                .map_err(|_| invalid())?
            }
        }
    }
    let terminal =
        oasis7_world_state::state::industry_transition::recipe_blocker_is_terminal_v1(kind);
    let outcome = crate::world::incremental_step::factory_schedule_write_io::paid(
        step,
        read,
        parent,
        write,
        materials,
        work,
        common,
        live,
        |step, read, parent, write, materials, work| {
            // These two caller-specific suppressions precede the normal
            // transition's commitment/idempotency checks in the original driver.
            if matches!(
                &source.cause,
                DueRecipeFailureCauseV1::IndustrialTerminal { .. }
            ) {
                let mut ctx = context!(step, read);
                let guard = ctx.transaction.guard();
                let current = oasis7_world_state::incremental_factory_schedule_factory::recover_current_schedule_complete_factory_v1(
                    &job.factory_id, sources.lifecycle.factories, &mut ctx, &mut work.factory, &guard, live,
                ).map_err(overlay)?;
                if let Some(current) = current {
                    let bytes = current
                        .production
                        .current_blocker_kind
                        .as_ref()
                        .map_or(0, String::len)
                        .checked_add(
                            current
                                .production
                                .current_blocker_detail
                                .as_ref()
                                .map_or(0, String::len),
                        )
                        .and_then(|n| n.checked_add(kind.len()))
                        .and_then(|n| n.checked_add(detail.len()))
                        .ok_or_else(invalid)?;
                    spend(work, bytes)?;
                    if current.production.current_blocker_kind.as_deref() == Some(kind)
                        && current.production.current_blocker_detail.as_deref()
                            == Some(detail.as_str())
                    {
                        return Ok(None);
                    }
                }
            }
            if matches!(
                &source.cause,
                DueRecipeFailureCauseV1::PersistedProductValidation { .. }
            ) {
                let mut ctx = context!(step, read);
                if read_industry_record_root_overlay_v1(
                    IndustryRecordKeyV1 {
                        kind: IndustryRecordKindV1::Failure,
                        id: job.job_id,
                    },
                    &mut ctx,
                )
                .map_err(overlay)?
                .is_some()
                {
                    return Ok(None);
                }
            }
            let mut already = false;
            {
                let mut ctx = context!(step, read);
                let failure = IndustryRecordKeyV1 {
                    kind: IndustryRecordKindV1::Failure,
                    id: job.job_id,
                };
                if terminal
                    && read_industry_record_root_overlay_v1(failure, &mut ctx)
                        .map_err(overlay)?
                        .is_some()
                {
                    // Original idempotency compares only these five commitment fields.
                    for (column, value) in [
                        (
                            IndustryColumnV1::RecordRequesterAgentId,
                            job.requester_agent_id.as_str(),
                        ),
                        (IndustryColumnV1::RecordFactoryId, job.factory_id.as_str()),
                        (IndustryColumnV1::RecordRecipeId, job.recipe_id.as_str()),
                        (IndustryColumnV1::RecordBlockerKind, kind),
                        (IndustryColumnV1::RecordBlockerDetail, detail.as_str()),
                    ] {
                        let guard = ctx.transaction.guard();
                        change_industry_cell_overlay_v1(IndustryCellChangeV1{key:failure,cell:IndustryCellV1::Column(column),expected:Some(IndustryValueRefV1::Text(value)),after:Some(IndustryValueRefV1::Text(value)),max_current_payload_bytes:work.lifecycle.max_scalar_bytes as u64},&mut ctx,write,&mut work.lifecycle.industry,&guard,live).map_err(|error|match error {
                        oasis7_world_state::incremental_field_overlay::WorldFieldOverlayErrorV1::Value(ValueTreeError::Invalid("industry expected cell")) => recipe_commit::state_error(ctx.values.memory, "product-validation blocker conflicts with persisted disposition: job_id=".len()+20,format_args!("product-validation blocker conflicts with persisted disposition: job_id={}",job.job_id)),
                        other=>overlay(other),
                    })?;
                    }
                    already = true;
                }
                let pending = IndustryRecordKeyV1 {
                    kind: IndustryRecordKindV1::Recipe,
                    id: job.job_id,
                };
                if terminal
                    && read_industry_record_root_overlay_v1(pending, &mut ctx)
                        .map_err(overlay)?
                        .is_none()
                {
                    already = true;
                }
            }
            if !already {
                let factory = apply_factory_completion_v1(
                    job,
                    RecipeFactoryUpdateV1::Blocked {
                        kind,
                        detail: &detail,
                    },
                    sources,
                    step,
                    read,
                    write,
                    work,
                    live,
                )?;
                if !terminal {
                    apply_recipe_progress_v1(
                        job,
                        job_layout,
                        RecipeProgressUpdateV1::Blocked,
                        factory.as_deref(),
                        sources,
                        step,
                        read,
                        write,
                        work,
                        live,
                    )?;
                }
                let exists = {
                    use oasis7_world_state::incremental_agent_subtree::{
                        CurrentAgentPresenceV1, read_current_agent_presence_v1,
                    };
                    let mut ctx = context!(step, read);
                    let guard = ctx.transaction.guard();
                    read_current_agent_presence_v1(
                        &job.requester_agent_id,
                        sources.lifecycle.agents,
                        sources.roots,
                        &mut ctx,
                        parent,
                        &mut work.lifecycle.pending_entries_remaining,
                        &guard,
                        live,
                    )
                    .map_err(overlay)?
                        == CurrentAgentPresenceV1::Existing
                };
                if exists {
                    crate::world::incremental_normal_action_factory_schedule::set_blocked_recipe_activity_v1(&job.requester_agent_id,job.job_id,&job.factory_id,kind,&detail,sources.lifecycle.agents,step,read,work,live)?;
                    update_requester_v1(job, false, sources, step, read, parent, work, live)?;
                }
                let guard = step.transaction.guard();
                materials.contribute(step.transaction, step.values, step.mutation, &guard, live)?;
                if terminal {
                    settle_recipe_rows_v1(job, sources, step, read, write, work, live)?;
                    let disposition=oasis7_world_state::state::industry_transition::recipe_failure_disposition_v1(job.job_id,job,kind,&detail);
                    let mut ctx = context!(step, read);
                    let guard = ctx.transaction.guard();
                    create_industry_record_overlay_v1(
                        IndustryRecordCreateV1 {
                            key: IndustryRecordKeyV1 {
                                kind: IndustryRecordKindV1::Failure,
                                id: job.job_id,
                            },
                            after: IndustryRecordRefV1::Failure(&disposition),
                            qualification: sources.lifecycle.industry,
                        },
                        &mut ctx,
                        write,
                        &mut work.lifecycle.industry,
                        &guard,
                        live,
                    )
                    .map_err(overlay)?;
                }
            } else {
                let guard = step.transaction.guard();
                materials.contribute(step.transaction, step.values, step.mutation, &guard, live)?;
            }
            let sequence = step
                .read_allocator(crate::world::incremental_allocators::RuntimeAllocatorV1::Event)?;
            let (id, _, _) = crate::World::preview_next_event_id(sequence.next, sequence.era);
            let fee = step
                .mutation
                .memory
                .reserve(
                    std::mem::size_of::<crate::WorldEvent>(),
                    MemoryClass::Ordinary,
                )
                .map_err(ValueTreeError::Memory)?;
            let event = fee.retain(crate::WorldEvent {
                id,
                time: now,
                caused_by: None,
                body: crate::WorldEventBody::Domain(crate::DomainEvent::FactoryProductionBlocked {
                    action_id: job.job_id,
                    requester_agent_id: job.requester_agent_id.clone(),
                    factory_id: job.factory_id.clone(),
                    recipe_id: job.recipe_id.clone(),
                    blocker_kind: kind.into(),
                    blocker_detail: detail,
                }),
            });
            Ok(Some((event, step.transaction.guard(), sequence.era)))
        },
    )?;
    let Some((event, guard, event_era)) = outcome else {
        return Ok(None);
    };
    Ok(Some(PreparedNormalRecipeFailureV1(DueEventStateV1 {
        event,
        guard,
        identity,
        memory: common.memory().clone(),
        now,
        event_era,
        _phase: phase,
        _sources: Vec::new(),
    })))
}
