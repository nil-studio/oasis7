//! The only dynamic audit column is the original UTF-8 effect ID set key.
use super::*;
use crate::incremental_utf8_output::Utf8Output;
use std::io::{self, Write};
struct SetKey<'a, 'b, W> {
    prefix: &'a [u8],
    offset: usize,
    tail: &'a mut Utf8Output<'b, W>,
    invalid: bool,
}
impl<W: Write> Write for SetKey<'_, '_, W> {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        let n = (self.prefix.len() - self.offset.min(self.prefix.len())).min(bytes.len());
        if self
            .prefix
            .get(self.offset.min(self.prefix.len())..self.offset.min(self.prefix.len()) + n)
            != Some(&bytes[..n])
        {
            self.invalid = true;
            return Err(io::ErrorKind::InvalidData.into());
        }
        self.offset += n;
        if n < bytes.len() {
            self.tail.write_all(&bytes[n..])?;
            self.offset += bytes.len() - n;
        }
        Ok(bytes.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
pub(crate) fn validate<R: ValueNodeReader, C: FnMut() -> bool>(
    entry: &StateEntryV1,
    records: &mut InventoryValueReader<'_, '_, R, C>,
) -> Result<(), ValueTreeError> {
    let key = WorldFieldKeyV1::from_bytes(&entry.key).map_err(ValueTreeError::Invalid)?;
    if key.kind != WorldFieldRecordKindV1::Entry {
        return Err(invalid());
    }
    let key = WorldFieldKeyV1::new(
        AuthorizationBodyKindV1::Audit.descriptor(),
        key.kind,
        key.semantic_key,
    )
    .map_err(ValueTreeError::Invalid)?;
    if key.as_bytes() != entry.key {
        return Err(invalid());
    }
    let limits = MemberRecordLimitsV1 {
        semantic_bytes: records.semantic_bytes,
        payload_bytes: 0,
        record_bytes: records.record_bytes,
    };
    records
        .values
        .verify_closure(&entry.value, records.record_bytes, &mut *records.cancelled)?;
    let info = read_member_record_header_v1(
        &entry.value,
        &key,
        records.values,
        limits,
        &mut *records.cancelled,
    )?;
    if info.codec != MemberPayloadCodecV1::Unit {
        return Err(invalid());
    }
    let mut prefix = [0u8; 160];
    let mut cursor = io::Cursor::new(prefix.as_mut_slice());
    crate::incremental_semantic_key::write_semantic_key_v1(
        &("committed_effect_receipt_ids", ("member", "")),
        &mut cursor,
    )
    .map_err(|_| invalid())?;
    let len = cursor.position() as usize;
    let id_len = info
        .semantic_bytes
        .checked_sub(len as u64)
        .ok_or_else(invalid)?;
    prefix[len - 8..len].copy_from_slice(&id_len.to_be_bytes());
    let mut sink = io::sink();
    let mut utf8 = Utf8Output::new(&mut sink, "audit effect ID UTF8");
    let mut key_output = SetKey {
        prefix: &prefix[..len],
        offset: 0,
        tail: &mut utf8,
        invalid: false,
    };
    let result = crate::incremental_field_member::read_member_record_key_v1(
        &entry.value,
        &key,
        records.values,
        limits,
        &mut key_output,
        &mut *records.cancelled,
    );
    let invalid_prefix = key_output.invalid;
    let bad = invalid_prefix || key_output.offset as u64 != info.semantic_bytes;
    if (records.cancelled)() {
        return Err(ValueTreeError::Cancelled);
    }
    if let Err(error) = result {
        if !invalid_prefix && !utf8.is_invalid() {
            return Err(error);
        }
        return Err(invalid());
    }
    if bad || !utf8.is_complete() {
        return Err(invalid());
    }
    read_member_record_profile_v1(
        &entry.value,
        &key,
        records.values,
        MemberReadProfileV1 {
            codec: MemberPayloadCodecV1::Unit,
            semantic_bytes: info.semantic_bytes,
            limits,
        },
        &mut io::sink(),
        &mut io::sink(),
        &mut *records.cancelled,
    )?;
    Ok(())
}
