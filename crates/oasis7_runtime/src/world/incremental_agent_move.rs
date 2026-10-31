//! Position/last-active lowering for the existing AgentMoved reducer. Reads
//! authenticated overlay fields, without materializing an AgentCell.
use super::{
    incremental_agent_genesis::write_agent_scalar_value,
    incremental_body::{BodyMutationContext, BodyMutationError, write_body_value},
    incremental_body_event::read_fixed,
};
use oasis7_agent_api::world_action::GeoPos;
use oasis7_formal_protocol::{
    incremental_execution::ExecutionTransaction,
    incremental_state::{MerkleNodeReader, StateNamespaceV1 as Ns},
    incremental_value::{ValueNodeReader, ValueNodeWriter},
};
use oasis7_world_state::{
    incremental_agent_body::{
        AGENT_BODY_VALUE_BYTES, AgentBodyKeyV1, AgentBodyValueV1, WORLD_TIME_KEY_V1,
        world_time_bytes_v1,
    },
    incremental_agent_fields::{
        AGENT_I64_VALUE_BYTES, AgentFieldKeyV1, AgentFieldV1, AgentScalarRefV1,
    },
};

pub(super) fn contribute_move<
    R: MerkleNodeReader,
    V: ValueNodeReader,
    W: ValueNodeWriter,
    C: FnMut() -> bool,
>(
    agent: &str,
    to: &GeoPos,
    time: u64,
    txn: &mut ExecutionTransaction<'_, R>,
    values: &V,
    context: &mut BodyMutationContext<'_, W, C>,
) -> Result<(), BodyMutationError> {
    txn.contribute(|txn| {
        if !txn.uses_budget(context.memory) {
            return Err(BodyMutationError::Invalid("transaction memory pool"));
        }
        let clock = read_fixed::<{ b"oasis7.iws.world-time-value.v1\0".len() + 8 }, _, _, _, _>(
            txn,
            Ns::Allocator,
            WORLD_TIME_KEY_V1,
            values,
            context,
        )?;
        if clock != world_time_bytes_v1(time) {
            return Err(BodyMutationError::Invalid("move clock precondition"));
        }
        let key = AgentBodyKeyV1::new(agent).map_err(BodyMutationError::Invalid)?;
        let mut body = AgentBodyValueV1::from_canonical_bytes(&read_fixed::<
            AGENT_BODY_VALUE_BYTES,
            _,
            _,
            _,
            _,
        >(
            txn,
            Ns::Business,
            key.as_bytes(),
            values,
            context,
        )?)
        .map_err(BodyMutationError::Invalid)?;
        let GeoPos { x_cm, y_cm, z_cm } = to;
        for (field, after) in [
            (AgentFieldV1::PositionX, *x_cm),
            (AgentFieldV1::PositionY, *y_cm),
            (AgentFieldV1::PositionZ, *z_cm),
        ] {
            let key = AgentFieldKeyV1::new(agent, field, 0).map_err(BodyMutationError::Invalid)?;
            let bytes = read_fixed::<AGENT_I64_VALUE_BYTES, _, _, _, _>(
                txn,
                Ns::Business,
                key.as_bytes(),
                values,
                context,
            )?;
            let AgentScalarRefV1::I64(before) =
                AgentScalarRefV1::from_canonical_bytes(&bytes, field)
                    .map_err(BodyMutationError::Invalid)?
            else {
                return Err(BodyMutationError::Invalid("move position type"));
            };
            if before != after {
                let id = write_agent_scalar_value(
                    field,
                    AgentScalarRefV1::I64(after),
                    context,
                    AGENT_I64_VALUE_BYTES as u64,
                )?;
                txn.write(Ns::Business, key.as_bytes(), Some(id))?;
            }
        }
        // The existing reducer uses the event's `to` and reducer clock; `from`
        // is original historical content, not a second cache precondition.
        if body.last_active != time {
            body.last_active = time;
            let id = write_body_value(&body, context)?;
            txn.write(Ns::Business, key.as_bytes(), Some(id))?;
        }
        Ok(())
    })
}
