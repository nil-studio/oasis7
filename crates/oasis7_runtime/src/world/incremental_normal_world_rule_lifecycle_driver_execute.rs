//! Fixed five-lane business dispatch. Rejections retain their real error fee;
//! IO, memory, cancellation and source failures abort the actual transaction.
//! No caller callback, boolean permission or arbitrary DomainEvent is accepted.
use super::delivery::{
    IndustrialDeliveryErrorV1, IndustrialDeliverySourcesV1, IndustrialDeliveryWorkV1,
};
use super::driver_sources::NormalWorldRuleLifecycleSourcesV1;
use super::written::{
    FreshWrittenLifecycleCommandV1, LifecycleCommandPreparationV1, LifecycleWrittenSourceV1,
};
use super::*;
use crate::world::{
    incremental_body_event::materials::CorePolicyMaterialContextV1,
    incremental_normal_sources::BoundNormalTickParentV1,
    incremental_normal_world_rule_observation_v2::ObservationJsonOriginalSinkV2,
};
use oasis7_formal_protocol::{
    controlled_authority::formal::bounded_context::FormalBoundedContextV1,
    incremental_state::StatePageWriter,
    incremental_value::{ValueNodeReader, ValueNodeWriter, ValueTreeError},
};
use oasis7_world_state::{
    incremental_field_member::MemberRecordWriteContext,
    incremental_field_overlay::WorldFieldOverlayContextV1,
    incremental_field_read::WorldFieldReadContextV1,
    incremental_world_rules_family::WorldRulesFamilyWorkV1,
};
#[allow(clippy::too_many_arguments)]
pub(super) fn execute_current_lifecycle_command_v1<M, P, V, R, W, E, C, PE, PC, CW, ME, S>(
    header: header::PreparedFreshLifecycleHeaderV1,
    normal: &NormalWorldRuleLifecycleSourcesV1<'_>,
    current: &IndustrialDeliverySourcesV1<'_, '_>,
    normal_parent: &ReservedValue<BoundNormalTickParentV1<'_>>,
    state: &M,
    ctx: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    parent: &mut WorldFieldReadContextV1<'_, P, V, PE, PC>,
    write: &mut MemberRecordWriteContext<'_, W, CW>,
    materials: &mut CorePolicyMaterialContextV1<'_, M, ME>,
    work: &mut IndustrialDeliveryWorkV1,
    world: &mut WorldRulesFamilyWorkV1,
    extra: &mut emergency_close::sources::EmergencyServiceSourceWorkV1<'_>,
    common: &mut FormalBoundedContextV1<'_>,
    now: u64,
    live: &ExecutionFence,
    sink: &mut S,
) -> Result<LifecycleCommandPreparationV1, IndustrialDeliveryErrorV1>
where
    M: MerkleNodeReader,
    P: MerkleNodeReader,
    V: ValueNodeReader,
    R: ValueNodeReader,
    W: ValueNodeReader + ValueNodeWriter + StatePageWriter,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    PE: FnMut() -> Option<ValueTreeError>,
    PC: FnMut() -> bool,
    CW: FnMut() -> bool,
    ME: FnMut() -> Option<ValueTreeError>,
    S: ObservationJsonOriginalSinkV2,
    S::Error: std::fmt::Debug + Send + Sync + 'static,
{
    header.validate(
        ctx.transaction,
        common.memory(),
        now,
        common.work().identity(),
        live,
    )?;
    let result: Result<LifecycleCommandPreparationV1, IndustrialDeliveryErrorV1> = (|| {
        let crate::DomainEvent::WorldRuleCommitted { command, .. } = header.event() else {
            return Err(BodyMutationError::Invalid("fixed lifecycle header domain").into());
        };
        let source = match command {
            WorldRuleCommandV1::CloseSupply { .. } => {
                let prepared = match supply_close::prepare_complete_current_supply_close_v1(
                    &header,
                    normal.rules,
                    current,
                    state,
                    ctx,
                    parent,
                    write,
                    materials,
                    work,
                    common,
                    now,
                    live,
                )? {
                    supply_close::CompleteSupplyClosePreparationV1::Ready(source) => source,
                    supply_close::CompleteSupplyClosePreparationV1::Rejected(error) => {
                        return Ok(LifecycleCommandPreparationV1::Rejected(error));
                    }
                };
                LifecycleWrittenSourceV1::SupplyClose(
                    supply_close::commit_complete_current_supply_close_v1(
                        prepared, &header, ctx, parent, write, materials, work, common, now, live,
                    )?,
                )
            }
            WorldRuleCommandV1::SettleSupply { .. } => {
                return super::driver_settle::execute_current_supply_settlement_v1(
                    header,
                    normal.rules,
                    normal.factory.lifecycle.industry,
                    normal.factory.roots,
                    ctx,
                    write,
                    common,
                    work.max_identity_bytes,
                    now,
                    live,
                );
            }
            WorldRuleCommandV1::AdvanceAgentOrder { .. } => {
                let prepared = match order::prepare_current_order_advance_v1(
                    &header, current, state, ctx, parent, write, materials, work, common, now, live,
                )? {
                    order::OrderAdvancePreparationV1::Ready(source) => source,
                    order::OrderAdvancePreparationV1::Refund(source) => {
                        match order::refund_current_order_advance_v1(
                            source, current, state, ctx, parent, write, materials, work, common,
                            now, live,
                        )? {
                            order::CurrentOrderRefundOutcomeV1::Ready(source) => source,
                            order::CurrentOrderRefundOutcomeV1::Rejected(error) => {
                                return Ok(LifecycleCommandPreparationV1::Rejected(error));
                            }
                        }
                    }
                    order::OrderAdvancePreparationV1::Rejected(error) => {
                        return Ok(LifecycleCommandPreparationV1::Rejected(error));
                    }
                };
                LifecycleWrittenSourceV1::Advance(order::commit_current_order_advance_v1(
                    prepared, &header, current, state, ctx, parent, write, materials, work, common,
                    now, live,
                )?)
            }
            WorldRuleCommandV1::DeliverAgentOrder { .. } => {
                LifecycleWrittenSourceV1::Deliver(delivery::deliver_current_industrial_order_v1(
                    &header, current, state, ctx, parent, write, materials, work, common, now, live,
                )?)
            }
            WorldRuleCommandV1::EmergencySupply {
                command: EmergencySupplyCommandV1::Observe { .. },
            } => {
                return Ok(LifecycleCommandPreparationV1::Fresh(
                    observe::execute_lifecycle_observe_v2(
                        header,
                        normal_parent,
                        &emergency_close::EmergencyCloseSourcesV1 {
                            delivery: current,
                            rules: normal.rules,
                        },
                        state,
                        ctx,
                        parent,
                        write,
                        materials,
                        work,
                        extra,
                        world,
                        common,
                        now,
                        live,
                        sink,
                    )?,
                ));
            }
            WorldRuleCommandV1::EmergencySupply {
                command: EmergencySupplyCommandV1::Close { .. },
            } => LifecycleWrittenSourceV1::EmergencyClose(
                emergency_close::close_current_emergency_authorization_v1(
                    &header,
                    &emergency_close::EmergencyCloseSourcesV1 {
                        delivery: current,
                        rules: normal.rules,
                    },
                    state,
                    ctx,
                    parent,
                    write,
                    materials,
                    work,
                    extra,
                    world,
                    common,
                    now,
                    live,
                )?,
            ),
            _ => {
                return Err(
                    BodyMutationError::Invalid("non-lifecycle command in fixed tail").into(),
                );
            }
        };
        Ok(LifecycleCommandPreparationV1::Fresh(
            FreshWrittenLifecycleCommandV1 { header, source },
        ))
    })();
    match result {
        // Only original business refusal is pending work, never a budget/IO error.
        Err(IndustrialDeliveryErrorV1::World(error)) => {
            ctx.transaction
                .validate_guard(&ctx.transaction.guard(), live)?;
            common
                .charge(1, 512)
                .map_err(|e| BodyMutationError::Original(Box::new(e)))?;
            let fee = common
                .memory()
                .reserve(
                    512 + std::mem::size_of::<ReservedValue<crate::WorldError>>(),
                    oasis7_proto::memory_budget::MemoryClass::Ordinary,
                )
                .map_err(ValueTreeError::Memory)?;
            Ok(LifecycleCommandPreparationV1::Rejected(fee.retain(error)))
        }
        Err(error) => {
            let _: Result<(), oasis7_formal_protocol::incremental_execution::ExecutionError> =
                ctx.transaction.contribute(|_| {
                    Err(oasis7_formal_protocol::incremental_execution::ExecutionError::Aborted)
                });
            Err(error)
        }
        ok => ok,
    }
}
