//! Native WorldState records retain each original outer map key. A fixed
//! value-only child envelope also represents original scalar/optional fields.
use crate::{incremental_projection::WORLD_STATE_FIELDS, incremental_typed_record_codec::*};
use oasis7_formal_protocol::incremental_execution::ExecutionWriteV1;
use oasis7_proto::memory_budget::{MemoryBudget, MemoryClass, ReservedValue};

#[path = "incremental_world_native_read.rs"]
mod read;
#[cfg(test)]
pub(crate) use read::{read_body_v1, recover_world_native_record_v1};

pub(crate) enum WorldNativeKeyRefV1<'a> {
    Unit,
    Text(&'a str),
    Unsigned,
}
/// Outer Value entries use the original Unit key. This is independent of
/// collection-member keys, whose closed Map/Set grammar stays unchanged.
pub(crate) trait WorldNativeKeyV1 {
    type Semantic<'a>: SemanticKeyV1
    where
        Self: 'a;
    const UNIT: bool;
    fn semantic_key(&self) -> Self::Semantic<'_>;
    fn key_ref(&self) -> WorldNativeKeyRefV1<'_>;
    fn accepts_column(key: &ColumnKeyV1) -> bool;
}
impl WorldNativeKeyV1 for () {
    type Semantic<'a> = ();
    const UNIT: bool = true;
    fn semantic_key(&self) {}
    fn key_ref(&self) -> WorldNativeKeyRefV1<'_> {
        WorldNativeKeyRefV1::Unit
    }
    fn accepts_column(_: &ColumnKeyV1) -> bool {
        false
    }
}
impl WorldNativeKeyV1 for String {
    type Semantic<'a> = &'a str;
    const UNIT: bool = false;
    fn semantic_key(&self) -> &str {
        self.as_str()
    }
    fn key_ref(&self) -> WorldNativeKeyRefV1<'_> {
        WorldNativeKeyRefV1::Text(self)
    }
    fn accepts_column(key: &ColumnKeyV1) -> bool {
        matches!(key, ColumnKeyV1::Text(_))
    }
}
impl WorldNativeKeyV1 for u64 {
    type Semantic<'a> = u64;
    const UNIT: bool = false;
    fn semantic_key(&self) -> u64 {
        *self
    }
    fn key_ref(&self) -> WorldNativeKeyRefV1<'_> {
        WorldNativeKeyRefV1::Unsigned
    }
    fn accepts_column(key: &ColumnKeyV1) -> bool {
        matches!(key, ColumnKeyV1::Ordinal(_))
    }
}

pub(crate) trait WorldNativeFieldSpecV1 {
    type Key: WorldNativeKeyV1;
    type Value: Codec;
    const NAME: &'static str;
    const COLUMNS: &'static str;
    const SHAPE: FieldShape;
    const MAX_RECORDS: Option<u64> = None;

    fn validate_source_record(
        _key: &Self::Key,
        _value: &Self::Value,
        _memory: &MemoryBudget,
        _class: MemoryClass,
    ) -> Result<(), ValueTreeError> {
        Ok(())
    }

    fn validate_record_binding<
        M: MerkleNodeReader,
        R: ValueNodeReader,
        E: FnMut() -> Option<ValueTreeError>,
        C: FnMut() -> bool,
    >(
        _key: WorldNativeKeyRefV1<'_>,
        _body: &StateRootV1,
        _ctx: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
        _scan: &mut StateScanBudget,
        _work: &mut TypedRecordReadWorkV1,
    ) -> Result<(), ValueTreeError> {
        Ok(())
    }

    fn descriptor() -> Result<FieldDescriptor, ValueTreeError> {
        let field = WORLD_STATE_FIELDS
            .iter()
            .copied()
            .find(|field| {
                field.owner == FieldOwner::WorldState
                    && field.name == Self::NAME
                    && field.shape == Self::SHAPE
            })
            .ok_or_else(invalid)?;
        if !matches!(field.destination, FieldDestination::State(_))
            || !matches!(field.shape, FieldShape::Value | FieldShape::Map)
            || (field.shape == FieldShape::Value) != Self::Key::UNIT
            || Self::COLUMNS.strip_suffix(".columns") != Some(Self::NAME)
        {
            return Err(invalid());
        }
        Ok(field)
    }
    fn profile() -> Result<TypedRecordProfileV1, ValueTreeError> {
        let outer = Self::descriptor()?;
        let mut hash = blake3::Hasher::new();
        hash.update(b"oasis7.iws.world-native.record.v1\0");
        hash.update(Self::NAME.as_bytes());
        Ok(TypedRecordProfileV1 {
            field: FieldDescriptor {
                name: Self::COLUMNS,
                shape: FieldShape::Map,
                ..outer
            },
            schema: *hash.finalize().as_bytes(),
            reference_schema: None,
            labels: RecordTreeLabelsV1 {
                read: "world native record read",
                pending: "world native pending nodes",
                tree: "world native record tree",
                digest: "world native node digest",
                read_allowance: "world native read allowance",
                admission: "world native admission",
                bound: "world native source bound",
                producer_pool: "world native producer pool",
                page: "world native state page",
                write_allowance: "world native write allowance",
            },
        })
    }
}
fn invalid() -> ValueTreeError {
    ValueTreeError::Invalid("world native field binding")
}

