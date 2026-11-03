//! Shared bounded cursor for original native verification. Callers retain the
//! source pin and descriptor admission for its whole lifetime; construction
//! also needs admission for temporary clone/validation scratch.
use super::*;

pub const MAX_NATIVE_VERIFY_CHUNKS_PER_STEP: usize = 8;

#[derive(Debug)]
pub enum NativeVerifyError<E> {
    ChunkIntegrity,
    ComponentIntegrity,
    Source(E),
}
pub struct NativeVerificationCursor {
    descriptor: NativeExecutionEnvironmentV1,
    component: usize,
    chunk: usize,
    hasher: blake3::Hasher,
    verified: u64,
}
impl NativeVerificationCursor {
    pub fn new(descriptor: NativeExecutionEnvironmentV1) -> Result<Self, String> {
        descriptor.validate()?;
        Ok(Self {
            descriptor,
            component: 0,
            chunk: 0,
            hasher: blake3::Hasher::new(),
            verified: 0,
        })
    }
    pub fn complete(&self) -> bool {
        self.component == self.descriptor.components.len()
    }
    pub fn verified_chunks(&self) -> u64 {
        self.verified
    }
    pub fn advance<B: AsRef<[u8]>, E>(
        &mut self,
        read: impl FnMut(&str) -> Result<B, E>,
    ) -> Result<(), NativeVerifyError<E>> {
        self.advance_with_visit(read, |_, _| Ok(()))
    }
    /// Visitor effects remain provisional until the whole cursor is complete.
    pub fn advance_with_visit<B: AsRef<[u8]>, E>(
        &mut self,
        mut read: impl FnMut(&str) -> Result<B, E>,
        mut visit: impl FnMut(&str, &[u8]) -> Result<(), E>,
    ) -> Result<(), NativeVerifyError<E>> {
        for _ in 0..MAX_NATIVE_VERIFY_CHUNKS_PER_STEP {
            if self.complete() {
                break;
            }
            let component = &self.descriptor.components[self.component];
            let chunk = &component.chunks[self.chunk];
            let carrier = read(&chunk.digest).map_err(NativeVerifyError::Source)?;
            let bytes = carrier.as_ref();
            if bytes.len() != chunk.byte_len as usize
                || blake3::hash(bytes).to_hex().as_str() != chunk.digest
            {
                return Err(NativeVerifyError::ChunkIntegrity);
            }
            visit(&chunk.digest, bytes).map_err(NativeVerifyError::Source)?;
            let mut next = self.hasher.clone();
            next.update(bytes);
            if self.chunk + 1 == component.chunks.len()
                && next.finalize().to_hex().as_str() != component.content_digest
            {
                return Err(NativeVerifyError::ComponentIntegrity);
            }
            self.hasher = next;
            self.chunk += 1;
            self.verified += 1;
            if self.chunk == component.chunks.len() {
                self.component += 1;
                self.chunk = 0;
                self.hasher = blake3::Hasher::new();
            }
        }
        Ok(())
    }
}

#[cfg(test)]
#[path = "native_environment_verification_tests.rs"]
mod tests;
