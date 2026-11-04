use super::*;
macro_rules! scalar {
    ($ty:ty,$variant:ident,$payload:ident) => {
        impl Codec for $ty {
            fn encode<
                W: ValueNodeWriter + ValueNodeReader + StatePageWriter,
                C: FnMut() -> bool,
            >(
                &self,
                _profile: &TypedRecordProfileV1,
                _ctx: &mut MemberRecordWriteContext<'_, W, C>,
                _work: &mut TypedRecordWriteWorkV1,
            ) -> Result<Encoded<'_>, MemberRecordError> {
                Ok(Encoded::Scalar(MemberPayloadRefV1::$payload(*self)))
            }
            fn decode<
                M: MerkleNodeReader,
                R: ValueNodeReader,
                E: FnMut() -> Option<ValueTreeError>,
                C: FnMut() -> bool,
            >(
                cell: ColumnCellV1,
                _d: &mut Decoder<'_, '_, M, R, E, C>,
            ) -> Result<Self, ValueTreeError> {
                match cell {
                    ColumnCellV1::$variant(v) => Ok(v),
                    _ => Err(invalid()),
                }
            }
        }
    };
}
scalar!(u64, U64, U64);
scalar!(i64, I64, I64);
scalar!(bool, Bool, Bool);
macro_rules! unsigned {($($ty:ty),*)=>{$(impl Codec for $ty {
fn encode<W:ValueNodeWriter+ValueNodeReader+StatePageWriter,C:FnMut()->bool>(&self,_profile:&TypedRecordProfileV1,_ctx:&mut MemberRecordWriteContext<'_,W,C>,_work:&mut TypedRecordWriteWorkV1)->Result<Encoded<'_>,MemberRecordError>{Ok(Encoded::Scalar(MemberPayloadRefV1::U64(u64::from(*self))))}
fn decode<M:MerkleNodeReader,R:ValueNodeReader,E:FnMut()->Option<ValueTreeError>,C:FnMut()->bool>(cell:ColumnCellV1,d:&mut Decoder<'_, '_,M,R,E,C>)->Result<Self,ValueTreeError>{Self::try_from(u64::decode(cell,d)?).map_err(|_|invalid())}
})*};}
unsigned!(u8, u16, u32);
impl Codec for String {
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
        crate::incremental_typed_record_diff::accept_text(profile, before, after, d)
    }
    fn encode<W: ValueNodeWriter + ValueNodeReader + StatePageWriter, C: FnMut() -> bool>(
        &self,
        _profile: &TypedRecordProfileV1,
        _ctx: &mut MemberRecordWriteContext<'_, W, C>,
        _work: &mut TypedRecordWriteWorkV1,
    ) -> Result<Encoded<'_>, MemberRecordError> {
        Ok(Encoded::Scalar(MemberPayloadRefV1::Text(self)))
    }
    fn decode<
        M: MerkleNodeReader,
        R: ValueNodeReader,
        E: FnMut() -> Option<ValueTreeError>,
        C: FnMut() -> bool,
    >(
        cell: ColumnCellV1,
        _d: &mut Decoder<'_, '_, M, R, E, C>,
    ) -> Result<Self, ValueTreeError> {
        match cell {
            ColumnCellV1::Text(s) => Ok(s),
            _ => Err(invalid()),
        }
    }
}
impl<T: Codec> Codec for Option<T> {
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
        crate::incremental_typed_record_diff::accept_option::<T, _, _, _, _>(
            profile, before, after, d,
        )
    }

    fn encode<W: ValueNodeWriter + ValueNodeReader + StatePageWriter, C: FnMut() -> bool>(
        &self,
        profile: &TypedRecordProfileV1,
        ctx: &mut MemberRecordWriteContext<'_, W, C>,
        work: &mut TypedRecordWriteWorkV1,
    ) -> Result<Encoded<'_>, MemberRecordError> {
        match self {
            None => Ok(Encoded::Scalar(MemberPayloadRefV1::Unit)),
            Some(v) => v.encode(profile, ctx, work),
        }
    }
    fn decode<
        M: MerkleNodeReader,
        R: ValueNodeReader,
        E: FnMut() -> Option<ValueTreeError>,
        C: FnMut() -> bool,
    >(
        cell: ColumnCellV1,
        d: &mut Decoder<'_, '_, M, R, E, C>,
    ) -> Result<Self, ValueTreeError> {
        if matches!(cell, ColumnCellV1::Unit) {
            Ok(None)
        } else {
            Ok(Some(T::decode(cell, d)?))
        }
    }
}
impl<T: Codec> Codec for Box<T> {
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
        crate::incremental_typed_record_diff::accept_box::<T, _, _, _, _>(profile, before, after, d)
    }

    fn encode<W: ValueNodeWriter + ValueNodeReader + StatePageWriter, C: FnMut() -> bool>(
        &self,
        profile: &TypedRecordProfileV1,
        ctx: &mut MemberRecordWriteContext<'_, W, C>,
        work: &mut TypedRecordWriteWorkV1,
    ) -> Result<Encoded<'_>, MemberRecordError> {
        (**self).encode(profile, ctx, work)
    }
    fn decode<
        M: MerkleNodeReader,
        R: ValueNodeReader,
        E: FnMut() -> Option<ValueTreeError>,
        C: FnMut() -> bool,
    >(
        cell: ColumnCellV1,
        d: &mut Decoder<'_, '_, M, R, E, C>,
    ) -> Result<Self, ValueTreeError> {
        d.reserve_slot()?;
        let charge = d
            .ctx
            .values
            .memory
            .reserve(std::mem::size_of::<T>(), d.ctx.values.class)?;
        let value = T::decode(cell, d)?;
        d.hold(charge)?;
        Ok(Box::new(value))
    }
}
impl<T: Codec> Codec for Vec<T> {
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
        crate::incremental_typed_record_diff::accept_vec::<T, _, _, _, _>(profile, before, after, d)
    }

    fn encode<W: ValueNodeWriter + ValueNodeReader + StatePageWriter, C: FnMut() -> bool>(
        &self,
        profile: &TypedRecordProfileV1,
        ctx: &mut MemberRecordWriteContext<'_, W, C>,
        work: &mut TypedRecordWriteWorkV1,
    ) -> Result<Encoded<'_>, MemberRecordError> {
        let mut root = new_tree(profile, ctx, work)?;
        put(
            profile,
            &mut root,
            "_length",
            Encoded::Scalar(MemberPayloadRefV1::U64(self.len() as u64)),
            ctx,
            work,
        )?;
        for (i, v) in self.iter().enumerate() {
            let value = v.encode(profile, ctx, work)?;
            put(profile, &mut root, &(i as u64), value, ctx, work)?;
        }
        Ok(Encoded::Tree(root))
    }
    fn decode<
        M: MerkleNodeReader,
        R: ValueNodeReader,
        E: FnMut() -> Option<ValueTreeError>,
        C: FnMut() -> bool,
    >(
        cell: ColumnCellV1,
        d: &mut Decoder<'_, '_, M, R, E, C>,
    ) -> Result<Self, ValueTreeError> {
        let mut c = d.read(cell)?;
        let n = usize::try_from(u64::decode(take_named(&mut c, "_length")?, d)?)
            .map_err(|_| invalid())?;
        c.spend_output(
            n.checked_mul(std::mem::size_of::<T>())
                .ok_or_else(invalid)?,
        )?;
        if c.cells_mut().len() != n {
            return Err(invalid());
        }
        let mut out = Vec::with_capacity(n);
        for i in 0..n {
            d.ctx.check()?;
            let cell = c
                .cells_mut()
                .remove(&ColumnKeyV1::Ordinal(i as u64))
                .ok_or_else(invalid)?;
            out.push(T::decode(cell, d)?);
        }
        d.hold(c.finish_parts()?)?;
        Ok(out)
    }
}
impl<K: CollectionKey, T: Codec> Codec for BTreeMap<K, T> {
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
        crate::incremental_typed_record_diff::accept_map_key::<K, T, _, _, _, _>(
            profile, before, after, d,
        )
    }

    fn encode<W: ValueNodeWriter + ValueNodeReader + StatePageWriter, C: FnMut() -> bool>(
        &self,
        profile: &TypedRecordProfileV1,
        ctx: &mut MemberRecordWriteContext<'_, W, C>,
        work: &mut TypedRecordWriteWorkV1,
    ) -> Result<Encoded<'_>, MemberRecordError> {
        let mut root = new_tree(profile, ctx, work)?;
        put(
            profile,
            &mut root,
            "_length",
            Encoded::Scalar(MemberPayloadRefV1::U64(self.len() as u64)),
            ctx,
            work,
        )?;
        for (k, v) in self {
            let value = v.encode(profile, ctx, work)?;
            put(
                profile,
                &mut root,
                &("entry", k.semantic_key()),
                value,
                ctx,
                work,
            )?;
        }
        Ok(Encoded::Tree(root))
    }
    fn decode<
        M: MerkleNodeReader,
        R: ValueNodeReader,
        E: FnMut() -> Option<ValueTreeError>,
        C: FnMut() -> bool,
    >(
        cell: ColumnCellV1,
        d: &mut Decoder<'_, '_, M, R, E, C>,
    ) -> Result<Self, ValueTreeError> {
        let mut c = d.read(cell)?;
        let n = usize::try_from(u64::decode(take_named(&mut c, "_length")?, d)?)
            .map_err(|_| invalid())?;
        if c.cells_mut().len() != n {
            return Err(invalid());
        }
        c.spend_output(
            n.checked_mul(
                std::mem::size_of::<T>()
                    .checked_add(std::mem::size_of::<K>())
                    .ok_or_else(invalid)?
                    .checked_add(128)
                    .ok_or_else(invalid)?,
            )
            .ok_or_else(invalid)?,
        )?;
        let mut out = BTreeMap::new();
        while let Some((key, value)) = c.cells_mut().pop_first() {
            d.ctx.check()?;
            let ColumnKeyV1::Pair(left, right) = key else {
                return Err(invalid());
            };
            let ColumnKeyV1::Text(tag) = *left else {
                return Err(invalid());
            };
            let key = K::from_column(*right)?;
            if tag != "entry" {
                return Err(invalid());
            }
            if out.insert(key, T::decode(value, d)?).is_some() {
                return Err(invalid());
            }
        }
        d.hold(c.finish_parts()?)?;
        Ok(out)
    }
}
impl<K: CollectionKey> Codec for BTreeSet<K> {
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
        crate::incremental_typed_record_diff::accept_set_key::<K, _, _, _, _>(
            profile, before, after, d,
        )
    }

    fn encode<W: ValueNodeWriter + ValueNodeReader + StatePageWriter, C: FnMut() -> bool>(
        &self,
        profile: &TypedRecordProfileV1,
        ctx: &mut MemberRecordWriteContext<'_, W, C>,
        work: &mut TypedRecordWriteWorkV1,
    ) -> Result<Encoded<'_>, MemberRecordError> {
        let mut root = new_tree(profile, ctx, work)?;
        put(
            profile,
            &mut root,
            "_length",
            Encoded::Scalar(MemberPayloadRefV1::U64(self.len() as u64)),
            ctx,
            work,
        )?;
        for k in self {
            put(
                profile,
                &mut root,
                &("entry", k.semantic_key()),
                Encoded::Scalar(MemberPayloadRefV1::Bool(true)),
                ctx,
                work,
            )?;
        }
        Ok(Encoded::Tree(root))
    }
    fn decode<
        M: MerkleNodeReader,
        R: ValueNodeReader,
        E: FnMut() -> Option<ValueTreeError>,
        C: FnMut() -> bool,
    >(
        cell: ColumnCellV1,
        d: &mut Decoder<'_, '_, M, R, E, C>,
    ) -> Result<Self, ValueTreeError> {
        let mut c = d.read(cell)?;
        let n = usize::try_from(u64::decode(take_named(&mut c, "_length")?, d)?)
            .map_err(|_| invalid())?;
        if c.cells_mut().len() != n {
            return Err(invalid());
        }
        c.spend_output(
            n.checked_mul(
                std::mem::size_of::<K>()
                    .checked_add(128)
                    .ok_or_else(invalid)?,
            )
            .ok_or_else(invalid)?,
        )?;
        let mut out = BTreeSet::new();
        while let Some((key, value)) = c.cells_mut().pop_first() {
            d.ctx.check()?;
            let ColumnKeyV1::Pair(left, right) = key else {
                return Err(invalid());
            };
            let ColumnKeyV1::Text(tag) = *left else {
                return Err(invalid());
            };
            let key = K::from_column(*right)?;
            if tag != "entry" || !bool::decode(value, d)? || !out.insert(key) {
                return Err(invalid());
            }
        }
        d.hold(c.finish_parts()?)?;
        Ok(out)
    }
}
#[path = "incremental_typed_record_json_change.rs"]
mod json_change;
impl Codec for serde_json::Value {
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
        json_change::accept(profile, before, after, d)
    }

    fn encode<W: ValueNodeWriter + ValueNodeReader + StatePageWriter, C: FnMut() -> bool>(
        &self,
        profile: &TypedRecordProfileV1,
        ctx: &mut MemberRecordWriteContext<'_, W, C>,
        work: &mut TypedRecordWriteWorkV1,
    ) -> Result<Encoded<'_>, MemberRecordError> {
        let charge = ctx
            .memory
            .reserve(512, MemoryClass::Ordinary)
            .map_err(ValueTreeError::Memory)?;
        let pool = ctx
            .memory
            .reserve(0, MemoryClass::Ordinary)
            .map_err(ValueTreeError::Memory)?
            .retain(());
        let root = crate::incremental_json_records::prepare_json_subtree_v1(
            crate::incremental_json_records::JsonRecordProfileV1 {
                field: profile.field,
                schema: profile.schema,
            },
            self,
            ctx,
            &mut work.json,
        )?;
        Ok(Encoded::Tree(TypedTreeBuilderV1 { root, charge, pool }))
    }
    fn decode<
        M: MerkleNodeReader,
        R: ValueNodeReader,
        E: FnMut() -> Option<ValueTreeError>,
        C: FnMut() -> bool,
    >(
        cell: ColumnCellV1,
        d: &mut Decoder<'_, '_, M, R, E, C>,
    ) -> Result<Self, ValueTreeError> {
        d.reserve_slot()?;
        let ColumnCellV1::Root(root) = cell else {
            return Err(invalid());
        };
        let profile = crate::incremental_json_records::JsonRecordProfileV1 {
            field: d.profile.field,
            schema: d.profile.schema,
        };
        let proof = crate::incremental_json_records::qualify_json_subtree_records_v1(
            profile,
            &root,
            &mut d.ctx.records(),
            &mut d.work.json,
        )?;
        let (value, charge) = crate::incremental_json_records::materialize_records(
            profile,
            &proof,
            &mut d.ctx.records(),
            &mut d.work.json,
            d.work.max_tree_heap,
        )?;
        d.hold(charge)?;
        Ok(value)
    }
}
