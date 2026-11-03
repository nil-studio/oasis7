//! Provisional generic field/group existence; no member count or authority.
pub use crate::incremental_field_member::MemberRecordError as PresenceRecordError;
use crate::{
    incremental_projection::FieldDescriptor,
    incremental_semantic_key::{
        SemanticKeyBudget, SemanticKeyV1, hash_semantic_key_v1, write_semantic_key_v1,
    },
    incremental_world_field_key::{WorldFieldKeyV1, WorldFieldRecordKindV1},
};
use oasis7_formal_protocol::incremental_value::{
    ValueIoBudget, ValueNodeWriter, ValueStreamWriter, ValueTreeError,
};
use oasis7_proto::memory_budget::{MemoryBudget, MemoryClass};
use std::io::Write;

pub const FIELD_PRESENCE_DOMAIN: &[u8] = b"oasis7.iws.field-presence.v1\0";
pub const FIELD_PRESENCE_HEADER_BYTES: usize = FIELD_PRESENCE_DOMAIN.len() + 10;

#[derive(Debug, Clone, Copy)]
pub struct PresenceRecordLimitsV1 {
    pub semantic_bytes: u64,
    pub record_bytes: u64,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PresenceRecordInfoV1 {
    pub kind: WorldFieldRecordKindV1,
    pub semantic_bytes: u64,
}
impl PresenceRecordInfoV1 {
    fn total(self, field_bytes: usize) -> Result<u64, ValueTreeError> {
        match self.kind {
            WorldFieldRecordKindV1::Metadata if self.semantic_bytes == 0 => {}
            WorldFieldRecordKindV1::Group if self.semantic_bytes != 0 => {}
            _ => return Err(ValueTreeError::Invalid("presence kind/key length")),
        }
        (FIELD_PRESENCE_HEADER_BYTES as u64)
            .checked_add(field_bytes as u64)
            .and_then(|n| n.checked_add(self.semantic_bytes))
            .ok_or(ValueTreeError::Invalid("presence length overflow"))
    }
    fn admit(self, total: u64, limits: PresenceRecordLimitsV1) -> Result<(), ValueTreeError> {
        if self.semantic_bytes > limits.semantic_bytes || total > limits.record_bytes {
            return Err(ValueTreeError::ResourceLimited("presence byte limits"));
        }
        Ok(())
    }
}
pub struct PresenceRecordV1 {
    pub field_key: WorldFieldKeyV1,
    pub value_root: [u8; 32],
    pub info: PresenceRecordInfoV1,
}
pub struct PresenceRecordWriteContext<'a, W, C> {
    pub sink: &'a mut W,
    pub memory: &'a MemoryBudget,
    pub io: &'a mut ValueIoBudget,
    pub key_work: &'a mut SemanticKeyBudget,
    pub limits: PresenceRecordLimitsV1,
    pub cancelled: C,
}
/// A metadata marker has no semantic key and never serializes a sentinel key.
pub fn write_field_metadata_v1<W: ValueNodeWriter, C: FnMut() -> bool>(
    field: FieldDescriptor,
    context: &mut PresenceRecordWriteContext<'_, W, C>,
) -> Result<PresenceRecordV1, PresenceRecordError> {
    write_presence::<(), _, _>(field, None, context)
}
/// Preserves the original outer key, including when its group has no members.
pub fn write_field_group_v1<K: SemanticKeyV1 + ?Sized, W: ValueNodeWriter, C: FnMut() -> bool>(
    field: FieldDescriptor,
    key: &K,
    context: &mut PresenceRecordWriteContext<'_, W, C>,
) -> Result<PresenceRecordV1, PresenceRecordError> {
    write_presence(field, Some(key), context)
}
fn write_presence<K: SemanticKeyV1 + ?Sized, W: ValueNodeWriter, C: FnMut() -> bool>(
    field: FieldDescriptor,
    key: Option<&K>,
    context: &mut PresenceRecordWriteContext<'_, W, C>,
) -> Result<PresenceRecordV1, PresenceRecordError> {
    if (context.cancelled)() {
        return Err(ValueTreeError::Cancelled.into());
    }
    let _metadata = context
        .memory
        .reserve(4096, MemoryClass::Ordinary)
        .map_err(ValueTreeError::Memory)?;
    let kind = if key.is_some() {
        WorldFieldRecordKindV1::Group
    } else {
        WorldFieldRecordKindV1::Metadata
    };
    // Reject non-state descriptors and illegal group shapes before hashing.
    WorldFieldKeyV1::new(field, kind, key.map(|_| [0; 32]))
        .map_err(PresenceRecordError::Invalid)?;
    let semantic = key
        .map(|key| {
            hash_semantic_key_v1(
                key,
                context.memory,
                MemoryClass::Ordinary,
                context.key_work,
                &mut context.cancelled,
            )
        })
        .transpose()
        .map_err(PresenceRecordError::Key)?;
    let field_key = WorldFieldKeyV1::new(field, kind, semantic.as_ref().map(|s| s.digest))
        .map_err(PresenceRecordError::Invalid)?;
    let info = PresenceRecordInfoV1 {
        kind,
        semantic_bytes: semantic.as_ref().map_or(0, |s| s.encoded_bytes),
    };
    let total = info.total(field_key.as_bytes().len())?;
    info.admit(total, context.limits)?;
    let mut header = [0; FIELD_PRESENCE_HEADER_BYTES];
    let p = FIELD_PRESENCE_DOMAIN.len();
    header[..p].copy_from_slice(FIELD_PRESENCE_DOMAIN);
    header[p..p + 2].copy_from_slice(&(field_key.as_bytes().len() as u16).to_be_bytes());
    header[p + 2..].copy_from_slice(&info.semantic_bytes.to_be_bytes());
    let mut writer = ValueStreamWriter::new(
        context.sink,
        context.memory,
        context.io,
        total,
        &mut context.cancelled,
    )?;
    let writing = (|| {
        writer.write_all(&header)?;
        writer.write_all(field_key.as_bytes())?;
        if let Some(key) = key {
            write_semantic_key_v1(key, &mut writer)?;
        }
        Ok::<(), std::io::Error>(())
    })();
    let root = writer.finish()?;
    writing.map_err(PresenceRecordError::Io)?;
    if root.0.logical_len != total {
        return Err(PresenceRecordError::Invalid("presence written length"));
    }
    Ok(PresenceRecordV1 {
        field_key,
        value_root: root.digest()?,
        info,
    })
}

#[path = "incremental_field_presence_read.rs"]
mod read;
pub use read::read_field_presence_v1;

#[cfg(test)]
#[path = "incremental_field_presence_tests.rs"]
mod tests;
