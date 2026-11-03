//! Exact borrowed fixed projections; dynamic members are never replaced here.
use super::*;
use crate::{
    incremental_expected_bytes::Compare,
    incremental_field_overlay::{WorldFieldOverlayContextV1, WorldFieldOverlayErrorV1},
    incremental_inventory_value_read::InventoryValueReader,
};
use oasis7_formal_protocol::{
    incremental_execution::{ExecutionFence, PreparedGuard},
    incremental_state::{StatePageWriter, StateWriteV1},
};
fn invalid() -> ValueTreeError {
    ValueTreeError::Invalid("supply scalar expected/profile")
}
fn equal(a: Option<MemberPayloadRefV1<'_>>, b: Option<MemberPayloadRefV1<'_>>) -> bool {
    match (a, b) {
        (None, None) => true,
        (Some(MemberPayloadRefV1::Unit), Some(MemberPayloadRefV1::Unit)) => true,
        (Some(MemberPayloadRefV1::Text(a)), Some(MemberPayloadRefV1::Text(b))) => a == b,
        (Some(MemberPayloadRefV1::U64(a)), Some(MemberPayloadRefV1::U64(b))) => a == b,
        (Some(MemberPayloadRefV1::I64(a)), Some(MemberPayloadRefV1::I64(b))) => a == b,
        (Some(MemberPayloadRefV1::Bool(a)), Some(MemberPayloadRefV1::Bool(b))) => a == b,
        _ => false,
    }
}
fn expected<R: ValueNodeReader, C: FnMut() -> bool>(
    payload: MemberPayloadRefV1<'_>,
    value: &[u8; 32],
    key: &(WorldFieldKeyV1, u64),
    records: &mut InventoryValueReader<'_, '_, R, C>,
    max: u64,
) -> Result<(), ValueTreeError> {
    let mut bytes = [0u8; 8];
    let (codec, data) = match payload {
        MemberPayloadRefV1::Unit => (MemberPayloadCodecV1::Unit, &bytes[..0]),
        MemberPayloadRefV1::U64(v) => {
            bytes = v.to_be_bytes();
            (MemberPayloadCodecV1::U64, &bytes[..])
        }
        MemberPayloadRefV1::I64(v) => {
            bytes = v.to_be_bytes();
            (MemberPayloadCodecV1::I64, &bytes[..])
        }
        MemberPayloadRefV1::Bool(v) => {
            bytes[0] = u8::from(v);
            (MemberPayloadCodecV1::Bool, &bytes[..1])
        }
        MemberPayloadRefV1::Text(v) => (MemberPayloadCodecV1::Text, v.as_bytes()),
        _ => return Err(invalid()),
    };
    let mut comparison = Compare::new(data);
    let profile = MemberReadProfileV1 {
        codec,
        semantic_bytes: key.1,
        limits: MemberRecordLimitsV1 {
            semantic_bytes: key.1,
            payload_bytes: if codec == MemberPayloadCodecV1::Text {
                max
            } else {
                data.len() as u64
            },
            record_bytes: records.record_bytes,
        },
    };
    if codec == MemberPayloadCodecV1::Text {
        crate::incremental_utf8_output::read_text_value(
            value,
            &key.0,
            records.values,
            profile,
            &mut comparison,
            "supply scalar UTF-8",
            &mut *records.cancelled,
        )?;
    } else {
        read_member_record_profile_v1(
            value,
            &key.0,
            records.values,
            profile,
            &mut std::io::sink(),
            &mut comparison,
            &mut *records.cancelled,
        )?;
    }
    if (records.cancelled)() {
        return Err(ValueTreeError::Cancelled);
    }
    if !comparison.is_equal() {
        return Err(invalid());
    }
    Ok(())
}
pub struct SupplyScalarChangeV1<'a> {
    pub owner: SupplyRootKeyV1<'a>,
    pub qualification: &'a ReservedValue<SupplyRecordQualificationV1>,
    pub expected: SupplyRootSourceV1<'a>,
    pub after: SupplyRootSourceV1<'a>,
    pub max_current_payload_bytes: u64,
    pub max_pending_node_bytes: usize,
}
/// Only closed scalar columns are compared/replaced. Source collections may be
/// nonempty and remain entirely outside this operation; use member setters.
pub fn change_qualified_supply_scalars_v1<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    W: StatePageWriter,
    CW: FnMut() -> bool,
