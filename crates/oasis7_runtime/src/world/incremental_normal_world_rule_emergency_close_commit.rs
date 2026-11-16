//! The complete Current Close operation. Only this fixed path creates the
//! private post-write source consumed by the common lifecycle publisher.
use super::*;
use crate::world::incremental_body_event::materials::CorePolicyMaterialContextV1;
use oasis7_agent_api::world_action::{EmergencySupplyCommandV1, ResourceKind, WorldRuleCommandV1};
use oasis7_formal_protocol::{
    controlled_authority::formal::bounded_context::FormalBoundedContextV1,
    incremental_execution::ExecutionError,
};
use oasis7_world_state::{
    incremental_agent_fields::{AgentFieldV1, AgentScalarRefV1},
    incremental_agent_subtree::*,
    incremental_value_mutation::ValueMutationContextV1,
};
struct NeedChange {
    selected: ReservedValue<CurrentEmergencyNeedV1>,
    after: i64,
}
pub(in crate::world) struct CommittedCurrentEmergencyCloseV1 {
    overlay: ReservedValue<EmergencyOverlay>,
    result: String,
    event: crate::DomainEvent,
    guard: PreparedGuard,
    identity: WorkIdentityV1,
    memory: MemoryBudget,
    now: u64,
    _phase: MemoryReservation,
}
fn header_parts(
    e: &crate::DomainEvent,
) -> Result<(&str, &str, &str, Option<&str>, u64, u64), IndustrialDeliveryErrorV1> {
    let crate::DomainEvent::WorldRuleCommitted {
        action_id,
        governance_epoch,
        actor_id,
        request_id,
        command:
            WorldRuleCommandV1::EmergencySupply {
                command: EmergencySupplyCommandV1::Close { authorization_id },
            },
        world_binding,
        ..
    } = e
    else {
        return Err(BodyMutationError::Invalid("Emergency Close exact header").into());
    };
    Ok((
        actor_id,
        request_id,
        authorization_id,
        world_binding.as_deref(),
        *action_id,
        *governance_epoch,
    ))
}
impl CommittedCurrentEmergencyCloseV1 {
    pub(in crate::world) fn result_id(&self) -> &str {
        &self.result
    }

    pub(in crate::world) fn validate<M: MerkleNodeReader>(
        &self,
        header: &header::PreparedFreshLifecycleHeaderV1,
        tx: &ExecutionTransaction<'_, M>,
        now: u64,
        tail: &mut NormalTailWorkV1<'_, '_>,
        live: &ExecutionFence,
    ) -> Result<(), IndustrialDeliveryErrorV1> {
        tx.validate_guard(&self.guard, live)?;
        if self.identity != tail.identity()
            || !self.memory.same_pool(tail.memory())
            || !self.overlay.uses_budget(tail.memory())
            || !tx.uses_budget(tail.memory())
            || self.now != now
        {
            return Err(BodyMutationError::Invalid("Emergency Close complete source scope").into());
        }
        let (a, r, id, b, action, epoch) = header_parts(&self.event)?;
        let (aa, rr, ii, bb, action_after, epoch_after) = header_parts(header.event())?;
        tail.charge(
            a.len()
                .min(aa.len())
                .checked_add(r.len().min(rr.len()))
                .and_then(|n| n.checked_add(id.len().min(ii.len())))
                .and_then(|n| {
                    n.checked_add(match (b, bb) {
                        (Some(a), Some(b)) => a.len().min(b.len()),
                        _ => 0,
                    })
                })
                .ok_or(ValueTreeError::ResourceLimited("Close header comparisons"))?,
        )?;
        if a != aa
            || r != rr
            || id != ii
            || b != bb
            || action != action_after
            || epoch != epoch_after
        {
            return Err(
                BodyMutationError::Invalid("Emergency Close complete header contents").into(),
            );
        }
        Ok(())
    }
}
#[allow(clippy::too_many_arguments)]
pub(in crate::world) fn close_current_emergency_authorization_v1<
    M,
    P,
    V,
    R,
    W,
    E,
    C,
    PE,
    PC,
    CW,
    ME,
