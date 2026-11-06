//! Authenticate the actual Frozen material commitment using its original hash
//! domain/framing and exact serde-CBOR wire, without a whole encoded Vec.
use super::*;
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
fn digest<C: FnMut() -> bool>(
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
pub(super) fn verify(
    candidate: &FormalCandidateMaterialV1,
    context: &oasis7_formal_protocol::controlled_authority::formal::VerifiedCandidateContextV1,
    core: &CheckedCommitCore,
    memory: &MemoryBudget,
    left: &mut u64,
    cancelled: &mut impl FnMut() -> bool,
) -> Result<(), FormalError> {
    if cancelled() {
        return Err(invalid("Frozen binding cancelled"));
    }
    let (certificate, material) = match (&candidate.frozen, &candidate.frozen_material) {
        (None, None)
            if context.record().frozen_reservation_id.is_none()
                && core.parent_record_id().is_none()
                && core.roots().height == 0 =>
        {
            return Ok(());
        }
        (Some(c), Some(m)) => (c, m),
        _ => return Err(invalid("Frozen material/certificate presence")),
    };
    let body = &certificate.reservation.body;
    // The genuine context already verified the signatures/threshold. Its
    // record's reservation ID authenticates this exact body, not DTO presence.
    let reservation = body.reservation_id()?;
    let mut native = [0; 64];
    hex::encode_to_slice(core.execution_artifacts().native, &mut native)
        .map_err(|_| invalid("Frozen native fixed ID"))?;
    if material.roots.get(&crate::controlled_authority::replicated_protocol::ArtifactRole::NativeExecutionEnvironment).map(|id|id.as_bytes())!=Some(native.as_slice())
        || context.record().frozen_reservation_id.as_ref()!=Some(&reservation)
        ||material.payload_digest!=body.payload_digest
        ||material.before_state_root!=body.pre_state_root||material.after_state_root!=body.pre_state_root
        ||material.roots.get(&crate::controlled_authority::replicated_protocol::ArtifactRole::Input)!=Some(&body.payload_digest)
        ||material.roots.get(&crate::controlled_authority::replicated_protocol::ArtifactRole::ExecutionManifest)!=Some(&body.runtime_manifest_digest)
        ||material.roots.get(&crate::controlled_authority::replicated_protocol::ArtifactRole::NativeExecutionEnvironment)
            !=candidate.material.roots.get(&crate::controlled_authority::replicated_protocol::ArtifactRole::NativeExecutionEnvironment){
        return Err(invalid("Frozen exact reservation/input/prestate/native binding"));
    }
    let mut expected = [0; 32];
    hex::decode_to_slice(&body.frozen_material_digest, &mut expected)
        .map_err(|_| invalid("Frozen material digest"))?;
    if digest(material, memory, left, &mut *cancelled)? != expected {
        return Err(invalid("Frozen signed actual material digest"));
    }
    if cancelled() {
        return Err(invalid("Frozen binding final cancellation"));
    }
    Ok(())
}
#[cfg(test)]
mod tests {
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
        let oracle =
            oasis7_formal_protocol::controlled_authority::formal::frozen_material_digest(&material)
                .unwrap();
        let memory = MemoryBudget::new(1 << 20, 1 << 19).unwrap();
        let mut left = 1 << 20;
        assert_eq!(
            hex::encode(digest(&material, &memory, &mut left, || false).unwrap()),
            oracle
        );
        assert!(left < 1 << 20);
        assert_eq!(memory.used(), 0);
        assert!(digest(&material, &memory, &mut 0, || false).is_err());
        assert!(digest(&material, &memory, &mut (1 << 20), || true).is_err());
        assert_eq!(memory.used(), 0);
    }
}
