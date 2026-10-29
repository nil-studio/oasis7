use super::*;
use ed25519_dalek::{Signature, VerifyingKey};
use oasis7_consensus::tendermint_v3::BftPeerContextV3;
pub(super) const PROTOCOL: &str = "/oasis7/bft/private-candidate/2.0.0";
pub(super) const DOMAIN: &str = "oasis7.bft.private-candidate.exchange.v2";
pub(super) const MAX_FRAME: usize = 64 * 1024;
pub(super) const CHUNK: u32 = 16 * 1024;
pub(super) const MAX_PACKAGE: usize = 66 * 1024 * 1024;
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Target {
    pub context: BftPeerContextV3,
    pub height: u64,
    pub header_id: String,
    pub execution_commitment_hash: String,
    pub kind: NodeBftPrivateFetchKindV3,
    pub requester_peer_id: String,
    pub serving_peer_id: String,
    pub nonce: String,
    pub operation: BftCandidateReadOperationV3,
}
impl Target {
    pub fn validate(&self) -> Result<(), String> {
        self.context.scope.validate()?;
        if self.height == 0 {
            return Err("private target height is zero".into());
        }
        for id in [
            &self.context.activation_digest,
            &self.context.target_digest,
            &self.nonce,
            &self.header_id,
            &self.execution_commitment_hash,
        ] {
            hash(id)?;
        }
        self.requester_peer_id
            .parse::<libp2p::PeerId>()
            .map_err(|_| "private requester peer identity")?;
        self.serving_peer_id
            .parse::<libp2p::PeerId>()
            .map_err(|_| "private serving peer identity")?;
        self.operation.validate()
    }
    pub fn base_digest(&self) -> Result<String, String> {
        let mut base = self.clone();
        base.operation = BftCandidateReadOperationV3::Manifest;
        Ok(digest(b"oasis7.bft.private-target.v2", &encode(&base)?))
    }
    pub fn validate_value(&self, value: &BftHeaderValueV3) -> Result<(), String> {
        value.validate_binding()?;
        if self.context.scope != value.execution_commitment.scope
            || self.context.activation_digest != value.execution_commitment.activation_digest
            || self.height != value.execution_commitment.height
            || self.header_id != value.block_id()?
            || self.execution_commitment_hash != value.header.execution_commitment_hash
        {
            return Err("private compact target differs from original public value".into());
        }
        Ok(())
    }
    pub fn pending_body(
        &self,
        value: &BftHeaderValueV3,
        validator_id: String,
        public_key: String,
        policy: String,
        generation: u64,
    ) -> Result<BftCandidateReadBodyV3, String> {
        self.validate_value(value)?;
        let c = &value.execution_commitment;
        Ok(BftCandidateReadBodyV3 {
            schema_version: 3,
            activation_digest: self.context.activation_digest.clone(),
            target_digest: self.context.target_digest.clone(),
            scope: self.context.scope.clone(),
            validator_id,
            validator_public_key_hex: public_key,
            candidate_policy_digest: policy,
            candidate_grant_generation: generation,
            authority_epoch: c.authority_epoch,
            active_validator_set_hash: c.active_validator_set_hash.clone(),
            schedule_policy_hash: c.schedule_policy_hash.clone(),
            candidate_height: c.height,
            parent_height: c.parent_tick,
            parent_commit_ref: c.parent_commit_ref.clone(),
            parent_state_root: c.parent_semantic_state_root.clone(),
            parent_manifest_hash: c.manifest_hash.clone(),
            header_id: self.header_id.clone(),
            execution_commitment_hash: self.execution_commitment_hash.clone(),
            requester_peer_id: self.requester_peer_id.clone(),
            serving_peer_id: self.serving_peer_id.clone(),
            nonce: self.nonce.clone(),
            operation: self.operation.clone(),
        })
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Request {
    pub domain: String,
    pub schema_version: u32,
    pub target: Target,
    pub authorization: NodeBftPrivateReadAuthorizationV3,
    pub chunk_index: Option<u32>,
}
impl Request {
    pub fn validate(&self) -> Result<(), String> {
        if self.domain != DOMAIN || self.schema_version != 2 {
            return Err("private request domain/schema".into());
        }
        self.target.validate()?;
        match (&self.target.operation, self.chunk_index) {
            (BftCandidateReadOperationV3::Manifest, None) => Ok(()),
            (op @ BftCandidateReadOperationV3::Package { .. }, Some(index)) => {
                op.chunk_range(index).map(|_| ())
            }
            _ => Err("private manifest/package operation mismatch".into()),
        }
    }
    pub fn request_digest(&self) -> Result<String, String> {
        Ok(digest(b"oasis7.bft.private-request.v2", &encode(self)?))
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Manifest {
    pub package_digest: String,
    pub total_bytes: u64,
    pub chunk_size: u32,
    pub total_chunks: u32,
}
impl Manifest {
    pub fn operation(&self) -> BftCandidateReadOperationV3 {
        BftCandidateReadOperationV3::Package {
            package_digest: self.package_digest.clone(),
            total_bytes: self.total_bytes,
            chunk_size: self.chunk_size,
            total_chunks: self.total_chunks,
        }
    }
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Response {
    pub domain: String,
    pub request_digest: String,
    pub manifest: Manifest,
    pub chunk_index: Option<u32>,
    pub chunk_hex: Option<String>,
}
pub(super) fn encode<T: Serialize>(v: &T) -> Result<Vec<u8>, String> {
    serde_cbor::to_vec(v).map_err(|_| "private typed encode".into())
}
pub(super) fn encode_frame<T: Serialize>(v: &T) -> Result<Vec<u8>, String> {
    let b = encode(v)?;
    if b.len() > MAX_FRAME {
        return Err("private frame exceeds 64KiB".into());
    }
    Ok(b)
}
pub(super) fn decode_frame<T: for<'a> Deserialize<'a> + Serialize>(
    bytes: &[u8],
) -> Result<T, String> {
    if bytes.len() > MAX_FRAME {
        return Err("private frame exceeds 64KiB".into());
    }
    scan(bytes, false)?;
    let v: T = serde_cbor::from_slice(bytes).map_err(|_| "private typed frame decode")?;
    if encode(&v)? != bytes {
        return Err("noncanonical private typed frame".into());
    }
    Ok(v)
}
pub(super) fn digest(domain: &[u8], bytes: &[u8]) -> String {
    let mut h = blake3::Hasher::new();
    h.update(&(domain.len() as u64).to_be_bytes());
    h.update(domain);
    h.update(&(bytes.len() as u64).to_be_bytes());
    h.update(bytes);
    h.finalize().to_hex().to_string()
}
pub(super) fn hash(v: &str) -> Result<(), String> {
    if v.len() != 64
        || !v
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    {
        return Err("private digest identity".into());
    }
    Ok(())
}
pub(super) fn committed_payload(
    target: &Target,
    grant: &FormalNodeReadGrantV1,
    subject: &str,
) -> Result<Vec<u8>, String> {
    let bytes = encode(&(target, grant, subject))?;
    let mut payload = b"oasis7.bft.committed-private-read.v2.sign\0".to_vec();
    payload.extend(bytes);
    if payload.len() > MAX_FRAME {
        return Err("committed read preimage capacity".into());
    }
    Ok(payload)
}
pub(super) fn verify_signature(key: &str, payload: &[u8], signature: &str) -> Result<(), String> {
    hash(key)?;
    if signature.len() != 128
        || !signature
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    {
        return Err("private request signature encoding".into());
    }
    let k: [u8; 32] = hex::decode(key)
        .map_err(|_| "private reader key")?
        .try_into()
        .map_err(|_| "private reader key size")?;
    let s = Signature::from_slice(&hex::decode(signature).map_err(|_| "private reader signature")?)
        .map_err(|_| "private reader signature size")?;
    VerifyingKey::from_bytes(&k)
        .map_err(|_| "private reader key encoding")?
        .verify_strict(payload, &s)
        .map_err(|_| "private reader signature mismatch".into())
}
// No attacker-declared collection or string length reaches serde allocation
// before these frame/package structural and remaining-byte checks.
pub(super) fn scan(bytes: &[u8], package: bool) -> Result<(), String> {
    fn item(b: &[u8], p: &mut usize, d: usize, package: bool) -> Result<(), String> {
        if d > 48 {
            return Err("private CBOR nesting".into());
        }
        let h = *b.get(*p).ok_or("private CBOR truncated")?;
        *p += 1;
        let major = h >> 5;
        let low = h & 31;
        let n = match low {
            0..=23 => u64::from(low),
            24..=27 => {
                let width = 1usize << (low - 24);
                let end = p.checked_add(width).ok_or("private CBOR overflow")?;
                let bytes = b.get(*p..end).ok_or("private CBOR length")?;
                *p = end;
                bytes.iter().fold(0u64, |v, x| (v << 8) | u64::from(*x))
            }
            _ => return Err("private CBOR indefinite".into()),
        };
        match major {
            0 | 1 => {}
            2 | 3 => {
                let max = if major == 2 && package {
                    4 * 1024 * 1024 + 128
                } else {
                    32768
                };
                if n > max {
                    return Err("private CBOR string capacity".into());
                }
                *p = p
                    .checked_add(n as usize)
                    .filter(|v| *v <= b.len())
                    .ok_or("private CBOR string length")?;
            }
            4 | 5 => {
                let limit = if major == 5 {
                    64
                } else if package {
                    4096
                } else {
                    256
                };
                if n > limit {
                    return Err("private CBOR collection capacity".into());
                }
                let count = if major == 5 { n * 2 } else { n };
                if count > (b.len() - *p) as u64 {
                    return Err("private CBOR declared collection".into());
                }
                for _ in 0..count {
                    item(b, p, d + 1, package)?;
                }
            }
            7 if matches!(low, 20..=22) => {}
            _ => return Err("private CBOR type".into()),
        }
        Ok(())
    }
    let mut p = 0;
    item(bytes, &mut p, 0, package)?;
    if p != bytes.len() {
        return Err("private CBOR trailing".into());
    }
    Ok(())
}

#[cfg(test)]
#[path = "wire_tests.rs"]
mod tests;
