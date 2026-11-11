//! Closed two-leaf byte-map machinery shared by registered Runtime owners.
//! This produces/verifies data only; it cannot mint a typed owner capability.
use crate::{
    incremental_field_member::*, incremental_field_presence::*,
    incremental_field_read::WorldFieldReadContextV1, incremental_typed_record_codec::*,
};
use oasis7_formal_protocol::incremental_state::{StateScanContext, StateScanError};
use oasis7_proto::memory_budget::MemoryReservation;
use std::cell::RefCell;
struct Labels {
    namespace: StateNamespaceV1,
    invalid: &'static str,
    heap: &'static str,
    scan: &'static str,
}
fn labels(field: FieldDescriptor) -> Result<Labels, ValueTreeError> {
    if field.owner != FieldOwner::Runtime || field.shape != FieldShape::ByteMap {
        return Err(ValueTreeError::Invalid("closed byte-map owner"));
    }
    match (field.name, field.destination) {
        (
            "capability_revocation_state.bytes",
            FieldDestination::State(StateNamespaceV1::Authorization),
        ) => Ok(Labels {
            namespace: StateNamespaceV1::Authorization,
            invalid: "closed runtime revocation columns",
            heap: "revocation bytes selected heap",
            scan: "revocation bytes scan",
        }),
        ("pending_actions.bytes", FieldDestination::State(StateNamespaceV1::Scheduler)) => {
            Ok(Labels {
                namespace: StateNamespaceV1::Scheduler,
                invalid: "closed pending action bytes",
                heap: "pending action bytes selected heap",
                scan: "pending action bytes scan",
            })
        }
        _ => Err(ValueTreeError::Invalid("closed byte-map owner")),
    }
}
pub(crate) struct CapturedByteMapV1 {
    pub(crate) value: Vec<u8>,
    pub(crate) charge: MemoryReservation,
}
pub(crate) fn write_byte_map_v1<
    W: ValueNodeWriter + ValueNodeReader + StatePageWriter,
    C: FnMut() -> bool,
