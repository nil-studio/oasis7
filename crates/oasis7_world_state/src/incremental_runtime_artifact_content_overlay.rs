//! Same-transaction keyed contributions. Content IO is provisional; the typed
//! receiver still verifies new raw originals independently before adoption.
use super::read::{check_entry, invalid};
use super::*;
use crate::{
    incremental_field_member::*,
    incremental_field_overlay::{WorldFieldOverlayContextV1, WorldFieldOverlayErrorV1, lookup},
    incremental_inventory_value_read::InventoryValueReader,
};
use oasis7_formal_protocol::{
    incremental_execution::{ExecutionFence, ExecutionTransaction, PreparedGuard},
    incremental_state::{MerkleNodeReader, StateEntryV1, StateNamespaceV1},
    incremental_value::ValueNodeReader,
};
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RuntimeArtifactContentValueV1 {
    Registered(bool),
    Original(Option<OriginalArtifactRefV1>),
}
#[derive(Debug, Clone, Copy)]
pub enum RuntimeArtifactContentChangeV1<'a> {
    Registered {
        key: &'a str,
        expected: bool,
        after: bool,
    },
    Original {
        key: &'a str,
        expected: Option<OriginalArtifactRefV1>,
        after: Option<&'a [u8]>,
    },
}
impl<'a> RuntimeArtifactContentChangeV1<'a> {
    fn field(self) -> RuntimeArtifactContentFieldV1 {
        match self {
            Self::Registered { .. } => RuntimeArtifactContentFieldV1::Registered,
            Self::Original { .. } => RuntimeArtifactContentFieldV1::Original,
        }
    }
    fn key(self) -> &'a str {
        match self {
            Self::Registered { key, .. } | Self::Original { key, .. } => key,
        }
    }
    fn expected(self) -> RuntimeArtifactContentValueV1 {
        match self {
            Self::Registered { expected, .. } => {
                RuntimeArtifactContentValueV1::Registered(expected)
            }
            Self::Original { expected, .. } => RuntimeArtifactContentValueV1::Original(expected),
        }
    }
}
fn resolve<M, R, E, C>(
    field: RuntimeArtifactContentFieldV1,
    key: &str,
    tx: &mut ExecutionTransaction<'_, M>,
    records: &mut InventoryValueReader<'_, '_, R, C>,
    error: &mut E,
) -> Result<(WorldFieldKeyV1, RuntimeArtifactContentValueV1), WorldFieldOverlayErrorV1>
where
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
{
    let metadata = WorldFieldKeyV1::new(field.descriptor(), WorldFieldRecordKindV1::Metadata, None)
        .map_err(ValueTreeError::Invalid)?;
    let id = lookup(tx, &metadata, records, error)?.ok_or_else(invalid)?;
    records.presence(&id, &metadata, 0)?;
    let kind = if field == RuntimeArtifactContentFieldV1::Original {
        WorldFieldRecordKindV1::Bytes
    } else {
        WorldFieldRecordKindV1::Entry
    };
    let (encoded, _) = records.key(field.descriptor(), kind, key)?;
    let root = lookup(tx, &encoded, records, error)?;
    let original = if let Some(value) = root {
        check_entry(
            field,
            kind,
            &StateEntryV1 {
                namespace: StateNamespaceV1::Artifacts,
                key: encoded.as_bytes().to_vec(),
                value,
            },
            records,
        )?
    } else {
        None
    };
    Ok((
        encoded,
        match field {
            RuntimeArtifactContentFieldV1::Registered => {
                RuntimeArtifactContentValueV1::Registered(root.is_some())
            }
            RuntimeArtifactContentFieldV1::Original => {
                RuntimeArtifactContentValueV1::Original(original)
            }
        },
    ))
}
pub fn read_runtime_artifact_content_overlay_v1<M, R, E, C>(
    field: RuntimeArtifactContentFieldV1,
    key: &str,
    ctx: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
) -> Result<RuntimeArtifactContentValueV1, WorldFieldOverlayErrorV1>
where
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
{
    ctx.transaction.contribute(|tx| {
        if ctx.values.class != MemoryClass::Ordinary || !tx.uses_budget(ctx.values.memory) {
            return Err(invalid().into());
        }
        let _scratch = ctx
            .values
            .memory
            .reserve(8192, MemoryClass::Ordinary)
            .map_err(ValueTreeError::Memory)?;
        let mut records = InventoryValueReader {
            values: &mut ctx.values,
            key_work: ctx.key_work,
            semantic_bytes: ctx.semantic_bytes,
            record_bytes: ctx.record_bytes,
            cancelled: &mut ctx.cancelled,
        };
        let (_, value) = resolve(field, key, tx, &mut records, &mut ctx.take_state_error)?;
        if (records.cancelled)() {
            return Err(ValueTreeError::Cancelled.into());
        }
        Ok(value)
    })
}
/// Request strings/bytes remain caller-admitted. `persist` reports completion
/// of the exact original's physical write/readback, never business permission.
pub fn change_runtime_artifact_content_overlay_v1<M, R, E, C, W, CW, P>(
    change: RuntimeArtifactContentChangeV1<'_>,
    ctx: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    write: &mut MemberRecordWriteContext<'_, W, CW>,
    work: &mut RuntimeArtifactOriginalWorkV1,
    persist: &mut P,
    guard: &PreparedGuard,
    live: &ExecutionFence,
) -> Result<bool, WorldFieldOverlayErrorV1>
where
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    W: ValueNodeWriter,
    CW: FnMut() -> bool,
    P: FnMut(OriginalArtifactRefV1, &[u8]) -> Result<(), ValueTreeError>,
{
    let cancelled = &mut ctx.cancelled;
    ctx.transaction.contribute(|tx| {
        tx.validate_guard(guard, live)?;
        if ctx.values.class != MemoryClass::Ordinary
            || !tx.uses_budget(ctx.values.memory)
            || !tx.uses_budget(write.memory)
        {
            return Err(invalid().into());
        }
        let _scratch = ctx
            .values
            .memory
            .reserve(8192, MemoryClass::Ordinary)
            .map_err(ValueTreeError::Memory)?;
        let mut cancel = || cancelled() || (write.cancelled)();
        let (key, current) = resolve(
            change.field(),
            change.key(),
            tx,
            &mut InventoryValueReader {
                values: &mut ctx.values,
                key_work: ctx.key_work,
                semantic_bytes: ctx.semantic_bytes,
                record_bytes: ctx.record_bytes,
                cancelled: &mut cancel,
            },
            &mut ctx.take_state_error,
        )?;
        if current != change.expected() {
            return Err(invalid().into());
        }
        let after = match change {
            RuntimeArtifactContentChangeV1::Registered { after, .. } => {
                RuntimeArtifactContentValueV1::Registered(after)
            }
            RuntimeArtifactContentChangeV1::Original { after, .. } => {
                RuntimeArtifactContentValueV1::Original(
                    after
                        .map(|bytes| {
                            super::prepare_borrowed_original(
                                bytes,
                                ctx.values.memory,
                                work,
                                &mut cancel,
                            )
                        })
                        .transpose()?,
                )
            }
        };
        let unchanged = current == after;
        if cancel() {
            return Err(ValueTreeError::Cancelled.into());
        }
        if unchanged {
            return Ok(false);
        }
        if let (
            RuntimeArtifactContentChangeV1::Original {
                after: Some(bytes), ..
            },
            RuntimeArtifactContentValueV1::Original(Some(original)),
        ) = (change, after)
        {
            persist(original, bytes)?;
            if cancel() {
                return Err(ValueTreeError::Cancelled.into());
            }
        }
        let binding = match after {
            RuntimeArtifactContentValueV1::Original(Some(original)) => {
                Some(runtime_artifact_original_bytes_v1(original))
            }
            _ => None,
        };
        let new_root = match after {
            RuntimeArtifactContentValueV1::Registered(false)
            | RuntimeArtifactContentValueV1::Original(None) => None,
            _ => {
                let mut combined = MemberRecordWriteContext {
                    sink: &mut *write.sink,
                    memory: write.memory,
                    io: &mut *write.io,
                    key_work: &mut *write.key_work,
                    limits: write.limits,
                    cancelled: &mut cancel,
                };
                let row = write_member_record_v1(
                    change.field().descriptor(),
                    if binding.is_some() {
                        WorldFieldRecordKindV1::Bytes
                    } else {
                        WorldFieldRecordKindV1::Entry
                    },
                    change.key(),
                    if let Some(bytes) = &binding {
                        MemberPayloadRefV1::Bytes(bytes)
                    } else {
                        MemberPayloadRefV1::Unit
                    },
                    &mut combined,
                )?;
                if row.field_key != key {
                    return Err(invalid().into());
                }
                Some(row.value_root)
            }
        };
        if cancel() {
            return Err(ValueTreeError::Cancelled.into());
        }
        tx.write(StateNamespaceV1::Artifacts, key.as_bytes(), new_root)?;
        if cancel() {
            return Err(ValueTreeError::Cancelled.into());
        }
        Ok(true)
    })
}
