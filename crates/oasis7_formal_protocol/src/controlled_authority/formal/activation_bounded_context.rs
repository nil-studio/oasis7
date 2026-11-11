//! Fixed paid context digest. No caller-selected serializer or hash callback.
use super::*;
use crate::controlled_authority::formal::bounded_context::FormalBoundedContextV1;
use oasis7_proto::memory_budget::{MemoryClass, ReservedValue};

fn hash_paid<T: Serialize>(
    domain: &str,
    value: &T,
    ctx: &mut FormalBoundedContextV1<'_>,
) -> Result<ReservedValue<String>, FormalError> {
    let wire = crate::controlled_authority::formal::context_wire::encode_paid(
        value,
        64 * 1024 * 1024,
        ctx,
        "formal encoding exceeds byte bound",
        "verified authority context",
    )?;
    let fee = ctx
        .memory()
        .reserve(64 + std::mem::size_of::<String>(), MemoryClass::Ordinary)
        .map_err(|_| invalid("authority context digest admission"))?;
    let mut hash = blake3::Hasher::new();
    for bytes in [
        &(domain.len() as u64).to_be_bytes()[..],
        domain.as_bytes(),
        wire.as_slice(),
    ] {
        for chunk in bytes.chunks(8192) {
            ctx.charge(1, chunk.len() as u64)?;
            hash.update(chunk);
        }
    }
    ctx.charge(1, 64)?;
    let digest = hash.finalize().to_hex().to_string();
    if digest.capacity() != 64 {
        return Err(invalid("authority context digest capacity"));
    }
    Ok(fee.retain(digest))
}
impl VerifiedAuthorityV1 {
    pub fn context_digest_bounded(
        &self,
        ctx: &mut FormalBoundedContextV1<'_>,
    ) -> Result<ReservedValue<String>, FormalError> {
        let legacy = hash_paid(
            "oasis7.formal.verified_authority_context.v1",
            &self.context_digest_payload(),
            ctx,
        )?;
        match &self.durable_set_activation_digest {
            None => Ok(legacy),
            Some(digest) => hash_paid(
                "oasis7.formal.verified_authority_context.v2",
                &(&*legacy, digest),
                ctx,
            ),
        }
    }
}

#[cfg(test)]
#[path = "activation_bounded_context_tests.rs"]
mod tests;
