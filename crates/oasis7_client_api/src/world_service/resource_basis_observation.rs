use super::*;
use crate::world_service::{self as ws, private_goal::PreparedGoalReceiptV1};
use oasis7_agent_api::game::{
    GameParticipantAdmissionV1, GameParticipantRegisterV1, GameTaskControlV1, GameTaskFenceV1,
};
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ResourceWorldV1 {
    pub world_id: String,
    pub genesis_digest: String,
}
impl From<&ws::WorldIdentity> for ResourceWorldV1 {
    fn from(v: &ws::WorldIdentity) -> Self {
        Self {
            world_id: v.world_id.clone(),
            genesis_digest: v.genesis_digest.clone(),
        }
    }
}
impl From<ResourceWorldV1> for ws::WorldIdentity {
    fn from(v: ResourceWorldV1) -> Self {
        Self {
            world_id: v.world_id,
            genesis_digest: v.genesis_digest,
        }
    }
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ResourceExecutionBindingV1 {
    pub provider_world_id: String,
    pub branch_id: String,
    pub finality_ref: String,
    pub reorg_generation: DecimalU64,
    pub governing_manifest_ref: String,
    pub authority_generation: DecimalU64,
    pub permission_generation: DecimalU64,
}
impl From<&ws::ExecutionBinding> for ResourceExecutionBindingV1 {
    fn from(v: &ws::ExecutionBinding) -> Self {
        Self {
            provider_world_id: v.provider_world_id.clone(),
            branch_id: v.branch_id.clone(),
            finality_ref: v.finality_ref.clone(),
            reorg_generation: DecimalU64(v.reorg_generation),
            governing_manifest_ref: v.governing_manifest_ref.clone(),
            authority_generation: DecimalU64(v.authority_generation),
            permission_generation: DecimalU64(v.permission_generation),
        }
    }
}
impl From<ResourceExecutionBindingV1> for ws::ExecutionBinding {
    fn from(v: ResourceExecutionBindingV1) -> Self {
        Self {
            provider_world_id: v.provider_world_id,
            branch_id: v.branch_id,
            finality_ref: v.finality_ref,
            reorg_generation: v.reorg_generation.0,
            governing_manifest_ref: v.governing_manifest_ref,
            authority_generation: v.authority_generation.0,
            permission_generation: v.permission_generation.0,
        }
    }
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ResourceCommitRefV1 {
    pub world: ResourceWorldV1,
    pub binding: ResourceExecutionBindingV1,
    pub position: DecimalU64,
    pub execution_block_hash: String,
    pub state_root_ref: String,
}
impl From<&ws::CommitRef> for ResourceCommitRefV1 {
    fn from(v: &ws::CommitRef) -> Self {
        Self {
            world: (&v.world).into(),
            binding: (&v.binding).into(),
            position: DecimalU64(v.position),
            execution_block_hash: v.execution_block_hash.clone(),
            state_root_ref: v.state_root_ref.clone(),
        }
    }
}
impl From<ResourceCommitRefV1> for ws::CommitRef {
    fn from(v: ResourceCommitRefV1) -> Self {
        Self {
            world: v.world.into(),
            binding: v.binding.into(),
            position: v.position.0,
            execution_block_hash: v.execution_block_hash,
            state_root_ref: v.state_root_ref,
        }
    }
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ResourceOwnerFenceV1 {
    pub agent_id: String,
    pub player_id: String,
    pub public_key: String,
    pub world_id: String,
    pub reorg_epoch: DecimalU64,
    pub branch_id: String,
    pub agent_identity_generation: DecimalU64,
    #[serde(deserialize_with = "required_option")]
    pub current_intent_id: Option<String>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ResourceSignerRecordV1 {
    pub world_id: String,
    pub branch_id: String,
    pub agent_id: String,
    pub owner_binding: String,
    pub agent_identity_generation: DecimalU64,
    pub generation: DecimalU64,
    pub delegate_public_key: String,
    pub revoked: bool,
    pub nonce: DecimalU64,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ResourceOwnerStateV1 {
    pub schema_revision: u32,
    pub world: ResourceWorldV1,
    pub read_commit: ResourceCommitRefV1,
    pub logical_tick: DecimalU64,
    pub owner: ResourceOwnerFenceV1,
    pub control: GameTaskControlV1,
    #[serde(deserialize_with = "required_option")]
    pub signer_delegation: Option<ResourceSignerRecordV1>,
}
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ResourceCurrentGoalV2 {
    pub schema_revision: u32,
    pub control: GameTaskControlV1,
    pub read_commit: ResourceCommitRefV1,
    #[serde(deserialize_with = "required_option")]
    pub activation_commit: Option<ResourceCommitRefV1>,
    #[serde(deserialize_with = "required_option")]
    pub active_goal: Option<PreparedGoalReceiptV1>,
}
impl From<&ws::private_goal_read::CurrentGoalResponse> for ResourceCurrentGoalV2 {
    fn from(v: &ws::private_goal_read::CurrentGoalResponse) -> Self {
        Self {
            schema_revision: v.schema_revision,
            control: v.control.clone(),
            read_commit: (&v.read_commit).into(),
            activation_commit: v.activation_commit.as_ref().map(Into::into),
            active_goal: v.active_goal.clone(),
        }
    }
}
impl From<ResourceCurrentGoalV2> for ws::private_goal_read::CurrentGoalResponse {
    fn from(v: ResourceCurrentGoalV2) -> Self {
        Self {
            schema_revision: v.schema_revision,
            control: v.control,
            read_commit: v.read_commit.into(),
            activation_commit: v.activation_commit.map(Into::into),
            active_goal: v.active_goal,
        }
    }
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SignedResourceReadV1<T> {
    pub request: T,
    pub subject_public_key: String,
    pub signature_hex: String,
}
impl<T: Clone> SignedResourceReadV1<T> {
    pub fn legacy(&self) -> ws::SignedReadRequest<T> {
        ws::SignedReadRequest {
            request: self.request.clone(),
            subject_public_key: self.subject_public_key.clone(),
            signature_hex: self.signature_hex.clone(),
        }
    }
}
impl<T> From<ws::SignedReadRequest<T>> for SignedResourceReadV1<T> {
    fn from(v: ws::SignedReadRequest<T>) -> Self {
        Self {
            request: v.request,
            subject_public_key: v.subject_public_key,
            signature_hex: v.signature_hex,
        }
    }
}
#[derive(Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SignedResourceResponseV1<T> {
    pub request_digest: String,
    pub payload: T,
    pub signature_hex: String,
}
impl<T> From<ws::SignedServiceResponse<T>> for SignedResourceResponseV1<T> {
    fn from(v: ws::SignedServiceResponse<T>) -> Self {
        Self {
            request_digest: v.request_digest,
            payload: v.payload,
            signature_hex: v.signature_hex,
        }
    }
}
#[derive(Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NativeResourceEnvelopeV1<T> {
    pub api_version: u8,
    pub ok: bool,
    pub world_response: SignedResourceResponseV1<T>,
}
pub fn encode_native_resource_envelope<T: Serialize>(
    response: SignedResourceResponseV1<T>,
) -> Result<Vec<u8>, String> {
    let bytes = serde_json::to_vec(&NativeResourceEnvelopeV1 {
        api_version: 2,
        ok: true,
        world_response: response,
    })
    .map_err(|_| no())?;
    if bytes.len() > MAX_RESOURCE_ENVELOPE_BYTES {
        return Err(no());
    }
    Ok(bytes)
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OwnerSetupFinalResourceReadV2 {
    pub api_version: u8,
    pub setup_id: String,
    pub provision_id: String,
    pub plan_digest: String,
    pub world: ResourceWorldV1,
    pub expected_fence: GameTaskFenceV1,
    pub expected_signer_delegation: ResourceSignerRecordV1,
    pub goal_reference: String,
    pub selection: ResourceBasisSelectionV1,
    pub observation_nonce: String,
}
#[derive(Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OwnerSetupFinalResourceObservationV2 {
    pub api_version: u8,
    pub owner: ResourceOwnerStateV1,
    pub current_goal: ResourceCurrentGoalV2,
    pub selection: ResourceBasisSelectionV1,
    pub resource: ResourceBasisResolutionV1,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OwnerResourceReadV1 {
    pub api_version: u8,
    pub world: ResourceWorldV1,
    pub expected_fence: GameTaskFenceV1,
    pub expected_signer_delegation: ResourceSignerRecordV1,
    pub session_id: String,
    pub approval_operation_id: String,
    pub prepared_resource_basis_digest: String,
    pub selection: ResourceBasisSelectionV1,
    pub observation_nonce: String,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ParticipantOwnerResourceViewRequestV1 {
    pub owner: SignedResourceReadV1<GameParticipantRegisterV1>,
    pub admission: SignedResourceReadV1<GameParticipantAdmissionV1>,
    pub read: SignedResourceReadV1<OwnerResourceReadV1>,
}
#[derive(Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ParticipantOwnerResourceObservationV1 {
    pub api_version: u8,
    pub owner: ResourceOwnerStateV1,
    pub selection: ResourceBasisSelectionV1,
    pub resource: ResourceBasisResolutionV1,
}
