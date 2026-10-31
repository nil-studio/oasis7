//! Closed, borrowed genesis production for required World u64 fields.
use crate::{
    incremental_field_member::{
        MemberPayloadRefV1, MemberRecordError, MemberRecordWriteContext, write_member_record_v1,
    },
    incremental_field_presence::{
        PresenceRecordLimitsV1, PresenceRecordWriteContext, write_field_metadata_v1,
    },
    incremental_projection::{
        FieldDescriptor, FieldDestination, FieldOwner, FieldShape, WORLD_STATE_FIELDS,
    },
    incremental_world_field_key::{WorldFieldKeyV1, WorldFieldRecordKindV1},
    state::WorldState,
};
use oasis7_formal_protocol::{
    incremental_execution::ExecutionWriteV1,
    incremental_state::StateNamespaceV1,
    incremental_value::{ValueNodeWriter, ValueTreeError},
};
use oasis7_proto::memory_budget::{MemoryClass, ReservedValue};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorldU64FieldV1 {
    Time,
    NextIndustrySettlementOrder,
    NextModuleReleaseRequestId,
    NextModuleInstanceId,
    NextModuleMarketOrderId,
    NextModuleMarketSaleId,
    AgentClaimLastProcessedEpoch,
}
impl WorldU64FieldV1 {
    pub const ALL: [Self; 7] = [
        Self::Time,
        Self::NextIndustrySettlementOrder,
        Self::NextModuleReleaseRequestId,
        Self::NextModuleInstanceId,
        Self::NextModuleMarketOrderId,
        Self::NextModuleMarketSaleId,
        Self::AgentClaimLastProcessedEpoch,
    ];
    pub fn name(self) -> &'static str {
        match self {
            Self::Time => "time",
            Self::NextIndustrySettlementOrder => "next_industry_settlement_order",
            Self::NextModuleReleaseRequestId => "next_module_release_request_id",
            Self::NextModuleInstanceId => "next_module_instance_id",
            Self::NextModuleMarketOrderId => "next_module_market_order_id",
            Self::NextModuleMarketSaleId => "next_module_market_sale_id",
            Self::AgentClaimLastProcessedEpoch => "agent_claim_last_processed_epoch",
        }
    }
    pub fn namespace(self) -> StateNamespaceV1 {
        match self {
            Self::AgentClaimLastProcessedEpoch => StateNamespaceV1::Scheduler,
            _ => StateNamespaceV1::Allocator,
        }
    }
    pub fn descriptor(self) -> Result<FieldDescriptor, MemberRecordError> {
        WORLD_STATE_FIELDS
            .iter()
            .copied()
            .find(|field| {
                field.name == self.name()
                    && field.owner == FieldOwner::WorldState
                    && field.shape == FieldShape::Value
                    && field.destination == FieldDestination::State(self.namespace())
            })
            .ok_or(MemberRecordError::Invalid("world u64 registration"))
    }
    pub fn value(self, source: &WorldState) -> u64 {
        match self {
            Self::Time => source.time,
            Self::NextIndustrySettlementOrder => source.next_industry_settlement_order,
            Self::NextModuleReleaseRequestId => source.next_module_release_request_id,
            Self::NextModuleInstanceId => source.next_module_instance_id,
            Self::NextModuleMarketOrderId => source.next_module_market_order_id,
            Self::NextModuleMarketSaleId => source.next_module_market_sale_id,
            Self::AgentClaimLastProcessedEpoch => source.agent_claim_last_processed_epoch,
        }
    }
}
/// All outputs are provisional. Caller admits its batch/index and discards the
/// entire prefix on error; success does not authorize a partial World root.
pub fn prepare_world_u64_genesis_v1<
    W: ValueNodeWriter,
    C: FnMut() -> bool,
    S: FnMut(ReservedValue<ExecutionWriteV1>) -> Result<(), ValueTreeError>,
