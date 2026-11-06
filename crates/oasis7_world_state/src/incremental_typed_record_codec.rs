//! Closed typed record codecs; owner wrappers pin the profile and authority scope.
pub(crate) use crate::{
    incremental_field_member::*,
    incremental_field_presence::*,
    incremental_field_read::WorldFieldReadContextV1,
    incremental_record_columns_read::*,
    incremental_record_tree::{self, RecordTreeLabelsV1, RecordTreeWritePolicyV1},
    incremental_semantic_key::{SemanticKeyV1, hash_semantic_key_v1},
};
pub(crate) use oasis7_formal_protocol::{
    incremental_record_root::RecordRootRefV1,
    incremental_state::{
        MerkleNodeReader, StateNamespaceV1, StatePageWriter, StateRootV1, StateScanBudget,
    },
    incremental_value::{ValueNodeReader, ValueNodeWriter, ValueTreeError},
};
use oasis7_proto::memory_budget::{MemoryBudget, MemoryClass, MemoryReservation, ReservedValue};
use std::collections::{BTreeMap, BTreeSet};
pub(crate) fn invalid() -> ValueTreeError {
    ValueTreeError::Invalid("closed typed record columns")
}
type TypedReferenceSchemaV1 = fn(&ColumnKeyV1) -> Option<[u8; 32]>;
#[derive(Clone, Copy)]
pub(crate) struct TypedRecordProfileV1 {
    pub(crate) field: FieldDescriptor,
    pub(crate) schema: [u8; 32],
    pub(crate) labels: RecordTreeLabelsV1,
    pub(crate) reference_schema: Option<TypedReferenceSchemaV1>,
}
impl TypedRecordProfileV1 {
    pub(crate) fn namespace(&self) -> Result<StateNamespaceV1, ValueTreeError> {
        match self.field.destination {
            FieldDestination::State(ns) => Ok(ns),
            _ => Err(invalid()),
        }
    }
}
/// Root buffers are admitted before allocation and travel with their data.
pub(crate) struct TypedTreeBuilderV1 {
    root: StateRootV1,
    charge: MemoryReservation,
    pool: ReservedValue<()>,
}
impl TypedTreeBuilderV1 {
    pub(crate) fn empty(
        ns: StateNamespaceV1,
        memory: &MemoryBudget,
    ) -> Result<Self, ValueTreeError> {
        let charge = memory.reserve(512, MemoryClass::Ordinary)?;
        let pool = memory.reserve(0, MemoryClass::Ordinary)?.retain(());
        let root = StateRootV1::empty(ns).map_err(|_| invalid())?;
        Ok(Self { root, charge, pool })
    }
    pub(crate) fn from_root(
        root: &StateRootV1,
        memory: &MemoryBudget,
    ) -> Result<Self, ValueTreeError> {
        root.validate().map_err(|_| invalid())?;
        let charge = memory.reserve(512, MemoryClass::Ordinary)?;
        let pool = memory.reserve(0, MemoryClass::Ordinary)?.retain(());
        Ok(Self {
            root: root.clone(),
            charge,
            pool,
        })
    }
    pub(crate) fn root(&self) -> &StateRootV1 {
        &self.root
    }
    pub(crate) fn check(&self, memory: &MemoryBudget) -> Result<(), ValueTreeError> {
        if !self.pool.uses_budget(memory) {
            return Err(invalid());
        }
        Ok(())
    }
    pub(crate) fn into_reserved(self) -> ReservedValue<StateRootV1> {
        self.charge.retain(self.root)
    }
}
pub struct TypedRecordWriteWorkV1 {
    pub cells_remaining: u64,
    pub max_pending_node_bytes: usize,
    pub json: crate::incremental_json_records::JsonWriteWorkV1,
}
impl TypedRecordWriteWorkV1 {
    pub(crate) fn spend(
        &mut self,
        stop: &mut impl FnMut() -> bool,
    ) -> Result<(), MemberRecordError> {
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
pub(crate) enum Encoded<'a> {
    Scalar(MemberPayloadRefV1<'a>),
    Tree(TypedTreeBuilderV1),
}
pub(crate) fn append<W: ValueNodeWriter + ValueNodeReader + StatePageWriter, C: FnMut() -> bool>(
    profile: &TypedRecordProfileV1,
    root: &mut TypedTreeBuilderV1,
    key: &WorldFieldKeyV1,
    value: [u8; 32],
    ctx: &mut MemberRecordWriteContext<'_, W, C>,
    work: &TypedRecordWriteWorkV1,
) -> Result<(), MemberRecordError> {
    root.check(ctx.memory)?;
    let _scratch = ctx
        .memory
        .reserve(512, MemoryClass::Ordinary)
        .map_err(ValueTreeError::Memory)?;
    root.root = incremental_record_tree::append(
        root.root.clone(),
        key,
        value,
        ctx,
        RecordTreeWritePolicyV1 {
            max_pending_node_bytes: work.max_pending_node_bytes,
            labels: profile.labels,
        },
    )?;
    Ok(())
}
pub(crate) fn new_tree<
    W: ValueNodeWriter + ValueNodeReader + StatePageWriter,
    C: FnMut() -> bool,
>(
    profile: &TypedRecordProfileV1,
    ctx: &mut MemberRecordWriteContext<'_, W, C>,
    work: &mut TypedRecordWriteWorkV1,
) -> Result<TypedTreeBuilderV1, MemberRecordError> {
    work.spend(&mut ctx.cancelled)?;
    let mut root = TypedTreeBuilderV1::empty(profile.namespace()?, ctx.memory)?;
    let row = write_field_metadata_v1(
        profile.field,
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
        profile,
        &mut root,
        &row.field_key,
        row.value_root,
        ctx,
        work,
    )?;
    Ok(root)
}
pub(crate) fn put<
    K: SemanticKeyV1 + ?Sized,
    W: ValueNodeWriter + ValueNodeReader + StatePageWriter,
    C: FnMut() -> bool,
>(
    profile: &TypedRecordProfileV1,
    root: &mut TypedTreeBuilderV1,
    key: &K,
    value: Encoded<'_>,
    ctx: &mut MemberRecordWriteContext<'_, W, C>,
    work: &mut TypedRecordWriteWorkV1,
) -> Result<(), MemberRecordError> {
    work.spend(&mut ctx.cancelled)?;
    root.check(ctx.memory)?;
    let _tree_charge;
    let reference;
    let payload = match value {
        Encoded::Scalar(p) => {
            _tree_charge = None;
            p
        }
        Encoded::Tree(child) => {
            let owner = hash_semantic_key_v1(
                key,
                ctx.memory,
                MemoryClass::Ordinary,
                ctx.key_work,
                &mut ctx.cancelled,
            )
            .map_err(MemberRecordError::Key)?;
            child.check(ctx.memory)?;
            let TypedTreeBuilderV1 {
                root: child_root,
                charge,
                pool: _pool,
            } = child;
            _tree_charge = Some(charge);
            reference = RecordRootRefV1::new(profile.schema, owner.digest, child_root)
                .map_err(|_| invalid())?;
            MemberPayloadRefV1::RecordRoot(&reference)
        }
    };
    let row = write_member_record_v1(
        profile.field,
        WorldFieldRecordKindV1::Entry,
        key,
        payload,
        ctx,
    )?;
    append(profile, root, &row.field_key, row.value_root, ctx, work)
}
pub(crate) trait Codec: Sized {
    fn accept_changed<
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
        crate::incremental_typed_record_diff::accept_fallback::<Self, _, _, _, _>(
            profile, before, after, d,
        )
    }

    fn encode<W: ValueNodeWriter + ValueNodeReader + StatePageWriter, C: FnMut() -> bool>(
        &self,
        profile: &TypedRecordProfileV1,
        ctx: &mut MemberRecordWriteContext<'_, W, C>,
        work: &mut TypedRecordWriteWorkV1,
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
pub struct TypedRecordReadWorkV1 {
    pub json: crate::incremental_json_records::JsonReadWorkV1,
    pub max_retained_trees: usize,
    pub max_tree_heap: usize,
}
pub struct RecoveredTypedRecordV1<T> {
    value: T,
    _charges: Vec<MemoryReservation>,
    // The vector allocation must drop before its metadata admission.
    _carrier: MemoryReservation,
}
impl<T> RecoveredTypedRecordV1<T> {
    pub fn value(&self) -> &T {
        &self.value
    }
}
pub(crate) struct Decoder<'a, 'v, M, R, E, C> {
    pub(crate) ctx: &'a mut WorldFieldReadContextV1<'v, M, R, E, C>,
    pub(crate) scan: &'a mut StateScanBudget,
    pub(crate) work: &'a mut TypedRecordReadWorkV1,
    charges: Vec<MemoryReservation>,
    carrier: MemoryReservation,
    remaining_slots: usize,
    pub(crate) profile: TypedRecordProfileV1,
}
impl<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
> Decoder<'_, '_, M, R, E, C>
{
    pub(crate) fn reserve_slot(&mut self) -> Result<(), ValueTreeError> {
        self.remaining_slots = self
            .remaining_slots
            .checked_sub(1)
            .ok_or(ValueTreeError::ResourceLimited("revocation retained trees"))?;
        Ok(())
    }
    pub(crate) fn read(&mut self, cell: ColumnCellV1) -> Result<ChargedColumnsV1, ValueTreeError> {
        self.reserve_slot()?;
        let ColumnCellV1::Root(root) = cell else {
            return Err(invalid());
        };
        read_columns_v1(
            ColumnReadProfileV1 {
                field: self.profile.field,
                schema: self.profile.schema,
                groups: &[],
                reference_schema: self.profile.reference_schema,
            },
            &root,
            self.ctx,
            self.scan,
            self.work.max_tree_heap,
        )
    }
    pub(crate) fn hold(&mut self, c: MemoryReservation) -> Result<(), ValueTreeError> {
        if self.charges.len() >= self.work.max_retained_trees {
            return Err(ValueTreeError::ResourceLimited("revocation retained trees"));
        }
        self.charges.push(c);
        Ok(())
    }
}
pub(crate) fn take_named(
    c: &mut ChargedColumnsV1,
    k: &str,
) -> Result<ColumnCellV1, ValueTreeError> {
    c.cells_mut()
        .remove(&ColumnKeyV1::text(k))
        .ok_or_else(invalid)
}
pub(crate) fn prepare_typed_record_body_v1<
    T: Codec,
    W: ValueNodeWriter + ValueNodeReader + StatePageWriter,
    C: FnMut() -> bool,
>(
    value: &T,
    profile: &TypedRecordProfileV1,
    ctx: &mut MemberRecordWriteContext<'_, W, C>,
    work: &mut TypedRecordWriteWorkV1,
) -> Result<ReservedValue<StateRootV1>, MemberRecordError> {
    let Encoded::Tree(root) = value.encode(profile, ctx, work)? else {
        return Err(invalid().into());
    };
    if (ctx.cancelled)() {
        return Err(ValueTreeError::Cancelled.into());
    }
    Ok(root.into_reserved())
}
pub(crate) fn recover_typed_record_body_v1<
    T: Codec,
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    root: &StateRootV1,
    profile: &TypedRecordProfileV1,
    ctx: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
    scan: &mut StateScanBudget,
    work: &mut TypedRecordReadWorkV1,
) -> Result<RecoveredTypedRecordV1<T>, ValueTreeError> {
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
    let _root_scratch = ctx.values.memory.reserve(512, ctx.values.class)?;
    let mut decoder = Decoder {
        ctx,
        scan,
        work,
        charges,
        carrier: charge,
        remaining_slots: capacity,
        profile: *profile,
    };
    let value = T::decode(ColumnCellV1::Root(root.clone()), &mut decoder)?;
    decoder.ctx.check()?;
    Ok(RecoveredTypedRecordV1 {
        value,
        _charges: decoder.charges,
        _carrier: decoder.carrier,
    })
}

#[path = "incremental_typed_record_dto.rs"]
mod dto_macro;
#[path = "incremental_typed_record_values.rs"]
mod values;
pub(crate) use dto_macro::dto;
#[path = "incremental_typed_record_enum.rs"]
mod enums;
pub(crate) use enums::tagged_codec;

pub(crate) fn edit<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    W: StatePageWriter,
    CW: FnMut() -> bool,
>(
    profile: &TypedRecordProfileV1,
    root: &mut TypedTreeBuilderV1,
    key: &WorldFieldKeyV1,
    change: (Option<[u8; 32]>, Option<[u8; 32]>),
    read: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
    write: &mut MemberRecordWriteContext<'_, W, CW>,
    work: &mut TypedRecordWriteWorkV1,
) -> Result<(), MemberRecordError> {
    root.check(write.memory)?;
    read.check()?;
    if read.values.class != MemoryClass::Ordinary || !read.values.memory.same_pool(write.memory) {
        return Err(invalid().into());
    }
    if let Some(error) = (read.take_state_error)() {
        return Err(error.into());
    }
    work.spend(&mut || (read.cancelled)() || (write.cancelled)())?;
    let _scratch = write
        .memory
        .reserve(512, MemoryClass::Ordinary)
        .map_err(ValueTreeError::Memory)?;
    let result = incremental_record_tree::edit(
        root.root.clone(),
        key,
        change,
        &mut read.values,
        write,
        &mut read.cancelled,
        RecordTreeWritePolicyV1 {
            max_pending_node_bytes: work.max_pending_node_bytes,
            labels: profile.labels,
        },
    );
    let error = (read.take_state_error)();
    read.check()?;
    if let Some(error) = error {
        return Err(error.into());
    }
    root.root = result?;
    Ok(())
}

#[path = "incremental_typed_record_collections.rs"]
mod collections;
pub(crate) use collections::CollectionKey;

/// Validate one already admitted changed cell, releasing its DTO before charges.
pub(crate) fn validate_typed_cell_v1<
    T: Codec,
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    cell: ColumnCellV1,
    profile: &TypedRecordProfileV1,
    ctx: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
    scan: &mut StateScanBudget,
    work: &mut TypedRecordReadWorkV1,
) -> Result<(), ValueTreeError> {
    let capacity = work.max_retained_trees;
    if capacity == 0 {
        return Err(ValueTreeError::ResourceLimited("typed retained outputs"));
    }
    let carrier = ctx.values.memory.reserve(
        capacity
            .checked_mul(std::mem::size_of::<MemoryReservation>())
            .ok_or_else(invalid)?,
        ctx.values.class,
    )?;
    let charges = Vec::with_capacity(capacity);
    if charges
        .capacity()
        .checked_mul(std::mem::size_of::<MemoryReservation>())
        .ok_or_else(invalid)?
        > carrier.bytes()
    {
        return Err(invalid());
    }
    let mut decoder = Decoder {
        ctx,
        scan,
        work,
        charges,
        carrier,
        remaining_slots: capacity,
        profile: *profile,
    };
    let value = T::decode(cell, &mut decoder)?;
    decoder.ctx.check()?;
    drop(value);
    Ok(())
}

#[cfg(test)]
#[path = "incremental_typed_record_charge_tests.rs"]
mod charge_tests;
