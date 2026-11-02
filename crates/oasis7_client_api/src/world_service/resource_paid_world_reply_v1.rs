//! Closed evidence of one consumed World substep, never Owner or Current authority.
use super::*;
use crate::world_service::{self as ws, participant_discovery::OwnerSetupObservationV1};
pub const RESOURCE_PAID_WORLD_REPLY_DOMAIN: &str = "oasis7.world.resource-paid-reply.v1";
pub const RESOURCE_PAID_PROOF_DIGEST_DOMAIN: &str = "oasis7.world.resource-paid-proof-binding.v1";
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ResourcePaidWorldStageV1 {
    OwnerSetupFinal,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ResourcePaidWorldSubstepV1 {
    OwnerPreflight,
    ResourceFinal,
}
impl ResourcePaidWorldSubstepV1 {
    pub fn world_path(self) -> &'static str {
        match self {
            Self::OwnerPreflight => ws::participant_discovery::PARTICIPANT_OWNER_VIEW_PATH,
            Self::ResourceFinal => RESOURCE_GOAL_PATH_V2,
        }
    }
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ResourcePaidWorldBindingV1 {
    pub api_version: u8,
    pub proof_digest: String,
    pub stage: ResourcePaidWorldStageV1,
    pub substep: ResourcePaidWorldSubstepV1,
    pub world_path: String,
    pub body_digest: String,
}
impl ResourcePaidWorldBindingV1 {
    pub fn validate(&self) -> Result<(), String> {
        let digest = |s: &str| s.strip_prefix("blake3:").is_some_and(hex64);
        if self.api_version != 1
            || self.world_path != self.substep.world_path()
            || !digest(&self.proof_digest)
            || !digest(&self.body_digest)
        {
            return Err(no());
        }
        Ok(())
    }
    pub fn request_digest(&self) -> Result<String, String> {
        self.validate()?;
        ws::request_digest(RESOURCE_PAID_WORLD_REPLY_DOMAIN, self)
    }
}
#[derive(Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "profile", rename_all = "snake_case", deny_unknown_fields)]
pub enum ResourcePaidWorldReplyV1 {
    OwnerPreflight {
        binding: ResourcePaidWorldBindingV1,
        observation: Box<ws::SignedServiceResponse<OwnerSetupObservationV1>>,
    },
    ResourceFinal {
        binding: ResourcePaidWorldBindingV1,
        observation: Box<SignedResourceResponseV1<OwnerSetupFinalResourceObservationV2>>,
    },
}
impl ResourcePaidWorldReplyV1 {
    pub fn binding(&self) -> &ResourcePaidWorldBindingV1 {
        match self {
            Self::OwnerPreflight { binding, .. } | Self::ResourceFinal { binding, .. } => binding,
        }
    }
    fn validate(&self) -> Result<(), String> {
        self.binding().validate()?;
        let expected = match self {
            Self::OwnerPreflight { .. } => ResourcePaidWorldSubstepV1::OwnerPreflight,
            Self::ResourceFinal { .. } => ResourcePaidWorldSubstepV1::ResourceFinal,
        };
        if self.binding().substep != expected {
            return Err(no());
        }
        Ok(())
    }
}
pub type SignedResourcePaidWorldReplyV1 = ws::SignedServiceResponse<ResourcePaidWorldReplyV1>;
/// Fixed Node pin is external. Shape and metadata alone never assert payment.
pub fn verify_paid_resource_world_reply<'a>(
    reply: &'a SignedResourcePaidWorldReplyV1,
    expected: &ResourcePaidWorldBindingV1,
    node_public_key: &str,
) -> Result<&'a ResourcePaidWorldReplyV1, String> {
    expected.validate()?;
    reply.payload.validate()?;
    if reply.payload.binding() != expected || reply.request_digest != expected.request_digest()? {
        return Err(no());
    }
    ws::private_goal::verify_owner_signature_strict(
        RESOURCE_PAID_WORLD_REPLY_DOMAIN,
        &(&reply.request_digest, &reply.payload),
        node_public_key,
        &reply.signature_hex,
    )?;
    Ok(&reply.payload)
}
/// Typed-first full-tree equality prevents nested legacy DTO unknown-field loss.
pub fn decode_paid_resource_world_reply(
    bytes: &[u8],
) -> Result<SignedResourcePaidWorldReplyV1, String> {
    if bytes.is_empty() || bytes.len() > MAX_RESOURCE_ENVELOPE_BYTES {
        return Err(no());
    }
    let reply: SignedResourcePaidWorldReplyV1 = serde_json::from_slice(bytes).map_err(|_| no())?;
    let raw: serde_json::Value = serde_json::from_slice(bytes).map_err(|_| no())?;
    if serde_json::to_value(&reply).map_err(|_| no())? != raw {
        return Err(no());
    }
    reply.payload.validate()?;
    Ok(reply)
}

/// Dedicated new result wire. The legacy generic envelope is never upgraded.
#[derive(Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PaidOwnerSetupResourceEnvelopeV1 {
    pub api_version: u8,
    pub ok: bool,
    pub preflight: SignedResourcePaidWorldReplyV1,
    pub resource_final: SignedResourcePaidWorldReplyV1,
}
impl PaidOwnerSetupResourceEnvelopeV1 {
    pub fn world_response(
        &self,
    ) -> Result<&SignedResourceResponseV1<OwnerSetupFinalResourceObservationV2>, String> {
        match &self.resource_final.payload {
            ResourcePaidWorldReplyV1::ResourceFinal { observation, .. } => Ok(observation),
            _ => Err(no()),
        }
    }
    pub fn validate_shape(&self) -> Result<(), String> {
        self.preflight.payload.validate()?;
        self.resource_final.payload.validate()?;
        if self.api_version != 1
            || !self.ok
            || !matches!(
                self.preflight.payload,
                ResourcePaidWorldReplyV1::OwnerPreflight { .. }
            )
            || !matches!(
                self.resource_final.payload,
                ResourcePaidWorldReplyV1::ResourceFinal { .. }
            )
            || self.preflight.payload.binding().proof_digest
                != self.resource_final.payload.binding().proof_digest
        {
            return Err(no());
        }
        Ok(())
    }
    pub fn encode(&self) -> Result<Vec<u8>, String> {
        self.validate_shape()?;
        let bytes = serde_json::to_vec(self).map_err(|_| no())?;
        if bytes.len() > MAX_RESOURCE_ENVELOPE_BYTES {
            return Err(no());
        }
        Ok(bytes)
    }
    pub fn decode(bytes: &[u8]) -> Result<Self, String> {
        if bytes.is_empty() || bytes.len() > MAX_RESOURCE_ENVELOPE_BYTES {
            return Err(no());
        }
        let value: Self = serde_json::from_slice(bytes).map_err(|_| no())?;
        let raw: serde_json::Value = serde_json::from_slice(bytes).map_err(|_| no())?;
        if serde_json::to_value(&value).map_err(|_| no())? != raw {
            return Err(no());
        }
        value.validate_shape()?;
        Ok(value)
    }
    pub fn verify(
        &self,
        preflight: &ResourcePaidWorldBindingV1,
        resource_final: &ResourcePaidWorldBindingV1,
        node_public_key: &str,
    ) -> Result<(), String> {
        self.validate_shape()?;
        verify_paid_resource_world_reply(&self.preflight, preflight, node_public_key)?;
        verify_paid_resource_world_reply(&self.resource_final, resource_final, node_public_key)?;
        Ok(())
    }
}
