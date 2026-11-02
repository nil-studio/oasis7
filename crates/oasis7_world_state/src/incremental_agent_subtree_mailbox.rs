//! Provisional queue references over the original mailbox wire. These APIs
//! do not certify original WorldEvent objects, accepted history, or permissions.
use super::*;
use crate::{
    incremental_agent_mutation::{BodyMutationContext, BodyMutationError, write_field_bytes},
    incremental_event_fields::{
        MAILBOX_EVENT_REFERENCE_BYTES, MAILBOX_FRONTIER_BYTES, MailboxFrontierV1, MailboxKeyV1,
        mailbox_event_id_v1, mailbox_event_ref_v1,
    },
    incremental_field_overlay::{WorldFieldOverlayContextV1, WorldFieldOverlayErrorV1, lookup_raw},
    incremental_inventory_value_read::InventoryValueReader,
};
use oasis7_formal_protocol::{
    incremental_execution::{ExecutionError, ExecutionFence, ExecutionTransaction, PreparedGuard},
    incremental_state::{MerkleNodeReader, StateNamespaceV1 as Ns},
    incremental_value::{ValueIoBudget, ValueNodeReader, ValueNodeWriter, ValueTreeError},
};
use oasis7_proto::memory_budget::{MemoryClass, ReservedValue};
#[derive(Clone, Copy)]
pub struct AgentSubtreeMailboxQueryV1<'a> {
    pub agent: &'a str,
    pub qualification: &'a ReservedValue<AgentSubtreeStructureQualificationV1>,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AgentSubtreeMailboxHeadV1 {
    pub frontier: MailboxFrontierV1,
    pub original: Option<[u8; 32]>,
}
/// Raw reference changes are provisional. A supplied ID is not a history proof.
pub enum AgentSubtreeMailboxMutationV1 {
    Append {
        expected: MailboxFrontierV1,
        original: [u8; 32],
    },
    Pop {
        expected: AgentSubtreeMailboxHeadV1,
    },
    Replace {
        expected: MailboxFrontierV1,
        ordinal: u64,
        original: [u8; 32],
        after: [u8; 32],
    },
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AgentSubtreeMailboxResultV1 {
    Appended(u64),
    Popped(Option<[u8; 32]>),
    Replaced(bool),
}
fn invalid() -> ValueTreeError {
    ValueTreeError::Invalid("agent mailbox binding/expected")
}
fn original<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    agent: &str,
    ordinal: u64,
    tx: &mut ExecutionTransaction<'_, M>,
    records: &mut InventoryValueReader<'_, '_, R, C>,
    error: &mut E,
) -> Result<Option<[u8; 32]>, WorldFieldOverlayErrorV1> {
    let key = MailboxKeyV1::new(agent, Some(ordinal)).map_err(ValueTreeError::Invalid)?;
    lookup_raw(tx, Ns::Scheduler, key.as_bytes(), records, error)?
        .map(|value| {
            mailbox_event_id_v1(&read::fixed_tree::<MAILBOX_EVENT_REFERENCE_BYTES, _, _>(
                &value,
                records.values,
                &mut *records.cancelled,
            )?)
            .map_err(ValueTreeError::Invalid)
            .map_err(WorldFieldOverlayErrorV1::from)
        })
        .transpose()
}
fn resolve<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    query: AgentSubtreeMailboxQueryV1<'_>,
    read_head: bool,
    tx: &mut ExecutionTransaction<'_, M>,
    records: &mut InventoryValueReader<'_, '_, R, C>,
    error: &mut E,
) -> Result<Option<AgentSubtreeMailboxHeadV1>, WorldFieldOverlayErrorV1> {
    let cell = point::resolve(
        AgentSubtreeQualifiedRequestV1 {
            cell: AgentSubtreeScalarRequestV1 {
                agent: query.agent,
                field: AgentFieldV1::Identity,
                ordinal: 0,
            },
            qualification: query.qualification,
        },
        tx,
        records,
        error,
        2048,
        &mut std::io::sink(),
    )?;
    if !cell.agent_exists {
        return Ok(None);
    }
    let key = MailboxKeyV1::new(query.agent, None).map_err(ValueTreeError::Invalid)?;
    let value =
        lookup_raw(tx, Ns::Scheduler, key.as_bytes(), records, error)?.ok_or_else(invalid)?;
    let frontier =
        MailboxFrontierV1::from_canonical_bytes(&read::fixed_tree::<MAILBOX_FRONTIER_BYTES, _, _>(
            &value,
            records.values,
            &mut *records.cancelled,
        )?)
        .map_err(ValueTreeError::Invalid)?;
    let original = if read_head && frontier.head < frontier.tail {
        Some(original(query.agent, frontier.head, tx, records, error)?.ok_or_else(invalid)?)
    } else {
        None
    };
    if (records.cancelled)() {
        return Err(ValueTreeError::Cancelled.into());
    }
    Ok(Some(AgentSubtreeMailboxHeadV1 { frontier, original }))
}
pub fn read_agent_subtree_mailbox_head_overlay_v1<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    query: AgentSubtreeMailboxQueryV1<'_>,
    context: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    guard: &PreparedGuard,
    live: &ExecutionFence,
) -> Result<Option<AgentSubtreeMailboxHeadV1>, WorldFieldOverlayErrorV1> {
    let cancel = &mut context.cancelled;
    let error = &mut context.take_state_error;
    context.transaction.contribute(|tx| {
        tx.validate_guard(guard, live)?;
        if context.values.class != MemoryClass::Ordinary
            || !tx.uses_budget(context.values.memory)
            || !query.qualification.uses_budget(context.values.memory)
        {
            return Err(invalid().into());
        }
        let _scratch = context
            .values
            .memory
            .reserve(32 << 10, MemoryClass::Ordinary)
            .map_err(ValueTreeError::Memory)?;
        let result = resolve(
            query,
            true,
            tx,
            &mut InventoryValueReader {
                values: &mut context.values,
                key_work: &mut *context.key_work,
                semantic_bytes: context.semantic_bytes,
                record_bytes: context.record_bytes,
                cancelled: cancel,
            },
            error,
        )?;
        Ok(result)
    })
}
fn put<M: MerkleNodeReader, R: ValueNodeReader, C: FnMut() -> bool, W: ValueNodeWriter>(
    tx: &mut ExecutionTransaction<'_, M>,
    records: &mut InventoryValueReader<'_, '_, R, C>,
    sink: &mut W,
    io: &mut ValueIoBudget,
    key: &MailboxKeyV1,
    bytes: &[u8],
) -> Result<(), BodyMutationError> {
    let value = write_field_bytes(
        bytes,
        &mut BodyMutationContext {
            sink,
            memory: records.values.memory,
            io,
            cancelled: &mut *records.cancelled,
        },
    )?;
    if (records.cancelled)() {
        return Err(ValueTreeError::Cancelled.into());
    }
    tx.write(Ns::Scheduler, key.as_bytes(), Some(value))?;
    if (records.cancelled)() {
        return Err(ValueTreeError::Cancelled.into());
    }
    Ok(())
}
/// Append/pop preserve absolute head/tail and never reset an empty queue.
/// Replacement affects one existing ordinal. Ordinary acceptance still needs
/// typed after validation and separate accepted-history/original obligations.
pub fn change_agent_subtree_mailbox_reference_v1<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    W: ValueNodeWriter,
    CW: FnMut() -> bool,