>(
    header: &header::PreparedFreshLifecycleHeaderV1,
    sources: &EmergencyCloseSourcesV1<'_, '_>,
    state: &M,
    ctx: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    parent: &mut WorldFieldReadContextV1<'_, P, V, PE, PC>,
    write: &mut MemberRecordWriteContext<'_, W, CW>,
    materials: &mut CorePolicyMaterialContextV1<'_, M, ME>,
    work: &mut IndustrialDeliveryWorkV1,
    extra: &mut sources::EmergencyServiceSourceWorkV1<'_>,
    rules_work: &mut WorldRulesFamilyWorkV1,
    common: &mut FormalBoundedContextV1<'_>,
    now: u64,
    live: &ExecutionFence,
) -> Result<ReservedValue<CommittedCurrentEmergencyCloseV1>, IndustrialDeliveryErrorV1>
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
{
    let identity = common.work().identity();
    header.validate(ctx.transaction, common.memory(), now, identity, live)?;
    let result = super::delivery::paid_io::run(
        ctx,
        parent,
        write,
        materials,
        work,
        common,
        |ctx, parent, write, _materials, work, tail| {
            let (actor, request, id, binding, _, _) = header_parts(header.event())?;
            let bytes = std::mem::size_of::<CommittedCurrentEmergencyCloseV1>()
                .checked_add(std::mem::size_of::<crate::DomainEvent>())
                .and_then(|n| n.checked_add(actor.len()))
                .and_then(|n| n.checked_add(request.len()))
                .and_then(|n| n.checked_add(id.len().checked_mul(2)?))
                .and_then(|n| n.checked_add(binding.map_or(0, str::len)))
                .ok_or(ValueTreeError::ResourceLimited("Close source size"))?;
            tail.charge(bytes)?;
            let phase = tail.reserve(bytes, MemoryClass::Ordinary)?;
            let captured = header.event().clone();
            let roots = sources.delivery.eligibility.roots;
            let guard = ctx.transaction.guard();
            let source = bind_current_emergency_lifecycle_v1(
                sources.rules.emergency(),
                roots,
                state,
                ctx,
                &mut work.eligibility.scan,
                &mut work.composite_read,
                &guard,
                live,
            )?;
            let guard = ctx.transaction.guard();
            let authorization = read_current_emergency_authorization_v1(
                &source,
                sources.rules.emergency(),
                roots,
                state,
                id,
                ctx,
                &mut work.eligibility.scan,
                &mut work.composite_read,
                &guard,
                live,
            )?;
            let auth = authorization
                .value()
                .ok_or_else(|| crate::world_rules::invalid("authorization_inactive"))?;
            if !auth.active {
                return Err(crate::world_rules::invalid("missing_authority").into());
            }
            tail.charge(actor.len().min(auth.provider_id.len()))?;
            let guard = ctx.transaction.guard();
            let directory = project_current_emergency_lifecycle_v1(
                &source,
                sources.rules.emergency(),
                roots,
                state,
                ctx,
                &mut work.eligibility.scan,
                &mut work.composite_read,
                &guard,
                live,
            )?;
            if actor != auth.provider_id
                && active::validate_current_active_v1(
                    id, &directory, &source, sources, state, ctx, parent, work, extra, rules_work,
                    tail, now, live,
                )?
                .is_ok()
            {
                return Err(crate::world_rules::invalid("missing_authority").into());
            }
            // Original outer BTree order and all matching batches, including committed.
            let count = directory.batches().len();
            let _batch_slots = tail.reserve(
                count
                    .checked_mul(
                        std::mem::size_of::<ReservedValue<CurrentEmergencyBatchV1>>()
                            + std::mem::size_of::<EmergencyBatchReleaseV1<'_>>(),
                    )
                    .ok_or(ValueTreeError::ResourceLimited("Close batch slots"))?,
                MemoryClass::Ordinary,
            )?;
            let mut batches = Vec::new();
            batches
                .try_reserve_exact(count)
                .map_err(|_| ValueTreeError::ResourceLimited("Close batch slots"))?;
            if batches.capacity() > count {
                return Err(ValueTreeError::ResourceLimited("Close batch capacity").into());
            }
            let mut allocations = 0usize;
            for row in directory.batches() {
                tail.charge(row.authorization_id.len().min(id.len()))?;
                if row.authorization_id != id {
                    continue;
                }
                let guard = ctx.transaction.guard();
                let batch = read_current_emergency_batch_v1(
                    &source,
                    sources.rules.emergency(),
                    roots,
                    state,
                    &row.outer_id,
                    ctx,
                    &mut work.eligibility.scan,
                    &mut work.composite_read,
                    &guard,
                    live,
                )?;
                let value = batch
                    .value()
                    .ok_or(ValueTreeError::Invalid("Current batch directory absent"))?;
                allocations = allocations
                    .checked_add(value.allocation.len())
                    .ok_or(ValueTreeError::ResourceLimited("Close allocation count"))?;
                batches.push(batch);
            }
            let _need_slots = tail.reserve(
                allocations
                    .checked_mul(
                        std::mem::size_of::<NeedChange>()
                            + std::mem::size_of::<EmergencyNeedReservationChangeV1<'_>>(),
                    )
                    .ok_or(ValueTreeError::ResourceLimited("Close need slots"))?,
                MemoryClass::Ordinary,
            )?;
            let mut needs: Vec<NeedChange> = Vec::new();
            needs
                .try_reserve_exact(allocations)
                .map_err(|_| ValueTreeError::ResourceLimited("Close need slots"))?;
            if needs.capacity() > allocations {
                return Err(ValueTreeError::ResourceLimited("Close need capacity").into());
            }
            for batch in &batches {
                for (need_id, amount) in &batch
                    .value()
                    .ok_or(ValueTreeError::Invalid("Close selected batch"))?
                    .allocation
                {
                    let mut at = None;
                    for (i, need) in needs.iter().enumerate() {
                        tail.charge(need_id.len().min(need.selected.outer_id().len()))?;
                        if need.selected.outer_id() == need_id {
                            at = Some(i);
                            break;
                        }
                    }
                    let index = if let Some(i) = at {
                        i
                    } else {
                        let guard = ctx.transaction.guard();
                        let selected = read_current_emergency_need_v1(
                            &source,
                            sources.rules.emergency(),
                            roots,
                            state,
                            need_id,
                            ctx,
                            &mut work.eligibility.scan,
                            &mut work.composite_read,
                            &guard,
                            live,
                        )?;
                        let reserved = selected
                            .value()
                            .ok_or(ValueTreeError::Invalid("original emergency need absent"))?
                            .reserved;
                        needs.push(NeedChange {
                            selected,
                            after: reserved,
                        });
                        needs.len() - 1
                    };
                    tail.charge_record()?;
                    let mut quantity = *amount;
                    oasis7_world_state::world_rules::release_emergency_need_reserved_amount_v1(
                        &mut needs[index].after,
                        &mut quantity,
                    );
                }
            }
            let guard = ctx.transaction.guard();
            if read_current_agent_presence_v1(
                &auth.provider_id,
                sources.delivery.eligibility.agents,
                roots,
                ctx,
                parent,
                &mut work.eligibility.pending_entries_remaining,
                &guard,
                live,
            )? != CurrentAgentPresenceV1::Existing
            {
                return Err(ValueTreeError::Invalid("original emergency provider absent").into());
            }
            let query = AgentSubtreeQualifiedRequestV1 {
                cell: AgentSubtreeScalarRequestV1 {
                    agent: &auth.provider_id,
                    field: AgentFieldV1::ResourceAmount,
                    ordinal: 0,
                },
                qualification: sources.delivery.eligibility.agents,
            };
            let guard = ctx.transaction.guard();
            let before = match read_agent_subtree_scalar_overlay_v1(
                query,
                ctx,
                64,
                &mut std::io::sink(),
                &guard,
                live,
            )? {
                None => None,
                Some(AgentSubtreeValueV1::I64(n)) => Some(n),
                _ => return Err(ValueTreeError::Invalid("Close provider electricity shape").into()),
            };
            let after = oasis7_agent_api::world_resource::checked_resource_add_v1(
                ResourceKind::Electricity,
                before.unwrap_or(0),
                auth.remaining,
            )
            .map_err(|_| crate::world_rules::invalid("numeric_overflow"))?;
            // Every original business failure was checked before any Current write.
            let need_refs: Vec<_> = needs
                .iter()
                .map(|n| EmergencyNeedReservationChangeV1 {
                    selected: &n.selected,
                    reserved: n.after,
                })
                .collect();
            let batch_refs: Vec<_> = batches
                .iter()
                .map(|b| EmergencyBatchReleaseV1 { selected: b })
                .collect();
            let guard = ctx.transaction.guard();
            let overlay = commit_current_emergency_close_columns_v1(
                &source,
                &authorization,
                &need_refs,
                &batch_refs,
                sources.rules.emergency(),
                roots,
                state,
                ctx,
                write,
                &mut work.eligibility.scan,
                &mut work.composite_read,
                &mut work.composite_write,
                &guard,
                live,
            )?;
            let query = AgentSubtreeQualifiedRequestV1 {
                cell: AgentSubtreeScalarRequestV1 {
                    agent: &auth.provider_id,
                    field: AgentFieldV1::ResourceAmount,
                    ordinal: 0,
                },
                qualification: sources.delivery.eligibility.agents,
            };
            let guard = ctx.transaction.guard();
            let mut output = ValueMutationContextV1 {
                sink: &mut *write.sink,
                memory: write.memory,
                io: &mut *write.io,
                cancelled: &mut write.cancelled,
            };
            change_agent_subtree_scalar_v1(
                AgentSubtreeCellChangeV1 {
                    query,
                    expected: before.map(AgentScalarRefV1::I64),
                    after: (after != 0).then_some(AgentScalarRefV1::I64(after)),
                    max_scalar_bytes: 64,
                },
                ctx,
                &mut output,
                &guard,
                live,
            )?;
            tail.charge_record()?;
            let guard = ctx.transaction.guard();
            ctx.transaction.validate_guard(&guard, live)?;
            let fee = tail.reserve(
                std::mem::size_of::<CommittedCurrentEmergencyCloseV1>(),
                MemoryClass::Ordinary,
            )?;
            Ok(fee.retain(CommittedCurrentEmergencyCloseV1 {
                overlay,
                result: id.to_owned(),
                event: captured,
                guard,
                identity: tail.identity(),
                memory: tail.memory().clone(),
                now,
                _phase: phase,
            }))
        },
    );
    if result
        .as_ref()
        .is_err_and(|e| !matches!(e, IndustrialDeliveryErrorV1::World(_)))
    {
        let _: Result<(), ExecutionError> =
            ctx.transaction.contribute(|_| Err(ExecutionError::Aborted));
    }
    result
}