/// The complete carrier returned by selected cold decoding retains all charges.
pub struct WorldNativeRecordV1<T> {
    value: T,
}
impl<T> WorldNativeRecordV1<T> {
    pub fn value(&self) -> &T {
        &self.value
    }
}
impl<T: Codec> Codec for WorldNativeRecordV1<T> {
    fn encode<W: ValueNodeWriter + ValueNodeReader + StatePageWriter, C: FnMut() -> bool>(
        &self,
        profile: &TypedRecordProfileV1,
        ctx: &mut MemberRecordWriteContext<'_, W, C>,
        work: &mut TypedRecordWriteWorkV1,
    ) -> Result<Encoded<'_>, MemberRecordError> {
        let mut body = new_tree(profile, ctx, work)?;
        let value = self.value.encode(profile, ctx, work)?;
        put(profile, &mut body, "value", value, ctx, work)?;
        Ok(Encoded::Tree(body))
    }
    fn decode<
        M: MerkleNodeReader,
        R: ValueNodeReader,
        E: FnMut() -> Option<ValueTreeError>,
        C: FnMut() -> bool,
    >(
        cell: ColumnCellV1,
        decoder: &mut Decoder<'_, '_, M, R, E, C>,
    ) -> Result<Self, ValueTreeError> {
        let mut columns = decoder.read(cell)?;
        columns.spend_output(std::mem::size_of::<Self>())?;
        let value = T::decode(take_named(&mut columns, "value")?, decoder)?;
        decoder.hold(columns.finish_parts()?)?;
        Ok(Self { value })
    }
    fn accept_changed<
        M: MerkleNodeReader,
        R: ValueNodeReader,
        E: FnMut() -> Option<ValueTreeError>,
        C: FnMut() -> bool,
    >(
        profile: &TypedRecordProfileV1,
        before: Option<&crate::incremental_typed_record_diff::TypedCellRefV1<'_>>,
        after: Option<&crate::incremental_typed_record_diff::TypedCellRefV1<'_>>,
        decoder: &mut crate::incremental_typed_record_diff::ChangeDecoder<'_, '_, M, R, E, C>,
    ) -> Result<(), ValueTreeError> {
        crate::incremental_typed_record_diff::accept_struct(
            profile,
            before,
            after,
            decoder,
            &["value"],
            |name, before, after, decoder| {
                if name != "value" {
                    return Err(invalid());
                }
                T::accept_changed(profile, before, after, decoder)
            },
        )
    }
}
fn spend(remaining: &mut u64, stop: &mut impl FnMut() -> bool) -> Result<(), ValueTreeError> {
    if stop() {
        return Err(ValueTreeError::Cancelled);
    }
    *remaining = remaining
        .checked_sub(1)
        .ok_or(ValueTreeError::ResourceLimited(
            "world native output records",
        ))?;
    Ok(())
}

pub(crate) fn prepare_world_native_metadata_v1<F: WorldNativeFieldSpecV1>(
    ctx: &mut MemberRecordWriteContext<
        '_,
        impl StatePageWriter + ValueNodeReader,
        impl FnMut() -> bool,
    >,
    records: &mut u64,
    sink: &mut impl FnMut(ReservedValue<ExecutionWriteV1>) -> Result<(), ValueTreeError>,
) -> Result<(), MemberRecordError> {
    spend(records, &mut ctx.cancelled)?;
    let row = write_field_metadata_v1(
        F::descriptor()?,
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
    crate::incremental_field_write::emit(row.field_key, row.value_root, ctx, sink)
}

pub(crate) fn prepare_world_native_record_v1<F: WorldNativeFieldSpecV1>(
    key: &F::Key,
    source: &F::Value,
    ctx: &mut MemberRecordWriteContext<
        '_,
        impl StatePageWriter + ValueNodeReader,
        impl FnMut() -> bool,
    >,
    records: &mut u64,
    work: &mut TypedRecordWriteWorkV1,
    sink: &mut impl FnMut(ReservedValue<ExecutionWriteV1>) -> Result<(), ValueTreeError>,
) -> Result<(), MemberRecordError> {
    spend(records, &mut ctx.cancelled)?;
    let _phase = ctx
        .memory
        .reserve(128 << 10, MemoryClass::Ordinary)
        .map_err(ValueTreeError::Memory)?;
    F::validate_source_record(key, source, ctx.memory, MemoryClass::Ordinary)?;
    let profile = F::profile()?;
    let mut body = new_tree(&profile, ctx, work)?;
    let value = source.encode(&profile, ctx, work)?;
    put(&profile, &mut body, "value", value, ctx, work)?;
    let body = body.into_reserved();
    let owner = hash_semantic_key_v1(
        &key.semantic_key(),
        ctx.memory,
        MemoryClass::Ordinary,
        ctx.key_work,
        &mut ctx.cancelled,
    )
    .map_err(MemberRecordError::Key)?;
    let reference = RecordRootRefV1::new(profile.schema, owner.digest, (*body).clone())
        .map_err(|_| invalid())?;
    let row = write_member_record_v1(
        F::descriptor()?,
        WorldFieldRecordKindV1::Entry,
        &key.semantic_key(),
        MemberPayloadRefV1::RecordRoot(&reference),
        ctx,
    )?;
    crate::incremental_field_write::emit(row.field_key, row.value_root, ctx, sink)
}

#[path = "incremental_world_native_family.rs"]
pub(crate) mod family;

#[path = "incremental_world_native_qualification.rs"]
mod qualification;
pub use qualification::*;

pub(crate) use qualification::accept_world_native_fields_verified_v1;
