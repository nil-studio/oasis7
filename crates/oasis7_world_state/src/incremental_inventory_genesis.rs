//! Borrowed, typed production for exactly three inventory fields at genesis.
//! This does not scan ordinary changes or authorize a partial World root.
pub use crate::incremental_field_member::MemberRecordError as InventoryGenesisError;
use crate::{
    incremental_field_member::{
        MemberPayloadRefV1, MemberRecordLimitsV1, MemberRecordWriteContext, write_member_record_v1,
    },
    incremental_field_presence::{
        PresenceRecordLimitsV1, PresenceRecordWriteContext, write_field_group_v1,
        write_field_metadata_v1,
    },
    incremental_projection::{
        FieldDescriptor, FieldDestination, FieldOwner, FieldShape, WORLD_STATE_FIELDS,
    },
    incremental_semantic_key::{SemanticKeyBudget, SemanticKeyV1},
    incremental_world_field_key::{WorldFieldKeyV1, WorldFieldRecordKindV1},
    state::WorldState,
};
use oasis7_formal_protocol::{
    incremental_state::{StateNamespaceV1, StateWriteV1},
    incremental_value::{ValueIoBudget, ValueNodeWriter, ValueTreeError},
};
use oasis7_proto::memory_budget::{MemoryBudget, MemoryClass, ReservedValue};

pub trait InventoryWriteSink {
    /// Retain the charge while retaining this write. Admit batch/index storage
    /// separately; sort/coalesce admitted batches before building StateDelta.
    fn put_write(&mut self, write: ReservedValue<StateWriteV1>) -> Result<(), ValueTreeError>;
}
pub struct InventoryGenesisContext<'a, W, S, C> {
    pub values: &'a mut W,
    pub writes: &'a mut S,
    pub memory: &'a MemoryBudget,
    pub io: &'a mut ValueIoBudget,
    pub key_work: &'a mut SemanticKeyBudget,
    pub semantic_bytes: u64,
    pub record_bytes: u64,
    pub records_remaining: u64,
    pub cancelled: C,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InventoryGenesisSummaryV1 {
    pub fields: u8,
    pub groups: u64,
    pub entries: u64,
}
pub(crate) fn descriptor(
    name: &str,
    shape: FieldShape,
) -> Result<FieldDescriptor, InventoryGenesisError> {
    WORLD_STATE_FIELDS
        .iter()
        .find(|field| field.name == name)
        .copied()
        .filter(|field| {
            field.owner == FieldOwner::WorldState
                && field.shape == shape
                && field.destination == FieldDestination::State(StateNamespaceV1::Business)
        })
        .ok_or(InventoryGenesisError::Invalid(
            "inventory field registration",
        ))
}
impl<W, S, C: FnMut() -> bool> InventoryGenesisContext<'_, W, S, C> {
    fn start_record(&mut self) -> Result<(), InventoryGenesisError> {
        if (self.cancelled)() {
            return Err(ValueTreeError::Cancelled.into());
        }
        if self.records_remaining == 0 {
            return Err(ValueTreeError::ResourceLimited("inventory record allowance").into());
        }
        self.records_remaining -= 1;
        Ok(())
    }
}
fn emit<W, S: InventoryWriteSink, C: FnMut() -> bool>(
    context: &mut InventoryGenesisContext<'_, W, S, C>,
    key: WorldFieldKeyV1,
    value: [u8; 32],
) -> Result<(), InventoryGenesisError> {
    if (context.cancelled)() {
        return Err(ValueTreeError::Cancelled.into());
    }
    // Conservative allowance covers the owned write, Vec buffer and reservation
    // metadata. The sink separately owns any batching/container admission.
    let charge = context
        .memory
        .reserve(1024 + key.as_bytes().len(), MemoryClass::Ordinary)
        .map_err(ValueTreeError::Memory)?;
    let write = charge.retain(StateWriteV1 {
        key: key.as_bytes().to_vec(),
        before: None,
        after: Some(value),
    });
    let result = context.writes.put_write(write);
    if (context.cancelled)() {
        return Err(ValueTreeError::Cancelled.into());
    }
    result?;
    Ok(())
}
fn metadata<W: ValueNodeWriter, S: InventoryWriteSink, C: FnMut() -> bool>(
    field: FieldDescriptor,
    context: &mut InventoryGenesisContext<'_, W, S, C>,
) -> Result<(), InventoryGenesisError> {
    context.start_record()?;
    let record = write_field_metadata_v1(
        field,
        &mut PresenceRecordWriteContext {
            sink: &mut *context.values,
            memory: context.memory,
            io: &mut *context.io,
            key_work: &mut *context.key_work,
            limits: PresenceRecordLimitsV1 {
                semantic_bytes: context.semantic_bytes,
                record_bytes: context.record_bytes,
            },
            cancelled: &mut context.cancelled,
        },
    )?;
    emit(context, record.field_key, record.value_root)
}
fn group<
    K: SemanticKeyV1 + ?Sized,
    W: ValueNodeWriter,
    S: InventoryWriteSink,
    C: FnMut() -> bool,
