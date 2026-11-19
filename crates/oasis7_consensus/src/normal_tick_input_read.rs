//! Original Node wire decoding only. This owner is not a verified Node decision.
use super::*;
use oasis7_formal_protocol::controlled_authority::formal::{
    CheckedIncrementalFrozenMaterialV1, FormalError, bounded_context::FormalBoundedContextV1,
};
use oasis7_proto::memory_budget::{MemoryClass, ReservedValue};
use std::io::{self, Read};

struct PaidReader<'a, 'c> {
    bytes: &'a [u8],
    ctx: &'a mut FormalBoundedContextV1<'c>,
    error: Option<FormalError>,
}
impl Read for PaidReader<'_, '_> {
    fn read(&mut self, out: &mut [u8]) -> io::Result<usize> {
        let n = out.len().min(self.bytes.len()).min(8192);
        if let Err(error) = self.ctx.charge(1, n as u64) {
            self.error = Some(error);
            return Err(io::Error::other("normal input bounded read"));
        }
        out[..n].copy_from_slice(&self.bytes[..n]);
        self.bytes = &self.bytes[n..];
        Ok(n)
    }
}
fn decode<T: serde::de::DeserializeOwned>(
    bytes: &[u8],
    ctx: &mut FormalBoundedContextV1<'_>,
) -> Result<T, FormalError> {
    let mut reader = PaidReader {
        bytes,
        ctx,
        error: None,
    };
    let result = serde_cbor::from_reader(&mut reader);
    if let Some(error) = reader.error {
        return Err(error);
    }
    result.map_err(|e| FormalError::Invalid(format!("normal input original: {e}")))
}
pub struct PaidNormalContextOriginalsV1 {
    values: ReservedValue<(NodeExecutionCommitContext, NodeLocalExecutionContinuation)>,
}
impl PaidNormalContextOriginalsV1 {
    pub fn context(&self) -> &NodeExecutionCommitContext {
        &self.values.0
    }
    pub fn continuation(&self) -> &NodeLocalExecutionContinuation {
        &self.values.1
    }
    pub fn uses_budget(&self, memory: &oasis7_proto::memory_budget::MemoryBudget) -> bool {
        self.values.uses_budget(memory)
    }
}
pub fn read_normal_context_originals_bounded(
    checked: &CheckedIncrementalFrozenMaterialV1<'_>,
    ctx: &mut FormalBoundedContextV1<'_>,
) -> Result<PaidNormalContextOriginalsV1, FormalError> {
    let invalid = || FormalError::Invalid("normal input original layout/hash".into());
    let refs = [checked.input(), checked.continuation()];
    let mut originals: [&[u8]; 2] = [&[], &[]];
    for (index, reference) in refs.into_iter().enumerate() {
        let mut found = None;
        for object in &checked.material().objects {
            ctx.charge(1, object.content_hash.len() as u64)?;
            let mut id = [0; 32];
            if hex::decode_to_slice(&object.content_hash, &mut id).is_ok() && id == reference.id {
                found = Some(object.bytes.as_slice());
                break;
            }
        }
        let bytes = found.ok_or_else(invalid)?;
        if bytes.len() as u64 != reference.byte_len {
            return Err(invalid());
        }
        let mut hash = blake3::Hasher::new();
        for chunk in bytes.chunks(8192) {
            ctx.charge(1, chunk.len() as u64)?;
            hash.update(chunk);
        }
        if hash.finalize().as_bytes() != &reference.id {
            return Err(invalid());
        }
        originals[index] = bytes;
    }
    let n = originals[0]
        .len()
        .checked_add(originals[1].len())
        .ok_or_else(invalid)?;
    // Locked serde cautious Vec hints <=1MiB per active sequence. Both DTOs'
    // retained action arrays/payloads/Strings, IoRead scratch and error bridge
    // coexist; this conservative phase is retained with the decoded originals.
    let bytes = n
        .checked_mul(2 * std::mem::size_of::<NodeConsensusAction>() + 32)
        .and_then(|n| n.checked_add(4 << 20))
        .ok_or_else(invalid)?;
    let fee = ctx
        .memory()
        .reserve(bytes, MemoryClass::Ordinary)
        .map_err(|_| invalid())?;
    let context = decode(originals[0], ctx)?;
    let continuation = decode(originals[1], ctx)?;
    ctx.consume_steps(1)?;
    Ok(PaidNormalContextOriginalsV1 {
        values: fee.retain((context, continuation)),
    })
}
