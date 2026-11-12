//! Actual Current treasury resource points; no business/event permission.
use crate::{
    incremental_field_member::*,
    incremental_field_overlay::{WorldFieldOverlayContextV1, WorldFieldOverlayErrorV1, lookup},
    incremental_inventory_value_read::InventoryValueReader,
    incremental_projection::{FieldDestination, FieldOwner, FieldShape, WORLD_STATE_FIELDS},
    incremental_world_field_key::{WorldFieldKeyV1, WorldFieldRecordKindV1},
};
use oasis7_agent_api::world_action::ResourceKind;
use oasis7_formal_protocol::{
    incremental_execution::{ExecutionFence, PreparedGuard},
    incremental_state::{MerkleNodeReader, StateNamespaceV1},
    incremental_value::{ValueNodeReader, ValueNodeWriter, ValueTreeError},
};
use oasis7_proto::memory_budget::MemoryClass;
fn field() -> Result<crate::incremental_projection::FieldDescriptor, ValueTreeError> {
    WORLD_STATE_FIELDS
        .iter()
        .copied()
        .find(|f| {
            f.name == "resources"
                && f.owner == FieldOwner::WorldState
                && f.shape == FieldShape::Map
                && f.destination == FieldDestination::State(StateNamespaceV1::Business)
        })
        .ok_or(ValueTreeError::Invalid("resource field registration"))
}
fn resolve<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    kind: ResourceKind,
    context: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
) -> Result<(WorldFieldKeyV1, Option<i64>), WorldFieldOverlayErrorV1> {
    let mut records = InventoryValueReader {
        values: &mut context.values,
        key_work: &mut *context.key_work,
        semantic_bytes: context.semantic_bytes,
        record_bytes: context.record_bytes,
        cancelled: &mut context.cancelled,
    };
    if records.values.class != MemoryClass::Ordinary
        || !context.transaction.uses_budget(records.values.memory)
    {
        return Err(ValueTreeError::Invalid("resource overlay pool/class").into());
    }
    let f = field()?;
    let metadata = WorldFieldKeyV1::new(f, WorldFieldRecordKindV1::Metadata, None)
        .map_err(ValueTreeError::Invalid)?;
    let root = lookup(
        context.transaction,
        &metadata,
        &mut records,
        &mut context.take_state_error,
    )?
    .ok_or(ValueTreeError::Invalid("resource metadata absent"))?;
    records.presence(&root, &metadata, 0)?;
    let (key, n) = records.key(f, WorldFieldRecordKindV1::Entry, &kind)?;
    let value = lookup(
        context.transaction,
        &key,
        &mut records,
        &mut context.take_state_error,
    )?
    .map(|value| records.quantity(&value, &key, n))
    .transpose()?;
    Ok((key, value))
}
pub fn read_current_world_resource_v1<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    kind: ResourceKind,
    context: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    guard: &PreparedGuard,
    live: &ExecutionFence,
) -> Result<Option<i64>, WorldFieldOverlayErrorV1> {
    context.transaction.validate_guard(guard, live)?;
    let result = resolve(kind, context).map(|(_, n)| n);
    if result.is_err() {
        let _: Result<(), oasis7_formal_protocol::incremental_execution::ExecutionError> =
            context.transaction.contribute(|_| {
                Err(oasis7_formal_protocol::incremental_execution::ExecutionError::Aborted)
            });
    }
    result
}
#[allow(clippy::too_many_arguments)]
pub fn change_current_world_resource_v1<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    W: ValueNodeWriter,
    CW: FnMut() -> bool,
>(
    kind: ResourceKind,
    expected: Option<i64>,
    after: Option<i64>,
    context: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    write: &mut MemberRecordWriteContext<'_, W, CW>,
    guard: &PreparedGuard,
    live: &ExecutionFence,
) -> Result<(), WorldFieldOverlayErrorV1> {
    let result = (|| {
        context.transaction.validate_guard(guard, live)?;
        if !context.values.memory.same_pool(write.memory) {
            return Err(ValueTreeError::Invalid("resource write pool").into());
        }
        let (key, before) = resolve(kind, context)?;
        if before != expected {
            return Err(ValueTreeError::Invalid("resource expected value").into());
        }
        let after = if let Some(n) = after {
            let mut combined = MemberRecordWriteContext {
                sink: &mut *write.sink,
                memory: write.memory,
                io: &mut *write.io,
                key_work: &mut *write.key_work,
                limits: write.limits,
                cancelled: || (context.cancelled)() || (write.cancelled)(),
            };
            let out = write_member_record_v1(
                field()?,
                WorldFieldRecordKindV1::Entry,
                &kind,
                MemberPayloadRefV1::I64(n),
                &mut combined,
            )?;
            if out.field_key != key {
                return Err(ValueTreeError::Invalid("resource writer key").into());
            }
            Some(out.value_root)
        } else {
            None
        };
        if (context.cancelled)() || (write.cancelled)() {
            return Err(ValueTreeError::Cancelled.into());
        }
        context
            .transaction
            .write(StateNamespaceV1::Business, key.as_bytes(), after)?;
        if (context.cancelled)() || (write.cancelled)() {
            return Err(ValueTreeError::Cancelled.into());
        }
        Ok(())
    })();
    if result.is_err() {
        let _: Result<(), oasis7_formal_protocol::incremental_execution::ExecutionError> =
            context.transaction.contribute(|_| {
                Err(oasis7_formal_protocol::incremental_execution::ExecutionError::Aborted)
            });
    }
    result
}
