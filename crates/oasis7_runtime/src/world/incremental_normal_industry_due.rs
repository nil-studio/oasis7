//! Fixed industrial due lifecycle. Private post-write owners are minted only
//! after pending removal, receipt/settled insertion and all related Current
//! changes have succeeded in the same transaction/Core.
use crate::world::incremental_body::BodyMutationError;
use oasis7_formal_protocol::{
    controlled_authority::formal::bounded_context::WorkIdentityV1,
    incremental_execution::{ExecutionFence, ExecutionTransaction, PreparedGuard},
    incremental_state::MerkleNodeReader,
};
use oasis7_proto::memory_budget::{MemoryBudget, MemoryReservation, ReservedValue};

#[path = "incremental_normal_industry_due_recipe_read.rs"]
mod recipe_read;

/// A genuine RecipeCompleted system-event publication source. No public
/// constructor, Clone, body setter or unbound ID can produce this owner.
pub(in crate::world) struct PreparedNormalRecipeCompletedV1 {
    event: ReservedValue<crate::WorldEvent>,
    guard: PreparedGuard,
    identity: WorkIdentityV1,
    now: u64,
    memory: MemoryBudget,
    _event_phase: MemoryReservation,
    _pending: ReservedValue<crate::RecipeJobState>,
    _receipt: ReservedValue<crate::RecipeCompletionReceiptV1>,
}
impl PreparedNormalRecipeCompletedV1 {
    pub(in crate::world) fn event(&self) -> &crate::WorldEvent {
        &self.event
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
        if self.identity != identity
            || !tx.uses_budget(memory)
            || !self.memory.same_pool(memory)
            || !self.event.uses_budget(memory)
            || !self._pending.uses_budget(memory)
            || !self._receipt.uses_budget(memory)
            || self.now != now
            || self.event.time != now
            || self.event.caused_by.is_some()
        {
            return Err(BodyMutationError::Invalid(
                "RecipeCompleted post-write owner binding",
            ));
        }
        match &self.event.body {
            crate::WorldEventBody::Domain(crate::DomainEvent::RecipeCompleted {
                job_id, ..
            }) if *job_id == self._pending.job_id && *job_id == self._receipt.job_id => Ok(()),
            _ => Err(BodyMutationError::Invalid(
                "RecipeCompleted post-write event",
            )),
        }
    }
}
#[path = "incremental_normal_industry_due_current.rs"]
mod current;

/// A ready terminal is privately produced only after Current industrial
/// root/owner/capacity checks and real output validation receipts succeed.
/// It is consumed once by the actual state committer; it is not an Action or
/// ordinary publication permit and cannot be constructed by API callers.
struct ValidatedDueRecipeTerminalV1 {
    pending: ReservedValue<crate::RecipeJobState>,
    guard: PreparedGuard,
    identity: WorkIdentityV1,
    memory: MemoryBudget,
    now: u64,
    _terminal_fees: Vec<MemoryReservation>,
}
impl ValidatedDueRecipeTerminalV1 {
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
            || !tx.uses_budget(memory)
            || !self.memory.same_pool(memory)
            || !self.pending.uses_budget(memory)
            || self.now != now
        {
            return Err(BodyMutationError::Invalid("due Recipe terminal binding"));
        }
        Ok(())
    }
}
#[path = "incremental_normal_industry_due_build_select.rs"]
mod build_select;
#[path = "incremental_normal_industry_due_owner.rs"]
mod owner;
#[path = "incremental_normal_industry_due_recipe_commit.rs"]
mod recipe_commit;

#[path = "incremental_normal_industry_due_product_call.rs"]
mod product_call;
pub(in crate::world) use product_call::PreparedDueProductCallV1;

#[path = "incremental_normal_industry_due_caps.rs"]
mod caps;
pub(in crate::world) use caps::{
    PreparedNormalProductAttemptV1, PreparedNormalProductCursorV1,
    PreparedNormalProductModuleFailureV1, PreparedNormalProductRecordedV1,
    PreparedNormalProductResultV1, PreparedNormalRecipeFailureV1,
};

