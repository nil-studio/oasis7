//! Closed typed enum codec shared by record owners.
macro_rules! tagged_codec {
    (@impl $mode:ident, $ty:ty, units {$($unit:ident = $utag:literal),* $(,)?},
     tuples {$($tuple:ident($value:ident) = $ttag:literal),* $(,)?},
     records {$($record:ident {$($field:ident),+} = $rtag:literal),* $(,)?}) => {
        $crate::incremental_typed_record_codec::tagged_codec!(@impl_fields plain,$mode,$ty,
            units {$($unit=$utag),*},tuples {$($tuple($value)=$ttag),*},
            records {$($record {$($field),+}=$rtag),*});
    };
    (@fields $hook:ident, $ty:ty, units {$($unit:ident = $utag:literal),* $(,)?},
     tuples {$($tuple:ident($value:ident) = $ttag:literal),* $(,)?},
     records {$($record:ident {$($field:ident),+} = $rtag:literal),* $(,)?}) => {
        $crate::incremental_typed_record_codec::tagged_codec!(@impl_fields $hook,multiple,$ty,
            units {$($unit=$utag),*},tuples {$($tuple($value)=$ttag),*},
            records {$($record {$($field),+}=$rtag),*});
    };
    (@impl_fields $hook:ident, $mode:ident, $ty:ty, units {$($unit:ident = $utag:literal),* $(,)?},
     tuples {$($tuple:ident($value:ident) = $ttag:literal),* $(,)?},
     records {$($record:ident {$($field:ident),+} = $rtag:literal),* $(,)?}) => {
        impl Codec for $ty {
            fn accept_changed<M: MerkleNodeReader, R: ValueNodeReader,
                E: FnMut() -> Option<ValueTreeError>, C: FnMut() -> bool>(
                profile: &TypedRecordProfileV1,
                before: Option<&$crate::incremental_typed_record_diff::TypedCellRefV1<'_>>,
                after: Option<&$crate::incremental_typed_record_diff::TypedCellRefV1<'_>>,
                d: &mut $crate::incremental_typed_record_diff::ChangeDecoder<'_, '_, M, R, E, C>,
            ) -> Result<(), ValueTreeError> {
                let Some(choice) = $crate::incremental_typed_record_diff::tagged_choice_v1(
                    profile, before, after, d,
                )? else { return Ok(()); };
                let names: &'static [&'static str] = match choice.tag {
                    $($utag => &["tag"],)*
                    $($ttag => &["tag", "value"],)*
                    $($rtag => &["tag", $(stringify!($field)),+],)*
                    _ => return Err(ValueTreeError::Invalid("typed enum tag")),
                };
                let before = if choice.inherit_before { before } else { None };
                $crate::incremental_typed_record_diff::accept_struct(
                    profile, before, after, d, names, |name, old, new, d| {
                        if name == "tag" {
                            return <u64 as Codec>::accept_changed(profile, old, new, d);
                        }
                        match choice.tag {
                            $($utag => Err(ValueTreeError::Invalid("unit enum field")),)*
                            $($ttag => match name {
                                "value" => $crate::incremental_typed_record_diff::accept_projected::<Self, _, _, _, _, _>(
                                    $crate::incremental_typed_record_codec::tagged_codec!(@project $mode, tuple, $tuple, value),
                                    profile, old, new, d),
                                _ => Err(ValueTreeError::Invalid("tuple enum field")),
                            },)*
                            $($rtag => match name {
                                $(stringify!($field) => $crate::incremental_typed_record_codec::tagged_codec!(@field_hook $hook,accept,$field,
                                    $crate::incremental_typed_record_codec::tagged_codec!(@project $mode, record, $record, $field),
                                    profile, old, new, d),)+
                                _ => Err(ValueTreeError::Invalid("record enum field")),
                            },)*
                            _ => Err(ValueTreeError::Invalid("typed enum tag")),
                        }
                    },
                )
            }
            fn encode<W: ValueNodeWriter + ValueNodeReader + StatePageWriter, C: FnMut() -> bool>(
                &self, profile: &TypedRecordProfileV1,
                ctx: &mut MemberRecordWriteContext<'_, W, C>,
                work: &mut TypedRecordWriteWorkV1,
            ) -> Result<Encoded<'_>, MemberRecordError> {
                let mut root = new_tree(profile, ctx, work)?;
                let tag: u64 = match self {
                    $(Self::$unit => $utag,)*
                    $(Self::$tuple(_) => $ttag,)*
                    $(Self::$record { $($field: _),+ } => $rtag,)*
                };
                put(profile, &mut root, "tag", Encoded::Scalar(MemberPayloadRefV1::U64(tag)), ctx, work)?;
                match self {
                    $(Self::$unit => {},)*
                    $(Self::$tuple($value) => {
                        let encoded = $value.encode(profile, ctx, work)?;
                        put(profile, &mut root, "value", encoded, ctx, work)?;
                    },)*
                    $(Self::$record { $($field),+ } => {
                        $($crate::incremental_typed_record_codec::tagged_codec!(@field_hook $hook,put,$field,
                            profile,&mut root,$field,ctx,work)?;)+
                    },)*
                }
                Ok(Encoded::Tree(root))
            }
            fn decode<M: MerkleNodeReader, R: ValueNodeReader,
                E: FnMut() -> Option<ValueTreeError>, C: FnMut() -> bool>(
                cell: ColumnCellV1, d: &mut Decoder<'_, '_, M, R, E, C>,
            ) -> Result<Self, ValueTreeError> {
                let mut columns = d.read(cell)?;
                columns.spend_output(std::mem::size_of::<Self>())?;
                let tag = u64::decode(take_named(&mut columns, "tag")?, d)?;
                let value = match tag {
                    $($utag => Self::$unit,)*
                    $($ttag => Self::$tuple(Codec::decode(take_named(&mut columns, "value")?, d)?),)*
                    $($rtag => Self::$record {
                        $($field: $crate::incremental_typed_record_codec::tagged_codec!(@field_hook $hook,decode,$field,
                            take_named(&mut columns, stringify!($field))?,d)?),+
                    },)*
                    _ => return Err(ValueTreeError::Invalid("typed enum tag")),
                };
                d.hold(columns.finish_parts()?)?;
                Ok(value)
            }
        }
    };
    (@field_hook plain,$op:ident,$($args:tt)*) => {
        $crate::incremental_typed_record_codec::tagged_codec!(@default_field $op,$($args)*)
    };
    (@field_hook $hook:ident,$op:ident,$($args:tt)*) => {$hook!($op,$($args)*)};
    (@default_field put,$field:ident,$profile:expr,$parent:expr,$value:expr,$ctx:expr,$work:expr) => {{
        let encoded=$value.encode($profile,$ctx,$work)?;
        put($profile,$parent,stringify!($field),encoded,$ctx,$work)
    }};
    (@default_field decode,$field:ident,$cell:expr,$decoder:expr) => {Codec::decode($cell,$decoder)};
    (@default_field accept,$field:ident,$projection:expr,$profile:expr,$old:expr,$new:expr,$decoder:expr) => {
        $crate::incremental_typed_record_diff::accept_projected::<Self,_,_,_,_,_>(
            $projection,$profile,$old,$new,$decoder)
    };
    (@project single, tuple, $variant:ident, $value:ident) => {
        |owner| { let Self::$variant($value) = owner; $value }
    };
    (@project single, record, $variant:ident, $field:ident) => {
        |owner| { let Self::$variant { $field, .. } = owner; $field }
    };
    (@project multiple, tuple, $variant:ident, $value:ident) => {
        |owner| match owner {
            Self::$variant($value) => $value,
            _ => unreachable!("unevaluated typed enum projection"),
        }
    };
    (@project multiple, record, $variant:ident, $field:ident) => {
        |owner| match owner {
            Self::$variant { $field, .. } => $field,
            _ => unreachable!("unevaluated typed enum projection"),
        }
    };
    ($ty:ty, units {}, tuples {$variant:ident($value:ident) = $tag:literal $(,)?}, records {}) => {
        $crate::incremental_typed_record_codec::tagged_codec!(@impl single, $ty,
            units {}, tuples {$variant($value) = $tag}, records {});
    };
    ($ty:ty, units {}, tuples {}, records {$variant:ident {$($field:ident),+} = $tag:literal $(,)?}) => {
        $crate::incremental_typed_record_codec::tagged_codec!(@impl single, $ty,
            units {}, tuples {}, records {$variant {$($field),+} = $tag});
    };
    ($ty:ty, units {$($unit:ident = $utag:literal),* $(,)?},
     tuples {$($tuple:ident($value:ident) = $ttag:literal),* $(,)?},
     records {$($record:ident {$($field:ident),+} = $rtag:literal),* $(,)?}) => {
        $crate::incremental_typed_record_codec::tagged_codec!(@impl multiple, $ty,
            units {$($unit = $utag),*}, tuples {$($tuple($value) = $ttag),*},
            records {$($record {$($field),+} = $rtag),*});
    };
}

pub(crate) use tagged_codec;