>(
    query: AgentSubtreeMailboxQueryV1<'_>,
    change: AgentSubtreeMailboxMutationV1,
    context: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    write: &mut BodyMutationContext<'_, W, CW>,
    guard: &PreparedGuard,
    live: &ExecutionFence,
) -> Result<AgentSubtreeMailboxResultV1, BodyMutationError> {
    let cancel = &mut context.cancelled;
    let error = &mut context.take_state_error;
    context.transaction.contribute(|tx| {
        tx.validate_guard(guard, live)?;
        if context.values.class != MemoryClass::Ordinary
            || !tx.uses_budget(context.values.memory)
            || !tx.uses_budget(write.memory)
            || !query.qualification.uses_budget(context.values.memory)
        {
            return Err(invalid().into());
        }
        let _scratch = write
            .memory
            .reserve(32 << 10, MemoryClass::Ordinary)
            .map_err(ExecutionError::Memory)?;
        let mut combined = || cancel() || (write.cancelled)();
        let mut records = InventoryValueReader {
            values: &mut context.values,
            key_work: &mut *context.key_work,
            semantic_bytes: context.semantic_bytes,
            record_bytes: context.record_bytes,
            cancelled: &mut combined,
        };
        let current = resolve(
            query,
            matches!(change, AgentSubtreeMailboxMutationV1::Pop { .. }),
            tx,
            &mut records,
            error,
        )
        .map_err(create::overlay_error)?
        .ok_or_else(invalid)?;
        let key = MailboxKeyV1::new(query.agent, None).map_err(ValueTreeError::Invalid)?;
        let result = match change {
            AgentSubtreeMailboxMutationV1::Append {
                expected,
                original: after,
            } => {
                if current.frontier != expected {
                    return Err(invalid().into());
                }
                let ordinal = expected.tail;
                let tail = ordinal
                    .checked_add(1)
                    .ok_or(ValueTreeError::ResourceLimited("agent mailbox tail"))?;
                if original(query.agent, ordinal, tx, &mut records, error)
                    .map_err(create::overlay_error)?
                    .is_some()
                {
                    return Err(invalid().into());
                }
                let member = MailboxKeyV1::new(query.agent, Some(ordinal))
                    .map_err(ValueTreeError::Invalid)?;
                put(
                    tx,
                    &mut records,
                    &mut *write.sink,
                    &mut *write.io,
                    &member,
                    &mailbox_event_ref_v1(after),
                )?;
                put(
                    tx,
                    &mut records,
                    &mut *write.sink,
                    &mut *write.io,
                    &key,
                    &MailboxFrontierV1 {
                        head: expected.head,
                        tail,
                    }
                    .canonical_bytes()
                    .map_err(ValueTreeError::Invalid)?,
                )?;
                AgentSubtreeMailboxResultV1::Appended(ordinal)
            }
            AgentSubtreeMailboxMutationV1::Pop { expected } => {
                if current != expected {
                    return Err(invalid().into());
                }
                if let Some(original) = current.original {
                    let member = MailboxKeyV1::new(query.agent, Some(current.frontier.head))
                        .map_err(ValueTreeError::Invalid)?;
                    tx.write(Ns::Scheduler, member.as_bytes(), None)?;
                    if (records.cancelled)() {
                        return Err(ValueTreeError::Cancelled.into());
                    }
                    let head = current.frontier.head.checked_add(1).ok_or_else(invalid)?;
                    put(
                        tx,
                        &mut records,
                        &mut *write.sink,
                        &mut *write.io,
                        &key,
                        &MailboxFrontierV1 {
                            head,
                            tail: current.frontier.tail,
                        }
                        .canonical_bytes()
                        .map_err(ValueTreeError::Invalid)?,
                    )?;
                    AgentSubtreeMailboxResultV1::Popped(Some(original))
                } else {
                    AgentSubtreeMailboxResultV1::Popped(None)
                }
            }
            AgentSubtreeMailboxMutationV1::Replace {
                expected,
                ordinal,
                original: before,
                after,
            } => {
                if current.frontier != expected
                    || ordinal < expected.head
                    || ordinal >= expected.tail
                    || original(query.agent, ordinal, tx, &mut records, error)
                        .map_err(create::overlay_error)?
                        != Some(before)
                {
                    return Err(invalid().into());
                }
                let unchanged = before == after;
                if (records.cancelled)() {
                    return Err(ValueTreeError::Cancelled.into());
                }
                if !unchanged {
                    let member = MailboxKeyV1::new(query.agent, Some(ordinal))
                        .map_err(ValueTreeError::Invalid)?;
                    put(
                        tx,
                        &mut records,
                        &mut *write.sink,
                        &mut *write.io,
                        &member,
                        &mailbox_event_ref_v1(after),
                    )?;
                }
                AgentSubtreeMailboxResultV1::Replaced(!unchanged)
            }
        };
        if (records.cancelled)() {
            return Err(ValueTreeError::Cancelled.into());
        }
        Ok(result)
    })
}
