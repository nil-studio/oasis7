//! Persistent participant authority values and stable identity codecs.
use super::request_digest;
use oasis7_agent_api::game::*;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ParticipantEntryPolicyV1 {
    pub api_rev: u32,
    pub world_id: String,
    pub genesis_digest: String,
    pub identity_namespace: GameIdentityNamespaceV1,
    pub issuer_public_key: String,
    pub generation: u64,
    pub max_grant_ticks: u64,
    pub max_participants: u64,
    pub registry_complete: bool,
    pub enabled: bool,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ParticipantOperationV1 {
    pub request: GameParticipantRegisterV1,
    pub receipt: GameParticipantReceiptV1,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct ParticipantRegistryV1 {
    pub policy: Option<ParticipantEntryPolicyV1>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub policy_history: Vec<super::participant_policy::ParticipantPolicyAuthorizationV1>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub migrated_bindings:
        BTreeMap<String, super::participant_policy::ParticipantMigrationBindingV1>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub devlocal_policy_history: Vec<
        super::SignedReadRequest<
            super::participant_policy::DevLocalParticipantPolicyAuthorizationV1,
        >,
    >,
    pub bindings: BTreeMap<String, GameParticipantReceiptV1>,
    pub operations: BTreeMap<String, ParticipantOperationV1>,
}
pub fn account_key(r: &GameParticipantRegisterV1) -> Result<String, String> {
    request_digest(
        "participant-account-world",
        &(
            &r.identity_namespace.service_id,
            &r.identity_namespace.environment_id,
            &r.account_id,
            &r.world_id,
            &r.genesis_digest,
        ),
    )
}
pub fn operation_key(r: &GameParticipantRegisterV1) -> Result<String, String> {
    request_digest(
        "participant-operation",
        &(account_key(r)?, &r.client_operation_id),
    )
}
pub fn business_digest(r: &GameParticipantRegisterV1) -> Result<String, String> {
    oasis7_client_api::world_service_correlation::participant_business_digest(r)
}
impl ParticipantRegistryV1 {
    pub fn is_empty(&self) -> bool {
        self.policy.is_none()
            && self.policy_history.is_empty()
            && self.devlocal_policy_history.is_empty()
            && self.migrated_bindings.is_empty()
            && self.bindings.is_empty()
            && self.operations.is_empty()
    }
}
