//! External native bytes are not embedded in the bounded ClosedRecord.
use super::*;
pub use oasis7_formal_protocol::native_environment::*;
use serde::{Deserialize, Serialize};
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NativeEnvironmentChunkBytesV1 {
    pub digest: String,
    #[serde(
        serialize_with = "super::endpoint::chunk::encode_bytes",
        deserialize_with = "super::endpoint::chunk::decode_bytes"
    )]
    pub bytes: Vec<u8>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NativeEnvironmentVerificationProgressV1 {
    pub complete: bool,
    pub verified_chunks: u64,
    pub descriptor_digest: String,
    pub endpoint_instance: fencing::EndpointInstanceBindingV1,
}
pub(crate) struct VerificationJob {
    pub descriptor: NativeExecutionEnvironmentV1,
    pub component: usize,
    pub chunk: usize,
    pub hasher: blake3::Hasher,
}
impl VerificationJob {
    pub fn new(descriptor: NativeExecutionEnvironmentV1) -> Self {
        Self {
            descriptor,
            component: 0,
            chunk: 0,
            hasher: blake3::Hasher::new(),
        }
    }
    pub fn complete(&self) -> bool {
        self.component == self.descriptor.components.len()
    }
    pub fn advance(
        &mut self,
        mut read: impl FnMut(&str) -> Result<Vec<u8>, FormalError>,
    ) -> Result<(), FormalError> {
        self.advance_with_visit(&mut read, |_, _| Ok(()))
    }
    pub fn advance_with_visit(
        &mut self,
        mut read: impl FnMut(&str) -> Result<Vec<u8>, FormalError>,
        mut visit: impl FnMut(&str, &[u8]) -> Result<(), FormalError>,
    ) -> Result<(), FormalError> {
        let mut budget = 8usize;
        while !self.complete() && budget > 0 {
            let component = &self.descriptor.components[self.component];
            let chunk = &component.chunks[self.chunk];
            let bytes = read(&chunk.digest)?;
            if bytes.len() != chunk.byte_len as usize
                || blake3::hash(&bytes).to_hex().as_str() != chunk.digest
            {
                return Err(crypto::invalid("native durable chunk length/hash"));
            }
            visit(&chunk.digest, &bytes)?;
            let mut next_hasher = self.hasher.clone();
            next_hasher.update(&bytes);
            if self.chunk + 1 == component.chunks.len()
                && next_hasher.finalize().to_hex().as_str() != component.content_digest
            {
                return Err(crypto::invalid("native durable component hash"));
            }
            self.hasher = next_hasher;
            self.chunk += 1;
            budget -= 1;
            if self.chunk == component.chunks.len() {
                self.component += 1;
                self.chunk = 0;
                self.hasher = blake3::Hasher::new();
            }
        }
        Ok(())
    }
}

/// Validate the signed external closure without allocating the complete native image.
/// The caller supplies local durable reads and records each exact chunk as a root.
pub(crate) fn verify_material_chunks(
    material: &crate::controlled_authority::replicated_protocol::ClosedRecord,
    mut read: impl FnMut(&str) -> Result<Vec<u8>, FormalError>,
    mut visit: impl FnMut(&str, &[u8]) -> Result<(), FormalError>,
) -> Result<(), FormalError> {
    if let Some(descriptor) = material
        .native_environment()
        .map_err(|e| crypto::invalid(e.to_string()))?
    {
        descriptor
            .verify(|id| {
                let bytes = read(id).map_err(|e| e.to_string())?;
                visit(id, &bytes).map_err(|e| e.to_string())?;
                Ok(bytes)
            })
            .map_err(crypto::invalid)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn descriptor(bytes: &[u8]) -> NativeExecutionEnvironmentV1 {
        let digest = blake3::hash(bytes).to_hex().to_string();
        NativeExecutionEnvironmentV1 {
            schema_version: 1,
            target: "test-linux".into(),
            os: "linux".into(),
            arch: "test".into(),
            components: [
                NativeEnvironmentRole::Executable,
                NativeEnvironmentRole::BuildMetadata,
            ]
            .into_iter()
            .enumerate()
            .map(|(index, role)| NativeEnvironmentComponentV1 {
                role,
                name: format!("component-{index}"),
                byte_len: bytes.len() as u64,
                content_digest: digest.clone(),
                chunks: vec![NativeEnvironmentChunkV1 {
                    digest: digest.clone(),
                    byte_len: bytes.len() as u32,
                }],
            })
            .collect(),
        }
    }
    #[test]
    fn native_verification_reads_exact_components_and_failed_retry_is_safe() {
        let bytes = b"original native bytes";
        let mut job = VerificationJob::new(descriptor(bytes));
        let mut visited = 0;
        job.advance_with_visit(
            |_| Ok(bytes.to_vec()),
            |_, _| {
                visited += 1;
                Ok(())
            },
        )
        .unwrap();
        assert!(job.complete());
        assert_eq!(visited, 2);
        let mut wrong = descriptor(bytes);
        wrong.components[0].content_digest = "00".repeat(32);
        let mut failed = VerificationJob::new(wrong);
        for _ in 0..2 {
            assert!(failed.advance(|_| Ok(bytes.to_vec())).is_err());
        }
        assert_eq!(failed.chunk, 0);
        assert!(!failed.complete());
    }
}

/// Endpoint evidence is valid only while this live representation lease survives.
pub(crate) struct EndpointVerificationJob {
    pub job: VerificationJob,
    pub _pin: std::fs::File,
}

#[cfg(test)]
mod chunk_wire_tests {
    use super::*;
    #[test]
    fn native_chunk_wire_is_bounded_byte_string_and_rejects_hostile_lengths() {
        let object = NativeEnvironmentChunkBytesV1 {
            digest: "aa".repeat(32),
            bytes: vec![0xff; MAX_NATIVE_CHUNK_BYTES],
        };
        let encoded = serde_cbor::to_vec(&object).unwrap();
        assert!(encoded.len() < MAX_NATIVE_CHUNK_BYTES + 128);
        let decoded: NativeEnvironmentChunkBytesV1 = serde_cbor::from_slice(&encoded).unwrap();
        assert_eq!(decoded.bytes, object.bytes);
        let oversized = NativeEnvironmentChunkBytesV1 {
            digest: object.digest,
            bytes: vec![0; MAX_NATIVE_CHUNK_BYTES + 1],
        };
        assert!(
            serde_cbor::from_slice::<NativeEnvironmentChunkBytesV1>(
                &serde_cbor::to_vec(&oversized).unwrap()
            )
            .is_err()
        );
        // A huge declared byte string without its payload cannot reserve its claim.
        let mut hostile = vec![0xa2, 0x66];
        hostile.extend_from_slice(b"digest");
        hostile.push(0x60);
        hostile.push(0x65);
        hostile.extend_from_slice(b"bytes");
        hostile.extend_from_slice(&[0x5b, 0x7f, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff]);
        assert!(serde_cbor::from_slice::<NativeEnvironmentChunkBytesV1>(&hostile).is_err());
    }
}
