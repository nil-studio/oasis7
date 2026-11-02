use super::*;
use crate::{
    incremental_expected_bytes::Compare,
    incremental_field_member::*,
    incremental_field_overlay::{WorldFieldOverlayContextV1, WorldFieldOverlayErrorV1, lookup},
    incremental_inventory_value_read::InventoryValueReader,
    incremental_record_tree::{self as tree, RecordTreeWritePolicyV1},
};
use oasis7_formal_protocol::{
    incremental_execution::{ExecutionFence, ExecutionTransaction, PreparedGuard},
    incremental_state::{MerkleNodeReader, StateNamespaceV1, StatePageWriter, StateRootV1},
    incremental_value::{ValueNodeReader, ValueTreeError},
};
use oasis7_proto::memory_budget::{MemoryClass, ReservedValue};
use std::io::Write;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum ArtifactConfigScalarRefV1<'a> {
    Unit,
    Unsigned(u64),
    Boolean(bool),
    Text(&'a str),
}
impl<'a> ArtifactConfigScalarRefV1<'a> {
    fn payload(self) -> MemberPayloadRefV1<'a> {
        match self {
            Self::Unit => MemberPayloadRefV1::Unit,
            Self::Unsigned(value) => MemberPayloadRefV1::U64(value),
            Self::Boolean(value) => MemberPayloadRefV1::Bool(value),
            Self::Text(value) => MemberPayloadRefV1::Text(value),
        }
    }
    fn matches(self, actual: &ArtifactConfigValueV1, text_equal: bool) -> bool {
        match (self, actual) {
            (Self::Unit, ArtifactConfigValueV1::Unit) => true,
            (Self::Unsigned(expected), ArtifactConfigValueV1::Unsigned(actual)) => {
                expected == *actual
            }
            (Self::Boolean(expected), ArtifactConfigValueV1::Boolean(actual)) => {
                expected == *actual
            }
            (Self::Text(expected), ArtifactConfigValueV1::Label(actual)) => expected == *actual,
            (Self::Text(expected), ArtifactConfigValueV1::Text { utf8_bytes }) => {
                expected.len() as u64 == *utf8_bytes && text_equal
            }
            _ => false,
        }
    }
}
#[derive(Clone, Copy)]
pub struct ArtifactConfigCellRequestV1<'a> {
    pub field: ArtifactConfigFieldV1,
    pub key: ArtifactConfigRecordKeyV1<'a>,
    pub address: ArtifactConfigAddressV1<'a>,
    pub family: &'a ReservedValue<ArtifactConfigFamilyQualificationV1>,
    pub qualification: &'a ReservedValue<ArtifactConfigBodyQualificationV1>,
    /// Independent operation limit; never inferred from expected Text length.
    pub max_payload_bytes: u64,
}
pub struct ArtifactConfigCellChangeV1<'a> {
    pub request: ArtifactConfigCellRequestV1<'a>,
    pub expected: ArtifactConfigScalarRefV1<'a>,
    pub after: ArtifactConfigScalarRefV1<'a>,
    pub max_pending_node_bytes: usize,
}
pub(super) struct Resolved {
    pub outer: [Option<(ArtifactConfigFieldV1, WorldFieldKeyV1)>; 4],
    pub child: StateRootV1,
    pub cell_key: (WorldFieldKeyV1, u64),
    pub cell_root: [u8; 32],
    pub value: ArtifactConfigValueV1,
}
pub(super) struct ResolvedRecord {
    pub outer: [Option<(ArtifactConfigFieldV1, WorldFieldKeyV1)>; 4],
    pub child: Option<StateRootV1>,
}
pub(super) struct Publication<'a> {
    pub field: ArtifactConfigFieldV1,
    pub key: ArtifactConfigRecordKeyV1<'a>,
    pub outer: [Option<(ArtifactConfigFieldV1, WorldFieldKeyV1)>; 4],
    pub child: Option<StateRootV1>,
}
/// Caller has established complete new child closure by a typed scalar edit,
/// JSON advance or exhaustive borrowed production. This helper only publishes
/// the original outer bindings; it never qualifies an arbitrary supplied root.
pub(super) fn publish<M, W, C, CW>(
    publication: Publication<'_>,
    tx: &mut ExecutionTransaction<'_, M>,
    write: &mut MemberRecordWriteContext<'_, W, CW>,
    cancel: &mut C,
) -> Result<Option<ReservedValue<ArtifactConfigBodyQualificationV1>>, WorldFieldOverlayErrorV1>
where
    M: MerkleNodeReader,
    W: StatePageWriter,
    C: FnMut() -> bool,
    CW: FnMut() -> bool,
{
    if publication.field.global() && publication.child.is_none()
        || publication
            .child
            .as_ref()
            .is_some_and(|root| root.namespace != StateNamespaceV1::Artifacts)
    {
        return Err(read::invalid().into());
    }
    let fields = if publication.field.body_kind() == ArtifactConfigBodyKindV1::Module {
        family::RECORD_FIELDS
    } else {
        [publication.field; 4]
    };
    let count = if publication.field.body_kind() == ArtifactConfigBodyKindV1::Module {
        4
    } else {
        1
    };
    if publication
        .outer
        .iter()
        .filter(|item| item.is_some())
        .count()
        != count
    {
        return Err(read::invalid().into());
    }
    for (index, entry) in publication.outer.into_iter().take(count).enumerate() {
        let (field, expected_key) = entry.ok_or_else(read::invalid)?;
        if field != fields[index] {
            return Err(read::invalid().into());
        }
        let value = if let Some(child) = &publication.child {
            let row = {
                let mut combined = MemberRecordWriteContext {
                    sink: &mut *write.sink,
                    memory: write.memory,
                    io: &mut *write.io,
                    key_work: &mut *write.key_work,
                    limits: write.limits,
                    cancelled: || cancel() || (write.cancelled)(),
                };
                match publication.key {
                    ArtifactConfigRecordKeyV1::Global => genesis::root_row(
                        field.descriptor(),
                        &(),
                        field.schema(),
                        child.clone(),
                        &mut combined,
                    )?,
                    ArtifactConfigRecordKeyV1::Member(key) => genesis::root_row(
                        field.descriptor(),
                        key,
                        field.schema(),
                        child.clone(),
                        &mut combined,
                    )?,
                }
            };
            if row.field_key != expected_key {
                return Err(read::invalid().into());
            }
            Some(row.value_root)
        } else {
            None
        };
        tx.write(StateNamespaceV1::Artifacts, expected_key.as_bytes(), value)?;
        if cancel() || (write.cancelled)() {
            return Err(ValueTreeError::Cancelled.into());
        }
    }
    let result = publication
        .child
        .as_ref()
        .map(|root| {
            closure::mint(
                publication.field.body_kind(),
                root,
                write.memory,
                MemoryClass::Ordinary,
            )
        })
        .transpose()?;
    if cancel() || (write.cancelled)() {
        return Err(ValueTreeError::Cancelled.into());
    }
    Ok(result)
}
pub(super) fn resolve_record<M, R, E, C>(
    field: ArtifactConfigFieldV1,
    key: ArtifactConfigRecordKeyV1<'_>,
    family: &ReservedValue<ArtifactConfigFamilyQualificationV1>,
    expected: Option<&ReservedValue<ArtifactConfigBodyQualificationV1>>,
    tx: &mut ExecutionTransaction<'_, M>,
    records: &mut InventoryValueReader<'_, '_, R, C>,
    error: &mut E,
) -> Result<ResolvedRecord, WorldFieldOverlayErrorV1>
where
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
{
    if records.values.class != MemoryClass::Ordinary
        || !tx.uses_budget(records.values.memory)
        || !family.uses_budget(records.values.memory)
        || family.allocation_class() != records.values.class
        || family.reserved_bytes() < std::mem::size_of::<ArtifactConfigFamilyQualificationV1>()
        || expected.is_some_and(|proof| {
            !proof.uses_budget(records.values.memory)
                || proof.allocation_class() != records.values.class
                || proof.reserved_bytes()
                    < std::mem::size_of::<ArtifactConfigBodyQualificationV1>()
                        + proof.root().node_root.capacity()
        })
    {
        return Err(read::invalid().into());
    }
    family.validate_binding(&tx.parent_fence().parent_roots)?;
    if field.global() && expected.is_none() {
        return Err(read::invalid().into());
    }
    let fields = if field.body_kind() == ArtifactConfigBodyKindV1::Module {
        family::RECORD_FIELDS
    } else {
        [field; 4]
    };
    let count = if field.body_kind() == ArtifactConfigBodyKindV1::Module {
        4
    } else {
        1
    };
    let mut outer = std::array::from_fn(|_| None);
    let mut child = None;
    for (index, field) in fields.into_iter().take(count).enumerate() {
        let meta = WorldFieldKeyV1::new(field.descriptor(), WorldFieldRecordKindV1::Metadata, None)
            .map_err(ValueTreeError::Invalid)?;
        let id = lookup(tx, &meta, records, error)?.ok_or_else(read::invalid)?;
        records.presence(&id, &meta, 0)?;
        let key = family::record_key(field, key, records)?;
        let id = lookup(tx, &key.0, records, error)?;
        match (id, expected) {
            (Some(id), Some(expected)) => {
                let root = family::outer(field, &key.0, &id, records)?;
                expected.validate_binding(field.body_kind(), &root)?;
                if child.is_none() {
                    child = Some((*root).clone());
                }
            }
            (None, None) => {}
            _ => return Err(read::invalid().into()),
        }
        outer[index] = Some((field, key.0));
    }
    if (records.cancelled)() {
        return Err(ValueTreeError::Cancelled.into());
    }
    Ok(ResolvedRecord { outer, child })
}
pub(super) fn resolve<M, R, E, C>(
    request: ArtifactConfigCellRequestV1<'_>,
    tx: &mut ExecutionTransaction<'_, M>,
    records: &mut InventoryValueReader<'_, '_, R, C>,
    error: &mut E,
    output: &mut impl Write,
) -> Result<Resolved, WorldFieldOverlayErrorV1>
where
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
{
    let resolved = resolve_record(
        request.field,
        request.key,
        request.family,
        Some(request.qualification),
        tx,
        records,
        error,
    )?;
    let child = resolved.child.ok_or_else(read::invalid)?;
    let key = read::key(request.address, records)?;
    let id = tree::lookup(&child, &key.0, records, genesis::LABELS)?.ok_or_else(read::invalid)?;
    let value = read::value(
        request.field.body_kind(),
        request.address,
        &id,
        &key,
        request.max_payload_bytes,
        records,
        output,
    )?;
    if (records.cancelled)() {
        return Err(ValueTreeError::Cancelled.into());
    }
    Ok(Resolved {
        outer: resolved.outer,
        child,
        cell_key: key,
        cell_root: id,
        value,
    })
}

