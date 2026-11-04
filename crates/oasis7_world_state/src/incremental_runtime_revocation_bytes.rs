//! Original byte strings use the existing ByteMap/Bytes wire contract.
use super::*;
use oasis7_formal_protocol::incremental_state::{StateScanContext, StateScanError};
use std::cell::RefCell;
pub(super) fn descriptor() -> FieldDescriptor {
    FieldDescriptor {
        owner: FieldOwner::Runtime,
        name: "capability_revocation_state.bytes",
        shape: FieldShape::ByteMap,
        destination: FieldDestination::State(StateNamespaceV1::Authorization),
    }
}
pub(super) fn byte_schema() -> [u8; 32] {
    *blake3::hash(b"oasis7.iws.runtime-revocation.bytes.v1\0").as_bytes()
}
pub(super) fn reference_schema(key: &ColumnKeyV1) -> Option<[u8; 32]> {
    Some(match key {
        ColumnKeyV1::Text(name) if name == "stop_predecessor_canonical_cbor" => byte_schema(),
        _ => schema(),
    })
}
pub(super) fn put_original<
    W: ValueNodeWriter + ValueNodeReader + StatePageWriter,
    C: FnMut() -> bool,
>(
    parent: &mut TypedTreeBuilderV1,
    name: &str,
    source: &[u8],
    ctx: &mut MemberRecordWriteContext<'_, W, C>,
    work: &mut RevocationWriteWorkV1,
) -> Result<(), MemberRecordError> {
    let mut child = TypedTreeBuilderV1::empty(StateNamespaceV1::Authorization, ctx.memory)?;
    work.spend(&mut ctx.cancelled)?;
    let meta = write_field_metadata_v1(
        descriptor(),
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
    crate::incremental_typed_record_codec::append(
        &profile(),
        &mut child,
        &meta.field_key,
        meta.value_root,
        ctx,
        work,
    )?;
    work.spend(&mut ctx.cancelled)?;
    let row = write_member_record_v1(
        descriptor(),
        WorldFieldRecordKindV1::Bytes,
        &(),
        MemberPayloadRefV1::Bytes(source),
        ctx,
    )?;
    crate::incremental_typed_record_codec::append(
        &profile(),
        &mut child,
        &row.field_key,
        row.value_root,
        ctx,
        work,
    )?;
    let owner = hash_semantic_key_v1(
        name,
        ctx.memory,
        MemoryClass::Ordinary,
        ctx.key_work,
        &mut ctx.cancelled,
    )
    .map_err(MemberRecordError::Key)?;
    let _reference_charge = ctx
        .memory
        .reserve(512, MemoryClass::Ordinary)
        .map_err(ValueTreeError::Memory)?;
    let reference = RecordRootRefV1::new(byte_schema(), owner.digest, child.root().clone())
        .map_err(|_| invalid())?;
    work.spend(&mut ctx.cancelled)?;
    let row = write_member_record_v1(
        field(),
        WorldFieldRecordKindV1::Entry,
        name,
        MemberPayloadRefV1::RecordRoot(&reference),
        ctx,
    )?;
    crate::incremental_typed_record_codec::append(
        &profile(),
        parent,
        &row.field_key,
        row.value_root,
        ctx,
        work,
    )
}
struct Captured {
    value: Vec<u8>,
    charge: oasis7_proto::memory_budget::MemoryReservation,
}
fn read_bytes<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    root: &StateRootV1,
    source: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
    scan: &mut StateScanBudget,
    capture: Option<usize>,
) -> Result<Option<Captured>, ValueTreeError> {
    root.validate().map_err(|_| invalid())?;
    source.check()?;
    if root.namespace != StateNamespaceV1::Authorization || root.leaf_count != 2 {
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
                if entry.namespace != StateNamespaceV1::Authorization {
                    return Err(invalid());
                }
                let parsed =
                    WorldFieldKeyV1::from_bytes(&entry.key).map_err(ValueTreeError::Invalid)?;
                let key = WorldFieldKeyV1::new(descriptor(), parsed.kind, parsed.semantic_key)
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
                                .key(descriptor(), WorldFieldRecordKindV1::Bytes, &())?;
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
                            return Err(ValueTreeError::ResourceLimited(
                                "revocation bytes selected heap",
                            ));
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
                return Err(ValueTreeError::ResourceLimited("revocation bytes scan"));
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
        .map(|(value, charge)| Captured { value, charge }))
}
pub(super) fn recover<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    root: &StateRootV1,
    d: &mut Decoder<'_, '_, M, R, E, C>,
) -> Result<Vec<u8>, ValueTreeError> {
    d.reserve_slot()?;
    let Captured { value, charge } =
        read_bytes(root, d.ctx, d.scan, Some(d.work.max_tree_heap))?.ok_or_else(invalid)?;
    d.hold(charge)?;
    Ok(value)
}
pub(super) fn accept_changed<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    profile: &TypedRecordProfileV1,
    before: Option<&crate::incremental_typed_record_diff::TypedCellRefV1<'_>>,
    after: Option<&crate::incremental_typed_record_diff::TypedCellRefV1<'_>>,
    d: &mut crate::incremental_typed_record_diff::ChangeDecoder<'_, '_, M, R, E, C>,
) -> Result<(), ValueTreeError> {
    d.check()?;
    if crate::incremental_typed_record_diff::unchanged(before, after) {
        return Ok(());
    }
    let Some(after) = after else {
        return Ok(());
    };
    let crate::incremental_typed_record_diff::TypedCellRefV1::Entry(entry) = after else {
        return Err(invalid());
    };
    let row = crate::incremental_record_columns_read::read_column_row_v1(
        ColumnReadProfileV1 {
            field: profile.field,
            schema: profile.schema,
            groups: &[],
            reference_schema: profile.reference_schema,
        },
        entry,
        &mut d.ctx.records(),
        d.work.max_tree_heap,
    )?;
    match &row.cell {
        ColumnCellV1::Unit => (),
        ColumnCellV1::Root(root) => {
            d.trees_remaining =
                d.trees_remaining
                    .checked_sub(1)
                    .ok_or(ValueTreeError::ResourceLimited(
                        "revocation bytes changed trees",
                    ))?;
            read_bytes(root, d.ctx, d.scan, None)?;
        }
        _ => return Err(invalid()),
    }
    d.check()
}
