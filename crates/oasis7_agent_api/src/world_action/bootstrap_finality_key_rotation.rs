//! Versioned finality-key possession material; it conveys no controller authority.
use super::GovernanceAuthorizationV1;
use serde::{Deserialize, Serialize};

pub const BOOTSTRAP_FINALITY_SOURCE_DOMAIN_V1: &str =
    "oasis7.governance.bootstrap-finality-source.v1";

pub const BOOTSTRAP_FINALITY_KEY_ROTATION_MAX_BYTES_V1: usize = 16 * 1024;
pub const BOOTSTRAP_FINALITY_KEY_ROTATION_OLD_DOMAIN_V1: &str =
    "oasis7.governance.bootstrap-finality-key-rotation.old.v1";
pub const BOOTSTRAP_FINALITY_KEY_ROTATION_NEW_DOMAIN_V1: &str =
    "oasis7.governance.bootstrap-finality-key-rotation.new.v1";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum BootstrapFinalityKeyRotationRoleV1 {
    OldKeyAuthorization,
    NewKeyPossession,
}
impl BootstrapFinalityKeyRotationRoleV1 {
    pub fn domain(self) -> &'static str {
        match self {
            Self::OldKeyAuthorization => BOOTSTRAP_FINALITY_KEY_ROTATION_OLD_DOMAIN_V1,
            Self::NewKeyPossession => BOOTSTRAP_FINALITY_KEY_ROTATION_NEW_DOMAIN_V1,
        }
    }
}

/// The full unsigned controller context is part of both key proofs. Neither
/// proof nor controller signatures enter this body, avoiding a signing cycle.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BootstrapFinalityKeyRotationBodyV1 {
    pub schema_version: u16,
    pub authorization: GovernanceAuthorizationV1,
    pub rotation_id: String,
    pub bootstrap_registry_digest_hex: String,
    pub bootstrap_public_key_hex: String,
    pub node_id: String,
    pub expected_generation: u64,
    pub old_public_key_hex: String,
    pub new_public_key_hex: String,
    pub decision_height: u64,
    pub activation_height: u64,
    pub target_epoch: u64,
}
fn lower_hex(value: &str, length: usize) -> bool {
    value.len() == length
        && value
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}
impl BootstrapFinalityKeyRotationBodyV1 {
    /// Structural bounds only. Registry activity, key uniqueness, epoch mapping,
    /// controller threshold and actual parent ownership are verified by World.
    pub fn validate(&self) -> Result<(), String> {
        self.authorization.validate()?;
        if self.schema_version != 1 || !self.authorization.signatures.is_empty() {
            return Err("rotation body version or signed controller context".into());
        }
        for id in [&self.rotation_id, &self.node_id] {
            if id.is_empty() || id.len() > 256 || id.trim() != id {
                return Err("rotation identity bounds".into());
            }
        }
        if !lower_hex(&self.bootstrap_registry_digest_hex, 64)
            || !lower_hex(&self.bootstrap_public_key_hex, 64)
            || !lower_hex(&self.old_public_key_hex, 64)
            || !lower_hex(&self.new_public_key_hex, 64)
            || self.old_public_key_hex == self.new_public_key_hex
        {
            return Err("rotation key encoding or unchanged key".into());
        }
        if self.expected_generation.checked_add(1).is_none()
            || self.authorization.parent_position.checked_add(1) != Some(self.decision_height)
            || self.decision_height.checked_add(2) != Some(self.activation_height)
            || self.target_epoch == 0
            || self.decision_height < self.authorization.valid_from_position
            || self.decision_height > self.authorization.valid_through_position
        {
            return Err("rotation generation or exact decision/activation boundary".into());
        }
        Ok(())
    }
    pub fn next_generation(&self) -> Result<u64, String> {
        self.validate()?;
        self.expected_generation
            .checked_add(1)
            .ok_or_else(|| "rotation generation overflow".into())
    }
    /// Compare every outer context field while excluding only its signatures.
    pub fn validate_authorization(&self, outer: &GovernanceAuthorizationV1) -> Result<(), String> {
        self.validate()?;
        outer.validate()?;
        let mut unsigned = outer.clone();
        unsigned.signatures.clear();
        if unsigned != self.authorization {
            return Err("rotation body differs from outer controller context".into());
        }
        Ok(())
    }
    /// u64be domain length || domain || u64be canonical typed-CBOR length || CBOR.
    /// The role is selected by trusted code and independently domain-separated.
    pub fn signing_bytes(
        &self,
        role: BootstrapFinalityKeyRotationRoleV1,
    ) -> Result<Vec<u8>, String> {
        self.validate()?;
        let encoded = serde_cbor::to_vec(self).map_err(|_| "rotation body encoding")?;
        let domain = role.domain().as_bytes();
        let size = 16usize
            .checked_add(domain.len())
            .and_then(|v| v.checked_add(encoded.len()))
            .ok_or("rotation preimage size overflow")?;
        if size > BOOTSTRAP_FINALITY_KEY_ROTATION_MAX_BYTES_V1 {
            return Err("rotation preimage byte capacity".into());
        }
        let mut output = Vec::with_capacity(size);
        output.extend_from_slice(&(domain.len() as u64).to_be_bytes());
        output.extend_from_slice(domain);
        output.extend_from_slice(&(encoded.len() as u64).to_be_bytes());
        output.extend_from_slice(&encoded);
        Ok(output)
    }
    /// Provider callers must bound the original body before decoding it.
    /// Re-encoding rejects trailing bytes and alternate integer/map encodings.
    pub fn decode_canonical(bytes: &[u8]) -> Result<Self, String> {
        if bytes.len() > BOOTSTRAP_FINALITY_KEY_ROTATION_MAX_BYTES_V1 {
            return Err("rotation body byte capacity".into());
        }
        let body: Self = serde_cbor::from_slice(bytes).map_err(|_| "rotation body decoding")?;
        body.validate()?;
        if serde_cbor::to_vec(&body).map_err(|_| "rotation body encoding")? != bytes {
            return Err("noncanonical rotation body".into());
        }
        Ok(body)
    }
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BootstrapFinalityKeyRotationProofV1 {
    pub schema_version: u16,
    pub role: BootstrapFinalityKeyRotationRoleV1,
    pub signature_hex: String,
}
impl BootstrapFinalityKeyRotationProofV1 {
    /// Shape validation never substitutes for verifying the role's exact key.
    pub fn validate_for(
        &self,
        body: &BootstrapFinalityKeyRotationBodyV1,
        expected_role: BootstrapFinalityKeyRotationRoleV1,
    ) -> Result<(), String> {
        body.validate()?;
        if self.schema_version != 1
            || self.role != expected_role
            || !lower_hex(&self.signature_hex, 128)
        {
            return Err("rotation proof role/version/signature encoding".into());
        }
        Ok(())
    }
}

#[cfg(test)]
#[path = "bootstrap_finality_key_rotation_tests.rs"]
mod tests;
