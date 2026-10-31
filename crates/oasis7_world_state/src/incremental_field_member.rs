//! Provisional generic-field members retaining their original semantic key.
//! Raw reads authenticate ranges, not typed fields, business identity or World.
pub use crate::incremental_projection::{
    FieldDescriptor, FieldDestination, FieldOwner, FieldShape,
};
pub use crate::incremental_semantic_key::SemanticKeyBudget;
pub use crate::incremental_world_field_key::{WorldFieldKeyV1, WorldFieldRecordKindV1};
use crate::{
    MaterialLedgerId,
    incremental_ledger_value::{ledger_value_len_v1, write_ledger_value_v1},
    incremental_semantic_key::{
        SemanticKeyError, SemanticKeyV1, hash_semantic_key_v1, write_semantic_key_v1,
    },
};
use oasis7_formal_protocol::incremental_value::{
    ValueIoBudget, ValueNodeWriter, ValueStreamWriter, ValueTreeError,
};
use oasis7_proto::memory_budget::{MemoryBudget, MemoryClass};
use std::io::{self, Write};

pub const FIELD_MEMBER_DOMAIN: &[u8] = b"oasis7.iws.field-member.v1\0";
pub const FIELD_MEMBER_HEADER_BYTES: usize = FIELD_MEMBER_DOMAIN.len() + 19;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum MemberPayloadCodecV1 {
    Unit = 1,
    U64 = 2,
    I64 = 3,
    Bool = 4,
    Text = 5,
    Ledger = 6,
    Bytes = 7,
}
impl MemberPayloadCodecV1 {
    fn parse(tag: u8) -> Result<Self, &'static str> {
        Ok(match tag {
            1 => Self::Unit,
            2 => Self::U64,
            3 => Self::I64,
            4 => Self::Bool,
            5 => Self::Text,
            6 => Self::Ledger,
            7 => Self::Bytes,
            _ => return Err("member payload codec"),
        })
    }
    fn framing(self, kind: WorldFieldRecordKindV1, length: u64) -> Result<(), &'static str> {
        if !(matches!((kind, self), (WorldFieldRecordKindV1::Bytes, Self::Bytes))
            || kind == WorldFieldRecordKindV1::Entry && self != Self::Bytes)
        {
            return Err("member key/payload kind");
        }
        let fixed = match self {
            Self::Unit => Some(0),
            Self::U64 | Self::I64 => Some(8),
            Self::Bool => Some(1),
            _ => None,
        };
        if fixed.is_some_and(|n| n != length) {
            return Err("member fixed payload length");
        }
        Ok(())
    }
}
#[derive(Clone, Copy)]
pub enum MemberPayloadRefV1<'a> {
    Unit,
    U64(u64),
    I64(i64),
    Bool(bool),
    Text(&'a str),
    Ledger(&'a MaterialLedgerId),
    Bytes(&'a [u8]),
}
impl MemberPayloadRefV1<'_> {
    fn profile(self) -> Result<(MemberPayloadCodecV1, u64), &'static str> {
        let length = |n| u64::try_from(n).map_err(|_| "member payload length");
        Ok(match self {
            Self::Unit => (MemberPayloadCodecV1::Unit, 0),
            Self::U64(_) => (MemberPayloadCodecV1::U64, 8),
            Self::I64(_) => (MemberPayloadCodecV1::I64, 8),
            Self::Bool(_) => (MemberPayloadCodecV1::Bool, 1),
            Self::Text(s) => (MemberPayloadCodecV1::Text, length(s.len())?),
            Self::Ledger(v) => (MemberPayloadCodecV1::Ledger, ledger_value_len_v1(v)?),
            Self::Bytes(b) => (MemberPayloadCodecV1::Bytes, length(b.len())?),
        })
    }
    fn write(self, output: &mut impl Write) -> io::Result<()> {
        match self {
            Self::Unit => Ok(()),
            Self::U64(v) => output.write_all(&v.to_be_bytes()),
            Self::I64(v) => output.write_all(&v.to_be_bytes()),
            Self::Bool(v) => output.write_all(&[u8::from(v)]),
            Self::Text(s) => output.write_all(s.as_bytes()),
            Self::Ledger(v) => write_ledger_value_v1(v, output).map(|_| ()),
            Self::Bytes(b) => output.write_all(b),
        }
    }
}
#[derive(Debug, Clone, Copy)]
pub struct MemberRecordLimitsV1 {
    pub semantic_bytes: u64,
    pub payload_bytes: u64,
    pub record_bytes: u64,
}
/// Expected declared profile; payload semantics remain the owner's job.
#[derive(Debug, Clone, Copy)]
pub struct MemberReadProfileV1 {
    pub codec: MemberPayloadCodecV1,
    pub semantic_bytes: u64,
    pub limits: MemberRecordLimitsV1,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MemberRecordInfoV1 {
    pub codec: MemberPayloadCodecV1,
    pub semantic_bytes: u64,
    pub payload_bytes: u64,
}
impl MemberRecordInfoV1 {
    fn total(self, field_bytes: usize) -> Result<u64, ValueTreeError> {
        (FIELD_MEMBER_HEADER_BYTES as u64)
            .checked_add(field_bytes as u64)
            .and_then(|n| n.checked_add(self.semantic_bytes))
            .and_then(|n| n.checked_add(self.payload_bytes))
            .ok_or(ValueTreeError::Invalid("member record length overflow"))
    }
    fn admit(self, total: u64, limits: MemberRecordLimitsV1) -> Result<(), ValueTreeError> {
        if self.semantic_bytes > limits.semantic_bytes
            || self.payload_bytes > limits.payload_bytes
            || total > limits.record_bytes
        {
            return Err(ValueTreeError::ResourceLimited("member record lengths"));
        }
        Ok(())
    }
}
pub struct MemberRecordV1 {
    pub field_key: WorldFieldKeyV1,
    pub value_root: [u8; 32],
    pub info: MemberRecordInfoV1,
}
pub struct MemberRecordWriteContext<'a, W, C> {
    pub sink: &'a mut W,
    pub memory: &'a MemoryBudget,
    pub io: &'a mut ValueIoBudget,
    pub key_work: &'a mut SemanticKeyBudget,
    pub limits: MemberRecordLimitsV1,
    pub cancelled: C,
}
#[derive(Debug)]
pub enum MemberRecordError {
    Key(SemanticKeyError),
    Value(ValueTreeError),
    Invalid(&'static str),
    Io(io::Error),
}
impl std::fmt::Display for MemberRecordError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Key(e) => e.fmt(f),
            Self::Value(e) => e.fmt(f),
            Self::Invalid(e) => f.write_str(e),
            Self::Io(e) => e.fmt(f),
        }
    }
}
impl std::error::Error for MemberRecordError {}
impl From<ValueTreeError> for MemberRecordError {
    fn from(e: ValueTreeError) -> Self {
        Self::Value(e)
    }
}

