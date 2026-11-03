use super::*;
use oasis7_proto::memory_budget::{MemoryBudget, MemoryClass, MemoryLimitExceeded};

#[derive(Debug, Clone)]
pub struct SemanticKeyBudget {
    pub bytes_remaining: u64,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SemanticKeyDigestV1 {
    pub digest: [u8; 32],
    pub encoded_bytes: u64,
}
#[derive(Debug)]
pub enum SemanticKeyError {
    Memory(MemoryLimitExceeded),
    ResourceLimited,
    Cancelled,
    Invalid(&'static str),
}
impl std::fmt::Display for SemanticKeyError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Memory(e) => e.fmt(f),
            Self::ResourceLimited => f.write_str("semantic key byte allowance"),
            Self::Cancelled => f.write_str("semantic key cancelled"),
            Self::Invalid(e) => f.write_str(e),
        }
    }
}
impl std::error::Error for SemanticKeyError {}
struct HashSink<'a, C> {
    hash: blake3::Hasher,
    bytes: u64,
    budget: &'a mut SemanticKeyBudget,
    cancelled: C,
    error: Option<SemanticKeyError>,
}
impl<C: FnMut() -> bool> HashSink<'_, C> {
    fn check(&mut self) -> Result<(), SemanticKeyError> {
        if (self.cancelled)() {
            Err(SemanticKeyError::Cancelled)
        } else {
            Ok(())
        }
    }
    fn consume(&mut self, bytes: &[u8]) -> Result<(), SemanticKeyError> {
        self.check()?;
        let length = bytes.len() as u64;
        if self.budget.bytes_remaining < length {
            return Err(SemanticKeyError::ResourceLimited);
        }
        let next = self
            .bytes
            .checked_add(length)
            .ok_or(SemanticKeyError::ResourceLimited)?;
        // Charge before any chunk; cancellation/unwind never restores work.
        self.budget.bytes_remaining -= length;
        self.bytes = next;
        for chunk in bytes.chunks(64 * 1024) {
            self.check()?;
            self.hash.update(chunk);
        }
        self.check()
    }
    fn failed(&mut self, error: SemanticKeyError) -> io::Error {
        self.error = Some(error);
        io::Error::other("semantic key hash rejected")
    }
}
impl<C: FnMut() -> bool> Write for HashSink<'_, C> {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        if self.error.is_some() {
            return Err(io::Error::other("aborted semantic key hash"));
        }
        match self.consume(bytes) {
            Ok(()) => Ok(bytes.len()),
            Err(error) => Err(self.failed(error)),
        }
    }
    fn flush(&mut self) -> io::Result<()> {
        if self.error.is_some() {
            return Err(io::Error::other("aborted semantic key hash"));
        }
        self.check().map_err(|e| self.failed(e))
    }
}
/// Hash only this immutable source key. Input storage remains the caller's
/// responsibility. A digest is content evidence, never live execution or
/// recovery qualification; source context enforces the applicable class.
pub fn hash_semantic_key_v1<K: SemanticKeyV1 + ?Sized>(
    key: &K,
    memory: &MemoryBudget,
    class: MemoryClass,
    budget: &mut SemanticKeyBudget,
    cancelled: impl FnMut() -> bool,
) -> Result<SemanticKeyDigestV1, SemanticKeyError> {
    let _workspace = memory
        .reserve(4096, class)
        .map_err(SemanticKeyError::Memory)?;
    let mut sink = HashSink {
        hash: blake3::Hasher::new(),
        bytes: 0,
        budget,
        cancelled,
        error: None,
    };
    if write_semantic_key_v1(key, &mut sink).is_err() {
        return Err(sink
            .error
            .take()
            .unwrap_or(SemanticKeyError::Invalid("semantic key source/depth")));
    }
    sink.check()?;
    Ok(SemanticKeyDigestV1 {
        digest: *sink.hash.finalize().as_bytes(),
        encoded_bytes: sink.bytes,
    })
}
