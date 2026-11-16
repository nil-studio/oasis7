//! Actual bound SupplySettle pipeline; only the genuine fixed header's job is
//! read, and the complete source owner survives request/event publication.
use super::delivery::{
    IndustrialDeliveryErrorV1, IndustrialDeliverySourcesV1, IndustrialDeliveryWorkV1,
};
use super::written::{
    BoundCommittedSupplySettlementV1, FreshWrittenLifecycleCommandV1,
    LifecycleCommandPreparationV1, LifecycleWrittenSourceV1,
};
use super::*;
use oasis7_formal_protocol::{
    controlled_authority::formal::bounded_context::FormalBoundedContextV1,
    incremental_state::StatePageWriter,
    incremental_value::{ValueNodeReader, ValueNodeWriter, ValueTreeError},
};
use oasis7_world_state::{
    incremental_field_member::MemberRecordWriteContext,
    incremental_field_overlay::WorldFieldOverlayContextV1,
    incremental_industry_jobs::IndustryFamilyQualificationV1,
    incremental_world_rules_family::WorldRulesQualificationV1,
};
use std::mem::size_of;
pub(super) fn execute_current_supply_settlement_v1<M, R, E, C, W, CW>(
    header: header::PreparedFreshLifecycleHeaderV1,
    rules: &ReservedValue<WorldRulesQualificationV1>,
    industry: &ReservedValue<IndustryFamilyQualificationV1>,
    roots: &oasis7_formal_protocol::incremental_execution::RootSetV1,
    ctx: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    write: &mut MemberRecordWriteContext<'_, W, CW>,
    common: &mut FormalBoundedContextV1<'_>,
    max_text: usize,
    now: u64,
    live: &ExecutionFence,
) -> Result<LifecycleCommandPreparationV1, IndustrialDeliveryErrorV1>
where
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    W: ValueNodeReader + ValueNodeWriter + StatePageWriter,
    CW: FnMut() -> bool,
{
    header.validate(
        ctx.transaction,
        common.memory(),
        now,
        common.work().identity(),
        live,
    )?;
    let (crate::DomainEvent::WorldRuleCommitted {
        actor_id,
        request_id,
        command: WorldRuleCommandV1::SettleSupply { job_id },
        world_binding,
        ..
    }) = header.event()
    else {
        return Err(BodyMutationError::Invalid("lifecycle settlement header kind").into());
    };
    let id = *job_id;
    let bytes = actor_id
        .len()
        .checked_add(request_id.len())
        .and_then(|n| n.checked_add(world_binding.as_ref().map_or(0, String::len)))
        .and_then(|n| n.checked_add(size_of::<BoundCommittedSupplySettlementV1>() + 20))
        .ok_or(BodyMutationError::Invalid(
            "lifecycle settlement source layout",
        ))?;
    common
        .charge(1, bytes as u64)
        .map_err(|e| BodyMutationError::Original(Box::new(e)))?;
    let phase = common
        .memory()
        .reserve(bytes, oasis7_proto::memory_budget::MemoryClass::Ordinary)
        .map_err(ValueTreeError::Memory)?;
    let event = header.event().clone();
    let result = id.to_string();
    let prepared = match supply::prepare_settlement_v1(
        id, rules, industry, roots, max_text, ctx, write, common, live,
    )? {
        supply::SupplyPreparationV1::Ready(source) => source,
        supply::SupplyPreparationV1::Rejected(error) => {
            return Ok(LifecycleCommandPreparationV1::Rejected(error));
        }
    };
    let source = supply::apply_settlement_v1(prepared, ctx, write, common, max_text as u64, live)?;
    let bound = BoundCommittedSupplySettlementV1 {
        source,
        event,
        result,
        memory: common.memory().clone(),
        identity: common.work().identity(),
        now,
        _phase: phase,
    };
    Ok(LifecycleCommandPreparationV1::Fresh(
        FreshWrittenLifecycleCommandV1 {
            header,
            source: LifecycleWrittenSourceV1::SupplySettle(bound),
        },
    ))
}