/// Creates only provisional pages. The caller owns source/output admission,
/// field registration and publication; no complete key or payload is copied.
pub fn write_member_record_v1<K: SemanticKeyV1 + ?Sized, W: ValueNodeWriter, C: FnMut() -> bool>(
    field: FieldDescriptor,
    kind: WorldFieldRecordKindV1,
    key: &K,
    payload: MemberPayloadRefV1<'_>,
    context: &mut MemberRecordWriteContext<'_, W, C>,
) -> Result<MemberRecordV1, MemberRecordError> {
    if (context.cancelled)() {
        return Err(ValueTreeError::Cancelled.into());
    }
    let (codec, payload_bytes) = payload.profile().map_err(MemberRecordError::Invalid)?;
    codec
        .framing(kind, payload_bytes)
        .map_err(MemberRecordError::Invalid)?;
    let _metadata = context
        .memory
        .reserve(4096, MemoryClass::Ordinary)
        .map_err(ValueTreeError::Memory)?;
    let semantic = hash_semantic_key_v1(
        key,
        context.memory,
        MemoryClass::Ordinary,
        context.key_work,
        &mut context.cancelled,
    )
    .map_err(MemberRecordError::Key)?;
    let field_key = WorldFieldKeyV1::new(field, kind, Some(semantic.digest))
        .map_err(MemberRecordError::Invalid)?;
    let info = MemberRecordInfoV1 {
        codec,
        semantic_bytes: semantic.encoded_bytes,
        payload_bytes,
    };
    let total = info.total(field_key.as_bytes().len())?;
    info.admit(total, context.limits)?;
    let mut header = [0; FIELD_MEMBER_HEADER_BYTES];
    let p = FIELD_MEMBER_DOMAIN.len();
    header[..p].copy_from_slice(FIELD_MEMBER_DOMAIN);
    header[p..p + 2].copy_from_slice(&(field_key.as_bytes().len() as u16).to_be_bytes());
    header[p + 2] = codec as u8;
    header[p + 3..p + 11].copy_from_slice(&info.semantic_bytes.to_be_bytes());
    header[p + 11..].copy_from_slice(&info.payload_bytes.to_be_bytes());
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
        write_semantic_key_v1(key, &mut writer)?;
        payload.write(&mut writer)
    })();
    // A writer error retains its original budget/cancel cause in finish.
    let root = writer.finish()?;
    writing.map_err(MemberRecordError::Io)?;
    if root.0.logical_len != total {
        return Err(MemberRecordError::Invalid("member written length"));
    }
    Ok(MemberRecordV1 {
        field_key,
        value_root: root.digest()?,
        info,
    })
}

#[path = "incremental_field_member_read.rs"]
mod read;
pub use read::{read_member_record_profile_v1, read_member_record_v1};

#[cfg(test)]
#[path = "incremental_field_member_tests.rs"]
mod tests;
