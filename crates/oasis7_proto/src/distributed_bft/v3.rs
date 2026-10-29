//! Public block identity foundation. This type grants no signing or quorum authority.
use serde::{Deserialize, Serialize};

use super::{BFT_MAX_ACTIONS_CBOR_BYTES_V1, domain_framed, hash_hex};

mod candidate_read;
mod signatures;
pub use candidate_read::*;
pub use signatures::*;

pub const BFT_PROTOCOL_VERSION_V3: u16 = 3;
pub const BFT_HEADER_ID_DOMAIN_V3: &str = "oasis7.bft.header_id.v3.hash";
pub const BFT_ACTIONS_DIGEST_DOMAIN_V3: &str = "oasis7.bft.actions_cbor.v3.hash";
pub const BFT_HEADER_MAX_BYTES_V3: usize = 4096;

/// Commits to the public execution preimage and the exact private payload bytes.
/// V2 votes and handoff targets do not authorize this independent identity.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BftBlockHeaderV3 {
    pub protocol_version: u16,
    pub execution_commitment_hash: String,
    pub actions_cbor_digest: String,
}

impl BftBlockHeaderV3 {
    pub fn validate(&self) -> Result<(), String> {
        if self.protocol_version != BFT_PROTOCOL_VERSION_V3 {
            return Err("unsupported BFT public header version".into());
        }
        hash_hex("execution commitment", &self.execution_commitment_hash)?;
        hash_hex("actions digest", &self.actions_cbor_digest)
    }

    pub fn canonical_bytes(&self) -> Result<Vec<u8>, String> {
        self.validate()?;
        serde_cbor::to_vec(self).map_err(|_| "encode BFT public header".into())
    }

    pub fn decode_canonical(bytes: &[u8]) -> Result<Self, String> {
        if bytes.len() > BFT_HEADER_MAX_BYTES_V3 {
            return Err("BFT public header exceeds bound".into());
        }
        let value: Self =
            serde_cbor::from_slice(bytes).map_err(|_| "decode BFT public header".to_string())?;
        if value.canonical_bytes()? != bytes {
            return Err("noncanonical BFT public header".into());
        }
        Ok(value)
    }

    pub fn block_id(&self) -> Result<String, String> {
        self.validate()?;
        Ok(blake3::hash(&domain_framed(BFT_HEADER_ID_DOMAIN_V3, self)?)
            .to_hex()
            .to_string())
    }

    pub fn verify_private_payload(&self, bytes: &[u8]) -> Result<(), String> {
        self.validate()?;
        if actions_cbor_digest_v3(bytes)? != self.actions_cbor_digest {
            return Err("BFT private payload digest mismatch".into());
        }
        Ok(())
    }
}

/// Byte binding only: action decoding, action-root checks and execution are separate.
pub fn actions_cbor_digest_v3(bytes: &[u8]) -> Result<String, String> {
    if bytes.len() > BFT_MAX_ACTIONS_CBOR_BYTES_V1 {
        return Err("BFT private payload exceeds bound".into());
    }
    let mut hasher = blake3::Hasher::new();
    hasher.update(&(BFT_ACTIONS_DIGEST_DOMAIN_V3.len() as u64).to_be_bytes());
    hasher.update(BFT_ACTIONS_DIGEST_DOMAIN_V3.as_bytes());
    hasher.update(&(bytes.len() as u64).to_be_bytes());
    hasher.update(bytes);
    Ok(hasher.finalize().to_hex().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn public_header_binds_private_bytes_without_disclosing_them() {
        let secret = b"private-action-canary-4975";
        let header = BftBlockHeaderV3 {
            protocol_version: BFT_PROTOCOL_VERSION_V3,
            execution_commitment_hash: "11".repeat(32),
            actions_cbor_digest: actions_cbor_digest_v3(secret).unwrap(),
        };
        let encoded = header.canonical_bytes().unwrap();
        assert!(!encoded.windows(secret.len()).any(|part| part == secret));
        assert_eq!(
            BftBlockHeaderV3::decode_canonical(&encoded).unwrap(),
            header
        );
        header.verify_private_payload(secret).unwrap();
        assert!(
            header
                .verify_private_payload(b"changed private action")
                .is_err()
        );
        let mut changed = header.clone();
        changed.execution_commitment_hash = "22".repeat(32);
        assert_ne!(changed.block_id().unwrap(), header.block_id().unwrap());
        changed.protocol_version = 2;
        assert!(changed.block_id().is_err());
        let mut trailing = encoded;
        trailing.push(0);
        assert!(BftBlockHeaderV3::decode_canonical(&trailing).is_err());
    }

    #[test]
    fn payload_digest_frames_raw_bytes_at_exact_bound() {
        let bytes = vec![255; BFT_MAX_ACTIONS_CBOR_BYTES_V1];
        let mut frame = Vec::new();
        frame.extend_from_slice(&(BFT_ACTIONS_DIGEST_DOMAIN_V3.len() as u64).to_be_bytes());
        frame.extend_from_slice(BFT_ACTIONS_DIGEST_DOMAIN_V3.as_bytes());
        frame.extend_from_slice(&(bytes.len() as u64).to_be_bytes());
        frame.extend_from_slice(&bytes);
        assert_eq!(
            actions_cbor_digest_v3(&bytes).unwrap(),
            blake3::hash(&frame).to_hex().to_string()
        );
        assert!(actions_cbor_digest_v3(&vec![0; BFT_MAX_ACTIONS_CBOR_BYTES_V1 + 1]).is_err());
    }
}
