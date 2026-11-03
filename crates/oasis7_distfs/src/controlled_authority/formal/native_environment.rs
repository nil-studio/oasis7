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
pub(crate) struct VerificationJob(NativeVerificationCursor);
impl VerificationJob {
    pub fn new(descriptor: NativeExecutionEnvironmentV1) -> Result<Self, FormalError> {
        NativeVerificationCursor::new(descriptor)
            .map(Self)
            .map_err(crypto::invalid)
    }
    pub fn complete(&self) -> bool {
        self.0.complete()
    }
    pub fn verified_chunks(&self) -> u64 {
        self.0.verified_chunks()
    }
    pub fn advance(
        &mut self,
        read: impl FnMut(&str) -> Result<Vec<u8>, FormalError>,
    ) -> Result<(), FormalError> {
        self.advance_with_visit(read, |_, _| Ok(()))
    }
    pub fn advance_with_visit(
        &mut self,
        read: impl FnMut(&str) -> Result<Vec<u8>, FormalError>,
        visit: impl FnMut(&str, &[u8]) -> Result<(), FormalError>,
    ) -> Result<(), FormalError> {
        self.0
            .advance_with_visit(read, visit)
            .map_err(|error| match error {
                NativeVerifyError::Source(error) => error,
                NativeVerifyError::ChunkIntegrity => {
                    crypto::invalid("native durable chunk length/hash")
                }
                NativeVerifyError::ComponentIntegrity => {
                    crypto::invalid("native durable component hash")
                }
            })
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
        let mut job = VerificationJob::new(descriptor(bytes)).unwrap();
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
        let mut failed = VerificationJob::new(wrong).unwrap();
        for _ in 0..2 {
            assert!(failed.advance(|_| Ok(bytes.to_vec())).is_err());
        }
        assert_eq!(failed.verified_chunks(), 0);
        assert!(!failed.complete());
    }
}

/// Endpoint evidence is valid only while this live representation lease survives.
pub(crate) struct EndpointVerificationJob {
    pub job: VerificationJob,
    pub _pin: std::fs::File,
    pub last_activity: std::time::Instant,
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