>(
    field: FieldDescriptor,
    key: &K,
    context: &mut InventoryGenesisContext<'_, W, S, C>,
) -> Result<(), InventoryGenesisError> {
    context.start_record()?;
    let record = write_field_group_v1(
        field,
        key,
        &mut PresenceRecordWriteContext {
            sink: &mut *context.values,
            memory: context.memory,
            io: &mut *context.io,
            key_work: &mut *context.key_work,
            limits: PresenceRecordLimitsV1 {
                semantic_bytes: context.semantic_bytes,
                record_bytes: context.record_bytes,
            },
            cancelled: &mut context.cancelled,
        },
    )?;
    emit(context, record.field_key, record.value_root)
}
fn entry<
    K: SemanticKeyV1 + ?Sized,
    W: ValueNodeWriter,
    S: InventoryWriteSink,
    C: FnMut() -> bool,
>(
    field: FieldDescriptor,
    key: &K,
    quantity: i64,
    context: &mut InventoryGenesisContext<'_, W, S, C>,
) -> Result<(), InventoryGenesisError> {
    context.start_record()?;
    let record = write_member_record_v1(
        field,
        WorldFieldRecordKindV1::Entry,
        key,
        MemberPayloadRefV1::I64(quantity),
        &mut MemberRecordWriteContext {
            sink: &mut *context.values,
            memory: context.memory,
            io: &mut *context.io,
            key_work: &mut *context.key_work,
            limits: MemberRecordLimitsV1 {
                semantic_bytes: context.semantic_bytes,
                payload_bytes: 8,
                record_bytes: context.record_bytes,
            },
            cancelled: &mut context.cancelled,
        },
    )?;
    emit(context, record.field_key, record.value_root)
}
/// Writes only these registered fields, borrowing their original IDs and i64
/// quantities. Caller admits the source and discards any incomplete prefix.
/// Every output remains provisional until complete World/authority validation.
pub fn prepare_inventory_genesis_v1<
    W: ValueNodeWriter,
    S: InventoryWriteSink,
    C: FnMut() -> bool,
>(
    source: &WorldState,
    context: &mut InventoryGenesisContext<'_, W, S, C>,
) -> Result<InventoryGenesisSummaryV1, InventoryGenesisError> {
    let resources = descriptor("resources", FieldShape::Map)?;
    let materials = descriptor("materials", FieldShape::Map)?;
    let ledgers = descriptor("material_ledgers", FieldShape::NestedMap)?;
    let mut summary = InventoryGenesisSummaryV1 {
        fields: 3,
        groups: 0,
        entries: 0,
    };
    metadata(resources, context)?;
    for (key, quantity) in &source.resources {
        entry(resources, key, *quantity, context)?;
        summary.entries = summary
            .entries
            .checked_add(1)
            .ok_or(InventoryGenesisError::Invalid("inventory entry count"))?;
    }
    metadata(materials, context)?;
    for (key, quantity) in &source.materials {
        entry(materials, key, *quantity, context)?;
        summary.entries = summary
            .entries
            .checked_add(1)
            .ok_or(InventoryGenesisError::Invalid("inventory entry count"))?;
    }
    metadata(ledgers, context)?;
    for (ledger, members) in &source.material_ledgers {
        group(ledgers, ledger, context)?;
        summary.groups = summary
            .groups
            .checked_add(1)
            .ok_or(InventoryGenesisError::Invalid("inventory group count"))?;
        for (material, quantity) in members {
            entry(ledgers, &(ledger, material), *quantity, context)?;
            summary.entries = summary
                .entries
                .checked_add(1)
                .ok_or(InventoryGenesisError::Invalid("inventory entry count"))?;
        }
    }
    if (context.cancelled)() {
        return Err(ValueTreeError::Cancelled.into());
    }
    Ok(summary)
}
