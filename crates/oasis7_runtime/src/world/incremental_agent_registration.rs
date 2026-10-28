//! New agent fields are inserted through the common transaction, never by
//! cloning a World or installing a provisional materialized AgentCell.
use super::{
    incremental_agent_genesis::{AgentGenesisContext, write_agent_cell_fields},
    incremental_body::{BodyMutationContext, BodyMutationError},
    incremental_body_event::read_fixed,
};
use oasis7_agent_api::world_action::GeoPos;
use oasis7_formal_protocol::{
    incremental_execution::{ExecutionError, ExecutionTransaction},
    incremental_state::{MerkleNodeReader, StateNamespaceV1 as Ns},
    incremental_value::{ValueNodeReader, ValueNodeWriter},
};
use oasis7_proto::memory_budget::MemoryClass;
use oasis7_world_state::{
    AgentActivityV1, AgentCell, AgentState,
    incremental_agent_body::{AgentBodyKeyV1, WORLD_TIME_KEY_V1, world_time_bytes_v1},
};

pub(super) fn contribute_registration<
    R: MerkleNodeReader,
    V: ValueNodeReader,
    W: ValueNodeWriter,
    C: FnMut() -> bool,
>(
    agent: &str,
    pos: &GeoPos,
    version: u8,
    time: u64,
    txn: &mut ExecutionTransaction<'_, R>,
    values: &V,
    context: &mut BodyMutationContext<'_, W, C>,
) -> Result<(), BodyMutationError> {
    txn.contribute(|txn| {
        if !txn.uses_budget(context.memory) {
            return Err(BodyMutationError::Invalid("transaction memory pool"));
        }
        AgentBodyKeyV1::new(agent).map_err(BodyMutationError::Invalid)?;
        if version > 1 {
            return Err(BodyMutationError::Invalid(
                "unsupported_registration_contract",
            ));
        }
        if version == 1 && agent.starts_with("industrial:") {
            return Err(BodyMutationError::Invalid(
                "industrial_identity_requires_delivery",
            ));
        }
        let clock = read_fixed::<{ b"oasis7.iws.world-time-value.v1\0".len() + 8 }, _, _, _, _>(
            txn,
            Ns::Allocator,
            WORLD_TIME_KEY_V1,
            values,
            context,
        )?;
        if clock != world_time_bytes_v1(time) {
            return Err(BodyMutationError::Invalid(
                "registration clock precondition",
            ));
        }
        // AgentState::new owns a bounded initial body/slot set. Acquire its
        // temporary metadata/string model before constructing any of it.
        let _cell = context
            .memory
            .reserve(64 * 1024 + agent.len() * 2, MemoryClass::Ordinary)
            .map_err(ExecutionError::Memory)?;
        let mut cell = AgentCell::new(AgentState::new(agent, *pos), time);
        cell.activity = Some(AgentActivityV1::idle(time));
        write_agent_cell_fields(
            agent,
            &cell,
            &mut AgentGenesisContext {
                mutation: BodyMutationContext {
                    sink: context.sink,
                    memory: context.memory,
                    io: context.io,
                    cancelled: &mut context.cancelled,
                },
                emit: |namespace, key: &[u8], id| {
                    if txn.read(namespace, key)?.is_some() {
                        return Err(BodyMutationError::Invalid(
                            "registration field already present",
                        ));
                    }
                    txn.write(namespace, key, Some(id))?;
                    Ok(())
                },
                mailbox_original: |_, _: &oasis7_world_state::DomainEvent| {
                    Err(BodyMutationError::Invalid(
                        "new agent has unexpected mailbox",
                    ))
                },
                max_scalar_bytes: 4096,
            },
        )?;
        Ok(())
    })
}
