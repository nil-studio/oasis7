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
    parent: &mut StateRootV1,
    name: &str,
    source: &[u8],
    ctx: &mut MemberRecordWriteContext<'_, W, C>,
    work: &mut RevocationWriteWorkV1,
) -> Result<(), MemberRecordError> {
    let mut child = StateRootV1::empty(StateNamespaceV1::Authorization).map_err(|_| invalid())?;
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
    append(&mut child, &meta.field_key, meta.value_root, ctx, work)?;
    work.spend(&mut ctx.cancelled)?;
    let row = write_member_record_v1(
        descriptor(),
        WorldFieldRecordKindV1::Bytes,
        &(),
        MemberPayloadRefV1::Bytes(source),
        ctx,
    )?;
    append(&mut child, &row.field_key, row.value_root, ctx, work)?;
    let owner = hash_semantic_key_v1(
        name,
        ctx.memory,
        MemoryClass::Ordinary,
        ctx.key_work,
        &mut ctx.cancelled,
    )
    .map_err(MemberRecordError::Key)?;
    let reference =
        RecordRootRefV1::new(byte_schema(), owner.digest, child).map_err(|_| invalid())?;
    work.spend(&mut ctx.cancelled)?;
    let row = write_member_record_v1(
        field(),
        WorldFieldRecordKindV1::Entry,
        name,
        MemberPayloadRefV1::RecordRoot(&reference),
        ctx,
    )?;
    append(parent, &row.field_key, row.value_root, ctx, work)
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
    root.validate().map_err(|_| invalid())?;
    d.ctx.check()?;
    if root.namespace != StateNamespaceV1::Authorization || root.leaf_count != 2 {
        return Err(invalid());
    }
    let _scratch = d.ctx.values.memory.reserve(32 << 10, d.ctx.values.class)?;
    let mut metadata = false;
    // Data drops before its reservation on every early return.
    let mut output_charge = None;
    let mut output = None;
    let max_heap = d.work.max_tree_heap;
    {
        let stop = RefCell::new(&mut d.ctx.cancelled);
        let mut ctx = WorldFieldReadContextV1 {
            root,
            state: d.ctx.state,
            values: oasis7_formal_protocol::incremental_value::ValueReadContext {
                source: d.ctx.values.source,
                memory: d.ctx.values.memory,
                class: d.ctx.values.class,
                io: &mut *d.ctx.values.io,
            },
            key_work: &mut *d.ctx.key_work,
            semantic_bytes: d.ctx.semantic_bytes,
            record_bytes: d.ctx.record_bytes,
            take_state_error: &mut d.ctx.take_state_error,
            cancelled: || (stop.borrow_mut())(),
        };
        let result = StateScanContext {
            source: ctx.state,
            memory: ctx.values.memory,
            class: ctx.values.class,
            io: &mut *d.scan,
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
                        if output.is_some() {
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
                            payload_bytes: max_heap as u64,
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
    if !metadata {
        return Err(invalid());
    }
    let value = output.ok_or_else(invalid)?;
    d.ctx.check()?;
    d.hold(output_charge.ok_or_else(invalid)?)?;
    Ok(value)
}
