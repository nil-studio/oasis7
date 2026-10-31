//! Independent dynamic members of a qualified Supply record.
use super::*;
use crate::{
    incremental_field_overlay::{WorldFieldOverlayContextV1, WorldFieldOverlayErrorV1},
    incremental_inventory_value_read::InventoryValueReader,
};
use oasis7_formal_protocol::{
    incremental_execution::{ExecutionFence, PreparedGuard},
    incremental_state::{StatePageWriter, StateWriteV1},
};
#[derive(Clone, Copy)]
pub enum SupplyMemberKeyV1<'a> {
    Beneficiary(&'a str),
    Request(&'a str),
    Demand(&'a str),
    Allocation(&'a str),
    FrozenLineage(&'a str),
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SupplyMemberValueV1 {
    Unit,
    I64(i64),
    Lineage {
        target: i64,
        fulfilled: i64,
        reserved: i64,
    },
}
impl<'a> SupplyMemberKeyV1<'a> {
    fn paths(
        self,
        owner: SupplyRootKeyV1<'_>,
    ) -> Result<(&'static str, &'a str, usize), ValueTreeError> {
        Ok(match (owner, self) {
            (SupplyRootKeyV1::Authorization(_), Self::Beneficiary(k)) => ("beneficiaries", k, 1),
            (SupplyRootKeyV1::Authorization(_), Self::Request(k)) => ("requests", k, 1),
            (SupplyRootKeyV1::Batch(_), Self::Demand(k)) => ("demands", k, 1),
            (SupplyRootKeyV1::Batch(_), Self::Allocation(k)) => ("allocation", k, 1),
            (SupplyRootKeyV1::Batch(_), Self::FrozenLineage(k)) => ("frozen_lineages", k, 4),
            _ => return Err(ValueTreeError::Invalid("supply member owner")),
        })
    }
    fn payloads(
        self,
        value: Option<SupplyMemberValueV1>,
    ) -> Result<[Option<MemberPayloadRefV1<'static>>; 4], ValueTreeError> {
        Ok(match (self, value) {
            (_, None) => [None; 4],
            (Self::Beneficiary(_), Some(SupplyMemberValueV1::Unit)) => {
                [Some(MemberPayloadRefV1::Unit), None, None, None]
            }
            (
                Self::Request(_) | Self::Demand(_) | Self::Allocation(_),
                Some(SupplyMemberValueV1::I64(v)),
            ) => [Some(MemberPayloadRefV1::I64(v)), None, None, None],
            (
                Self::FrozenLineage(_),
                Some(SupplyMemberValueV1::Lineage {
                    target,
                    fulfilled,
                    reserved,
                }),
            ) => [
                Some(MemberPayloadRefV1::Unit),
                Some(MemberPayloadRefV1::I64(target)),
                Some(MemberPayloadRefV1::I64(fulfilled)),
                Some(MemberPayloadRefV1::I64(reserved)),
            ],
            _ => return Err(ValueTreeError::Invalid("supply member value")),
        })
    }
}
fn paths<'a>(collection: &'static str, member: &'a str) -> [RecordPathV1<'a>; 4] {
    [
        RecordPathV1::Member(collection, member),
        RecordPathV1::MemberField(collection, member, "target"),
        RecordPathV1::MemberField(collection, member, "fulfilled"),
        RecordPathV1::MemberField(collection, member, "reserved"),
    ]
}
fn number<R: ValueNodeReader, C: FnMut() -> bool>(
    root: &[u8; 32],
    key: &(WorldFieldKeyV1, u64),
    records: &mut InventoryValueReader<'_, '_, R, C>,
) -> Result<i64, ValueTreeError> {
    let mut bytes = [0; 8];
    read_member_record_profile_v1(
        root,
        &key.0,
        records.values,
        MemberReadProfileV1 {
            codec: MemberPayloadCodecV1::I64,
            semantic_bytes: key.1,
            limits: MemberRecordLimitsV1 {
                semantic_bytes: key.1,
                payload_bytes: 8,
                record_bytes: records.record_bytes,
            },
        },
        &mut std::io::sink(),
        &mut Cursor::new(bytes.as_mut_slice()),
        &mut *records.cancelled,
    )?;
    if (records.cancelled)() {
        return Err(ValueTreeError::Cancelled);
    }
    Ok(i64::from_be_bytes(bytes))
}
fn unit<R: ValueNodeReader, C: FnMut() -> bool>(
    root: &[u8; 32],
    key: &(WorldFieldKeyV1, u64),
    records: &mut InventoryValueReader<'_, '_, R, C>,
) -> Result<(), ValueTreeError> {
    closure::primitive(
        Kind::Unit,
        root,
        key,
        records.values,
        records.record_bytes,
        0,
        records.cancelled,
    )?;
    Ok(())
}
type Keys = [Option<(WorldFieldKeyV1, u64)>; 4];
type Roots = [Option<[u8; 32]>; 4];
fn read<R: ValueNodeReader, C: FnMut() -> bool>(
    owner: SupplyRootKeyV1<'_>,
    member: SupplyMemberKeyV1<'_>,
    root: &StateRootV1,
    records: &mut InventoryValueReader<'_, '_, R, C>,
) -> Result<(Keys, Roots, Option<SupplyMemberValueV1>), ValueTreeError> {
    let (collection, name, n) = member.paths(owner)?;
    let record = owner.relative();
    for path in [RecordPathV1::Record, RecordPathV1::Collection(collection)] {
        let key = record.overlay_path(path, records)?;
        let value = inner_tree::lookup(root, &key.0, records)?
            .ok_or(ValueTreeError::Invalid("supply member required marker"))?;
        unit(&value, &key, records)?;
    }
    let mut keys: Keys = std::array::from_fn(|_| None);
    let mut roots = [None; 4];
    for (i, path) in paths(collection, name).into_iter().take(n).enumerate() {
        let key = record.overlay_path(path, records)?;
        roots[i] = inner_tree::lookup(root, &key.0, records)?;
        keys[i] = Some(key);
    }
    let value = if let Some(first) = roots[0] {
        if matches!(member, SupplyMemberKeyV1::Beneficiary(_)) {
            unit(&first, keys[0].as_ref().unwrap(), records)?;
            Some(SupplyMemberValueV1::Unit)
        } else if n == 1 {
            Some(SupplyMemberValueV1::I64(number(
                &first,
                keys[0].as_ref().unwrap(),
                records,
            )?))
        } else {
            unit(&first, keys[0].as_ref().unwrap(), records)?;
            let mut nums = [0; 3];
            for i in 1..4 {
                nums[i - 1] = number(
                    &roots[i].ok_or(ValueTreeError::Invalid("supply frozen missing column"))?,
                    keys[i].as_ref().unwrap(),
                    records,
                )?;
            }
            Some(SupplyMemberValueV1::Lineage {
                target: nums[0],
                fulfilled: nums[1],
                reserved: nums[2],
            })
        }
    } else {
        if roots[1..].iter().any(Option::is_some) {
            return Err(ValueTreeError::Invalid("supply frozen orphan column"));
        }
        None
    };
    if (records.cancelled)() {
        return Err(ValueTreeError::Cancelled);
    }
    Ok((keys, roots, value))
}
pub struct SupplyMemberRequestV1<'a> {
    pub owner: SupplyRootKeyV1<'a>,
    pub member: SupplyMemberKeyV1<'a>,
    pub qualification: &'a ReservedValue<SupplyRecordQualificationV1>,
}
/// Context points at the exact qualified inner root. No unrelated member/Text is scanned.
pub fn read_qualified_supply_member_v1<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    request: SupplyMemberRequestV1<'_>,
    context: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
) -> Result<Option<SupplyMemberValueV1>, ValueTreeError> {
    context.check()?;
    if !request.qualification.uses_budget(context.values.memory) {
        return Err(ValueTreeError::Invalid("supply member read pool"));
    }
    let outer = request
        .owner
        .outer()
        .fixed_path(RecordPathV1::Record, context)?;
    let digest = WorldFieldKeyV1::from_bytes(outer.0.as_bytes())
        .map_err(ValueTreeError::Invalid)?
        .semantic_key
        .ok_or(ValueTreeError::Invalid("supply member owner digest"))?;
    request
        .qualification
        .validate_binding(request.owner, &digest, context.root)?;
    let mut records = InventoryValueReader {
        values: &mut context.values,
        key_work: context.key_work,
        semantic_bytes: context.semantic_bytes,
        record_bytes: context.record_bytes,
        cancelled: &mut context.cancelled,
    };
    let (_, _, value) = read(request.owner, request.member, context.root, &mut records)?;
    Ok(value)
}
pub struct SupplyMemberChangeV1<'a> {
    pub request: SupplyMemberRequestV1<'a>,
    pub expected: Option<SupplyMemberValueV1>,
    pub after: Option<SupplyMemberValueV1>,
    pub max_pending_node_bytes: usize,
}
pub fn change_qualified_supply_member_v1<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    W: StatePageWriter,
    CW: FnMut() -> bool,
>(
    change: SupplyMemberChangeV1<'_>,
    context: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    write: &mut MemberRecordWriteContext<'_, W, CW>,
    guard: &PreparedGuard,
    live: &ExecutionFence,
) -> Result<Option<ReservedValue<SupplyRecordQualificationV1>>, WorldFieldOverlayErrorV1> {
    let owner = change.request.owner;
    let cancel = &mut context.cancelled;
    let error = &mut context.take_state_error;
    context.transaction.contribute(|tx| {
        tx.validate_guard(guard, live)?;
        if !tx.uses_budget(write.memory) {
            return Err(ValueTreeError::Invalid("supply member writer pool").into());
        }
        let _operation = write
            .memory
            .reserve(16 << 10, MemoryClass::Ordinary)
            .map_err(ValueTreeError::Memory)?;
        let (collection, name, n) = change.request.member.paths(owner)?;
        let after_payloads = change.request.member.payloads(change.after)?;
        let old_payloads = change.request.member.payloads(change.expected)?;
        let (outer, child, keys, roots, current) = {
            let mut combined = || cancel() || (write.cancelled)();
            let mut records = InventoryValueReader {
                values: &mut context.values,
                key_work: context.key_work,
                semantic_bytes: context.semantic_bytes,
                record_bytes: context.record_bytes,
                cancelled: &mut combined,
            };
            let (outer, child) =
                access::current(owner, change.request.qualification, tx, &mut records, error)?;
            let (keys, roots, current) = read(owner, change.request.member, child, &mut records)?;
            (outer, child, keys, roots, current)
        };
        if current != change.expected {
            return Err(ValueTreeError::Invalid("supply member expected").into());
        }
        if cancel() || (write.cancelled)() {
            return Err(ValueTreeError::Cancelled.into());
        }
        if current == change.after {
            return Ok(None);
        }
        let _rows = write
            .memory
            .reserve(4096, MemoryClass::Ordinary)
            .map_err(ValueTreeError::Memory)?;
        let mut mutations = Vec::with_capacity(n);
        for (i, path) in paths(collection, name).into_iter().take(n).enumerate() {
            let unchanged = match (old_payloads[i], after_payloads[i]) {
                (None, None) | (Some(MemberPayloadRefV1::Unit), Some(MemberPayloadRefV1::Unit)) => {
                    true
                }
                (Some(MemberPayloadRefV1::I64(a)), Some(MemberPayloadRefV1::I64(b))) => a == b,
                _ => false,
            };
            if cancel() || (write.cancelled)() {
                return Err(ValueTreeError::Cancelled.into());
            }
            if unchanged {
                continue;
            }
            let key = keys[i].as_ref().unwrap();
            let after = if let Some(payload) = after_payloads[i] {
                let mut combined = MemberRecordWriteContext {
                    sink: &mut *write.sink,
                    memory: write.memory,
                    io: &mut *write.io,
                    key_work: &mut *write.key_work,
                    limits: write.limits,
                    cancelled: || cancel() || (write.cancelled)(),
                };
                let row = owner.relative().write_path(path, payload, &mut combined)?;
                if row.field_key != key.0 {
                    return Err(ValueTreeError::Invalid("supply member write identity").into());
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
            .ok_or(ValueTreeError::Invalid("supply member owner digest"))?;
        let proof = closure::from_typed(owner, digest, &delta.after, write.memory)?;
        if cancel() || (write.cancelled)() {
            return Err(ValueTreeError::Cancelled.into());
        }
        Ok(Some(proof))
    })
}