>(
    source: &WorldState,
    context: &mut MemberRecordWriteContext<'_, W, C>,
    records_remaining: &mut u64,
    sink: &mut S,
) -> Result<(), MemberRecordError> {
    for field in WorldU64FieldV1::ALL {
        let descriptor = field.descriptor()?;
        for metadata in [true, false] {
            if (context.cancelled)() {
                return Err(ValueTreeError::Cancelled.into());
            }
            if *records_remaining == 0 {
                return Err(ValueTreeError::ResourceLimited("world u64 records").into());
            }
            *records_remaining -= 1;
            let (key, root) = if metadata {
                let record = write_field_metadata_v1(
                    descriptor,
                    &mut PresenceRecordWriteContext {
                        sink: &mut *context.sink,
                        memory: context.memory,
                        io: &mut *context.io,
                        key_work: &mut *context.key_work,
                        limits: PresenceRecordLimitsV1 {
                            semantic_bytes: context.limits.semantic_bytes,
                            record_bytes: context.limits.record_bytes,
                        },
                        cancelled: &mut context.cancelled,
                    },
                )?;
                (record.field_key, record.value_root)
            } else {
                let record = write_member_record_v1(
                    descriptor,
                    WorldFieldRecordKindV1::Entry,
                    &(),
                    MemberPayloadRefV1::U64(field.value(source)),
                    context,
                )?;
                (record.field_key, record.value_root)
            };
            crate::incremental_field_write::emit(key, root, context, sink)?;
        }
    }
    Ok(())
}

/// Read a required scalar from its registered fixed namespace root. Absence and
/// wrong codec are errors, never an implicit zero or signed reinterpretation.
pub fn read_world_u64_v1<
    M: oasis7_formal_protocol::incremental_state::MerkleNodeReader,
    R: oasis7_formal_protocol::incremental_value::ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    field: WorldU64FieldV1,
    context: &mut crate::incremental_field_read::WorldFieldReadContextV1<'_, M, R, E, C>,
) -> Result<u64, ValueTreeError> {
    context.check()?;
    let descriptor = field
        .descriptor()
        .map_err(|_| ValueTreeError::Invalid("world u64 registration"))?;
    let metadata = WorldFieldKeyV1::new(descriptor, WorldFieldRecordKindV1::Metadata, None)
        .map_err(ValueTreeError::Invalid)?;
    let root = context
        .lookup(&metadata)?
        .ok_or(ValueTreeError::Invalid("world u64 missing metadata"))?;
    context.records().presence(&root, &metadata, 0)?;
    let (key, bytes) = context.key(descriptor, WorldFieldRecordKindV1::Entry, &())?;
    let root = context
        .lookup(&key)?
        .ok_or(ValueTreeError::Invalid("world u64 missing entry"))?;
    context.records().unsigned(&root, &key, bytes)
}

/// Reads the current shared overlay, retaining both required read dependencies.
/// Any error poisons the transaction via contribute; it never reads a live head.
pub fn read_world_u64_overlay_v1<
    M: oasis7_formal_protocol::incremental_state::MerkleNodeReader,
    R: oasis7_formal_protocol::incremental_value::ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    field: WorldU64FieldV1,
    context: &mut crate::incremental_field_overlay::WorldFieldOverlayContextV1<
        '_,
        '_,
        '_,
        M,
        R,
        E,
        C,
    >,
) -> Result<u64, crate::incremental_field_overlay::WorldFieldOverlayErrorV1> {
    use crate::incremental_inventory_value_read::InventoryValueReader;
    let mut records = InventoryValueReader {
        values: &mut context.values,
        key_work: context.key_work,
        semantic_bytes: context.semantic_bytes,
        record_bytes: context.record_bytes,
        cancelled: &mut context.cancelled,
    };
    let take_state_error = &mut context.take_state_error;
    context.transaction.contribute(|transaction| {
        if records.values.class != MemoryClass::Ordinary
            || !transaction.uses_budget(records.values.memory)
        {
            return Err(ValueTreeError::Invalid("world u64 overlay pool/class").into());
        }
        resolve_overlay_u64(field, transaction, &mut records, take_state_error)
            .map(|(_, value)| value)
    })
}

