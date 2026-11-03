//! Bounded public metadata chunks; no confidential admission input is encoded.
use super::binding::{BftSignerErrorV3, invalid};
use super::crypto::BFT_SIGNER_MAX_FRAME_BYTES_V3;
use serde::{Deserialize, Serialize};
use std::io::Write;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "digest", deny_unknown_fields)]
pub(crate) enum RetentionProofTargetV1 {
    Operation(String),
    SelectionOperation(String),
    Seal(String),
}
impl RetentionProofTargetV1 {
    pub fn digest(&self) -> &str {
        match self {
            Self::Operation(s) | Self::SelectionOperation(s) | Self::Seal(s) => s,
        }
    }
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct RetentionProofObjectV1 {
    pub digest: String,
    pub bytes: u64,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct RetentionProofReadV1 {
    pub target: RetentionProofTargetV1,
    pub revision: u64,
    pub state_digest: String,
    pub object: Option<RetentionProofObjectV1>,
    pub offset: u64,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct RetentionProofChunkV1 {
    pub read: RetentionProofReadV1,
    pub object: RetentionProofObjectV1,
    pub bytes: Vec<u8>,
    pub reserved_response_bytes: u64,
}
impl RetentionProofReadV1 {
    pub fn validate(&self) -> Result<(), BftSignerErrorV3> {
        super::state::validate_digest(self.target.digest())?;
        super::state::validate_digest(&self.state_digest)?;
        if let Some(object) = &self.object {
            super::state::validate_digest(&object.digest)?;
            if object.bytes == 0
                || object.bytes > BFT_SIGNER_MAX_FRAME_BYTES_V3 as u64
                || self.offset >= object.bytes
            {
                return Err(invalid("retention proof object/range bounds"));
            }
        } else if self.offset != 0 {
            return Err(invalid("retention proof first offset"));
        }
        Ok(())
    }
}
/// Uses the exact CAS canonical codec (sorted Value and self-describe tag).
/// The canonical Value is one existing-codec metadata working copy; output is
/// only one archive-sized chunk, never a second complete encoded proof buffer.
pub(crate) fn canonical_chunk<T: Serialize>(
    value: &T,
    offset: u64,
) -> Result<(RetentionProofObjectV1, Vec<u8>), BftSignerErrorV3> {
    let value = serde_cbor::value::to_value(value).map_err(|e| invalid(e.to_string()))?;
    let mut writer = RangeWriter {
        offset,
        count: 0,
        hasher: blake3::Hasher::new(),
        bytes: Vec::new(),
    };
    let mut serializer = serde_cbor::Serializer::new(serde_cbor::ser::IoWrite::new(&mut writer));
    serializer
        .self_describe()
        .map_err(|e| invalid(e.to_string()))?;
    value
        .serialize(&mut serializer)
        .map_err(|e| invalid(e.to_string()))?;
    if offset >= writer.count {
        return Err(invalid("retention proof offset outside object"));
    }
    Ok((
        RetentionProofObjectV1 {
            digest: writer.hasher.finalize().to_hex().to_string(),
            bytes: writer.count,
        },
        writer.bytes,
    ))
}
struct RangeWriter {
    offset: u64,
    count: u64,
    hasher: blake3::Hasher,
    bytes: Vec<u8>,
}
impl Write for RangeWriter {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        let end = self
            .count
            .checked_add(bytes.len() as u64)
            .filter(|n| *n <= BFT_SIGNER_MAX_FRAME_BYTES_V3 as u64)
            .ok_or_else(|| std::io::Error::other("retention proof exceeds original frame bound"))?;
        let limit = self
            .offset
            .saturating_add(super::archive_query::BFT_SUCCESSOR_ARCHIVE_CHUNK_MAX_BYTES_V1 as u64);
        let start = self.count.max(self.offset);
        let stop = end.min(limit);
        if start < stop {
            let slice = &bytes[(start - self.count) as usize..(stop - self.count) as usize];
            self.bytes
                .try_reserve_exact(slice.len())
                .map_err(std::io::Error::other)?;
            self.bytes.extend_from_slice(slice);
        }
        self.hasher.update(bytes);
        self.count = end;
        Ok(bytes.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}
/// The only self-reference is the cumulative counter's canonical integer width.
/// Width grows monotonically through at most five CBOR integer representations.
pub(crate) fn exact_response_reservation(
    current: u64,
    mut encoded_length: impl FnMut(u64) -> Result<usize, BftSignerErrorV3>,
) -> Result<u64, BftSignerErrorV3> {
    let mut amount = 0u64;
    for _ in 0..10 {
        let counter = current
            .checked_add(amount)
            .ok_or_else(|| invalid("retention proof transfer overflow"))?;
        let next = u64::try_from(encoded_length(counter)?)
            .map_err(|_| invalid("retention proof frame length conversion"))?;
        if next == amount {
            return Ok(amount);
        }
        if next < amount {
            return Err(invalid("retention proof frame length regressed"));
        }
        amount = next;
    }
    Err(invalid("retention proof frame length did not stabilize"))
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn retention_proof_reservation_matches_final_frame_across_cbor_counter_widths() {
        use super::super::wire::{
            BftSignerReplyBodyV3, BftSignerReplyKindV3, SignedBftSignerReplyV3, encode_frame,
        };
        let read = RetentionProofReadV1 {
            target: RetentionProofTargetV1::Operation("11".repeat(32)),
            revision: 1,
            state_digest: "22".repeat(32),
            object: None,
            offset: 0,
        };
        let mut frame = SignedBftSignerReplyV3 {
            body: BftSignerReplyBodyV3 {
                schema_version: 3,
                binding_digest: "33".repeat(32),
                request_digest: "44".repeat(32),
                challenge: "55".repeat(32),
                lease_generation: 1,
                lease_token: None,
                state_revision: 1,
                state_digest: read.state_digest.clone(),
                checkpoint: None,
                result: BftSignerReplyKindV3::PrivateRetentionProofChunk {
                    chunk: Box::new(RetentionProofChunkV1 {
                        read,
                        object: RetentionProofObjectV1 {
                            digest: "66".repeat(32),
                            bytes: 137,
                        },
                        bytes: vec![7; 137],
                        reserved_response_bytes: 0,
                    }),
                },
            },
            provider_signature_hex: "00".repeat(64),
        };
        for boundary in [1u64 << 16, 1u64 << 32] {
            let BftSignerReplyKindV3::PrivateRetentionProofChunk { chunk } = &mut frame.body.result
            else {
                unreachable!()
            };
            chunk.reserved_response_bytes = boundary;
            let boundary_length = encode_frame(&frame).unwrap().len() as u64;
            for current in [
                boundary - boundary_length - 1,
                boundary - boundary_length,
                boundary - boundary_length + 1,
                boundary - 1,
            ] {
                let charge = exact_response_reservation(current, |counter| {
                    let BftSignerReplyKindV3::PrivateRetentionProofChunk { chunk } =
                        &mut frame.body.result
                    else {
                        unreachable!()
                    };
                    chunk.reserved_response_bytes = counter;
                    Ok(encode_frame(&frame)?.len())
                })
                .unwrap();
                let BftSignerReplyKindV3::PrivateRetentionProofChunk { chunk } = &frame.body.result
                else {
                    unreachable!()
                };
                assert_eq!(chunk.reserved_response_bytes, current + charge);
                assert_eq!(
                    charge,
                    encode_frame(&frame).unwrap().len() as u64,
                    "reservation equals the final frame, including its own cumulative counter width"
                );
            }
        }
        assert!(exact_response_reservation(u64::MAX, |_| Ok(1)).is_err());
    }
    #[test]
    fn retention_proof_chunks_match_original_cas_canonical_codec() {
        let value = vec!["metadata".repeat(180_000), "tail".into()];
        let original =
            super::super::crypto::encode_bounded(&value, BFT_SIGNER_MAX_FRAME_BYTES_V3).unwrap();
        let mut assembled = Vec::new();
        while assembled.len() < original.len() {
            let (object, bytes) = canonical_chunk(&value, assembled.len() as u64).unwrap();
            assert_eq!(object.bytes, original.len() as u64);
            assert_eq!(object.digest, blake3::hash(&original).to_hex().to_string());
            assert!(
                bytes.len()
                    <= super::super::archive_query::BFT_SUCCESSOR_ARCHIVE_CHUNK_MAX_BYTES_V1
            );
            assembled.extend(bytes);
        }
        assert_eq!(assembled, original);
    }
}
