//! Fixed restricted-grant and agent-claim epoch stages. A successful stage is
//! not a normal-tick execution or publication permit.
use crate::world::incremental_body::BodyMutationError;
use oasis7_formal_protocol::{
    controlled_authority::formal::bounded_context::WorkIdentityV1,
    incremental_execution::{ExecutionFence, ExecutionTransaction, PreparedGuard},
    incremental_state::MerkleNodeReader,
};
use oasis7_proto::memory_budget::{MemoryBudget, MemoryReservation, ReservedValue};
#[path = "incremental_normal_claim_epochs_evaluate.rs"]
pub(in crate::world) mod evaluate;
/// Borrowed actual parent DATA. Current records are re-opened by each fixed
/// committer; this view carries no read-set freshness or acceptance permission.
pub(in crate::world) struct ClaimEpochStateSourcesV1<'a>{
 pub roots:&'a oasis7_formal_protocol::incremental_execution::RootSetV1,
 pub agents:&'a ReservedValue<oasis7_world_state::incremental_agent_subtree::AgentSubtreeStructureQualificationV1>,
 pub native:&'a ReservedValue<oasis7_world_state::incremental_world_native::WorldNativeFieldsQualificationV1>,
 pub fixed:&'a ReservedValue<oasis7_world_state::incremental_fixed_economy_records::FixedEconomyFamilyQualificationV1>,
}
/// Only a complete fixed Current committer constructs this post-write source.
/// The event, exact Current dependencies and their allocation fees remain live
/// through counted persistence and the phase's deferred routing.
pub(in crate::world) struct PreparedNormalClaimEpochEventV1 {
    event: ReservedValue<crate::WorldEvent>,
    source: PreparedClaimEpochMutationV1,
    guard: PreparedGuard,
}
struct PreparedClaimEpochMutationV1 {
    event: Option<crate::DomainEvent>,
    guard: PreparedGuard,
    identity: WorkIdentityV1,
    memory: MemoryBudget,
    now: u64,
    preserved_last_active: Option<u64>,
    _phase: MemoryReservation,
    _state: CommittedEpochStateV1,
}
impl PreparedNormalClaimEpochEventV1 {
    pub(in crate::world) fn event(&self) -> &crate::WorldEvent {
        &self.event
    }
    pub(in crate::world) fn preserved_actor_last_active_v1(&self) -> Option<u64> {
        self.source.preserved_last_active
    }
    pub(in crate::world) fn validate<M: MerkleNodeReader>(
        &self,
        tx: &ExecutionTransaction<'_, M>,
        memory: &MemoryBudget,
        now: u64,
        identity: WorkIdentityV1,
        live: &ExecutionFence,
    ) -> Result<(), BodyMutationError> {
        tx.validate_guard(&self.guard, live)?;
        if !self.source.memory.same_pool(memory)
            || !self.event.uses_budget(memory)
            || !tx.uses_budget(memory)
            || self.source.now != now
            || self.source.identity != identity
            || self.event.time != now
            || self.event.caused_by.is_some()
        {
            return Err(BodyMutationError::Invalid("claim epoch committed source"));
        }
        if !matches!(
            &self.event.body,
            crate::WorldEventBody::Domain(
                crate::DomainEvent::RestrictedStarterClaimGrantExpired { .. }
                    | crate::DomainEvent::AgentClaimUpkeepSettled {
                        activity_contract_version: 1,
                        ..
                    }
                    | crate::DomainEvent::AgentClaimEnteredGrace { .. }
                    | crate::DomainEvent::AgentClaimIdleWarning { .. }
                    | crate::DomainEvent::AgentClaimReleased { .. }
                    | crate::DomainEvent::AgentClaimReclaimed { .. }
            )
        ) {
            return Err(BodyMutationError::Invalid("claim epoch event family"));
        }
        Ok(())
    }
}
fn finalize_current_claim_epoch_v1<M, R, E, C>(
    mut source: PreparedClaimEpochMutationV1,
    ctx: &mut oasis7_world_state::incremental_field_overlay::WorldFieldOverlayContextV1<
        '_,
        '_,
        '_,
        M,
        R,
        E,
        C,
    >,
    common:&mut oasis7_formal_protocol::controlled_authority::formal::bounded_context::FormalBoundedContextV1<'_>,
    live: &ExecutionFence,
) -> Result<PreparedNormalClaimEpochEventV1, BodyMutationError>
where
    M: MerkleNodeReader,
    R: oasis7_formal_protocol::incremental_value::ValueNodeReader,
    E: FnMut() -> Option<oasis7_formal_protocol::incremental_value::ValueTreeError>,
    C: FnMut() -> bool,
{
    ctx.transaction.validate_guard(&source.guard, live)?;
    if !source.memory.same_pool(common.memory()) || source.identity != common.work().identity() {
        return Err(BodyMutationError::Invalid("claim epoch finalize Common"));
    }
    common
        .charge(1, std::mem::size_of::<crate::WorldEvent>() as u64)
        .map_err(|e| BodyMutationError::Original(Box::new(e)))?;
    let fee = common
        .memory()
        .reserve(
            std::mem::size_of::<crate::WorldEvent>(),
            oasis7_proto::memory_budget::MemoryClass::Ordinary,
        )
        .map_err(oasis7_formal_protocol::incremental_value::ValueTreeError::Memory)?;
    let id =
        crate::world::incremental_normal_tail::event_preview::preview_current_normal_event_id_v1(
            ctx, common,
        )?;
    // Move the existing paid Domain into its envelope. The mutation's original
    // payload reservation remains in source until the counted event is dropped.
    let domain = source
        .event
        .take()
        .ok_or(BodyMutationError::Invalid("claim epoch consumed domain"))?;
    let event = fee.retain(crate::WorldEvent {
        id,
        time: source.now,
        caused_by: None,
        body: crate::WorldEventBody::Domain(domain),
    });
    Ok(PreparedNormalClaimEpochEventV1 {
        event,
        source,
        guard: ctx.transaction.guard(),
    })
}
#[path = "incremental_normal_claim_epochs_agent.rs"]
mod agent;
#[path = "incremental_normal_claim_epochs_claim.rs"]
mod claim;
#[path = "incremental_normal_claim_epochs_grant.rs"]
mod grant;
#[path = "incremental_normal_claim_epochs_money.rs"]
mod money;
enum CommittedEpochStateV1 {
    Claim {
        source: claim::CommittedClaimEpochStateV1,
        _after_fee: MemoryReservation,
    },
    Grant(grant::CommittedGrantEpochStateV1),
}

#[path = "incremental_normal_claim_epochs_driver.rs"]
mod driver;
pub(in crate::world) use driver::NormalClaimEpochSourcesV1;

#[cfg(test)]
#[path = "incremental_normal_claim_epochs_tests.rs"]
mod tests;
