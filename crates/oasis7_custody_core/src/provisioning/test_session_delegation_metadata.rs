//! Decimal metadata representation in the NEW custody domain only.
//! The SDK original and its own canonical signing/HTTP codecs remain unchanged.
use super::*;
#[cfg_attr(test, derive(schemars::JsonSchema))]
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct DelegationMetadataV1 {
    world_id: String,
    branch_id: String,
    agent_id: String,
    owner_binding: String,
    agent_identity_generation: DecimalU64,
    generation: DecimalU64,
    delegate_public_key: String,
    revoked: bool,
    nonce: DecimalU64,
}
impl From<&AgentSignerDelegationChangeV1> for DelegationMetadataV1 {
    fn from(v: &AgentSignerDelegationChangeV1) -> Self {
        Self {
            world_id: v.world_id.clone(),
            branch_id: v.branch_id.clone(),
            agent_id: v.agent_id.clone(),
            owner_binding: v.owner_binding.clone(),
            agent_identity_generation: DecimalU64(v.agent_identity_generation),
            generation: DecimalU64(v.generation),
            delegate_public_key: v.delegate_public_key.clone(),
            revoked: v.revoked,
            nonce: DecimalU64(v.nonce),
        }
    }
}
impl From<DelegationMetadataV1> for AgentSignerDelegationChangeV1 {
    fn from(v: DelegationMetadataV1) -> Self {
        Self {
            world_id: v.world_id,
            branch_id: v.branch_id,
            agent_id: v.agent_id,
            owner_binding: v.owner_binding,
            agent_identity_generation: v.agent_identity_generation.0,
            generation: v.generation.0,
            delegate_public_key: v.delegate_public_key,
            revoked: v.revoked,
            nonce: v.nonce.0,
        }
    }
}
pub(super) fn serialize<S: serde::Serializer>(
    v: &AgentSignerDelegationChangeV1,
    serializer: S,
) -> std::result::Result<S::Ok, S::Error> {
    DelegationMetadataV1::from(v).serialize(serializer)
}
pub(super) fn deserialize<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> std::result::Result<AgentSignerDelegationChangeV1, D::Error> {
    DelegationMetadataV1::deserialize(deserializer).map(Into::into)
}