/// Fixed-terminal or recorded-product failure provenance. Constructors remain
/// inside this due driver; a reason string or caller event is never sufficient.
struct PreparedDueRecipeFailureV1<'a> {
    pending: &'a ReservedValue<crate::RecipeJobState>,
    cause: DueRecipeFailureCauseV1<'a>,
    guard: PreparedGuard,
    identity: WorkIdentityV1,
    memory: MemoryBudget,
    now: u64,
    _phase: MemoryReservation,
}
enum DueRecipeFailureCauseV1<'a> {
    IndustrialTerminal {
        error: ReservedValue<crate::WorldError>,
    },
    ProductValidation {
        receipt: &'a ReservedValue<crate::ProductValidationReceiptV1>,
        result: &'a PreparedNormalProductResultV1,
    },
    PersistedProductValidation {
        history: &'a ReservedValue<caps::validation_state::CurrentProductHistoryV1>,
    },
}
impl PreparedDueRecipeFailureV1<'_> {
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
            || !tx.uses_budget(memory)
            || !self.memory.same_pool(memory)
            || !self.pending.uses_budget(memory)
            || self.now != now
        {
            return Err(BodyMutationError::Invalid(
                "industrial failure Current source",
            ));
        }
        if let DueRecipeFailureCauseV1::IndustrialTerminal { error } = &self.cause {
            if !error.uses_budget(memory) {
                return Err(BodyMutationError::Invalid("industrial failure error pool"));
            }
        }
        if let DueRecipeFailureCauseV1::PersistedProductValidation { history } = &self.cause {
            if !history.uses_budget(memory) {
                return Err(BodyMutationError::Invalid(
                    "industrial persisted product pool",
                ));
            }
            history.validate_evidence_identity(tx, memory, now, identity, self.pending, live)?;
            let receipt = history.receipt().ok_or(BodyMutationError::Invalid(
                "industrial persisted product receipt",
            ))?;
            if crate::world::World::product_validation_decision_matches_stack(
                &receipt.decision,
                &receipt.stack,
            ) {
                return Err(BodyMutationError::Invalid(
                    "industrial persisted product is accepted",
                ));
            }
        }
        if let DueRecipeFailureCauseV1::ProductValidation { receipt, result } = &self.cause {
            result.validate_evidence_identity(tx, memory, now, identity, live)?;
            if !matches!(&result.event().body,crate::WorldEventBody::Domain(crate::DomainEvent::ActionRejected{action_id,..}) if *action_id==self.pending.job_id)
            {
                return Err(BodyMutationError::Invalid(
                    "industrial failure product rejection source",
                ));
            }
            if !receipt.uses_budget(memory)
                || receipt.job_id != self.pending.job_id
                || receipt.requester_agent_id != self.pending.requester_agent_id
            {
                return Err(BodyMutationError::Invalid(
                    "industrial failure recorded product source",
                ));
            }
        }
        Ok(())
    }
}

#[path = "incremental_normal_industry_due_build_driver.rs"]
mod build_driver;

#[path = "incremental_normal_industry_due_recipe_select.rs"]
mod recipe_select;

#[path = "incremental_normal_industry_due_graph.rs"]
mod graph;

#[path = "incremental_normal_industry_due_decision.rs"]
mod decision;

#[path = "incremental_normal_industry_due_product_values.rs"]
mod product_values;

#[path = "incremental_normal_industry_due_invalid_output.rs"]
mod invalid_output;

#[path = "incremental_normal_industry_due_product_driver.rs"]
mod product_driver;

#[path = "incremental_normal_industry_due_plan.rs"]
mod plan;

#[path = "incremental_normal_industry_due_terminal.rs"]
mod terminal;

#[path = "incremental_normal_industry_due_disposition.rs"]
mod disposition;

#[path = "incremental_normal_industry_due_directories.rs"]
mod directories;

pub(in crate::world) use caps::delivery;

#[path = "incremental_normal_industry_due_owner_sources.rs"]
mod owner_sources;

#[path = "incremental_normal_industry_due_driver.rs"]
mod driver;
pub(in crate::world) use driver::{NormalIndustryDueSourcesV1, NormalIndustryDueWorkV1};

#[path = "incremental_normal_industry_due_deferred.rs"]
pub(in crate::world) mod deferred;

#[cfg(test)]
#[path = "incremental_normal_industry_due_tests.rs"]
mod tests;