/// Getter and setter share exact current-overlay resolution. This getter's
/// final resolver poll is the independent baseline for a NoOp update's N+1.
pub fn read_qualified_artifact_config_overlay_v1<M, R, E, C>(
    request: ArtifactConfigCellRequestV1<'_>,
    context: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    guard: &PreparedGuard,
    live: &ExecutionFence,
    output: &mut impl Write,
) -> Result<ReservedValue<ArtifactConfigValueV1>, WorldFieldOverlayErrorV1>
where
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
{
    context.transaction.contribute(|tx| {
        tx.validate_guard(guard, live)?;
        let retained = context
            .values
            .memory
            .reserve(512, MemoryClass::Ordinary)
            .map_err(ValueTreeError::Memory)?;
        let _operation = context
            .values
            .memory
            .reserve(32 << 10, MemoryClass::Ordinary)
            .map_err(ValueTreeError::Memory)?;
        let mut records = InventoryValueReader {
            values: &mut context.values,
            key_work: &mut *context.key_work,
            semantic_bytes: context.semantic_bytes,
            record_bytes: context.record_bytes,
            cancelled: &mut context.cancelled,
        };
        Ok(retained.retain(
            resolve(
                request,
                tx,
                &mut records,
                &mut context.take_state_error,
                output,
            )?
            .value,
        ))
    })
}

