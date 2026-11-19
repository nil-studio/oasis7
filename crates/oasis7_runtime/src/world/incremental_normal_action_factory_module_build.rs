//! Original post-VM Build preflight, before the staged module state commits.
//! Uses the same fixed kernel and actual Current industrial source as direct
//! Build; it never invents a Rule outcome, Accepted receipt or World snapshot.
use super::*;
#[allow(clippy::too_many_arguments)]
pub(super) fn preflight<'s, M, P, R, V, E, C, PE, PC>(
    mut resolved: ResolvedFactoryModuleActionV1<'s>,
    sources: &FactoryScheduleSourcesV1<'_>,
    context: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    parent: &mut WorldFieldReadContextV1<'_, P, V, PE, PC>,
    work: &mut FactoryScheduleWorkV1,
    common: &mut FormalBoundedContextV1<'_>,
    now: u64,
    live: &ExecutionFence,
) -> Result<ResolvedFactoryModuleOutcomeV1<'s>, BodyMutationError>
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
    resolved.validate(context.transaction, common, now, live)?;
    if !context.transaction.is_staged_branch_v1()?
        || resolved.call.kind() != FactoryModuleCallKindV1::Build
        || resolved._build_preflight_fees.is_some()
    {
        return Err(invalid());
    }
    source_binding(sources, context.transaction, common)?;
    let mut fees = current::SourceFeesV1::new(work.max_source_records, common)?;
    let evaluated = {
        let mut facts = current::CurrentFactoryScheduleFactsV1 {
            sources,
            context,
            read: parent,
            work,
            common,
            now,
            live,
            fees: &mut fees,
        };
        match &*resolved.action {
   crate::Action::BuildFactory{builder_agent_id,site_id,spec}=>crate::world::event_processing::factory_schedule_fixed::evaluate_build_factory_v1(&mut facts,resolved.call.action_id(),builder_agent_id,site_id,spec,crate::world::event_processing::factory_schedule_fixed::BuildMaterialCheckV1::Required),
   _=>return Err(invalid()),
  }
    };
    let body = match evaluated {
        Ok(body) => body,
        Err(error) => return Err(evaluation_error(error, fees)),
    };
    match body {
        crate::WorldEventBody::Domain(crate::DomainEvent::ActionRejected { action_id, reason }) => {
            if action_id != resolved.call.action_id() {
                return Err(invalid());
            }
            // The fixed kernel's reserve_action_output and selected SourceFees already
            // cover this moved reason's heap. Its complete fees remain with this owner.
            charge(common, size_of::<crate::RejectReason>())?;
            let fee = common
                .memory()
                .reserve(size_of::<crate::RejectReason>(), MemoryClass::Ordinary)
                .map_err(ValueTreeError::Memory)?;
            Ok(ResolvedFactoryModuleOutcomeV1::Denied(
                DeniedFactoryModuleOutputV1 {
                    call: resolved.call,
                    execution: resolved.execution,
                    reason: fee.retain(reason),
                    _decoded: resolved._decoded,
                    guard: context.transaction.guard(),
                    identity: common.work().identity(),
                    now,
                    _build_preflight_fees: Some(fees),
                },
            ))
        }
        crate::WorldEventBody::Domain(crate::DomainEvent::FactoryBuildStarted {
            job_id, ..
        }) if job_id == resolved.call.action_id() => {
            charge(common, 0)?;
            context
                .transaction
                .validate_guard(&context.transaction.guard(), live)?;
            resolved.guard = context.transaction.guard();
            resolved._build_preflight_fees = Some(fees);
            Ok(ResolvedFactoryModuleOutcomeV1::Resolved(resolved))
        }
        _ => Err(invalid()),
    }
}
