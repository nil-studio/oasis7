//! Original serde_json DomainEvent bytes and original SHA-256, streamed.
use crate::world::incremental_body::BodyMutationError;
use oasis7_formal_protocol::{
    controlled_authority::formal::bounded_context::FormalBoundedContextV1,
    incremental_value::ValueTreeError,
};
use oasis7_proto::memory_budget::{MemoryClass, ReservedValue};
use sha2::{Digest, Sha256};
use std::{
    io::{self, Write},
    mem::size_of,
};
struct HashWriter<'a, 'b> {
    hash: Sha256,
    common: &'a mut FormalBoundedContextV1<'b>,
    first: Option<BodyMutationError>,
}
impl Write for HashWriter<'_, '_> {
    fn write(&mut self, b: &[u8]) -> io::Result<usize> {
        if let Err(e) = self.common.charge(1, b.len() as u64) {
            self.first = Some(BodyMutationError::Original(Box::new(e)));
            return Err(io::Error::other("Alliance event JSON work"));
        }
        self.hash.update(b);
        Ok(b.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
pub(super) fn membership_event_hash_v1(
    event: &crate::DomainEvent,
    common: &mut FormalBoundedContextV1<'_>,
) -> Result<ReservedValue<String>, BodyMutationError> {
    let fee = common
        .memory()
        .reserve(
            64 + size_of::<String>() + size_of::<HashWriter<'_, '_>>() + 32,
            MemoryClass::Ordinary,
        )
        .map_err(ValueTreeError::Memory)?;
    let mut writer = HashWriter {
        hash: Sha256::new(),
        common,
        first: None,
    };
    let result = serde_json::to_writer(&mut writer, event);
    if let Some(e) = writer.first {
        return Err(e);
    }
    result.map_err(|e| BodyMutationError::Original(Box::new(e)))?;
    writer
        .common
        .charge(1, 32)
        .map_err(|e| BodyMutationError::Original(Box::new(e)))?;
    let hash = writer.hash.finalize();
    let text = hex::encode(hash);
    if text.capacity() > 64 {
        return Err(ValueTreeError::ResourceLimited("Alliance event hash capacity").into());
    }
    Ok(fee.retain(text))
}
