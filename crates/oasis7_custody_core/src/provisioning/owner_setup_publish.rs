use super::*;
use crate::operations::OperationState;
impl AccountProvisionApprovalState {
    pub fn owner_setup_publish_checks(
        &self,
        id: &str,
        stage: OwnerSetupStageV1,
        candidate: &OwnerSetupCandidateV1,
        current: &OwnerSetupCurrent<'_, '_>,
        now: u64,
    ) -> Result<ExactSignatureChecks> {
        let bundle = self.owner_setup_signing_request(id, stage, current, now)?;
        self.validate_owner_setup_candidate(id, candidate, &bundle)?;
        let key = &self
            .provision
            .role_key(Role::Owner)
            .ok_or_else(invalid)?
            .public_key;
        let checks = append_owner_setup_checks(
            self.owner_setup_access_current_checks(id, stage, current, now)?,
            OwnerSetupSignatureFact::Original(&candidate.original, key),
        )?;
        Ok(append_owner_setup_checks(
            checks,
            OwnerSetupSignatureFact::Read(&candidate.observation, key),
        )?)
    }
    pub fn publish_owner_setup_original(
        &mut self,
        id: &str,
        read: OwnerSetupReadTicket,
        candidate: OwnerSetupCandidateV1,
        current: &OwnerSetupCurrent<'_, '_>,
        now: u64,
        verifier: &mut impl ApprovalSignatureVerifier,
    ) -> Result<OwnerSetupOutboxV1> {
        verifier
            .verify(&self.owner_setup_publish_checks(id, read.stage, &candidate, current, now)?)?;
        let mut next = self.clone();
        let o = next
            .owner_setups
            .get_mut(id)
            .and_then(|v| v.stages.get_mut(&read.stage))
            .ok_or_else(invalid)?;
        read.validate_role(Role::Owner)?;
        if read.ticket != OWNER_SETUP_MAX_ORACLE_READS || read.ticket > o.oracle_reads {
            return Err(invalid());
        }
        if let Some(old) = &o.original {
            return if old == &candidate.original
                && o.observation.as_ref() == Some(&candidate.observation)
            {
                Ok(o.clone())
            } else {
                Err(ProvisionApprovalError::Conflict)
            };
        }
        let bytes = serde_json::to_string(&candidate).map_err(|_| invalid())?;
        o.operation
            .prepare(bytes.as_bytes().to_vec())
            .map_err(|_| invalid())?;
        o.original_bytes = Some(serde_json::to_string(&candidate.original).map_err(|_| invalid())?);
        o.observation_bytes =
            Some(serde_json::to_string(&candidate.observation).map_err(|_| invalid())?);
        o.candidate_bytes = Some(bytes);
        o.original = Some(candidate.original);
        o.observation = Some(candidate.observation);
        o.published_at = Some(DecimalU64(now));
        let result = o.clone();
        next.validate_persistence()?;
        *self = next;
        Ok(result)
    }
    pub(super) fn validate_owner_setup_candidate(
        &self,
        id: &str,
        candidate: &OwnerSetupCandidateV1,
        expected: &OwnerSetupSigningBundleV1,
    ) -> Result<()> {
        self.validate_owner_setup_original(id, &candidate.original, &expected.action)?;
        let key = &self
            .provision
            .role_key(Role::Owner)
            .ok_or_else(invalid)?
            .public_key;
        let participant = self
            .participant_provisions
            .values()
            .find(|p| p.result.is_some())
            .ok_or_else(invalid)?;
        let o = &candidate.observation;
        if o.read.subject_public_key != *key
            || o.read.request != expected.observation
            || o.owner != participant.bundle.as_ref().ok_or_else(invalid)?.owner
            || o.admission != expected.participant_access.admission
            || o.response_profile
                != Some(ws::participant_discovery::ParticipantOwnerViewProfileV1::OwnerSetupV1)
        {
            return Err(invalid());
        }
        validate_owner_setup_signature_fact(OwnerSetupSignatureFact::Read(o, key))?;
        Ok(())
    }
    pub fn claim_owner_setup_send(
        &mut self,
        id: &str,
        stage: OwnerSetupStageV1,
        current: &OwnerSetupCurrent<'_, '_>,
        now: u64,
        verifier: &mut impl ApprovalSignatureVerifier,
    ) -> Result<OwnerSetupOutboxV1> {
        verifier.verify(&self.owner_setup_stored_admission_checks(id, stage, current, now)?)?;
        let mut next = self.clone();
        let o = next
            .owner_setups
            .get_mut(id)
            .and_then(|v| v.stages.get_mut(&stage))
            .ok_or_else(invalid)?;
        if o.original.is_none() || o.result.is_some() {
            return Err(invalid());
        }
        o.operation
            .claim(current.provision.approval.authority.security_version.0)
            .map_err(|_| invalid())?;
        o.claimed_at = Some(DecimalU64(now));
        let result = o.clone();
        next.validate_persistence()?;
        *self = next;
        Ok(result)
    }
    /// Local once-only lookup reservation precedes all remote observation I/O.
    pub fn reserve_owner_setup_lookup(
        &mut self,
        id: &str,
        stage: OwnerSetupStageV1,
        now: u64,
    ) -> Result<()> {
        self.owner_setup_local(id, now)?;
        let original = self.owner_setup_dispatched(id, stage)?;
        if original.lookup_reserved_at.is_some()
            || original.result.is_some()
            || !matches!(
                original.original,
                Some(OwnerSetupOriginalV1::Canonical { .. })
            )
        {
            return Err(invalid());
        }
        let mut next = self.clone();
        next.owner_setups
            .get_mut(id)
            .and_then(|v| v.stages.get_mut(&stage))
            .ok_or_else(invalid)?
            .lookup_reserved_at = Some(DecimalU64(now));
        next.validate_persistence()?;
        *self = next;
        Ok(())
    }
    pub fn claim_owner_setup_lookup(
        &mut self,
        id: &str,
        stage: OwnerSetupStageV1,
        current: &OwnerSetupCurrent<'_, '_>,
        now: u64,
        verifier: &mut impl ApprovalSignatureVerifier,
    ) -> Result<OwnerSetupOutboxV1> {
        verifier.verify(&self.owner_setup_stored_admission_checks(id, stage, current, now)?)?;
        let existing = self.owner_setup_dispatched(id, stage)?;
        if !matches!(
            existing.original,
            Some(OwnerSetupOriginalV1::Canonical { .. })
        ) || existing.lookup_reserved_at.is_none()
            || existing.lookup_claimed_at.is_some()
            || existing.result.is_some()
        {
            return Err(invalid());
        }
        let mut next = self.clone();
        let o = next
            .owner_setups
            .get_mut(id)
            .and_then(|v| v.stages.get_mut(&stage))
            .ok_or_else(invalid)?;
        o.lookup_claimed_at = Some(DecimalU64(now));
        let result = o.clone();
        next.validate_persistence()?;
        *self = next;
        Ok(result)
    }
    pub fn mark_owner_setup_unknown(&mut self, id: &str, stage: OwnerSetupStageV1) -> Result<()> {
        let mut next = self.clone();
        next.owner_setups
            .get_mut(id)
            .and_then(|v| v.stages.get_mut(&stage))
            .ok_or_else(invalid)?
            .operation
            .mark_unknown()
            .map_err(|_| invalid())?;
        next.validate_persistence()?;
        *self = next;
        Ok(())
    }
    pub(super) fn validate_owner_setup_original(
        &self,
        id: &str,
        original: &OwnerSetupOriginalV1,
        expected: &OwnerSetupSigningRequestV1,
    ) -> Result<()> {
        let v = self.owner_setups.get(id).ok_or_else(invalid)?;
        let key = &self
            .provision
            .role_key(Role::Owner)
            .ok_or_else(invalid)?
            .public_key;
        match (original, expected) {
            (OwnerSetupOriginalV1::Canonical { submit, lookup }, expected) => {
                let match_request = match (&submit.signed_payload, expected) {
                    (
                        WorldServicePayloadV1::GameTaskCreate(s),
                        OwnerSetupSigningRequestV1::CreateTask(r),
                    ) => s.subject_public_key == *key && s.request == *r,
                    (
                        WorldServicePayloadV1::GameGoalPublish(s),
                        OwnerSetupSigningRequestV1::PublishGoal(r),
                    ) => s.subject_public_key == *key && s.request == *r,
                    (
                        WorldServicePayloadV1::Delegation(s),
                        OwnerSetupSigningRequestV1::Delegation(r),
                    ) => s.subject_public_key == *key && s.request == *r,
                    (
                        WorldServicePayloadV1::GameExecutorControl(s),
                        OwnerSetupSigningRequestV1::Bind(r),
                    ) => s.subject_public_key == *key && s.request == *r,
                    _ => false,
                };
                let world = ws::WorldIdentity {
                    world_id: v.plan.world_identity.world_id.clone(),
                    genesis_digest: v.plan.world_identity.genesis_digest.clone(),
                };
                let correlation = oasis7_client_api::world_service_correlation::derive_correlation(
                    world,
                    &submit.signed_payload,
                )
                .map_err(|_| invalid())?;
                submit.validate().map_err(|_| invalid())?;
                if !match_request
                    || submit.correlation != correlation
                    || submit.deadline_unix_ms != Some(v.plan.deadline.0)
                    || lookup.original != submit.signed_payload
                    || lookup.request.contract_version != 1
                    || lookup.request.key != correlation.key
                {
                    return Err(invalid());
                }
            }
            (OwnerSetupOriginalV1::PrepareGoal(s), OwnerSetupSigningRequestV1::PrepareGoal(r)) => {
                if s.subject_public_key != *key || s.request != *r {
                    return Err(invalid());
                }
                s.verify().map_err(|_| invalid())?;
            }
            (
                OwnerSetupOriginalV1::FinalObservation { owner, goal },
                OwnerSetupSigningRequestV1::FinalObservation {
                    owner: read,
                    goal: request,
                },
            ) => {
                let participant = self
                    .participant_provisions
                    .values()
                    .find(|p| p.result.is_some())
                    .ok_or_else(invalid)?;
                if owner.read.subject_public_key != *key
                    || owner.read.request != **read
                    || goal.subject_public_key != *key
                    || goal.request != *request
                    || owner.owner != participant.bundle.as_ref().ok_or_else(invalid)?.owner
                    || owner.admission
                        != self
                            .owner_setup_stage_access(id, OwnerSetupStageV1::FinalObservation)?
                            .admission
                    || owner.response_profile
                        != Some(
                            ws::participant_discovery::ParticipantOwnerViewProfileV1::OwnerSetupV1,
                        )
                {
                    return Err(invalid());
                }
            }
            _ => return Err(invalid()),
        }
        Ok(())
    }
    pub(super) fn owner_setup_dispatched(
        &self,
        id: &str,
        stage: OwnerSetupStageV1,
    ) -> Result<&OwnerSetupOutboxV1> {
        let o = self
            .owner_setups
            .get(id)
            .and_then(|v| v.stages.get(&stage))
            .ok_or_else(invalid)?;
        if !matches!(
            o.operation.state(),
            OperationState::Dispatching | OperationState::Unknown | OperationState::Confirmed
        ) {
            return Err(invalid());
        }
        Ok(o)
    }
}
