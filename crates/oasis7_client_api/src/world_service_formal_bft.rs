//! Separate formal BFT read contract. V1 read DTOs and signing bytes stay unchanged.
use super::{ContractError, FormalProjectionInclusionV1, FormalReadAnchorRefV1, WorldIdentity};
use serde::{Deserialize, Serialize};

pub const FORMAL_BFT_READ_PATH_V2: &str = "/v2/world/read";
pub const FORMAL_BFT_READ_SCHEMA_VERSION_V2: u32 = 2;
pub const FORMAL_BFT_READ_CONTRACT_VERSION_V2: u32 = 2;
pub const MAX_FORMAL_BFT_CERTIFICATE_BYTES_V2: usize = 2 * 1024 * 1024;
pub const MAX_FORMAL_BFT_COMMITMENT_BYTES_V2: usize = 1024 * 1024;
pub const MAX_FORMAL_BFT_READ_RESPONSE_BYTES_V2: usize = 8 * 1024 * 1024;

/// This first consumer is an exact, externally pinned H+1 cut. Advancing beyond
/// it needs a separately authenticated follow-authorization and time policy.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FormalBftReadTargetV2 {
    pub height: u64,
    pub block_id: String,
}
impl FormalBftReadTargetV2 {
    pub fn validate(&self) -> Result<(), ContractError> {
        if self.height == 0 || !is_digest(&self.block_id) {
            return Err(ContractError("formal BFT exact cut pin"));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FormalBftReadProfileV2 {
    pub name: String,
    pub version: u32,
    pub wire_version: u16,
}
impl FormalBftReadProfileV2 {
    pub fn bft_v1() -> Self {
        Self {
            name: "bft".into(),
            version: 1,
            wire_version: 2,
        }
    }
    pub fn validate(&self) -> Result<(), ContractError> {
        if self != &Self::bft_v1() {
            return Err(ContractError("unsupported formal BFT read profile"));
        }
        Ok(())
    }
}

/// Signed on the independent `/v2/world/read` domain. The cut is an exact
/// local pin; it cannot be advanced by a response or by the service.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FormalBftReadRequestV2 {
    pub schema_version: u32,
    pub contract_version: u32,
    pub world: WorldIdentity,
    pub scope_id: String,
    pub observation_nonce: String,
    pub handoff_anchor: FormalReadAnchorRefV1,
    pub target_config_digest: String,
    pub activation_digest: String,
    pub exact_target: FormalBftReadTargetV2,
}
impl FormalBftReadRequestV2 {
    pub fn validate(&self) -> Result<(), ContractError> {
        if self.schema_version != FORMAL_BFT_READ_SCHEMA_VERSION_V2
            || self.contract_version != FORMAL_BFT_READ_CONTRACT_VERSION_V2
            || self.observation_nonce.len() != 64
            || !is_digest(&self.observation_nonce)
            || !is_digest(&self.target_config_digest)
            || !is_digest(&self.activation_digest)
            || self.scope_id != "public"
                && !self
                    .scope_id
                    .strip_prefix("agent:")
                    .is_some_and(|id| !id.trim().is_empty() && id.len() <= 256)
        {
            return Err(ContractError("formal BFT read request schema/binding"));
        }
        self.world.validate()?;
        self.handoff_anchor.validate()?;
        self.exact_target.validate()
    }
}

/// Cursor/head identity fields are repeated from the certified block so the
/// application projection cannot be detached from its QC.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FormalBftReadHeadV2 {
    pub height: u64,
    pub block_id: String,
    pub execution_block_hash: String,
    pub state_root: String,
    pub post_manifest_hash: String,
    pub projection_root: String,
    pub logical_tick: u64,
}
impl FormalBftReadHeadV2 {
    pub fn validate(&self) -> Result<(), ContractError> {
        if self.height == 0
            || !is_digest(&self.block_id)
            || !is_digest(&self.execution_block_hash)
            || !is_digest(&self.state_root)
            || !is_digest(&self.post_manifest_hash)
            || !is_digest(&self.projection_root)
        {
            return Err(ContractError("formal BFT read head"));
        }
        Ok(())
    }
}

/// Canonical CBOR is carried as lowercase hex so the JSON transport preserves
/// exact bytes and the verifier can reject alternate encodings.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FormalBftReadResponseV2<T> {
    pub schema_version: u32,
    pub contract_version: u32,
    pub profile: FormalBftReadProfileV2,
    pub world: WorldIdentity,
    pub scope_id: String,
    pub observation_nonce: String,
    pub handoff_anchor: FormalReadAnchorRefV1,
    pub target_config_digest: String,
    pub activation_digest: String,
    pub exact_target: FormalBftReadTargetV2,
    pub head: FormalBftReadHeadV2,
    pub certificate_cbor_hex: String,
    pub execution_commitment_cbor_hex: String,
    pub projection: FormalProjectionInclusionV1,
    pub view: T,
}
impl<T: Serialize> FormalBftReadResponseV2<T> {
    pub fn validate_structure(
        &self,
        request: &FormalBftReadRequestV2,
    ) -> Result<(), ContractError> {
        request.validate()?;
        self.profile.validate()?;
        self.world.validate()?;
        self.handoff_anchor.validate()?;
        self.exact_target.validate()?;
        self.head.validate()?;
        if self.schema_version != FORMAL_BFT_READ_SCHEMA_VERSION_V2
            || self.contract_version != FORMAL_BFT_READ_CONTRACT_VERSION_V2
            || self.world != request.world
            || self.scope_id != request.scope_id
            || self.observation_nonce != request.observation_nonce
            || self.handoff_anchor != request.handoff_anchor
            || self.target_config_digest != request.target_config_digest
            || self.activation_digest != request.activation_digest
            || self.exact_target != request.exact_target
            || self.head.height != self.exact_target.height
            || self.head.block_id != self.exact_target.block_id
            || !valid_hex_bytes(
                &self.certificate_cbor_hex,
                MAX_FORMAL_BFT_CERTIFICATE_BYTES_V2,
            )
            || !valid_hex_bytes(
                &self.execution_commitment_cbor_hex,
                MAX_FORMAL_BFT_COMMITMENT_BYTES_V2,
            )
            || self.projection.schema_version != 1
        {
            return Err(ContractError("formal BFT response binding/bounds"));
        }
        if serde_json::to_vec(self)
            .map_err(|_| ContractError("formal BFT response encoding"))?
            .len()
            > MAX_FORMAL_BFT_READ_RESPONSE_BYTES_V2
        {
            return Err(ContractError("formal BFT response byte bound"));
        }
        Ok(())
    }
}

