//! Versioned input for an operation-budgeted, read-only full sync.

use super::{
    FormalFullSyncConfigV1, FormalFullSyncEndpointV1, FormalNodeReadGrantV1, FormalNodeReadRoleV1,
    FormalReadTrustConfigV1,
};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct FormalFullSyncTransferPolicyV1 {
    pub api_version: String,
    pub attempt_timeout_ms: u64,
    pub max_attempts: u64,
    pub max_encoded_io_bytes: u64,
    pub max_exchanges: u64,
    pub max_proof_pages: u64,
    pub max_proof_bytes: u64,
    pub max_inflight_requests: u64,
    pub reopen_timeout_ms: u64,
}
impl FormalFullSyncTransferPolicyV1 {
    pub(crate) fn validate(&self) -> Result<(), String> {
        if self.api_version != "oasis7.formal-full-sync-transfer-policy/v1"
            || !(100..=600_000).contains(&self.attempt_timeout_ms)
            || !(1..=4).contains(&self.max_attempts)
            || !(1..=8 * 1024 * 1024 * 1024).contains(&self.max_encoded_io_bytes)
            || !(1..=262_660).contains(&self.max_exchanges)
            || !(1..=512).contains(&self.max_proof_pages)
            || !(1..=2 * 1024 * 1024 * 1024).contains(&self.max_proof_bytes)
            || self.max_inflight_requests != 1
            || !(100..=600_000).contains(&self.reopen_timeout_ms)
            || self
                .max_attempts
                .checked_mul(self.attempt_timeout_ms)
                .is_none()
        {
            return Err("invalid full-sync transfer policy".into());
        }
        Ok(())
    }
}

const FULL_SYNC_CONFIG_V2: &str = "oasis7.formal-full-state-sync/v2";

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct FormalFullSyncConfigV2 {
    pub api_version: String,
    pub endpoint: String,
    pub trusted_service_public_key: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub alternate_endpoints: Vec<FormalFullSyncEndpointV1>,
    pub world_id: String,
    pub chain_id: String,
    pub genesis_digest: String,
    pub formal_read_trust: FormalReadTrustConfigV1,
    #[serde(deserialize_with = "Option::deserialize")]
    pub grant: Option<FormalNodeReadGrantV1>,
    pub expected_role: FormalNodeReadRoleV1,
    pub poll_interval_ms: u64,
    pub request_timeout_ms: u64,
    pub max_total_sync_bytes: u64,
    pub transfer_policy: FormalFullSyncTransferPolicyV1,
}

impl FormalFullSyncConfigV2 {
    pub(crate) fn validate(&self) -> Result<(), String> {
        if self.api_version != FULL_SYNC_CONFIG_V2 {
            return Err("unsupported operation-budgeted full-sync configuration version".into());
        }
        self.transfer_policy.validate()?;
        self.read_material_config()
            .validate_with_authority_profile(Some(2))
    }

    /// The validated read protocol, grant and material limits keep their V1 wire
    /// semantics. The V2 policy remains part of the outer immutable identity.
    pub(in super::super) fn read_material_config(&self) -> FormalFullSyncConfigV1 {
        FormalFullSyncConfigV1 {
            api_version: super::API_VERSION.into(),
            endpoint: self.endpoint.clone(),
            trusted_service_public_key: self.trusted_service_public_key.clone(),
            alternate_endpoints: self.alternate_endpoints.clone(),
            world_id: self.world_id.clone(),
            chain_id: self.chain_id.clone(),
            genesis_digest: self.genesis_digest.clone(),
            formal_read_trust: self.formal_read_trust.clone(),
            grant: self.grant.clone(),
            expected_role: self.expected_role.clone(),
            poll_interval_ms: self.poll_interval_ms,
            request_timeout_ms: self.request_timeout_ms,
            max_total_sync_bytes: self.max_total_sync_bytes,
        }
    }
}

impl PartialEq for FormalFullSyncConfigV2 {
    fn eq(&self, other: &Self) -> bool {
        self.api_version == other.api_version
            && self.transfer_policy == other.transfer_policy
            && self.endpoint == other.endpoint
            && self.trusted_service_public_key == other.trusted_service_public_key
            && self.alternate_endpoints == other.alternate_endpoints
            && self.world_id == other.world_id
            && self.chain_id == other.chain_id
            && self.genesis_digest == other.genesis_digest
            && self.formal_read_trust.initial_trust == other.formal_read_trust.initial_trust
            && self.formal_read_trust.checkpoint == other.formal_read_trust.checkpoint
            && self.formal_read_trust.checkpoint_digest == other.formal_read_trust.checkpoint_digest
            && self.grant == other.grant
            && self.expected_role == other.expected_role
            && self.poll_interval_ms == other.poll_interval_ms
            && self.request_timeout_ms == other.request_timeout_ms
            && self.max_total_sync_bytes == other.max_total_sync_bytes
    }
}
impl Eq for FormalFullSyncConfigV2 {}
