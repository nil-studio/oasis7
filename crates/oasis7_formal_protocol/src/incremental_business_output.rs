//! Typed locator for original business descriptor/delta objects in the existing
//! ClosedRecord graph. It does not alter the nine roles or result-root meaning.
use crate::{
    incremental_artifacts::OriginalArtifactRefV1,
    incremental_execution::ExecutionError,
    incremental_history::wire::{Reader, Writer},
};
use oasis7_proto::memory_budget::{MemoryBudget, MemoryClass, ReservedValue};
pub const BUSINESS_OUTPUT_DOMAIN_V1: &[u8] = b"oasis7.iws.business-output.v1\0";
pub const BUSINESS_OUTPUT_MAX_BYTES: usize = 256;
const MAX_ORIGINAL_BYTES: u64 = 64 * 1024 * 1024;
/// Content DATA only; no accepted-parent, execution or publication authority.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BusinessOutputLocatorV1 {
    pub commit_body: [u8; 32],
    pub descriptor: OriginalArtifactRefV1,
    pub delta: OriginalArtifactRefV1,
}
impl BusinessOutputLocatorV1 {
    pub fn validate(&self) -> Result<(), ExecutionError> {
        for reference in [self.descriptor, self.delta] {
            if reference.byte_len == 0 || reference.byte_len > MAX_ORIGINAL_BYTES {
                return Err(ExecutionError::Invalid(
                    "business output original byte bound",
                ));
            }
        }
        if self.descriptor.id == self.delta.id {
            return Err(ExecutionError::Invalid(
                "business output distinct originals",
            ));
        }
        Ok(())
    }
    pub fn canonical_bytes_with_work(
        &self,
        memory: &MemoryBudget,
        work: &mut dyn FnMut(u64, u64) -> Result<(), ExecutionError>,
    ) -> Result<ReservedValue<Vec<u8>>, ExecutionError> {
        work(1, std::mem::size_of::<Self>() as u64)?;
        self.validate()?;
        // The shared writer's 112-byte payload grows to at most 128 bytes.
        // Its payload and final <=256-byte framed vector coexist. Keep the
        // conservative fixed 1KiB phase through the returned original's Drop.
        let fee = memory
            .reserve(1024, MemoryClass::Ordinary)
            .map_err(ExecutionError::Memory)?;
        let work = std::cell::RefCell::new(work);
        let meter = |s, b| (work.borrow_mut())(s, b);
        let mut writer = Writer::with_work(&meter);
        writer.hash(&self.commit_body);
        for reference in [self.descriptor, self.delta] {
            writer.hash(&reference.id);
            writer.u64(reference.byte_len);
        }
        let bytes = writer.finish(BUSINESS_OUTPUT_DOMAIN_V1, BUSINESS_OUTPUT_MAX_BYTES)?;
        Ok(fee.retain(bytes))
    }
    pub fn from_canonical_bytes_with_work(
        bytes: &[u8],
        work: &mut dyn FnMut(u64, u64) -> Result<(), ExecutionError>,
    ) -> Result<Self, ExecutionError> {
        let processing = (bytes.len() as u64)
            .checked_mul(3)
            .and_then(|n| n.checked_add(std::mem::size_of::<Self>() as u64))
            .ok_or(ExecutionError::Invalid("business output decode work"))?;
        work(8, processing)?;
        let mut reader = Reader::new(bytes, BUSINESS_OUTPUT_DOMAIN_V1, BUSINESS_OUTPUT_MAX_BYTES)?;
        let value = Self {
            commit_body: reader.array()?,
            descriptor: OriginalArtifactRefV1 {
                id: reader.array()?,
                byte_len: reader.u64()?,
            },
            delta: OriginalArtifactRefV1 {
                id: reader.array()?,
                byte_len: reader.u64()?,
            },
        };
        reader.finish()?;
        value.validate()?;
        Ok(value)
    }
}
