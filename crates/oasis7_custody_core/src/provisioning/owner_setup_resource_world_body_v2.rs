//! World receives only its own exact read originals, not Native MFA or CF SID metadata.
use super::*;
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OwnerSetupResourceWorldBodyV2 {
    api_version: u8,
    access: ws::participant_discovery::ParticipantOwnerViewRequestV1,
    resource: resource::SignedResourceReadV1<resource::OwnerSetupFinalResourceReadV2>,
}
impl OwnerSetupResourceWorldBodyV2 {
    pub(super) fn from_frame(f: &OwnerSetupResourceFinalHttpV2) -> Result<Self> {
        let EntryOwnerOperationV1::ReadResourceGoal(q) = &f.request.operation else {
            return Err(invalid());
        };
        Ok(Self {
            api_version: 2,
            access: f.request.access.clone(),
            resource: q.clone(),
        })
    }
    pub fn access(&self) -> &ws::participant_discovery::ParticipantOwnerViewRequestV1 {
        &self.access
    }
    pub fn resource(
        &self,
    ) -> &resource::SignedResourceReadV1<resource::OwnerSetupFinalResourceReadV2> {
        &self.resource
    }
    /// Actual local participant Admission/Owner/current/ledger checks are still
    /// mandatory at World; no caller key is accepted as a configured trust pin.
    pub fn validate_bindings(&self, p: &PaidResourceReadPayloadV1) -> Result<()> {
        Self::validate_preflight_bindings(&self.access, p)?;
        resource::verify_resource_goal_request(&self.resource).map_err(|_| invalid())?;
        let q = &self.resource.request;
        if self.api_version != 2
            || self.resource.subject_public_key != p.owner_public_key
            || q.setup_id != p.setup_id
            || q.provision_id != p.provision_id
            || q.plan_digest != p.plan_digest
            || q.world.world_id != p.world_audience.world_id
            || q.world.genesis_digest != p.world_audience.genesis_digest
            || q.expected_fence.owner_binding != p.owner_binding
            || self.access.read.request.scope_id != format!("agent:{}", q.expected_fence.agent_id)
            || p.selection_digest
                != digest(&("oasis7.custody.resource-selection.v1", &q.selection))?
            || p.task_fence_digest
                != digest(&("oasis7.custody.resource-task-fence.v1", &q.expected_fence))?
            || p.signer_record_digest
                != digest(&(
                    "oasis7.custody.resource-signer-record.v1",
                    &q.expected_signer_delegation,
                ))?
            || p.world_request_digest
                != paid_world_request_digest(resource::RESOURCE_GOAL_PATH_V2, &self.resource)?
        {
            return Err(invalid());
        }
        Ok(())
    }
    pub fn validate_preflight_bindings(
        access: &ws::participant_discovery::ParticipantOwnerViewRequestV1,
        p: &PaidResourceReadPayloadV1,
    ) -> Result<()> {
        p.validate_shape().map_err(|_| invalid())?;
        if p.stage != PaidResourceReadStageV1::OwnerSetupFinal
            || access.response_profile
                != Some(ws::participant_discovery::ParticipantOwnerViewProfileV1::OwnerSetupV1)
            || access.owner.subject_public_key != p.owner_public_key
            || access.owner.request.owner_public_key != p.owner_public_key
            || access.owner.request.account_id != p.participant_native_account_id
            || access.read.subject_public_key != p.owner_public_key
            || access.read.request.world.world_id != p.world_audience.world_id
            || access.read.request.world.genesis_digest != p.world_audience.genesis_digest
            || p.preflight_world_request_digest.as_deref()
                != Some(
                    paid_world_request_digest(
                        ws::participant_discovery::PARTICIPANT_OWNER_VIEW_PATH,
                        access,
                    )?
                    .as_str(),
                )
        {
            return Err(invalid());
        }
        ws::private_goal::verify_owner_signature_strict(
            oasis7_agent_api::game::GAME_PARTICIPANT_REGISTER_DOMAIN,
            &access.owner.request,
            &p.owner_public_key,
            &access.owner.signature_hex,
        )
        .map_err(|_| invalid())?;
        ws::private_goal::verify_owner_signature_strict(
            ws::VIEW_JSON_PATH,
            &access.read.request,
            &p.owner_public_key,
            &access.read.signature_hex,
        )
        .map_err(|_| invalid())?;
        Ok(())
    }
}
