//! Compose original raw-key GEN producers into one charged streaming sink.
use super::{
    World,
    incremental_body::{BodyMutationContext, BodyMutationError},
};
use oasis7_formal_protocol::{
    incremental_execution::ExecutionWriteV1,
    incremental_schedule::SchedulePageWriter,
    incremental_state::{StateNamespaceV1 as Ns, StateWriteV1},
    incremental_value::{ValueNodeReader, ValueNodeWriter, ValueTreeError},
};
use oasis7_proto::memory_budget::{MemoryBudget, MemoryClass, ReservedValue};
use oasis7_world_state::{
    DomainEvent, incremental_agent_genesis::AgentGenesisContext,
    incremental_module_state_recovery::ModuleStateContentBudgetV1,
};

/// All counters are supplied by the caller and consumed across their family.
pub struct RuntimeRawGenesisWorkV1 {
    pub writes_remaining: u64,
    pub agents_remaining: u64,
    pub instances_remaining: u64,
    pub instance_count_rows_remaining: u64,
    pub schedule_entries_remaining: u64,
    pub module_states: ModuleStateContentBudgetV1,
    pub max_scalar_bytes: u64,
}

fn emit<S: FnMut(ReservedValue<ExecutionWriteV1>) -> Result<(), ValueTreeError>>(
    memory: &MemoryBudget,
    remaining: &mut u64,
    sink: &mut S,
    namespace: Ns,
    key: &[u8],
    value: [u8; 32],
) -> Result<(), BodyMutationError> {
    *remaining = remaining
        .checked_sub(1)
        .ok_or(ValueTreeError::ResourceLimited("raw GEN output rows"))?;
    let bytes = std::mem::size_of::<ExecutionWriteV1>()
        .checked_add(key.len())
        .and_then(|n| n.checked_add(512))
        .ok_or(ValueTreeError::ResourceLimited("raw GEN output admission"))?;
    let charge = memory
        .reserve(bytes, MemoryClass::Ordinary)
        .map_err(ValueTreeError::Memory)?;
    let key = key.to_vec();
    if std::mem::size_of::<ExecutionWriteV1>()
        .checked_add(key.capacity())
        .is_none_or(|n| n > charge.bytes())
    {
        return Err(ValueTreeError::ResourceLimited("raw GEN key capacity").into());
    }
    sink(charge.retain(ExecutionWriteV1 {
        namespace,
        mutation: StateWriteV1 {
            key,
            before: None,
            after: Some(value),
        },
    }))?;
    Ok(())
}

impl World {
    /// Provisional only. The mailbox callback must persist the actual original
    /// occurrence; this API cannot confer accepted-history or World readiness.
    pub fn prepare_iws_runtime_raw_genesis_v1<
        W: ValueNodeWriter + ValueNodeReader + SchedulePageWriter,
        C: FnMut() -> bool,
        S: FnMut(ReservedValue<ExecutionWriteV1>) -> Result<(), ValueTreeError>,
        L: FnMut(&str, u64, &DomainEvent) -> Result<[u8; 32], BodyMutationError>,
    >(
        &self,
        context: &mut BodyMutationContext<'_, W, C>,
        work: &mut RuntimeRawGenesisWorkV1,
        mailbox_original: &mut L,
        sink: &mut S,
    ) -> Result<(), BodyMutationError> {
        let memory = context.memory;
        let mut output =
            |ns, key: &[u8], id| emit(memory, &mut work.writes_remaining, sink, ns, key, id);
        self.write_iws_allocator_genesis(context, |key, id| output(Ns::Allocator, key, id))?;
        self.write_iws_module_state_genesis(context, &mut work.module_states, |key, id| {
            output(Ns::Business, key, id)
        })?;
        self.write_iws_module_schedule_genesis(
            context,
            &mut work.schedule_entries_remaining,
            |key, id| output(Ns::Scheduler, key, id),
        )?;
        self.write_iws_module_instance_genesis_rows(
            context,
            &mut work.instances_remaining,
            &mut work.instance_count_rows_remaining,
            work.max_scalar_bytes,
            &mut output,
        )?;
        for agent in self.state.agents.keys() {
            if (context.cancelled)() {
                return Err(ValueTreeError::Cancelled.into());
            }
            work.agents_remaining = work
                .agents_remaining
                .checked_sub(1)
                .ok_or(ValueTreeError::ResourceLimited("raw GEN agents"))?;
            self.write_iws_agent_genesis_fields(
                agent,
                &mut AgentGenesisContext {
                    mutation: BodyMutationContext {
                        sink: &mut *context.sink,
                        memory,
                        io: &mut *context.io,
                        cancelled: &mut context.cancelled,
                    },
                    emit: &mut output,
                    mailbox_original: |ordinal, event: &DomainEvent| {
                        mailbox_original(agent, ordinal, event)
                    },
                    max_scalar_bytes: work.max_scalar_bytes,
                },
            )?;
        }
        if (context.cancelled)() {
            return Err(ValueTreeError::Cancelled.into());
        }
        Ok(())
    }
}

#[cfg(test)]
#[path = "incremental_genesis_raw_tests.rs"]
mod tests;
