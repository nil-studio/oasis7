//! Closed command results for the sole lifecycle loop. Every fresh variant
//! owns the concrete post-write source; retained Observe and request retries
//! are distinct real sources, never generic successful guard substitutions.
use super::*;
use crate::world::incremental_normal_tail::NormalTailWorkV1;
pub(super) enum LifecycleWrittenSourceV1 {
    SupplyClose(supply_close::CommittedCompleteSupplyCloseV1),
    SupplySettle(BoundCommittedSupplySettlementV1),
    Advance(order::CommittedCurrentOrderAdvanceV1),
    Deliver(delivery::CommittedWorldRuleOrderDeliveryV1),
    Observe(observe::CommittedCurrentObservationV2),
    RetainedObserve(observe::RetainedCurrentObservationV2),
    EmergencyClose(ReservedValue<emergency_close::CommittedCurrentEmergencyCloseV1>),
}
pub(super) struct FreshWrittenLifecycleCommandV1 {
    pub(super) header: header::PreparedFreshLifecycleHeaderV1,
    pub(super) source: LifecycleWrittenSourceV1,
}
impl FreshWrittenLifecycleCommandV1 {
    pub(super) fn result_id(&self) -> &str {
        match &self.source {
            LifecycleWrittenSourceV1::SupplyClose(source) => source.result_id(),
            LifecycleWrittenSourceV1::SupplySettle(source) => &source.result,
            LifecycleWrittenSourceV1::Advance(source) => source.result_id(),
            LifecycleWrittenSourceV1::Deliver(source) => source.target_agent_id(),
            LifecycleWrittenSourceV1::Observe(source) => source.result_id(),
            LifecycleWrittenSourceV1::RetainedObserve(source) => source.result_id(),
            LifecycleWrittenSourceV1::EmergencyClose(source) => source.result_id(),
        }
    }
    pub(super) fn validate<M: MerkleNodeReader>(
        &self,
        tx: &ExecutionTransaction<'_, M>,
        tail: &mut NormalTailWorkV1<'_, '_>,
        now: u64,
        live: &ExecutionFence,
    ) -> Result<(), delivery::IndustrialDeliveryErrorV1> {
        match &self.source {
            LifecycleWrittenSourceV1::SupplyClose(source) => source
                .validate(&self.header, tx, now, tail, live)
                .map_err(Into::into),
            LifecycleWrittenSourceV1::SupplySettle(source) => source
                .validate(&self.header, tx, tail, now, live)
                .map_err(Into::into),
            LifecycleWrittenSourceV1::Advance(source) => source
                .validate(&self.header, tx, now, tail, live)
                .map_err(Into::into),
            LifecycleWrittenSourceV1::Deliver(source) => source
                .validate(&self.header, tx, now, tail, live)
                .map_err(Into::into),
            LifecycleWrittenSourceV1::Observe(source) => source
                .validate(&self.header, tx, tail, now, live)
                .map_err(Into::into),
            LifecycleWrittenSourceV1::RetainedObserve(source) => source
                .validate(&self.header, tx, tail, now, live)
                .map_err(Into::into),
            LifecycleWrittenSourceV1::EmergencyClose(source) => source
                .validate(&self.header, tx, now, tail, live)
                .map_err(Into::into),
        }
    }
}
pub(super) struct BoundCommittedSupplySettlementV1 {
    pub(super) source: supply::CommittedSupplySettlementV1,
    pub(super) event: crate::DomainEvent,
    pub(super) result: String,
    pub(super) memory: MemoryBudget,
    pub(super) identity: WorkIdentityV1,
    pub(super) now: u64,
    pub(super) _phase: MemoryReservation,
}
impl BoundCommittedSupplySettlementV1 {
    fn validate<M: MerkleNodeReader>(
        &self,
        header: &header::PreparedFreshLifecycleHeaderV1,
        tx: &ExecutionTransaction<'_, M>,
        tail: &mut NormalTailWorkV1<'_, '_>,
        now: u64,
        live: &ExecutionFence,
    ) -> Result<(), BodyMutationError> {
        tx.validate_guard(&self.source.guard, live)?;
        if self.identity != tail.identity()
            || self.source.identity != tail.identity()
            || !self.memory.same_pool(tail.memory())
            || self.now != now
        {
            return Err(BodyMutationError::Invalid("bound settlement source"));
        }
        let (
            crate::DomainEvent::WorldRuleCommitted {
                action_id: a,
                actor_id: aa,
                request_id: ar,
                command: WorldRuleCommandV1::SettleSupply { job_id: aj },
                governance_epoch: ae,
                world_binding: ab,
            },
            crate::DomainEvent::WorldRuleCommitted {
                action_id: b,
                actor_id: ba,
                request_id: br,
                command: WorldRuleCommandV1::SettleSupply { job_id: bj },
                governance_epoch: be,
                world_binding: bb,
            },
        ) = (&self.event, header.event())
        else {
            return Err(BodyMutationError::Invalid("bound settlement header kind"));
        };
        let bytes = aa
            .len()
            .min(ba.len())
            .checked_add(ar.len().min(br.len()))
            .and_then(|n| {
                n.checked_add(match (ab, bb) {
                    (Some(a), Some(b)) => a.len().min(b.len()),
                    _ => 0,
                })
            })
            .ok_or(BodyMutationError::Invalid("bound settlement comparison"))?;
        tail.charge(bytes)?;
        if a != b || aa != ba || ar != br || aj != bj || ae != be || ab != bb {
            return Err(BodyMutationError::Invalid("bound settlement header"));
        }
        Ok(())
    }
}
pub(super) enum LifecycleCommandPreparationV1 {
    Fresh(FreshWrittenLifecycleCommandV1),
    Retained(header::PreparedRetainedLifecycleHeaderV1),
    Rejected(ReservedValue<crate::WorldError>),
}