>(
    change: SupplyScalarChangeV1<'_>,
    context: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    write: &mut MemberRecordWriteContext<'_, W, CW>,
    guard: &PreparedGuard,
    live: &ExecutionFence,
) -> Result<Option<ReservedValue<SupplyRecordQualificationV1>>, WorldFieldOverlayErrorV1> {
    let owner = change.owner;
    let cancel = &mut context.cancelled;
    let error = &mut context.take_state_error;
    context.transaction.contribute(|tx| {
        tx.validate_guard(guard, live)?;
        if !change.expected.matches(owner)
            || !change.after.matches(owner)
            || !tx.uses_budget(write.memory)
        {
            return Err(invalid().into());
        }
        let _operation = write
            .memory
            .reserve(32 << 10, MemoryClass::Ordinary)
            .map_err(ValueTreeError::Memory)?;
        let old = profile::fixed(change.expected);
        let new = profile::fixed(change.after);
        let mut keys: [Option<(WorldFieldKeyV1, u64)>; 29] = std::array::from_fn(|_| None);
        let mut roots = [None; 29];
        let (outer, child) = {
            let mut combined = || cancel() || (write.cancelled)();
            let mut records = InventoryValueReader {
                values: &mut context.values,
                key_work: context.key_work,
                semantic_bytes: context.semantic_bytes,
                record_bytes: context.record_bytes,
                cancelled: &mut combined,
            };
            let (outer, child) =
                access::current(owner, change.qualification, tx, &mut records, error)?;
            for i in 0..owner.count() {
                let key = owner
                    .relative()
                    .overlay_path(owner.spec(i).path, &mut records)?;
                let root = inner_tree::lookup(child, &key.0, &mut records)?;
                match (old[i], root) {
                    (Some(payload), Some(value)) => expected(
                        payload,
                        &value,
                        &key,
                        &mut records,
                        change.max_current_payload_bytes,
                    )?,
                    (None, None) => {}
                    _ => return Err(invalid().into()),
                }
                keys[i] = Some(key);
                roots[i] = root;
            }
            (outer, child)
        };
        let mut mutations = Vec::with_capacity(owner.count());
        for i in 0..owner.count() {
            let unchanged = equal(old[i], new[i]);
            if cancel() || (write.cancelled)() {
                return Err(ValueTreeError::Cancelled.into());
            }
            if unchanged {
                continue;
            }
            let key = keys[i].as_ref().unwrap();
            let after = if let Some(payload) = new[i] {
                let mut combined = MemberRecordWriteContext {
                    sink: &mut *write.sink,
                    memory: write.memory,
                    io: &mut *write.io,
                    key_work: &mut *write.key_work,
                    limits: write.limits,
                    cancelled: || cancel() || (write.cancelled)(),
                };
                let row =
                    owner
                        .relative()
                        .write_path(owner.spec(i).path, payload, &mut combined)?;
                if row.field_key != key.0 {
                    return Err(invalid().into());
                }
                Some(row.value_root)
            } else {
                None
            };
            if cancel() || (write.cancelled)() {
                return Err(ValueTreeError::Cancelled.into());
            }
            mutations.push(StateWriteV1 {
                key: key.0.as_bytes().to_vec(),
                before: roots[i],
                after,
            });
        }
        if cancel() || (write.cancelled)() {
            return Err(ValueTreeError::Cancelled.into());
        }
        if mutations.is_empty() {
            return Ok(None);
        }
        mutations.sort_by(|a, b| a.key.cmp(&b.key));
        let delta = {
            let mut combined = || cancel() || (write.cancelled)();
            let mut records = InventoryValueReader {
                values: &mut context.values,
                key_work: context.key_work,
                semantic_bytes: context.semantic_bytes,
                record_bytes: context.record_bytes,
                cancelled: &mut combined,
            };
            inner_tree::build(
                child,
                mutations,
                &mut records,
                change.max_pending_node_bytes,
            )?
        };
        let row = {
            let mut combined = MemberRecordWriteContext {
                sink: &mut *write.sink,
                memory: write.memory,
                io: &mut *write.io,
                key_work: &mut *write.key_work,
                limits: write.limits,
                cancelled: || cancel() || (write.cancelled)(),
            };
            access::store(owner, &outer, &delta, &mut combined)?
        };
        if cancel() || (write.cancelled)() {
            return Err(ValueTreeError::Cancelled.into());
        }
        tx.write(
            StateNamespaceV1::Authorization,
            outer.as_bytes(),
            Some(row.value_root),
        )?;
        let digest = WorldFieldKeyV1::from_bytes(outer.as_bytes())
            .map_err(ValueTreeError::Invalid)?
            .semantic_key
            .ok_or_else(invalid)?;
        let proof = closure::from_typed(owner, digest, &delta.after, write.memory)?;
        if cancel() || (write.cancelled)() {
            return Err(ValueTreeError::Cancelled.into());
        }
        Ok(Some(proof))
    })
}
