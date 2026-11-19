//! Closed post-write publication sources for the industrial due dispatcher.
//! None has a public constructor, Clone, raw-event setter or detachable body.
use super::*;
struct DueEventStateV1 {
    event: ReservedValue<crate::WorldEvent>,
    guard: PreparedGuard,
    identity: WorkIdentityV1,
    memory: MemoryBudget,
    now: u64,
    event_era: u64,
    _phase: MemoryReservation,
    _sources: Vec<MemoryReservation>,
}
impl DueEventStateV1 {
    // Publication/routing advances the guard revision. Existing private event
    // evidence remains usable only in its original live transaction/Core;
    // it cannot authorize another publication under its stale revision.
    fn validate_evidence_identity<M: MerkleNodeReader>(
        &self,
        tx: &ExecutionTransaction<'_, M>,
        memory: &MemoryBudget,
        now: u64,
        identity: WorkIdentityV1,
        live: &ExecutionFence,
    ) -> Result<(), BodyMutationError> {
        tx.validate_transaction_identity(&self.guard)?;
        tx.validate_guard(&tx.guard(), live)?;
        if self.identity != identity
            || !self.memory.same_pool(memory)
            || !tx.uses_budget(memory)
            || !self.event.uses_budget(memory)
            || self.now != now
            || self.event.time != now
            || self.event.caused_by.is_some()
        {
            return Err(BodyMutationError::Invalid(
                "due product existing evidence identity",
            ));
        }
        Ok(())
    }
    fn validate<M: MerkleNodeReader>(
        &self,
        tx: &ExecutionTransaction<'_, M>,
        memory: &MemoryBudget,
        now: u64,
        identity: WorkIdentityV1,
        live: &ExecutionFence,
    ) -> Result<(), BodyMutationError> {
        tx.validate_guard(&self.guard, live)?;
        if self.identity != identity
            || !self.memory.same_pool(memory)
            || !tx.uses_budget(memory)
            || !self.event.uses_budget(memory)
            || self.now != now
            || self.event.time != now
            || self.event.caused_by.is_some()
        {
            return Err(BodyMutationError::Invalid("due product post-write source"));
        }
        Ok(())
    }
}
macro_rules! cap {
    ($name:ident,$pattern:pat) => {
        pub(in crate::world) struct $name(DueEventStateV1);
        impl $name {
            pub(in crate::world) fn event(&self) -> &crate::WorldEvent {
                &self.0.event
            }
            pub(in crate::world) fn event_era(&self) -> u64 {
                self.0.event_era
            }
            pub(super) fn validate_evidence_identity<M: MerkleNodeReader>(
                &self,
                tx: &ExecutionTransaction<'_, M>,
                memory: &MemoryBudget,
                now: u64,
                identity: WorkIdentityV1,
                live: &ExecutionFence,
            ) -> Result<(), BodyMutationError> {
                self.0
                    .validate_evidence_identity(tx, memory, now, identity, live)?;
                if !matches!(&self.0.event.body, $pattern) {
                    return Err(BodyMutationError::Invalid(
                        "due product evidence event kind",
                    ));
                }
                Ok(())
            }

            pub(in crate::world) fn validate<M: MerkleNodeReader>(
                &self,
                tx: &ExecutionTransaction<'_, M>,
                memory: &MemoryBudget,
                now: u64,
                identity: WorkIdentityV1,
                live: &ExecutionFence,
            ) -> Result<(), BodyMutationError> {
                self.0.validate(tx, memory, now, identity, live)?;
                if !matches!(&self.0.event.body, $pattern) {
                    return Err(BodyMutationError::Invalid("due product source event kind"));
                }
                Ok(())
            }
        }
    };
}
cap!(
    PreparedNormalProductAttemptV1,
    crate::WorldEventBody::Domain(crate::DomainEvent::ProductValidationAttemptStarted { .. })
);
cap!(
    PreparedNormalProductRecordedV1,
    crate::WorldEventBody::Domain(crate::DomainEvent::ProductValidationRecorded { .. })
);
cap!(
    PreparedNormalProductResultV1,
    crate::WorldEventBody::Domain(
        crate::DomainEvent::ProductValidated { .. } | crate::DomainEvent::ActionRejected { .. }
    )
);
cap!(
    PreparedNormalRecipeFailureV1,
    crate::WorldEventBody::Domain(crate::DomainEvent::FactoryProductionBlocked { .. })
);
cap!(
    PreparedNormalProductModuleFailureV1,
    crate::WorldEventBody::ModuleCallFailed(..)
);
cap!(
    PreparedNormalProductCursorV1,
    crate::WorldEventBody::ProductValidationDeliveryCursorUpdated(..)
);

#[path = "incremental_normal_industry_due_validation_state.rs"]
pub(super) mod validation_state;

#[path = "incremental_normal_industry_due_failure_commit.rs"]
pub(super) mod failure_commit;

#[path = "incremental_normal_industry_due_module_failure_state.rs"]
pub(super) mod module_failure_state;

#[path = "incremental_normal_industry_due_delivery.rs"]
pub(in crate::world) mod delivery;
