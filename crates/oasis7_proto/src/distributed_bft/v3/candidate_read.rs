//! Typed validator key possession for candidate reads. This proves neither
//! Noise identity, read permission, active membership nor execution authority.
use super::super::v2::BftScopeV2;
use super::super::{domain_framed, hash_hex, public_key, signature};
use ed25519_dalek::{Signer, SigningKey};
use serde::{
    Deserialize, Deserializer, Serialize,
    de::{self, Visitor},
};

pub const BFT_CANDIDATE_READ_DOMAIN_V3: &str = "oasis7.bft.candidate-read.v3.sign";
pub const BFT_CANDIDATE_READ_MAX_BODY_BYTES_V3: usize = 16 * 1024;
/// 1 MiB actions + 64 MiB dependency bytes + 1 MiB fixed metadata,
/// including up to 4096 digest labels and the public commitment.
pub const BFT_CANDIDATE_READ_MAX_PACKAGE_BYTES_V3: u64 = 66 * 1024 * 1024;
pub const BFT_CANDIDATE_READ_MAX_CHUNK_BYTES_V3: u32 = 16 * 1024;
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", deny_unknown_fields)]
pub enum BftCandidateReadOperationV3 {
    Manifest,
    Package {
        #[serde(deserialize_with = "text")]
        package_digest: String,
        total_bytes: u64,
        chunk_size: u32,
        total_chunks: u32,
    },
}
impl BftCandidateReadOperationV3 {
    pub fn validate(&self) -> Result<(), String> {
        if let Self::Package {
            package_digest,
            total_bytes,
            chunk_size,
            total_chunks,
        } = self
        {
            hash_hex("candidate package digest", package_digest)?;
            if *total_bytes == 0
                || *total_bytes > BFT_CANDIDATE_READ_MAX_PACKAGE_BYTES_V3
                || *chunk_size == 0
                || *chunk_size > BFT_CANDIDATE_READ_MAX_CHUNK_BYTES_V3
            {
                return Err("candidate read package byte/chunk bounds".into());
            }
            let chunks = total_bytes
                .checked_add(u64::from(*chunk_size) - 1)
                .ok_or("candidate read chunk count overflow")?
                / u64::from(*chunk_size);
            if chunks != u64::from(*total_chunks) {
                return Err("candidate read package layout mismatch".into());
            }
        }
        Ok(())
    }
    /// Each chunk request must also pass the consumer's current authorization.
    /// A Manifest proof can never authorize object/chunk bytes.
    pub fn chunk_range(&self, index: u32) -> Result<(u64, u32), String> {
        self.validate()?;
        let Self::Package {
            total_bytes,
            chunk_size,
            total_chunks,
            ..
        } = self
        else {
            return Err("manifest proof cannot authorize candidate package bytes".into());
        };
        if index >= *total_chunks {
            return Err("candidate read chunk index outside original layout".into());
        }
        let offset = u64::from(index)
            .checked_mul(u64::from(*chunk_size))
            .ok_or("candidate chunk offset overflow")?;
        let len = (*total_bytes - offset).min(u64::from(*chunk_size));
        Ok((
            offset,
            u32::try_from(len).map_err(|_| "candidate chunk length overflow")?,
        ))
    }
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BftCandidateReadBodyV3 {
    pub schema_version: u32,
    #[serde(deserialize_with = "text")]
    pub activation_digest: String,
    #[serde(deserialize_with = "text")]
    pub target_digest: String,
    #[serde(deserialize_with = "scope")]
    pub scope: BftScopeV2,
    #[serde(deserialize_with = "text")]
    pub validator_id: String,
    #[serde(deserialize_with = "text")]
    pub validator_public_key_hex: String,
    #[serde(deserialize_with = "text")]
    pub candidate_policy_digest: String,
    pub candidate_grant_generation: u64,
    pub authority_epoch: u64,
    #[serde(deserialize_with = "text")]
    pub active_validator_set_hash: String,
    #[serde(deserialize_with = "text")]
    pub schedule_policy_hash: String,
    pub candidate_height: u64,
    pub parent_height: u64,
    #[serde(deserialize_with = "text")]
    pub parent_commit_ref: String,
    #[serde(deserialize_with = "text")]
    pub parent_state_root: String,
    #[serde(deserialize_with = "text")]
    pub parent_manifest_hash: String,
    #[serde(deserialize_with = "text")]
    pub header_id: String,
    #[serde(deserialize_with = "text")]
    pub execution_commitment_hash: String,
    #[serde(deserialize_with = "text")]
    pub requester_peer_id: String,
    #[serde(deserialize_with = "text")]
    pub serving_peer_id: String,
    #[serde(deserialize_with = "text")]
    pub nonce: String,
    pub operation: BftCandidateReadOperationV3,
}
impl BftCandidateReadBodyV3 {
    pub fn validate(&self) -> Result<(), String> {
        if self.schema_version != 3
            || self.parent_height.checked_add(1) != Some(self.candidate_height)
            || self.candidate_height == 0
            || self.candidate_grant_generation == 0
            || self.validator_id.is_empty()
            || self.validator_id.len() > 128
            || self.validator_id.trim() != self.validator_id
        {
            return Err("candidate read schema/height/validator bounds".into());
        }
        self.scope.validate()?;
        public_key(&self.validator_public_key_hex)?;
        for digest in [
            &self.activation_digest,
            &self.target_digest,
            &self.candidate_policy_digest,
            &self.active_validator_set_hash,
            &self.schedule_policy_hash,
            &self.parent_commit_ref,
            &self.parent_state_root,
            &self.parent_manifest_hash,
            &self.header_id,
            &self.execution_commitment_hash,
            &self.nonce,
        ] {
            hash_hex("candidate read digest", digest)?;
        }
        peer_id(&self.requester_peer_id)?;
        peer_id(&self.serving_peer_id)?;
        self.operation.validate()?;
        let bytes = serde_cbor::to_vec(self).map_err(|_| "encode candidate read metadata")?;
        if bytes.len() > BFT_CANDIDATE_READ_MAX_BODY_BYTES_V3 {
            return Err("candidate read metadata exceeds bound".into());
        }
        Ok(())
    }
    pub fn signing_payload(&self) -> Result<Vec<u8>, String> {
        self.validate()?;
        domain_framed(BFT_CANDIDATE_READ_DOMAIN_V3, self)
    }
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BftCandidateReadProofV3 {
    pub body: BftCandidateReadBodyV3,
    #[serde(deserialize_with = "text")]
    pub signature_hex: String,
}
impl BftCandidateReadProofV3 {
    pub fn sign(body: BftCandidateReadBodyV3, key: &SigningKey) -> Result<Self, String> {
        if body.validator_public_key_hex != hex::encode(key.verifying_key().to_bytes()) {
            return Err("candidate read key identity mismatch".into());
        }
        let signature_hex = hex::encode(key.sign(&body.signing_payload()?).to_bytes());
        Ok(Self {
            body,
            signature_hex,
        })
    }
    /// The expected body must come from the original request and actual
    /// authenticated transport context, never only a response's own echo.
    pub fn verify_for(
        &self,
        expected: &BftCandidateReadBodyV3,
        expected_public_key: &str,
    ) -> Result<(), String> {
        if &self.body != expected || self.body.validator_public_key_hex != expected_public_key {
            return Err("candidate read proof differs from original context/key".into());
        }
        public_key(expected_public_key)?
            .verify_strict(
                &self.body.signing_payload()?,
                &signature(&self.signature_hex)?,
            )
            .map_err(|_| "candidate read key possession signature mismatch".into())
    }
}
fn text<'de, D: Deserializer<'de>>(d: D) -> Result<String, D::Error> {
    struct Text;
    impl Visitor<'_> for Text {
        type Value = String;
        fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.write_str("at most 128 bytes of candidate-read text")
        }
        fn visit_str<E: de::Error>(self, value: &str) -> Result<String, E> {
            if value.len() > 128 {
                return Err(E::custom("candidate-read text exceeds bound"));
            }
            Ok(value.to_owned())
        }
    }
    d.deserialize_str(Text)
}
fn scope<'de, D: Deserializer<'de>>(d: D) -> Result<BftScopeV2, D::Error> {
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Scope {
        #[serde(deserialize_with = "text")]
        world_id: String,
        #[serde(deserialize_with = "text")]
        chain_id: String,
        #[serde(deserialize_with = "text")]
        genesis_hash: String,
    }
    let s = Scope::deserialize(d)?;
    Ok(BftScopeV2 {
        world_id: s.world_id,
        chain_id: s.chain_id,
        genesis_hash: s.genesis_hash,
    })
}
// Canonical base58btc libp2p PeerId multihash syntax, without a networking
// dependency. The consumer must still parse/compare its real Noise identities.
fn peer_id(value: &str) -> Result<(), String> {
    const ALPHABET: &[u8] = b"123456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz";
    if !(32..=128).contains(&value.len()) {
        return Err("candidate read peer identity length".into());
    }
    let mut little = vec![0u8];
    for c in value.bytes() {
        let mut carry = u32::try_from(
            ALPHABET
                .iter()
                .position(|b| *b == c)
                .ok_or("candidate read peer identity base58")?,
        )
        .map_err(|_| "peer identity alphabet")?;
        for digit in &mut little {
            carry += u32::from(*digit) * 58;
            *digit = carry as u8;
            carry >>= 8;
        }
        while carry > 0 {
            little.push(carry as u8);
            carry >>= 8;
        }
        if little.len() > 96 {
            return Err("candidate read peer identity byte bound".into());
        }
    }
    let leading = value.bytes().take_while(|b| *b == b'1').count();
    while little.len() > 1 && little.last() == Some(&0) {
        little.pop();
    }
    let mut bytes = vec![0u8; leading];
    if little != [0] {
        bytes.extend(little.into_iter().rev());
    }
    let mut p = 0usize;
    let code = varint(&bytes, &mut p)?;
    let len = varint(&bytes, &mut p)?;
    if usize::try_from(len).ok() != Some(bytes.len().saturating_sub(p))
        || !((code == 0 && len <= 42) || (code == 18 && len == 32))
    {
        return Err("candidate read peer identity multihash".into());
    }
    Ok(())
}
fn varint(bytes: &[u8], p: &mut usize) -> Result<u64, String> {
    let mut n = 0u64;
    for shift in (0..=63).step_by(7) {
        let b = *bytes.get(*p).ok_or("truncated peer identity varint")?;
        *p += 1;
        if shift == 63 && b > 1 {
            return Err("peer identity varint overflow".into());
        }
        n |= u64::from(b & 127) << shift;
        if b & 128 == 0 {
            if shift > 0 && b == 0 {
                return Err("noncanonical peer identity varint".into());
            }
            return Ok(n);
        }
    }
    Err("peer identity varint overflow".into())
}
#[cfg(test)]
#[path = "candidate_read_tests.rs"]
mod tests;
