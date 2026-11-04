//! Closed typed persistence for the complete original revocation DTO.
use crate::{
    incremental_field_member::*,
    incremental_field_presence::*,
    incremental_field_read::WorldFieldReadContextV1,
    incremental_record_columns_read::*,
    incremental_record_tree::{self, RecordTreeLabelsV1, RecordTreeWritePolicyV1},
    incremental_semantic_key::{SemanticKeyV1, hash_semantic_key_v1},
};
use oasis7_formal_protocol::{
    incremental_record_root::RecordRootRefV1,
    incremental_state::{
        MerkleNodeReader, StateNamespaceV1, StatePageWriter, StateRootV1, StateScanBudget,
    },
    incremental_value::{ValueNodeReader, ValueNodeWriter, ValueTreeError},
};
use oasis7_proto::memory_budget::{MemoryClass, MemoryReservation};
use std::collections::{BTreeMap, BTreeSet};
fn invalid() -> ValueTreeError {
    ValueTreeError::Invalid("closed runtime revocation columns")
}
fn field() -> FieldDescriptor {
    FieldDescriptor {
        owner: FieldOwner::Runtime,
        name: "capability_revocation_state.columns",
        shape: FieldShape::Map,
        destination: FieldDestination::State(StateNamespaceV1::Authorization),
    }
}
fn schema() -> [u8; 32] {
    *blake3::hash(b"oasis7.iws.runtime-revocation.columns.v1\0").as_bytes()
}
const LABELS: RecordTreeLabelsV1 = RecordTreeLabelsV1 {
    read: "runtime revocation child state read",
    pending: "runtime revocation pending node allowance",
    tree: "runtime revocation typed child state tree",
    digest: "runtime revocation state node digest",
    read_allowance: "runtime revocation child read allowance",
    admission: "runtime revocation child source admission",
    bound: "runtime revocation child source bound",
    producer_pool: "runtime revocation producer pool",
    page: "runtime revocation state page",
    write_allowance: "runtime revocation write allowance",
};

