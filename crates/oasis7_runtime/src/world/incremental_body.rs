//! Narrow body reducer contribution for an outer IWS transaction.
//! This does not append an event, advance allocators, install a cache or publish
//! a commit. The driver must stage those obligations in the same transaction.
use super::{World, body::PreparedBodyAttributesUpdate};
use crate::agent_state::BodyKernelView;
use oasis7_formal_protocol::{
    incremental_execution::{ExecutionError, ExecutionTransaction},
    incremental_state::{MerkleNodeReader, StateNamespaceV1},
    incremental_value::{ValueNodeWriter, ValueTreeError},
};
use oasis7_world_state::incremental_agent_body::{AgentBodyKeyV1, AgentBodyValueV1};

pub(super) use oasis7_world_state::incremental_agent_mutation::write_field_bytes;
pub use oasis7_world_state::incremental_agent_mutation::{
    BodyMutationContext, BodyMutationError, write_body_value,
};

pub(super) use oasis7_world_state::incremental_agent_mutation::DiscardValueNodesV1 as Discard;

impl World {
    /// Validate using the existing reducer and contribute one body subfield.
    /// A stale materialized body is rejected against the authenticated overlay;
    /// no unrelated agent, mailbox, full state or history is visited or cloned.
    pub fn contribute_iws_body_attributes_update<
        R: MerkleNodeReader,
        W: ValueNodeWriter,
        C: FnMut() -> bool,
    >(
        &self,
        agent_id: &str,
        view: &BodyKernelView,
        txn: &mut ExecutionTransaction<'_, R>,
        context: &mut BodyMutationContext<'_, W, C>,
    ) -> Result<(), BodyMutationError> {
        txn.contribute(|txn| {
            if !txn.uses_budget(context.memory) {
                return Err(BodyMutationError::Invalid("transaction memory pool"));
            }
            AgentBodyKeyV1::new(agent_id).map_err(BodyMutationError::Invalid)?;
            let _prepared_charge = context
                .memory
                .reserve(
                    agent_id.len() + 512,
                    oasis7_proto::memory_budget::MemoryClass::Ordinary,
                )
                .map_err(ExecutionError::Memory)?;
            let current = self
                .state
                .agents
                .get(agent_id)
                .ok_or(BodyMutationError::Invalid("body agent missing"))?;
            super::body::validate_body_kernel_view(&current.state.body_view, view)
                .map_err(BodyMutationError::Rejected)?;
            let prepared =
                PreparedBodyAttributesUpdate::new(agent_id.into(), view.clone(), self.state.time);
            prepared.contribute_iws(&current.state.body_view, current.last_active, txn, context)
        })
    }
}

impl PreparedBodyAttributesUpdate {
    pub(super) fn contribute_iws<R: MerkleNodeReader, W: ValueNodeWriter, C: FnMut() -> bool>(
        &self,
        current_view: &BodyKernelView,
        current_active: u64,
        txn: &mut ExecutionTransaction<'_, R>,
        context: &mut BodyMutationContext<'_, W, C>,
    ) -> Result<(), BodyMutationError> {
        let key = AgentBodyKeyV1::new(&self.agent_id).map_err(BodyMutationError::Invalid)?;
        super::incremental_clock::check_clock(self.last_active, txn, context)?;

        let before = AgentBodyValueV1 {
            body_view: current_view.clone(),
            last_active: current_active,
        };
        let before_ref = write_body_value(
            &before,
            &mut BodyMutationContext {
                sink: &mut Discard,
                memory: context.memory,
                io: context.io,
                cancelled: &mut context.cancelled,
            },
        )?;
        if txn.read(StateNamespaceV1::Business, key.as_bytes())? != Some(before_ref) {
            return Err(BodyMutationError::Invalid(
                "stale body cache or absent parent body",
            ));
        }
        let after = AgentBodyValueV1 {
            body_view: self.body_view.clone(),
            last_active: self.last_active,
        };
        let after_ref = write_body_value(&after, context)?;
        txn.write(StateNamespaceV1::Business, key.as_bytes(), Some(after_ref))?;
        if (context.cancelled)() {
            return Err(ValueTreeError::Cancelled.into());
        }
        Ok(())
    }
}

#[cfg(test)]
#[path = "incremental_body_tests.rs"]
pub(super) mod tests;

#[cfg(test)]
#[path = "incremental_body_cancellation_tests.rs"]
mod cancellation_tests;