fn valid_hex_bytes(value: &str, maximum: usize) -> bool {
    !value.is_empty()
        && value.len().is_multiple_of(2)
        && value.len() <= maximum.saturating_mul(2)
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn is_digest(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn request() -> FormalBftReadRequestV2 {
        FormalBftReadRequestV2 {
            schema_version: 2,
            contract_version: 2,
            world: WorldIdentity {
                world_id: "world".into(),
                genesis_digest: "11".repeat(32),
            },
            scope_id: "public".into(),
            observation_nonce: "22".repeat(32),
            handoff_anchor: FormalReadAnchorRefV1 {
                position: 1,
                record_id: "33".repeat(32),
                active_authority_id: "44".repeat(32),
            },
            target_config_digest: "55".repeat(32),
            activation_digest: "66".repeat(32),
            exact_target: FormalBftReadTargetV2 {
                height: 2,
                block_id: "77".repeat(32),
            },
        }
    }

    #[test]
    fn formal_bft_request_requires_exact_external_handoff_cut_and_profile() {
        let request = request();
        request.validate().unwrap();

        let mut unsupported = request.clone();
        unsupported.scope_id = "agent:a".into();
        assert!(unsupported.validate().is_ok());
        unsupported.scope_id = "private".into();
        assert!(unsupported.validate().is_err());

        let mut bad = request;
        bad.target_config_digest = "A5".repeat(32);
        assert!(bad.validate().is_err());
        assert!(
            FormalBftReadProfileV2 {
                name: "controlled_single_authority".into(),
                version: 1,
                wire_version: 2,
            }
            .validate()
            .is_err()
        );
    }
}