fn resolve_overlay_u64<
    M: oasis7_formal_protocol::incremental_state::MerkleNodeReader,
    R: oasis7_formal_protocol::incremental_value::ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    field: WorldU64FieldV1,
    transaction: &mut oasis7_formal_protocol::incremental_execution::ExecutionTransaction<'_, M>,
    records: &mut crate::incremental_inventory_value_read::InventoryValueReader<'_, '_, R, C>,
    take_state_error: &mut E,
) -> Result<(WorldFieldKeyV1, u64), crate::incremental_field_overlay::WorldFieldOverlayErrorV1> {
    let field = field
        .descriptor()
        .map_err(|_| ValueTreeError::Invalid("world u64 registration"))?;
    let (key, root, bytes) = crate::incremental_field_overlay::required_member(
        field,
        transaction,
        records,
        take_state_error,
    )?;
    let value = records.unsigned(&root, &key, bytes)?;
    Ok((key, value))
}

/// Writes an owner-decided final scalar against the current overlay. Does not
/// allocate IDs or authorize time/era changes. Every failure poisons the txn.
pub fn change_world_u64_overlay_v1<
    M: oasis7_formal_protocol::incremental_state::MerkleNodeReader,
    R: oasis7_formal_protocol::incremental_value::ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    W: ValueNodeWriter,
    CW: FnMut() -> bool,
>(
    field: WorldU64FieldV1,
    expected: u64,
    after: u64,
    context: &mut crate::incremental_field_overlay::WorldFieldOverlayContextV1<
        '_,
        '_,
        '_,
        M,
        R,
        E,
        C,
    >,
    write: &mut MemberRecordWriteContext<'_, W, CW>,
    guard: &oasis7_formal_protocol::incremental_execution::PreparedGuard,
    live: &oasis7_formal_protocol::incremental_execution::ExecutionFence,
) -> Result<bool, crate::incremental_field_overlay::WorldFieldOverlayErrorV1> {
    use crate::incremental_inventory_value_read::InventoryValueReader;
    let read_cancel = &mut context.cancelled;
    let take_state_error = &mut context.take_state_error;
    context.transaction.contribute(|transaction| {
        transaction.validate_guard(guard, live)?;
        if context.values.class != MemoryClass::Ordinary
            || !transaction.uses_budget(context.values.memory)
            || !transaction.uses_budget(write.memory)
        {
            return Err(ValueTreeError::Invalid("world u64 overlay pool/class").into());
        }
        let (key, current) = {
            let mut cancelled = || read_cancel() || (write.cancelled)();
            let mut records = InventoryValueReader {
                values: &mut context.values,
                key_work: context.key_work,
                semantic_bytes: context.semantic_bytes,
                record_bytes: context.record_bytes,
                cancelled: &mut cancelled,
            };
            resolve_overlay_u64(field, transaction, &mut records, take_state_error)?
        };
        if current != expected {
            return Err(ValueTreeError::Invalid("world u64 expected value").into());
        }
        if current == after {
            if read_cancel() || (write.cancelled)() {
                return Err(ValueTreeError::Cancelled.into());
            }
            return Ok(false);
        }
        let output = write_member_record_v1(
            field.descriptor()?,
            WorldFieldRecordKindV1::Entry,
            &(),
            MemberPayloadRefV1::U64(after),
            &mut MemberRecordWriteContext {
                sink: &mut *write.sink,
                memory: write.memory,
                io: &mut *write.io,
                key_work: &mut *write.key_work,
                limits: write.limits,
                cancelled: || read_cancel() || (write.cancelled)(),
            },
        )?;
        if output.field_key != key {
            return Err(ValueTreeError::Invalid("world u64 writer key").into());
        }
        if read_cancel() || (write.cancelled)() {
            return Err(ValueTreeError::Cancelled.into());
        }
        transaction.write(field.namespace(), key.as_bytes(), Some(output.value_root))?;
        if read_cancel() || (write.cancelled)() {
            return Err(ValueTreeError::Cancelled.into());
        }
        Ok(true)
    })
}