>(
    descriptor: FieldDescriptor,
    parent_profile: &TypedRecordProfileV1,
    source: &[u8],
    ctx: &mut MemberRecordWriteContext<'_, W, C>,
    work: &mut TypedRecordWriteWorkV1,
) -> Result<oasis7_proto::memory_budget::ReservedValue<StateRootV1>, MemberRecordError> {
    let labels = labels(descriptor)?;
    if parent_profile.field.owner != descriptor.owner
        || parent_profile.field.destination != descriptor.destination
    {
        return Err(ValueTreeError::Invalid("byte-map parent scope").into());
    }
    let mut child = TypedTreeBuilderV1::empty(labels.namespace, ctx.memory)?;
    work.spend(&mut ctx.cancelled)?;
    let meta = write_field_metadata_v1(
        descriptor,
        &mut PresenceRecordWriteContext {
            sink: &mut *ctx.sink,
            memory: ctx.memory,
            io: &mut *ctx.io,
            key_work: &mut *ctx.key_work,
            limits: PresenceRecordLimitsV1 {
                semantic_bytes: ctx.limits.semantic_bytes,
                record_bytes: ctx.limits.record_bytes,
            },
            cancelled: &mut ctx.cancelled,
        },
    )?;
    append(
        parent_profile,
        &mut child,
        &meta.field_key,
        meta.value_root,
        ctx,
        work,
    )?;
    work.spend(&mut ctx.cancelled)?;
    let row = write_member_record_v1(
        descriptor,
        WorldFieldRecordKindV1::Bytes,
        &(),
        MemberPayloadRefV1::Bytes(source),
        ctx,
    )?;
    append(
        parent_profile,
        &mut child,
        &row.field_key,
        row.value_root,
        ctx,
        work,
    )?;
    if (ctx.cancelled)() {
        return Err(ValueTreeError::Cancelled.into());
    }
    Ok(child.into_reserved())
}
pub(crate) fn read_byte_map_v1<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    descriptor: FieldDescriptor,
    root: &StateRootV1,
    source: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
    scan: &mut StateScanBudget,
    capture: Option<usize>,
) -> Result<Option<CapturedByteMapV1>, ValueTreeError> {
    let labels = labels(descriptor)?;
    let namespace = labels.namespace;
    let invalid = || ValueTreeError::Invalid(labels.invalid);
    root.validate().map_err(|_| invalid())?;
    source.check()?;
    if root.namespace != namespace || root.leaf_count != 2 {
        return Err(invalid());
    }
    let _scratch = source
        .values
        .memory
        .reserve(32 << 10, source.values.class)?;
    let mut metadata = false;
    // Data drops before its reservation on every early return.
    let mut output_charge = None;
    let mut output = None;
    let mut bytes_seen = false;
    let max_heap = capture.unwrap_or(0);
    {
        let stop = RefCell::new(&mut source.cancelled);
        let mut ctx = WorldFieldReadContextV1 {
            root,
            state: source.state,
            values: oasis7_formal_protocol::incremental_value::ValueReadContext {
                source: source.values.source,
                memory: source.values.memory,
                class: source.values.class,
                io: &mut *source.values.io,
            },
            key_work: &mut *source.key_work,
            semantic_bytes: source.semantic_bytes,
            record_bytes: source.record_bytes,
            take_state_error: &mut source.take_state_error,
            cancelled: || (stop.borrow_mut())(),
        };
        let result = StateScanContext {
            source: ctx.state,
            memory: ctx.values.memory,
            class: ctx.values.class,
            io: scan,
        }
        .visit_entries(
            root,
            |entry| {
                if entry.namespace != namespace {
                    return Err(invalid());
                }
                let parsed =
                    WorldFieldKeyV1::from_bytes(&entry.key).map_err(ValueTreeError::Invalid)?;
                let key = WorldFieldKeyV1::new(descriptor, parsed.kind, parsed.semantic_key)
                    .map_err(ValueTreeError::Invalid)?;
                if key.as_bytes() != entry.key {
                    return Err(invalid());
                }
                ctx.values
                    .verify_closure(&entry.value, ctx.record_bytes, &mut ctx.cancelled)?;
                match parsed.kind {
                    WorldFieldRecordKindV1::Metadata => {
                        if metadata {
                            return Err(invalid());
                        }
                        ctx.records().presence(&entry.value, &key, 0)?;
                        metadata = true;
                    }
                    WorldFieldRecordKindV1::Bytes => {
                        if bytes_seen {
                            return Err(invalid());
                        }
                        let (expected, _) =
                            ctx.records()
                                .key(descriptor, WorldFieldRecordKindV1::Bytes, &())?;
                        if key != expected {
                            return Err(invalid());
                        }
                        let limits = MemberRecordLimitsV1 {
                            semantic_bytes: ctx.semantic_bytes,
                            payload_bytes: capture.map(|n| n as u64).unwrap_or(ctx.record_bytes),
                            record_bytes: ctx.record_bytes,
                        };
                        let info = crate::incremental_field_member::read_member_record_header_v1(
                            &entry.value,
                            &key,
                            &mut ctx.values,
                            limits,
                            &mut ctx.cancelled,
                        )?;
                        if info.codec != MemberPayloadCodecV1::Bytes {
                            return Err(invalid());
                        }
                        if capture.is_none() {
                            let checked = read_member_record_v1(
                                &entry.value,
                                &key,
                                &mut ctx.values,
                                limits,
                                &mut std::io::sink(),
                                &mut std::io::sink(),
                                &mut ctx.cancelled,
                            )?;
                            if checked.codec != MemberPayloadCodecV1::Bytes
                                || checked.payload_bytes != info.payload_bytes
                            {
                                return Err(invalid());
                            }
                            bytes_seen = true;
                            return Ok(());
                        }
                        let n = usize::try_from(info.payload_bytes).map_err(|_| invalid())?;
                        let retained = n.checked_add(128).ok_or_else(invalid)?;
                        if retained > max_heap {
                            return Err(ValueTreeError::ResourceLimited(labels.heap));
                        }
                        let charge = ctx.values.memory.reserve(retained, ctx.values.class)?;
                        let mut bytes = vec![0; n];
                        let info = read_member_record_v1(
                            &entry.value,
                            &key,
                            &mut ctx.values,
                            limits,
                            &mut std::io::sink(),
                            &mut &mut bytes[..],
                            &mut ctx.cancelled,
                        )?;
                        if info.codec != MemberPayloadCodecV1::Bytes
                            || info.payload_bytes != n as u64
                        {
                            return Err(invalid());
                        }
                        output_charge = Some(charge);
                        output = Some(bytes);
                        bytes_seen = true;
                    }
                    _ => return Err(invalid()),
                }
                Ok(())
            },
            || (stop.borrow_mut())(),
        );
        let error = (ctx.take_state_error)();
        if (stop.borrow_mut())() {
            return Err(ValueTreeError::Cancelled);
        }
        if let Some(error) = error {
            return Err(error);
        }
        match result {
            Ok(_) => (),
            Err(StateScanError::Visitor(e)) => return Err(e),
            Err(StateScanError::Memory(e)) => return Err(e.into()),
            Err(StateScanError::Cancelled) => return Err(ValueTreeError::Cancelled),
            Err(StateScanError::ResourceLimited) => {
                return Err(ValueTreeError::ResourceLimited(labels.scan));
            }
            Err(_) => return Err(invalid()),
        }
    }
    if !metadata || !bytes_seen {
        return Err(invalid());
    }
    source.check()?;
    Ok(output
        .zip(output_charge)
        .map(|(value, charge)| CapturedByteMapV1 { value, charge }))
}