pub struct RevocationWriteWorkV1 {
    pub cells_remaining: u64,
    pub max_pending_node_bytes: usize,
    pub json: crate::incremental_json_records::JsonWriteWorkV1,
}
impl RevocationWriteWorkV1 {
    fn spend(&mut self, stop: &mut impl FnMut() -> bool) -> Result<(), MemberRecordError> {
        if stop() {
            return Err(ValueTreeError::Cancelled.into());
        }
        self.cells_remaining = self
            .cells_remaining
            .checked_sub(1)
            .ok_or(ValueTreeError::ResourceLimited("revocation cells"))?;
        Ok(())
    }
}
enum Encoded<'a> {
    Scalar(MemberPayloadRefV1<'a>),
    Tree(StateRootV1),
}
fn append<W: ValueNodeWriter + ValueNodeReader + StatePageWriter, C: FnMut() -> bool>(
    root: &mut StateRootV1,
    key: &WorldFieldKeyV1,
    value: [u8; 32],
    ctx: &mut MemberRecordWriteContext<'_, W, C>,
    work: &RevocationWriteWorkV1,
) -> Result<(), MemberRecordError> {
    *root = incremental_record_tree::append(
        root.clone(),
        key,
        value,
        ctx,
        RecordTreeWritePolicyV1 {
            max_pending_node_bytes: work.max_pending_node_bytes,
            labels: LABELS,
        },
    )?;
    Ok(())
}
fn new_tree<W: ValueNodeWriter + ValueNodeReader + StatePageWriter, C: FnMut() -> bool>(
    ctx: &mut MemberRecordWriteContext<'_, W, C>,
    work: &mut RevocationWriteWorkV1,
) -> Result<StateRootV1, MemberRecordError> {
    work.spend(&mut ctx.cancelled)?;
    let mut root = StateRootV1::empty(StateNamespaceV1::Authorization).map_err(|_| invalid())?;
    let row = write_field_metadata_v1(
        field(),
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
    append(&mut root, &row.field_key, row.value_root, ctx, work)?;
    Ok(root)
}
fn put<
    K: SemanticKeyV1 + ?Sized,
    W: ValueNodeWriter + ValueNodeReader + StatePageWriter,
    C: FnMut() -> bool,
>(
    root: &mut StateRootV1,
    key: &K,
    value: Encoded<'_>,
    ctx: &mut MemberRecordWriteContext<'_, W, C>,
    work: &mut RevocationWriteWorkV1,
) -> Result<(), MemberRecordError> {
    work.spend(&mut ctx.cancelled)?;
    let reference;
    let payload = match value {
        Encoded::Scalar(p) => p,
        Encoded::Tree(child) => {
            let owner = hash_semantic_key_v1(
                key,
                ctx.memory,
                MemoryClass::Ordinary,
                ctx.key_work,
                &mut ctx.cancelled,
            )
            .map_err(MemberRecordError::Key)?;
            reference =
                RecordRootRefV1::new(schema(), owner.digest, child).map_err(|_| invalid())?;
            MemberPayloadRefV1::RecordRoot(&reference)
        }
    };
    let row = write_member_record_v1(field(), WorldFieldRecordKindV1::Entry, key, payload, ctx)?;
    append(root, &row.field_key, row.value_root, ctx, work)
}
trait Codec: Sized {
    fn encode<W: ValueNodeWriter + ValueNodeReader + StatePageWriter, C: FnMut() -> bool>(
        &self,
        ctx: &mut MemberRecordWriteContext<'_, W, C>,
        work: &mut RevocationWriteWorkV1,
    ) -> Result<Encoded<'_>, MemberRecordError>;
    fn decode<
        M: MerkleNodeReader,
        R: ValueNodeReader,
        E: FnMut() -> Option<ValueTreeError>,
        C: FnMut() -> bool,
    >(
        cell: ColumnCellV1,
        d: &mut Decoder<'_, '_, M, R, E, C>,
    ) -> Result<Self, ValueTreeError>;
}
pub struct RevocationReadWorkV1 {
    pub json: crate::incremental_json_records::JsonReadWorkV1,
    pub max_retained_trees: usize,
    pub max_tree_heap: usize,
}
pub struct RevocationRecoveredV1<T> {
    value: T,
    _charges: Vec<MemoryReservation>,
    // The vector allocation must drop before its metadata admission.
    _carrier: MemoryReservation,
}
impl<T> RevocationRecoveredV1<T> {
    pub fn value(&self) -> &T {
        &self.value
    }
}
struct Decoder<'a, 'v, M, R, E, C> {
    ctx: &'a mut WorldFieldReadContextV1<'v, M, R, E, C>,
    scan: &'a mut StateScanBudget,
    work: &'a mut RevocationReadWorkV1,
    charges: Vec<MemoryReservation>,
    carrier: MemoryReservation,
    remaining_slots: usize,
}
impl<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
> Decoder<'_, '_, M, R, E, C>
{
    fn reserve_slot(&mut self) -> Result<(), ValueTreeError> {
        self.remaining_slots = self
            .remaining_slots
            .checked_sub(1)
            .ok_or(ValueTreeError::ResourceLimited("revocation retained trees"))?;
        Ok(())
    }
    fn read(&mut self, cell: ColumnCellV1) -> Result<ChargedColumnsV1, ValueTreeError> {
        self.reserve_slot()?;
        let ColumnCellV1::Root(root) = cell else {
            return Err(invalid());
        };
        read_columns_v1(
            ColumnReadProfileV1 {
                field: field(),
                schema: schema(),
                groups: &[],
                reference_schema: Some(bytes::reference_schema),
            },
            &root,
            self.ctx,
            self.scan,
            self.work.max_tree_heap,
        )
    }
    fn hold(&mut self, c: MemoryReservation) -> Result<(), ValueTreeError> {
        if self.charges.len() >= self.work.max_retained_trees {
            return Err(ValueTreeError::ResourceLimited("revocation retained trees"));
        }
        self.charges.push(c);
        Ok(())
    }
}
fn take_named(c: &mut ChargedColumnsV1, k: &str) -> Result<ColumnCellV1, ValueTreeError> {
    c.cells_mut()
        .remove(&ColumnKeyV1::text(k))
        .ok_or_else(invalid)
}
#[path = "incremental_runtime_revocation_codec.rs"]
mod codec;
#[path = "incremental_runtime_revocation_dto.rs"]
mod dto;
pub fn prepare_runtime_revocation_body_v1<
    W: ValueNodeWriter + ValueNodeReader + StatePageWriter,
    C: FnMut() -> bool,
>(
    value: &crate::CapabilityRevocationState,
    ctx: &mut MemberRecordWriteContext<'_, W, C>,
    work: &mut RevocationWriteWorkV1,
) -> Result<StateRootV1, MemberRecordError> {
    let Encoded::Tree(root) = value.encode(ctx, work)? else {
        return Err(invalid().into());
    };
    if (ctx.cancelled)() {
        return Err(ValueTreeError::Cancelled.into());
    }
    Ok(root)
}
pub fn recover_runtime_revocation_body_v1<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    root: &StateRootV1,
    ctx: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
    scan: &mut StateScanBudget,
    work: &mut RevocationReadWorkV1,
) -> Result<RevocationRecoveredV1<crate::CapabilityRevocationState>, ValueTreeError> {
    let capacity = work.max_retained_trees;
    let bytes = capacity
        .checked_mul(std::mem::size_of::<MemoryReservation>())
        .ok_or_else(invalid)?;
    let charge = ctx.values.memory.reserve(bytes, ctx.values.class)?;
    let charges = Vec::with_capacity(capacity);
    if charges
        .capacity()
        .checked_mul(std::mem::size_of::<MemoryReservation>())
        .ok_or_else(invalid)?
        > charge.bytes()
    {
        return Err(invalid());
    }
    if work.max_retained_trees == 0 {
        return Err(invalid());
    }
    let mut decoder = Decoder {
        ctx,
        scan,
        work,
        charges,
        carrier: charge,
        remaining_slots: capacity,
    };
    let value =
        crate::CapabilityRevocationState::decode(ColumnCellV1::Root(root.clone()), &mut decoder)?;
    decoder.ctx.check()?;
    Ok(RevocationRecoveredV1 {
        value,
        _charges: decoder.charges,
        _carrier: decoder.carrier,
    })
}

#[path = "incremental_runtime_revocation_family.rs"]
mod family;
pub use family::*;
#[path = "incremental_runtime_revocation_overlay.rs"]
mod overlay;
pub use overlay::*;

#[cfg(test)]
#[path = "incremental_runtime_revocation_tests.rs"]
mod tests;

#[path = "incremental_runtime_revocation_bytes.rs"]
mod bytes;
