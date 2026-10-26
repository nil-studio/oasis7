//! BFT governance uses its own verified parent and execution phase. It never
//! fabricates a controlled-mode retained-history parent or qualification.

use super::{World, WorldError};
use crate::{GovernanceMainTokenControllerRegistry, RuntimeCommittedTickContext};
use oasis7_consensus::tendermint_v2::BftHeightProposerScheduleV2;

#[cfg(test)]
#[path = "governance_bft_parent_tests.rs"]
mod tests;

#[derive(Debug, Clone)]
pub(super) struct BftGovernanceParentV2 {
    pub(super) schedule: BftHeightProposerScheduleV2,
    pub(super) controller_registry: Option<GovernanceMainTokenControllerRegistry>,
    execution: Option<(RuntimeCommittedTickContext, bool)>,
}

fn invalid(reason: impl Into<String>) -> WorldError {
    WorldError::GovernancePolicyInvalid {
        reason: reason.into(),
    }
}

impl World {
    /// Attach only after the caller has pinned the genesis and independently
    /// recovered this exact cursor. The opaque cursor validates protocol
    /// history; it does not authenticate arbitrary caller-supplied genesis.
    /// This method additionally binds the actual World and immutable policy.
    pub fn attach_bft_governance_parent_v2(
        &mut self,
        schedule: &BftHeightProposerScheduleV2,
    ) -> Result<(), WorldError> {
        if self
            .formal_history_reader
            .as_ref()
            .and_then(|reader| reader.parent.as_ref())
            .is_some()
            || self.formal_execution_tick.is_some()
            || self.bft_governance_execution_in_progress()
        {
            return Err(invalid(
                "BFT parent cannot replace an active qualified execution",
            ));
        }
        self.validate_bft_parent_world(schedule)?;
        let registry = self
            .governance_main_token_controller_registry()
            .cloned()
            .map(Self::validate_governance_main_token_controller_registry)
            .transpose()?;
        self.bft_governance_parent = Some(BftGovernanceParentV2 {
            schedule: schedule.clone(),
            controller_registry: registry,
            execution: None,
        });
        Ok(())
    }

    fn validate_bft_parent_world(
        &self,
        schedule: &BftHeightProposerScheduleV2,
    ) -> Result<(), WorldError> {
        if self.state.time != schedule.parent_tick()
            || self.current_state_root_hash()? != schedule.parent_state_root()
            || self.current_manifest_hash()? != schedule.parent_manifest_hash()
            || self.governance_execution_policy().epoch_length_ticks
                != schedule.policy().epoch_length_ticks
            || schedule.height()
                != schedule
                    .parent_tick()
                    .checked_add(1)
                    .ok_or_else(|| invalid("BFT governance parent height exhausted"))?
        {
            return Err(invalid(
                "BFT governance parent World/root/time/manifest/policy mismatch",
            ));
        }
        Ok(())
    }

    /// Begin a detached candidate's exact successor execution. This does not
    /// publish its World or make the candidate a committed result.
    pub fn begin_bft_execution_tick(
        &mut self,
        context: &RuntimeCommittedTickContext,
    ) -> Result<(), WorldError> {
        if self.formal_execution_tick.is_some() || self.bft_governance_execution_in_progress() {
            return Err(invalid("BFT execution tick already begun or conflicting"));
        }
        let parent = self
            .bft_governance_parent
            .as_ref()
            .ok_or_else(|| invalid("BFT execution requires its verified parent"))?;
        self.validate_bft_parent_world(&parent.schedule)?;
        if context.height != parent.schedule.height()
            || context.epoch != parent.schedule.authority_epoch()
            || context.node_block_hash.trim().is_empty()
            || context.action_root.trim().is_empty()
        {
            return Err(invalid(
                "BFT execution tick does not match its exact parent",
            ));
        }
        self.bft_governance_parent
            .as_mut()
            .expect("checked parent")
            .execution = Some((context.clone(), false));
        Ok(())
    }

    pub(super) fn bft_governance_execution_in_progress(&self) -> bool {
        self.bft_governance_parent
            .as_ref()
            .is_some_and(|parent| parent.execution.is_some())
    }

    pub(super) fn governance_execution_is_qualified(&self) -> bool {
        self.formal_execution_tick.is_some()
            || self.bft_governance_parent.as_ref().is_some_and(|parent| {
                parent
                    .execution
                    .as_ref()
                    .is_some_and(|(context, finished)| {
                        !finished && context.height == self.state.time
                    })
            })
    }

    pub(super) fn validate_bft_execution_step(
        &self,
        context: &RuntimeCommittedTickContext,
        resume: bool,
    ) -> Result<(), WorldError> {
        let Some(parent) = &self.bft_governance_parent else {
            return Ok(());
        };
        if resume || parent.execution.as_ref() != Some(&(context.clone(), false)) {
            return Err(invalid(
                "BFT successor requires its exact begun execution context",
            ));
        }
        self.validate_bft_parent_world(&parent.schedule)
    }

    pub(super) fn complete_bft_execution_step(&mut self, context: &RuntimeCommittedTickContext) {
        if let Some(parent) = self.bft_governance_parent.as_mut()
            && parent.execution.as_ref() == Some(&(context.clone(), false))
        {
            parent.execution = Some((context.clone(), true));
        }
    }

    /// The committed-context step already recorded its Tick. Finish only the
    /// private phase; a second finish cannot append another Tick or authorize.
    pub fn finish_bft_execution_tick(
        &mut self,
        context: &RuntimeCommittedTickContext,
    ) -> Result<(), WorldError> {
        let parent = self
            .bft_governance_parent
            .as_ref()
            .ok_or_else(|| invalid("BFT execution parent is absent"))?;
        if parent.execution.as_ref() != Some(&(context.clone(), true))
            || self.state.time != context.height
            || self.governance_execution_policy().epoch_length_ticks
                != parent.schedule.policy().epoch_length_ticks
        {
            return Err(invalid(
                "BFT execution has not completed its exact successor tick",
            ));
        }
        self.verify_tick_consensus_chain()?;
        self.bft_governance_parent = None;
        Ok(())
    }
}
