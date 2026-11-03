use super::*;
use crate::world_service::{self as ws, private_goal::verify_owner_signature_strict};
use oasis7_agent_api::game::GameTaskFenceV1;
impl ResourceLedgerV1 {
    pub fn reference(&self) -> String {
        match self {
            Self::World => "world".into(),
            Self::Agent { agent_id } => format!("agent:{agent_id}"),
            Self::Site { site_id } => format!("site:{site_id}"),
            Self::Factory { factory_id } => format!("factory:{factory_id}"),
        }
    }
}
impl OwnerSetupFinalResourceReadV2 {
    pub fn validate(&self) -> Result<(), String> {
        self.selection.validate()?;
        self.expected_fence.validate()?;
        let w: ws::WorldIdentity = self.world.clone().into();
        w.validate().map_err(|_| no())?;
        if self.api_version != 2
            || ![&self.setup_id, &self.provision_id, &self.goal_reference]
                .iter()
                .all(|v| id(v))
            || !hex64(&self.plan_digest)
            || !hex64(&self.observation_nonce)
            || self.expected_fence.world_id != w.world_id
            || self.expected_fence.genesis_digest != w.genesis_digest
            || self
                .expected_fence
                .active_goal_reference
                .as_ref()
                .is_none_or(|r| r.goal_ref != self.goal_reference)
            || self.expected_fence.executor_binding.is_none()
        {
            return Err(no());
        }
        Ok(())
    }
}
impl OwnerResourceReadV1 {
    pub fn validate(&self) -> Result<(), String> {
        self.selection.validate()?;
        self.expected_fence.validate()?;
        let w: ws::WorldIdentity = self.world.clone().into();
        w.validate().map_err(|_| no())?;
        if self.api_version != 1
            || ![&self.session_id, &self.approval_operation_id]
                .iter()
                .all(|v| id(v))
            || !hex64(&self.prepared_resource_basis_digest)
            || !hex64(&self.observation_nonce)
            || self.expected_fence.world_id != w.world_id
            || self.expected_fence.genesis_digest != w.genesis_digest
            || self.expected_fence.executor_binding.is_none()
        {
            return Err(no());
        }
        Ok(())
    }
}
pub fn verify_resource_goal_request(
    q: &SignedResourceReadV1<OwnerSetupFinalResourceReadV2>,
) -> Result<(), String> {
    q.request.validate()?;
    if q.subject_public_key != q.request.expected_fence.owner_public_key {
        return Err(no());
    }
    verify_owner_signature_strict(
        RESOURCE_GOAL_PATH_V2,
        &q.request,
        &q.subject_public_key,
        &q.signature_hex,
    )
}
pub fn verify_owner_resource_request(
    q: &ParticipantOwnerResourceViewRequestV1,
) -> Result<(), String> {
    q.read.request.validate()?;
    use oasis7_agent_api::game::GAME_PARTICIPANT_REGISTER_DOMAIN;
    q.owner.request.validate()?;
    if q.admission.request.api_rev != 1
        || q.admission.request.policy_generation.0 == 0
        || q.admission.request.valid_from_tick.0 > q.admission.request.valid_until_tick.0
        || q.admission.request.business_digest
            != crate::world_service_correlation::participant_business_digest(&q.owner.request)?
    {
        return Err(no());
    }

    if q.owner.request.owner_public_key != q.owner.subject_public_key
        || q.admission.request.owner_public_key != q.owner.subject_public_key
        || q.owner.request.identity_namespace != q.admission.request.identity_namespace
        || q.owner.request.account_id != q.admission.request.account_id
        || q.owner.request.world_id != q.read.request.world.world_id
        || q.admission.request.world_id != q.read.request.world.world_id
        || q.owner.request.genesis_digest != q.read.request.world.genesis_digest
        || q.admission.request.genesis_digest != q.read.request.world.genesis_digest
        || q.admission.request.branch_id != q.read.request.expected_fence.branch_id
        || q.admission.request.reorg_epoch != q.read.request.expected_fence.reorg_epoch
    {
        return Err(no());
    }
    verify_owner_signature_strict(
        GAME_PARTICIPANT_REGISTER_DOMAIN,
        &q.owner.request,
        &q.owner.subject_public_key,
        &q.owner.signature_hex,
    )?;

    if q.read.subject_public_key != q.read.request.expected_fence.owner_public_key
        || q.owner.subject_public_key != q.read.subject_public_key
    {
        return Err(no());
    }
    verify_owner_signature_strict(
        OWNER_RESOURCE_VIEW_PATH_V1,
        &q.read.request,
        &q.read.subject_public_key,
        &q.read.signature_hex,
    )
}
fn service<Q: Serialize, T: Serialize + Clone>(
    path: &str,
    q: &Q,
    r: &SignedResourceResponseV1<T>,
    key: &str,
) -> Result<(), String> {
    let digest = ws::request_digest(path, q)?;
    if r.request_digest != digest {
        return Err(no());
    }
    ws::verify_service_response(
        path,
        &digest,
        &ws::SignedServiceResponse {
            request_digest: r.request_digest.clone(),
            payload: r.payload.clone(),
            signature_hex: r.signature_hex.clone(),
        },
        key,
    )
}
impl ResourceOwnerStateV1 {
    pub fn validate(
        &self,
        fence: &GameTaskFenceV1,
        key: &str,
        expected_signer: &ResourceSignerRecordV1,
    ) -> Result<(), String> {
        let commit: ws::CommitRef = self.read_commit.clone().into();
        commit.validate().map_err(|_| no())?;
        let owner = &self.owner;
        let actual = GameTaskFenceV1::from_control(&self.control);
        if self.schema_revision != 1
            || self.world != self.read_commit.world
            || actual != *fence
            || self.world.world_id != fence.world_id
            || self.world.genesis_digest != fence.genesis_digest
            || owner.agent_id != fence.agent_id
            || owner.player_id != fence.owner_binding
            || owner.public_key != key
            || owner.public_key != fence.owner_public_key
            || owner.world_id != fence.world_id
            || owner.branch_id != fence.branch_id
            || owner.reorg_epoch != fence.reorg_epoch
            || owner.agent_identity_generation != fence.owner_generation
            || self.read_commit.binding.provider_world_id != fence.world_id
            || self.read_commit.binding.branch_id != fence.branch_id
            || self.read_commit.binding.reorg_generation != fence.reorg_epoch
        {
            return Err(no());
        }
        if self.signer_delegation.as_ref() != Some(expected_signer) {
            return Err(no());
        }
        let executor = fence.executor_binding.as_ref().ok_or_else(no)?;
        let signer = self.signer_delegation.as_ref().ok_or_else(no)?;
        if signer.world_id != fence.world_id
            || signer.branch_id != fence.branch_id
            || signer.agent_id != fence.agent_id
            || signer.owner_binding != fence.owner_binding
            || signer.agent_identity_generation != fence.owner_generation
            || signer.revoked
            || signer.generation != executor.delegation_generation
            || signer.delegate_public_key != executor.executor_public_key
            || signer.nonce.0 == 0
        {
            return Err(no());
        }
        Ok(())
    }
}
pub fn verify_resource_goal_response(
    q: &SignedResourceReadV1<OwnerSetupFinalResourceReadV2>,
    r: &SignedResourceResponseV1<OwnerSetupFinalResourceObservationV2>,
    key: &str,
) -> Result<(), String> {
    verify_resource_goal_request(q)?;
    service(RESOURCE_GOAL_PATH_V2, q, r, key)?;
    let p = &r.payload;
    p.owner.validate(
        &q.request.expected_fence,
        &q.subject_public_key,
        &q.request.expected_signer_delegation,
    )?;
    if p.api_version != 2
        || p.selection != q.request.selection
        || p.current_goal.read_commit != p.owner.read_commit
        || p.current_goal.control != p.owner.control
    {
        return Err(no());
    }
    let current: ws::private_goal_read::CurrentGoalResponse = p.current_goal.clone().into();
    ws::private_goal_read::verify_current_payload(
        &ws::private_goal_read::CurrentGoalRequest {
            schema_revision: 1,
            world: q.request.world.clone().into(),
            agent_id: q.request.expected_fence.agent_id.clone(),
            task_id: q.request.expected_fence.task_id.clone(),
        },
        &q.subject_public_key,
        &current,
    )?;
    if current
        .active_goal
        .as_ref()
        .is_none_or(|g| g.goal_ref != q.request.goal_reference)
    {
        return Err(no());
    }
    validate_resolution(&p.resource, &p.selection, &p.owner)
}
pub fn verify_owner_resource_response(
    q: &ParticipantOwnerResourceViewRequestV1,
    r: &SignedResourceResponseV1<ParticipantOwnerResourceObservationV1>,
    key: &str,
) -> Result<(), String> {
    verify_owner_resource_request(q)?;
    service(OWNER_RESOURCE_VIEW_PATH_V1, q, r, key)?;
    let p = &r.payload;
    p.owner.validate(
        &q.read.request.expected_fence,
        &q.read.subject_public_key,
        &q.read.request.expected_signer_delegation,
    )?;
    if p.api_version != 1 || p.selection != q.read.request.selection {
        return Err(no());
    }
    validate_resolution(&p.resource, &p.selection, &p.owner)
}
fn validate_resolution(
    v: &ResourceBasisResolutionV1,
    selection: &ResourceBasisSelectionV1,
    owner: &ResourceOwnerStateV1,
) -> Result<(), String> {
    match v {
        ResourceBasisResolutionV1::Blocked { .. } => Ok(()),
        ResourceBasisResolutionV1::Ready { fact } => fact.validate(selection, owner),
    }
}
