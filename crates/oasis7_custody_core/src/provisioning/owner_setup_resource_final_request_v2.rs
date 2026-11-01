//! Dedicated Resource final signing claim. No V1 Goal original or public bytes
//! signer is used; the Account SQL caller must commit this claim before I/O.
use super::*;
use oasis7_client_api::world_service::resource_basis as resource;
pub struct PreparedOwnerSetupResourceReadV2 {
    request: resource::OwnerSetupFinalResourceReadV2,
    owner_public_key: String,
    source_record_digest: String,
    valid_until: u64,
    observation: ws::ReadWorldViewRequest,
    participant_access: ws::participant_discovery::ParticipantOwnerViewRequestV1,
}
#[derive(Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OwnerSetupResourceCandidateV2 {
    pub original: resource::SignedResourceReadV1<resource::OwnerSetupFinalResourceReadV2>,
    pub observation: ws::participant_discovery::ParticipantOwnerViewRequestV1,
}
impl PreparedOwnerSetupResourceReadV2 {
    pub fn request(&self) -> &resource::OwnerSetupFinalResourceReadV2 {
        &self.request
    }
    pub fn owner_public_key(&self) -> &str {
        &self.owner_public_key
    }
    pub fn source_record_digest(&self) -> &str {
        &self.source_record_digest
    }
    pub fn valid_until(&self) -> u64 {
        self.valid_until
    }
    pub fn observation(&self) -> &ws::ReadWorldViewRequest {
        &self.observation
    }
    pub fn participant_access(&self) -> &ws::participant_discovery::ParticipantOwnerViewRequestV1 {
        &self.participant_access
    }
}
impl AccountProvisionApprovalState {
    pub fn claim_owner_setup_resource_read_v2(
        &mut self,
        id: &str,
        current: &OwnerSetupCurrent<'_, '_>,
        now: u64,
        verifier: &mut impl ApprovalSignatureVerifier,
    ) -> Result<PreparedOwnerSetupResourceReadV2> {
        self.validate_persistence()?;
        verifier.verify(&self.owner_setup_access_current_checks(
            id,
            OwnerSetupStageV1::FinalObservation,
            current,
            now,
        )?)?;
        let v = self.owner_setup_v2.as_ref().ok_or_else(invalid)?;
        let plan = &v.plan;
        if plan.setup_id != id
            || plan.selection.resource().is_none()
            || v.book
                .original(id)
                .is_none_or(|r| r.stage() != ApprovalAdmissionStage::Consumed)
        {
            return Err(invalid());
        }
        let outbox = v.resource_final.as_ref().ok_or_else(invalid)?;
        if outbox.signing_claimed_at.is_some()
            || outbox.unsigned_request_bytes.is_some()
            || outbox.original_bytes.is_some()
            || outbox.result.is_some()
        {
            return Err(ProvisionApprovalError::Conflict);
        }
        let common = &outbox.common;
        if common.oracle_reads.checked_add(1) != Some(OWNER_SETUP_MAX_ORACLE_READS) {
            return Err(invalid());
        }
        let expected = self
            .owner_setup_expected_control(id, OwnerSetupStageV1::FinalObservation)?
            .ok_or_else(invalid)?;
        let task = &expected;
        self.owner_setup_active_goal(id, task)?;
        let phase = self.owner_setup_engine_record(id)?;
        let Some(OwnerSetupOriginalV1::Canonical { submit, .. }) = phase
            .stages
            .get(&OwnerSetupStageV1::Delegation)
            .and_then(|o| o.original.as_ref())
        else {
            return Err(invalid());
        };
        let WorldServicePayloadV1::Delegation(original) = &submit.signed_payload else {
            return Err(invalid());
        };
        let delegation = &original.request;
        self.owner_setup_delegation(id, Some(delegation))?;
        let signing_window = ResourceFinalCurrentWindowV2::capture(current, now);
        let valid_until = signing_window.deadline(plan, &common.source)?;
        if now >= valid_until {
            return Err(invalid());
        }
        let request = resource::OwnerSetupFinalResourceReadV2 {
            api_version: 2,
            setup_id: plan.setup_id.clone(),
            provision_id: plan.provision_id.clone(),
            plan_digest: plan.digest().map_err(|_| invalid())?,
            world: resource::ResourceWorldV1 {
                world_id: plan.world_identity.world_id.clone(),
                genesis_digest: plan.world_identity.genesis_digest.clone(),
            },
            expected_fence: oasis7_agent_api::game::GameTaskFenceV1::from_control(task),
            expected_signer_delegation: resource::ResourceSignerRecordV1 {
                world_id: delegation.world_id.clone(),
                branch_id: delegation.branch_id.clone(),
                agent_id: delegation.agent_id.clone(),
                owner_binding: delegation.owner_binding.clone(),
                agent_identity_generation: DecimalU64(delegation.agent_identity_generation),
                generation: DecimalU64(delegation.generation),
                delegate_public_key: delegation.delegate_public_key.clone(),
                revoked: delegation.revoked,
                nonce: DecimalU64(delegation.nonce),
            },
            goal_reference: task
                .active_goal_reference
                .as_ref()
                .ok_or_else(invalid)?
                .goal_ref
                .clone(),
            selection: plan.selection.resource().ok_or_else(invalid)?.clone(),
            observation_nonce: digest(&(
                "oasis7.custody.owner-setup-resource-observation-nonce.v2",
                plan.digest().map_err(|_| invalid())?,
                &common.session_nonce,
            ))?,
        };
        request.validate().map_err(|_| invalid())?;
        let authority = PreparedOwnerSetupResourceReadV2 {
            owner_public_key: request.expected_fence.owner_public_key.clone(),
            source_record_digest: digest(&common.source)?,
            request: request.clone(),
            valid_until,
            observation: self.owner_setup_read_request(id, OwnerSetupStageV1::FinalObservation)?,
            participant_access: self
                .owner_setup_stage_access(id, OwnerSetupStageV1::FinalObservation)?,
        };
        let mut next = self.clone();
        let saved = next
            .owner_setup_v2
            .as_mut()
            .ok_or_else(invalid)?
            .resource_final
            .as_mut()
            .ok_or_else(invalid)?;
        saved.unsigned_request_bytes = Some(encode(&request)?);
        saved.signing_claimed_at = Some(DecimalU64(now));
        saved.signing_window = Some(signing_window);
        next.validate_persistence()?;
        *self = next;
        Ok(authority)
    }
    pub fn publish_owner_setup_resource_read_v2(
        &mut self,
        id: &str,
        candidate: OwnerSetupResourceCandidateV2,
        current: &OwnerSetupCurrent<'_, '_>,
        now: u64,
        verifier: &mut impl ApprovalSignatureVerifier,
    ) -> Result<()> {
        self.validate_persistence()?;
        verifier.verify(&self.owner_setup_access_current_checks(
            id,
            OwnerSetupStageV1::FinalObservation,
            current,
            now,
        )?)?;
        let v = self.owner_setup_v2.as_ref().ok_or_else(invalid)?;
        if v.plan.setup_id != id || v.plan.selection.resource().is_none() {
            return Err(invalid());
        }
        let saved = v.resource_final.as_ref().ok_or_else(invalid)?;
        if saved.common.oracle_reads != OWNER_SETUP_MAX_ORACLE_READS {
            return Err(invalid());
        }
        let at = saved.signing_claimed_at.ok_or_else(invalid)?.0;
        let published_window = ResourceFinalCurrentWindowV2::capture(current, now);
        let until = saved
            .signing_window
            .as_ref()
            .ok_or_else(invalid)?
            .deadline(&v.plan, &saved.common.source)?
            .min(published_window.deadline(&v.plan, &saved.common.source)?);
        if now < at || now >= until {
            return Err(invalid());
        }
        let expected: resource::OwnerSetupFinalResourceReadV2 =
            decode(saved.unsigned_request_bytes.as_ref().ok_or_else(invalid)?)?;
        let signed = &candidate.original;
        if signed.request != expected {
            return Err(ProvisionApprovalError::Conflict);
        }
        resource::verify_resource_goal_request(signed).map_err(|_| invalid())?;
        let expected_access =
            self.owner_setup_stage_access(id, OwnerSetupStageV1::FinalObservation)?;
        let key = &self
            .provision
            .role_key(Role::Owner)
            .ok_or_else(invalid)?
            .public_key;
        if candidate.observation.owner != expected_access.owner
            || candidate.observation.admission != expected_access.admission
            || candidate.observation.read.subject_public_key != *key
            || candidate.observation.read.request
                != self.owner_setup_read_request(id, OwnerSetupStageV1::FinalObservation)?
            || candidate.observation.response_profile
                != Some(ws::participant_discovery::ParticipantOwnerViewProfileV1::OwnerSetupV1)
        {
            return Err(invalid());
        }
        verifier.verify(&append_owner_setup_checks(
            self.owner_setup_access_current_checks(
                id,
                OwnerSetupStageV1::FinalObservation,
                current,
                now,
            )?,
            OwnerSetupSignatureFact::Read(&candidate.observation, key),
        )?)?;
        let bytes = encode(signed)?;
        if let Some(old) = &saved.original_bytes {
            return if *old == bytes
                && saved.common.observation.as_ref() == Some(&candidate.observation)
            {
                Ok(())
            } else {
                Err(ProvisionApprovalError::Conflict)
            };
        }
        let mut next = self.clone();
        let saved = next
            .owner_setup_v2
            .as_mut()
            .ok_or_else(invalid)?
            .resource_final
            .as_mut()
            .ok_or_else(invalid)?;
        saved.original_bytes = Some(bytes);
        saved.published_at = Some(DecimalU64(now));
        saved.published_window = Some(published_window);
        saved.common.published_at = Some(DecimalU64(now));
        saved.common.observation_bytes =
            Some(serde_json::to_string(&candidate.observation).map_err(|_| invalid())?);
        saved.common.observation = Some(candidate.observation);
        next.validate_persistence()?;
        *self = next;
        Ok(())
    }
}
