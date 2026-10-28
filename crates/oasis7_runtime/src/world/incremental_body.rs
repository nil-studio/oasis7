//! Narrow body reducer contribution for an outer IWS transaction.
//! This does not append an event, advance allocators, install a cache or publish
//! a commit. The driver must stage those obligations in the same transaction.
use super::{World, body::PreparedBodyAttributesUpdate};
use crate::agent_state::BodyKernelView;
use oasis7_formal_protocol::{
    incremental_execution::{ExecutionError, ExecutionTransaction},
    incremental_state::{MerkleNodeReader, StateNamespaceV1},
    incremental_value::{ValueIoBudget, ValueNodeWriter, ValueStreamWriter, ValueTreeError},
};
use oasis7_proto::memory_budget::{MemoryBudget, ReservedValue};
use oasis7_world_state::incremental_agent_body::{
    AgentBodyKeyV1, AgentBodyValueV1, WORLD_TIME_KEY_V1, world_time_bytes_v1,
};
use std::io::Write;

#[derive(Debug)]
pub enum BodyMutationError {
    Execution(ExecutionError),
    Value(ValueTreeError),
    Invalid(&'static str),
    Rejected(String),
}
impl From<ExecutionError> for BodyMutationError {
    fn from(error: ExecutionError) -> Self {
        Self::Execution(error)
    }
}
impl From<ValueTreeError> for BodyMutationError {
    fn from(error: ValueTreeError) -> Self {
        Self::Value(error)
    }
}
impl std::fmt::Display for BodyMutationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Execution(error) => error.fmt(f),
            Self::Value(error) => error.fmt(f),
            Self::Invalid(reason) => write!(f, "invalid IWS body mutation: {reason}"),
            Self::Rejected(reason) => write!(f, "rejected body update: {reason}"),
        }
    }
}
impl std::error::Error for BodyMutationError {}

pub struct BodyMutationContext<'a, W, C> {
    pub sink: &'a mut W,
    pub memory: &'a MemoryBudget,
    pub io: &'a mut ValueIoBudget,
    pub cancelled: C,
}

struct Discard;
impl ValueNodeWriter for Discard {
    fn put_node(&mut self, _: [u8; 32], _: ReservedValue<Vec<u8>>) -> Result<(), ValueTreeError> {
        Ok(())
    }
}

/// Reusable by new-world genesis lowering and the typed prepared reducer.
/// All output is provisional until the outer transaction is accepted.
pub fn write_body_value<W: ValueNodeWriter, C: FnMut() -> bool>(
    value: &AgentBodyValueV1,
    context: &mut BodyMutationContext<'_, W, C>,
) -> Result<[u8; 32], BodyMutationError> {
    write_field_bytes(&value.canonical_bytes(), context)
}

fn write_field_bytes<W: ValueNodeWriter, C: FnMut() -> bool>(
    bytes: &[u8],
    context: &mut BodyMutationContext<'_, W, C>,
) -> Result<[u8; 32], BodyMutationError> {
    let mut writer = ValueStreamWriter::new(
        context.sink,
        context.memory,
        context.io,
        bytes.len() as u64,
        &mut context.cancelled,
    )?;
    if writer.write_all(bytes).is_err() {
        // finish retains the original typed IO/resource/cancellation failure.
        return Err(writer
            .finish()
            .err()
            .unwrap_or(ValueTreeError::Invalid("body value write failed"))
            .into());
    }
    Ok(writer.finish()?.digest()?)
}

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
        let clock_ref = write_field_bytes(
            &world_time_bytes_v1(self.last_active),
            &mut BodyMutationContext {
                sink: &mut Discard,
                memory: context.memory,
                io: context.io,
                cancelled: &mut context.cancelled,
            },
        )?;
        if txn.read(StateNamespaceV1::Allocator, WORLD_TIME_KEY_V1)? != Some(clock_ref) {
            return Err(BodyMutationError::Invalid("stale world clock cache"));
        }

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
        Ok(())
    }
}

#[cfg(test)]
#[path = "incremental_body_tests.rs"]
mod tests;
