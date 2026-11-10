//! Exact legacy Frozen material digest with finite wire work and Ordinary admission.
//! A digest is content identity only; it grants no Frozen or execution authority.
use super::super::{FormalError, crypto::invalid};
use oasis7_proto::memory_budget::{MemoryBudget, MemoryClass};
use std::io::{self, Write};
const DOMAIN: &[u8] = b"oasis7.formal.frozen_material.v1";
struct Sink<'a, C> {
    hash: blake3::Hasher,
    encoded: u64,
    left: &'a mut u64,
    cancelled: C,
    failed: bool,
}
impl<C: FnMut() -> bool> Write for Sink<'_, C> {
    fn write(&mut self, input: &[u8]) -> io::Result<usize> {
        if self.failed {
            return Err(io::Error::other("Frozen hash poisoned"));
        }
        self.failed = true;
        if (self.cancelled)() {
            return Err(io::Error::other("Frozen hash cancelled"));
        }
        let input = &input[..input.len().min(64 << 10)];
        let next = self
            .encoded
            .checked_add(input.len() as u64)
            .filter(|n| *n <= 64 << 20)
            .ok_or_else(|| io::Error::other("Frozen canonical frame bound"))?;
        *self.left = self
            .left
            .checked_sub(input.len() as u64)
            .ok_or_else(|| io::Error::other("Frozen wire allowance"))?;
        self.hash.update(input);
        self.encoded = next;
        self.failed = false;
        Ok(input.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        if self.failed {
            Err(io::Error::other("Frozen hash poisoned"))
        } else {
            Ok(())
        }
    }
}
/// Hash the exact existing domain and CBOR wire without an encoded material copy.
/// `left` is one continuing wire-byte allowance; consumed work is not refunded
/// on failure. This does not validate roles, original dependencies, authority,
/// accepted parent, store continuity, or execution semantics.
pub fn frozen_material_digest_bounded_v1<C: FnMut() -> bool>(
    material: &crate::controlled_authority::replicated_protocol::ClosedRecord,
    memory: &MemoryBudget,
    left: &mut u64,
    cancelled: C,
) -> Result<[u8; 32], FormalError> {
    let _phase = memory
        .reserve(4096, MemoryClass::Ordinary)
        .map_err(|_| invalid("Frozen hash admission"))?;
    let mut sink = Sink {
        hash: blake3::Hasher::new(),
        encoded: 0,
        left,
        cancelled,
        failed: false,
    };
    sink.hash.update(&(DOMAIN.len() as u64).to_be_bytes());
    sink.hash.update(DOMAIN);
    serde_cbor::to_writer(&mut sink, material)
        .map_err(|_| invalid("Frozen actual material stream"))?;
    if sink.failed || (sink.cancelled)() {
        return Err(invalid("Frozen hash stopped"));
    }
    Ok(*sink.hash.finalize().as_bytes())
}
#[cfg(test)]
mod tests {
    use super::super::super::frozen_material_digest;
    use super::*;
    use crate::controlled_authority::replicated_protocol::{
        ArtifactObject, ArtifactRole, ClosedRecord,
    };
    #[test]
    fn streamed_frozen_material_digest_preserves_original_literal_domain_and_wire() {
        let mut roots = std::collections::BTreeMap::new();
        let mut objects = std::collections::BTreeMap::new();
        for role in [
            ArtifactRole::Input,
            ArtifactRole::Result,
            ArtifactRole::Snapshot,
            ArtifactRole::Journal,
            ArtifactRole::NonceIndex,
            ArtifactRole::EffectOutbox,
            ArtifactRole::ExecutionManifest,
            ArtifactRole::Wasm,
            ArtifactRole::Rules,
        ] {
            let bytes = format!("actual-{role:?}").into_bytes();
            let hash = blake3::hash(&bytes).to_hex().to_string();
            roots.insert(role, hash.clone());
            objects.insert(
                hash.clone(),
                ArtifactObject {
                    content_hash: hash,
                    bytes,
                    references: vec![],
                },
            );
        }
        let material = ClosedRecord {
            payload_digest: roots[&ArtifactRole::Input].clone(),
            before_state_root: roots[&ArtifactRole::Snapshot].clone(),
            after_state_root: roots[&ArtifactRole::Snapshot].clone(),
            roots,
            objects: objects.into_values().collect(),
        };
        let oracle = frozen_material_digest(&material).unwrap();
        let memory = MemoryBudget::new(1 << 20, 1 << 19).unwrap();
        let mut left = 1 << 20;
        assert_eq!(
            hex::encode(
                frozen_material_digest_bounded_v1(&material, &memory, &mut left, || false).unwrap()
            ),
            oracle
        );
        let consumed = (1 << 20) - left;
        assert!(consumed > 0);
        let mut exact = consumed;
        assert_eq!(
            hex::encode(
                frozen_material_digest_bounded_v1(&material, &memory, &mut exact, || false)
                    .unwrap()
            ),
            oracle
        );
        assert_eq!(exact, 0);
        let mut short = consumed - 1;
        assert!(
            frozen_material_digest_bounded_v1(&material, &memory, &mut short, || false).is_err()
        );
        assert!(short < consumed - 1);
        assert_eq!(memory.used(), 0);
        assert!(frozen_material_digest_bounded_v1(&material, &memory, &mut 0, || false).is_err());
        assert!(
            frozen_material_digest_bounded_v1(&material, &memory, &mut (1 << 20), || true).is_err()
        );
        assert_eq!(memory.used(), 0);
    }
}
