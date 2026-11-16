//! Actual Current release business sources. The approved-install bridge is
//! privately minted after all original release-review preconditions pass.
use super::current::CurrentModuleMarketAgentV1;
use super::*;
use oasis7_world_state::{
    incremental_world_native::CurrentFactoryNativeRecordV1, state::ModuleReleaseRequestState,
};
pub(super) struct ValidatedCurrentReleaseApplyV1 {
    before: ReservedValue<CurrentFactoryNativeRecordV1<ModuleReleaseRequestState>>,
    request: ReservedValue<ModuleReleaseRequestState>,
    operator: CurrentModuleMarketAgentV1,
    outer: u64,
    selected: usize,
    outcome: usize,
    action_id: u64,
    guard: PreparedGuard,
    core: WorkIdentityV1,
    now: u64,
    _facts: Box<CurrentReleaseFactsPartsV1>,
    _fees: Vec<MemoryReservation>,
    _layout: MemoryReservation,
}
impl ValidatedCurrentReleaseApplyV1 {
    pub(super) fn request(&self) -> &ModuleReleaseRequestState {
        &self.request
    }
    pub(super) fn outer_request_id(&self) -> u64 {
        self.outer
    }
    pub(super) fn operator_id(&self) -> &str {
        &self.operator.id
    }
    pub(super) fn now(&self) -> u64 {
        self.now
    }
    pub(super) fn validate_evidence<M: MerkleNodeReader>(
        &self,
        tx: &ExecutionTransaction<'_, M>,
        memory: &oasis7_proto::memory_budget::MemoryBudget,
        core: WorkIdentityV1,
        now: u64,
        live: &ExecutionFence,
    ) -> Result<(), BodyMutationError> {
        tx.validate_transaction_identity(&self.guard)?;
        tx.validate_guard(&tx.guard(), live)?;
        tx.validate_transaction_identity(&self.operator.guard)?;
        if self.core != core
            || self.now != now
            || !self._layout.uses_budget(memory)
            || !self.before.uses_budget(memory)
            || !self.request.uses_budget(memory)
            || !self.operator._fee.uses_budget(memory)
            || self._fees.iter().any(|fee| !fee.uses_budget(memory))
        {
            return Err(invalid());
        }
        if !self._facts.slots.uses_budget(memory)
            || self._facts.fees.iter().any(|fee| !fee.uses_budget(memory))
        {
            return Err(invalid());
        }
        for owner in &self._facts.epochs {
            owner.validate(tx, memory, core, now, live)?;
        }
        for owner in &self._facts.profiles {
            owner.validate_provenance(tx, memory, core, live)?;
        }
        Ok(())
    }
    pub(super) fn validate_provenance<M: MerkleNodeReader>(
        &self,
        selected: &SelectedPendingActionV1,
        outcome: &ReservedValue<RootRulePhaseOutcomeV1>,
        tx: &ExecutionTransaction<'_, M>,
        memory: &oasis7_proto::memory_budget::MemoryBudget,
        core: WorkIdentityV1,
        now: u64,
        live: &ExecutionFence,
    ) -> Result<(), BodyMutationError> {
        selected.validate(tx)?;
        if self.selected != selected as *const _ as usize
            || self.outcome != outcome as *const _ as usize
            || self.action_id != selected.envelope().id
            || !outcome.uses_budget(memory)
        {
            return Err(invalid());
        }
        self.validate_evidence(tx, memory, core, now, live)
    }
}

#[path = "incremental_normal_action_module_lifecycle_release_facts_current.rs"]
mod current;
pub(super) use self::current::{CurrentModuleReleaseFactsV1, CurrentReleaseFactsPartsV1};

#[path = "incremental_normal_action_module_lifecycle_release_facts_apply.rs"]
mod apply;
pub(super) use self::apply::{
    CurrentReleaseApplyDecisionV1, CurrentReleaseApplyRejectionV1, prepare_current_release_apply_v1,
};
