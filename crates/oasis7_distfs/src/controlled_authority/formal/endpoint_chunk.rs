//! Bounded private CAS ranges. Range bytes remain untrusted until the complete
//! declared object hashes to the checkpoint-authenticated content identifier.
use super::*;
use serde::{Deserialize, Serialize};
pub const FORMAL_ARCHIVE_OBJECT_LIMIT: u64 = 64 * 1024 * 1024;
pub const FORMAL_ARCHIVE_CHUNK_LIMIT: u32 = 1024 * 1024;
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FormalArchiveChunkV1 {
    pub object_id: String,
    pub total_length: u64,
    pub offset: u64,
    #[serde(serialize_with = "encode_bytes", deserialize_with = "decode_bytes")]
    pub bytes: Vec<u8>,
}
pub(crate) fn encode_bytes<S: serde::Serializer>(
    bytes: &[u8],
    encoder: S,
) -> Result<S::Ok, S::Error> {
    encoder.serialize_bytes(bytes)
}
pub(crate) fn decode_bytes<'de, D: serde::Deserializer<'de>>(
    decoder: D,
) -> Result<Vec<u8>, D::Error> {
    struct Visitor;
    impl<'de> serde::de::Visitor<'de> for Visitor {
        type Value = Vec<u8>;
        fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
            f.write_str("bounded CAS range byte string")
        }
        fn visit_bytes<E: serde::de::Error>(self, bytes: &[u8]) -> Result<Vec<u8>, E> {
            if bytes.len() > FORMAL_ARCHIVE_CHUNK_LIMIT as usize {
                return Err(E::custom("CAS chunk limit"));
            }
            Ok(bytes.to_vec())
        }
        fn visit_byte_buf<E: serde::de::Error>(self, bytes: Vec<u8>) -> Result<Vec<u8>, E> {
            if bytes.len() > FORMAL_ARCHIVE_CHUNK_LIMIT as usize {
                return Err(E::custom("CAS chunk limit"));
            }
            Ok(bytes)
        }
    }
    decoder.deserialize_byte_buf(Visitor)
}
impl FormalArchiveChunkV1 {
    pub(crate) fn from_verified_bytes(
        id: &str,
        bytes: &[u8],
        offset: u64,
        limit: u32,
    ) -> Result<Self, FormalError> {
        hash_hex(id)?;
        if limit == 0
            || limit > FORMAL_ARCHIVE_CHUNK_LIMIT
            || bytes.len() as u64 > FORMAL_ARCHIVE_OBJECT_LIMIT
            || offset > bytes.len() as u64
            || blake3::hash(bytes).to_hex().as_str() != id
        {
            return Err(invalid("formal CAS full object range request/checksum"));
        }
        let start = offset as usize;
        let end = start + std::cmp::min(limit as usize, bytes.len() - start);
        Ok(Self {
            object_id: id.into(),
            total_length: bytes.len() as u64,
            offset,
            bytes: bytes[start..end].to_vec(),
        })
    }
    pub fn validate(&self, id: &str, offset: u64, limit: u32) -> Result<(), FormalError> {
        hash_hex(id)?;
        if limit == 0
            || limit > FORMAL_ARCHIVE_CHUNK_LIMIT
            || offset > FORMAL_ARCHIVE_OBJECT_LIMIT
            || self.object_id != id
            || self.offset != offset
            || self.total_length > FORMAL_ARCHIVE_OBJECT_LIMIT
            || offset > self.total_length
            || self.bytes.len() as u64 != std::cmp::min(limit as u64, self.total_length - offset)
        {
            return Err(invalid("formal CAS range identity/offset/length"));
        }
        Ok(())
    }
}
pub(super) fn file_range(
    path: &Path,
    id: &str,
    offset: u64,
    limit: u32,
) -> Result<Option<FormalArchiveChunkV1>, FormalError> {
    super::super::business_pack::read_range(path, id, offset, limit).map(|value| {
        value.map(|(total_length, bytes)| FormalArchiveChunkV1 {
            object_id: id.into(),
            total_length,
            offset,
            bytes,
        })
    })
}

impl FileFormalEndpoint {
    pub fn archive_chunk(
        &mut self,
        id: &str,
        offset: u64,
        limit: u32,
    ) -> Result<Option<FormalArchiveChunkV1>, FormalError> {
        let _guard = self.guard()?;
        self.store.object_range(id, offset, limit)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn range_frame_is_byte_string_and_rejects_wrong_offset_identity_and_size() {
        let id = blake3::hash(b"abc").to_hex().to_string();
        let chunk = FormalArchiveChunkV1 {
            object_id: id.clone(),
            total_length: 3,
            offset: 0,
            bytes: b"abc".to_vec(),
        };
        chunk.validate(&id, 0, 1024).unwrap();
        assert!(chunk.validate(&id, 1, 1024).is_err());
        assert!(chunk.validate(&"00".repeat(32), 0, 1024).is_err());
        assert!(chunk.validate(&id, 0, 0).is_err());
        let encoded = serde_cbor::to_vec(&chunk).unwrap();
        let value: serde_cbor::Value = serde_cbor::from_slice(&encoded).unwrap();
        let serde_cbor::Value::Map(fields) = value else {
            panic!("map required")
        };
        assert!(matches!(
            fields.get(&serde_cbor::Value::Text("bytes".into())),
            Some(serde_cbor::Value::Bytes(_))
        ));
        let decoded: FormalArchiveChunkV1 = serde_cbor::from_slice(&encoded).unwrap();
        decoded.validate(&id, 0, 1024).unwrap();
        let over = FormalArchiveChunkV1 {
            bytes: vec![0; FORMAL_ARCHIVE_CHUNK_LIMIT as usize + 1],
            ..chunk
        };
        assert!(
            serde_cbor::from_slice::<FormalArchiveChunkV1>(&serde_cbor::to_vec(&over).unwrap())
                .is_err()
        );
    }
}
