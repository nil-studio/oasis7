//! Provisional new-cell contribution using the sole original borrowed GEN.
use super::*;
use crate::{
    incremental_agent_genesis::{AgentGenesisContext, write_agent_cell_fields},
    incremental_agent_mutation::{BodyMutationContext, BodyMutationError},
    incremental_field_overlay::{WorldFieldOverlayContextV1, WorldFieldOverlayErrorV1, lookup_raw},
    incremental_inventory_value_read::InventoryValueReader,
};
use oasis7_formal_protocol::{
    incremental_execution::{ExecutionFence, PreparedGuard},
    incremental_state::MerkleNodeReader,
    incremental_value::{ValueNodeReader, ValueNodeWriter, ValueTreeError},
};
use oasis7_proto::memory_budget::{MemoryClass, ReservedValue};
pub struct AgentSubtreeCreateV1<'a> {
    pub agent: &'a str,
    pub cell: &'a crate::AgentCell,
    pub qualification: &'a ReservedValue<AgentSubtreeStructureQualificationV1>,
    pub max_scalar_bytes: u64,
}
fn invalid() -> BodyMutationError {
    BodyMutationError::Invalid("agent creation requires absent current cell")
}
pub(super) fn overlay_error(error: WorldFieldOverlayErrorV1) -> BodyMutationError {
    match error {
        WorldFieldOverlayErrorV1::Execution(error) => BodyMutationError::Execution(error),
        WorldFieldOverlayErrorV1::Value(error) => BodyMutationError::Value(error),
        WorldFieldOverlayErrorV1::Member(error) => {
            use crate::{
                incremental_field_member::MemberRecordError as M,
                incremental_semantic_key::SemanticKeyError as K,
            };
            match error {
                M::Value(error) => BodyMutationError::Value(error),
                M::Invalid(error) => BodyMutationError::Invalid(error),
                M::Io(error) => BodyMutationError::Value(ValueTreeError::Io(error)),
                M::Key(error) => BodyMutationError::Value(match error {
                    K::Memory(error) => ValueTreeError::Memory(error),
                    K::ResourceLimited => {
                        ValueTreeError::ResourceLimited("semantic key byte allowance")
                    }
                    K::Cancelled => ValueTreeError::Cancelled,
                    K::Invalid(error) => ValueTreeError::Invalid(error),
                }),
            }
        }
    }
}
/// Output is provisional. The raw original callback has the original GEN
/// contract; neither its Ok nor this count grants accepted-history readiness.
/// Complete after closure and any history obligations remain receiver duties.
pub fn create_agent_subtree_v1<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    W: ValueNodeWriter,
    CW: FnMut() -> bool,
    L: FnMut(u64, &crate::DomainEvent) -> Result<[u8; 32], BodyMutationError>,
>(
    request: AgentSubtreeCreateV1<'_>,
    context: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    write: &mut BodyMutationContext<'_, W, CW>,
    original: &mut L,
    guard: &PreparedGuard,
    live: &ExecutionFence,
) -> Result<u64, BodyMutationError> {
    let cancel = &mut context.cancelled;
    let error = &mut context.take_state_error;
    context.transaction.contribute(|tx| {
        tx.validate_guard(guard, live)?;
        if context.values.class != MemoryClass::Ordinary
            || !tx.uses_budget(context.values.memory)
            || !tx.uses_budget(write.memory)
            || !request.qualification.uses_budget(context.values.memory)
        {
            return Err(invalid());
        }
        let _scratch = write
            .memory
            .reserve(32 << 10, MemoryClass::Ordinary)
            .map_err(oasis7_formal_protocol::incremental_execution::ExecutionError::Memory)?;
        {
            let mut combined = || cancel() || (write.cancelled)();
            let mut records = InventoryValueReader {
                values: &mut context.values,
                key_work: &mut *context.key_work,
                semantic_bytes: context.semantic_bytes,
                record_bytes: context.record_bytes,
                cancelled: &mut combined,
            };
            let resolved = point::resolve(
                AgentSubtreeQualifiedRequestV1 {
                    cell: AgentSubtreeScalarRequestV1 {
                        agent: request.agent,
                        field: AgentFieldV1::Identity,
                        ordinal: 0,
                    },
                    qualification: request.qualification,
                },
                tx,
                &mut records,
                error,
                request.max_scalar_bytes,
                &mut std::io::sink(),
            )
            .map_err(overlay_error)?;
            if resolved.agent_exists || resolved.before.is_some() {
                return Err(invalid());
            }
            // The two ResourceStock members are optional and cannot be inferred
            // from collection counts. Reject same-TX orphan adoption explicitly.
            for ordinal in 0..2 {
                let key =
                    AgentFieldKeyV1::new(request.agent, AgentFieldV1::ResourceAmount, ordinal)
                        .map_err(BodyMutationError::Invalid)?;
                if lookup_raw(
                    tx,
                    AgentFieldV1::ResourceAmount.namespace(),
                    key.as_bytes(),
                    &mut records,
                    error,
                )
                .map_err(overlay_error)?
                .is_some()
                {
                    return Err(invalid());
                }
            }
        }
        let result = write_agent_cell_fields(
            request.agent,
            request.cell,
            &mut AgentGenesisContext {
                mutation: BodyMutationContext {
                    sink: &mut *write.sink,
                    memory: write.memory,
                    io: &mut *write.io,
                    cancelled: || cancel() || (write.cancelled)(),
                },
                emit: |namespace, key: &[u8], id| {
                    tx.write(namespace, key, Some(id))?;
                    Ok(())
                },
                mailbox_original: original,
                max_scalar_bytes: request.max_scalar_bytes,
            },
        )?;
        if cancel() || (write.cancelled)() {
            return Err(ValueTreeError::Cancelled.into());
        }
        Ok(result)
    })
}
