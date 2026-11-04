//! Only ModuleSourcePackage.files selects the pending byte-map value codec.
use super::*;
use crate::incremental_typed_record_diff::{ChangeDecoder, TypedCellRefV1};
use std::collections::BTreeMap;

pub(super) fn schema() -> [u8; 32] {
    *blake3::hash(b"oasis7.iws.pending-actions.files.v1\0").as_bytes()
}
fn references(key: &ColumnKeyV1) -> Option<[u8; 32]> {
    Some(match key {
        ColumnKeyV1::Pair(tag, name)
            if matches!(&**tag,ColumnKeyV1::Text(s) if s=="entry")
                && matches!(&**name, ColumnKeyV1::Text(_)) =>
        {
            bytes::schema()
        }
        _ => schema(),
    })
}
fn profile(parent: &TypedRecordProfileV1) -> TypedRecordProfileV1 {
    TypedRecordProfileV1 {
        schema: schema(),
        reference_schema: Some(references),
        ..*parent
    }
}

struct FileBlob(Vec<u8>);
impl Codec for FileBlob {
    fn encode<W: ValueNodeWriter + ValueNodeReader + StatePageWriter, C: FnMut() -> bool>(
        &self,
        p: &TypedRecordProfileV1,
        c: &mut MemberRecordWriteContext<'_, W, C>,
        w: &mut TypedRecordWriteWorkV1,
    ) -> Result<Encoded<'_>, MemberRecordError> {
        let _ = (p, c, w, &self.0);
        Err(ValueTreeError::Invalid("pending file validator cannot mint records").into())
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
        Ok(Self(bytes::decode(cell, d)?))
    }
    fn accept_changed<
        M: MerkleNodeReader,
        R: ValueNodeReader,
        E: FnMut() -> Option<ValueTreeError>,
        C: FnMut() -> bool,
    >(
        p: &TypedRecordProfileV1,
        a: Option<&TypedCellRefV1<'_>>,
        b: Option<&TypedCellRefV1<'_>>,
        d: &mut ChangeDecoder<'_, '_, M, R, E, C>,
    ) -> Result<(), ValueTreeError> {
        bytes::accept_changed(p, a, b, d)
    }
}
impl Codec for ModuleSourcePackage {
    fn encode<W: ValueNodeWriter + ValueNodeReader + StatePageWriter, C: FnMut() -> bool>(
        &self,
        p: &TypedRecordProfileV1,
        c: &mut MemberRecordWriteContext<'_, W, C>,
        w: &mut TypedRecordWriteWorkV1,
    ) -> Result<Encoded<'_>, MemberRecordError> {
        let Self {
            manifest_path,
            files,
        } = self;
        let mut root = new_tree(p, c, w)?;
        let value = manifest_path.encode(p, c, w)?;
        put(p, &mut root, "manifest_path", value, c, w)?;
        let fp = profile(p);
        let mut file_root = new_tree(&fp, c, w)?;
        put(
            &fp,
            &mut file_root,
            "_length",
            Encoded::Scalar(MemberPayloadRefV1::U64(files.len() as u64)),
            c,
            w,
        )?;
        for (name, source) in files {
            bytes::put_original(&fp, &mut file_root, &("entry", name.as_str()), source, c, w)?;
        }
        put(&fp, &mut root, "files", Encoded::Tree(file_root), c, w)?;
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
        c.spend_output(std::mem::size_of::<Self>())?;
        let manifest_path = String::decode(take_named(&mut c, "manifest_path")?, d)?;
        let previous = d.profile;
        d.profile = profile(&previous);
        let result: Result<BTreeMap<String, Vec<u8>>, ValueTreeError> = (|| {
            let mut f = d.read(take_named(&mut c, "files")?)?;
            let n = usize::try_from(u64::decode(take_named(&mut f, "_length")?, d)?)
                .map_err(|_| invalid())?;
            if f.cells_mut().len() != n {
                return Err(invalid());
            }
            f.spend_output(
                n.checked_mul(std::mem::size_of::<String>() + std::mem::size_of::<Vec<u8>>() + 128)
                    .ok_or_else(invalid)?,
            )?;
            let mut files = BTreeMap::new();
            while let Some((key, cell)) = f.cells_mut().pop_first() {
                d.ctx.check()?;
                let ColumnKeyV1::Pair(tag, name) = key else {
                    return Err(invalid());
                };
                if !matches!(*tag,ColumnKeyV1::Text(ref s) if s=="entry") {
                    return Err(invalid());
                }
                let ColumnKeyV1::Text(name) = *name else {
                    return Err(invalid());
                };
                if files.insert(name, bytes::decode(cell, d)?).is_some() {
                    return Err(invalid());
                }
            }
            d.hold(f.finish_parts()?)?;
            Ok(files)
        })();
        d.profile = previous;
        let files = result?;
        d.hold(c.finish_parts()?)?;
        Ok(Self {
            manifest_path,
            files,
        })
    }
    fn accept_changed<
        M: MerkleNodeReader,
        R: ValueNodeReader,
        E: FnMut() -> Option<ValueTreeError>,
        C: FnMut() -> bool,
    >(
        p: &TypedRecordProfileV1,
        a: Option<&TypedCellRefV1<'_>>,
        b: Option<&TypedCellRefV1<'_>>,
        d: &mut ChangeDecoder<'_, '_, M, R, E, C>,
    ) -> Result<(), ValueTreeError> {
        crate::incremental_typed_record_diff::accept_struct(
            p,
            a,
            b,
            d,
            &["manifest_path", "files"],
            |name, a, b, d| match name {
                "manifest_path" => String::accept_changed(p, a, b, d),
                "files" => crate::incremental_typed_record_diff::accept_map_key::<
                    String,
                    FileBlob,
                    _,
                    _,
                    _,
                    _,
                >(&profile(p), a, b, d),
                _ => Err(invalid()),
            },
        )
    }
}
