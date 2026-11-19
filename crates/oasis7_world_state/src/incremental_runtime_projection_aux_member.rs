//! Original semantic-key/member/record-root reader shared by fixed aux families.
//! No key is inferred from its hashed locator.
use super::*;
use crate::{
    incremental_field_member::*,
    incremental_semantic_key::{SemanticKeyRefV1, read_semantic_key_v1},
};
use oasis7_formal_protocol::{
    incremental_record_root::{RECORD_ROOT_BYTES, RecordRootRefV1},
    incremental_state::StateRootV1,
};
pub(crate) struct TextRoot {
    pub key: ReservedValue<String>,
    pub root: StateRootV1,
}
pub(super) fn text_root<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    key: &WorldFieldKeyV1,
    id: &[u8; 32],
    schema: &[u8; 32],
    read: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
    p: &Processing<'_>,
) -> Result<TextRoot, ValueTreeError> {
    read.check()?;
    let header = read_member_record_header_v1(
        id,
        key,
        &mut read.values,
        MemberRecordLimitsV1 {
            semantic_bytes: read.semantic_bytes,
            payload_bytes: RECORD_ROOT_BYTES as u64,
            record_bytes: read.record_bytes,
        },
        &mut read.cancelled,
    )?;
    let n = usize::try_from(header.semantic_bytes).map_err(|_| limit())?;
    spend(p, n, &mut read.cancelled)?;
    let scratch = read.values.memory.reserve(n, MemoryClass::Ordinary)?;
    let mut semantic = Vec::new();
    semantic.try_reserve_exact(n).map_err(|_| limit())?;
    if semantic.capacity() > n {
        return Err(limit());
    }
    semantic.resize(n, 0);
    let mut bytes = [0u8; RECORD_ROOT_BYTES];
    let info = read_member_record_v1(
        id,
        key,
        &mut read.values,
        MemberRecordLimitsV1 {
            semantic_bytes: read.semantic_bytes,
            payload_bytes: RECORD_ROOT_BYTES as u64,
            record_bytes: read.record_bytes,
        },
        &mut std::io::Cursor::new(&mut semantic[..]),
        &mut &mut bytes[..],
        &mut read.cancelled,
    )?;
    if info.codec != MemberPayloadCodecV1::RecordRoot
        || info.payload_bytes != RECORD_ROOT_BYTES as u64
        || info.semantic_bytes != header.semantic_bytes
    {
        return Err(bad());
    }
    let SemanticKeyRefV1::Text(text) =
        read_semantic_key_v1(&semantic).map_err(ValueTreeError::Invalid)?
    else {
        return Err(bad());
    };
    spend(p, text.len(), &mut read.cancelled)?;
    let fee = read.values.memory.reserve(
        size_of::<String>()
            .checked_add(text.len())
            .ok_or_else(limit)?,
        MemoryClass::Ordinary,
    )?;
    let owned = text.to_owned();
    let parsed = WorldFieldKeyV1::from_bytes(key.as_bytes()).map_err(ValueTreeError::Invalid)?;
    let reference = RecordRootRefV1::from_canonical_bytes(&bytes).map_err(|_| bad())?;
    let root = reference
        .validate_binding(
            schema,
            &parsed.semantic_key.ok_or_else(bad)?,
            parsed.namespace,
        )
        .map_err(|_| bad())?
        .clone();
    drop(scratch);
    read.check()?;
    Ok(TextRoot {
        key: fee.retain(owned),
        root,
    })
}

use std::io::{self, Write};
pub(crate) struct Text<'a, 'p> {
    pub(crate) bytes: Vec<u8>,
    pub(crate) fee: MemoryReservation,
    pub(crate) memory: &'a MemoryBudget,
    pub(crate) p: &'a Processing<'p>,
    pub(crate) first: Option<ValueTreeError>,
}
impl Write for Text<'_, '_> {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        let result = (|| {
            spend(self.p, bytes.len(), &mut || false)?;
            let end = self
                .bytes
                .len()
                .checked_add(bytes.len())
                .ok_or_else(limit)?;
            if end > self.bytes.capacity() {
                let target = end.checked_next_power_of_two().ok_or_else(limit)?;
                spend(self.p, target, &mut || false)?;
                let mut fee = self.memory.reserve(target, MemoryClass::Ordinary)?;
                self.bytes
                    .try_reserve_exact(target - self.bytes.len())
                    .map_err(|_| limit())?;
                if self.bytes.capacity() > target {
                    return Err(limit());
                }
                self.fee.merge(&mut fee).map_err(ValueTreeError::Invalid)?;
            }
            self.bytes.extend_from_slice(bytes);
            Ok(bytes.len())
        })();
        result.map_err(|error| {
            self.first = Some(error);
            io::ErrorKind::Other.into()
        })
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