pub fn change_qualified_artifact_config_cell_v1<M, R, E, C, W, CW>(
    change: ArtifactConfigCellChangeV1<'_>,
    context: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    write: &mut MemberRecordWriteContext<'_, W, CW>,
    guard: &PreparedGuard,
    live: &ExecutionFence,
) -> Result<Option<ReservedValue<ArtifactConfigBodyQualificationV1>>, WorldFieldOverlayErrorV1>
where
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    W: StatePageWriter,
    CW: FnMut() -> bool,
{
    let cancel = &mut context.cancelled;
    let error = &mut context.take_state_error;
    context.transaction.contribute(|tx| {
        tx.validate_guard(guard, live)?;
        let request = change.request;
        if !schema::point_supported(request.field.body_kind(), request.address)
            || !tx.uses_budget(write.memory)
        {
            return Err(read::invalid().into());
        }
        schema::validate(
            request.field.body_kind(),
            request.address,
            ArtifactConfigPayloadV1::Primitive(change.expected.payload()),
        )?;
        schema::validate(
            request.field.body_kind(),
            request.address,
            ArtifactConfigPayloadV1::Primitive(change.after.payload()),
        )?;
        let _operation = write
            .memory
            .reserve(32 << 10, MemoryClass::Ordinary)
            .map_err(ValueTreeError::Memory)?;
        let mut compare = Compare::new(match change.expected {
            ArtifactConfigScalarRefV1::Text(value) => value.as_bytes(),
            _ => &[],
        });
        let resolved = {
            let mut combined = || cancel() || (write.cancelled)();
            let mut records = InventoryValueReader {
                values: &mut context.values,
                key_work: &mut *context.key_work,
                semantic_bytes: context.semantic_bytes,
                record_bytes: context.record_bytes,
                cancelled: &mut combined,
            };
            resolve(request, tx, &mut records, error, &mut compare)?
        };
        let expected = change.expected.matches(&resolved.value, compare.is_equal());
        let unchanged = change.expected == change.after;
        if cancel() || (write.cancelled)() {
            return Err(ValueTreeError::Cancelled.into());
        }
        if !expected {
            return Err(ValueTreeError::Invalid("artifact config expected value").into());
        }
        if unchanged {
            return Ok(None);
        }
        let row = {
            let mut combined = MemberRecordWriteContext {
                sink: &mut *write.sink,
                memory: write.memory,
                io: &mut *write.io,
                key_work: &mut *write.key_work,
                limits: write.limits,
                cancelled: || cancel() || (write.cancelled)(),
            };
            genesis::row(
                genesis::body_descriptor(),
                &request.address.semantic(),
                change.after.payload(),
                &mut combined,
            )?
        };
        if row.field_key != resolved.cell_key.0 {
            return Err(read::invalid().into());
        }
        let child = tree::edit(
            resolved.child,
            &resolved.cell_key.0,
            (Some(resolved.cell_root), Some(row.value_root)),
            &mut context.values,
            write,
            cancel,
            RecordTreeWritePolicyV1 {
                max_pending_node_bytes: change.max_pending_node_bytes,
                labels: genesis::LABELS,
            },
        )?;
        publish(
            Publication {
                field: request.field,
                key: request.key,
                outer: resolved.outer,
                child: Some(child),
            },
            tx,
            write,
            cancel,
        )
    })
}
