use super::*;
impl AccountProvisionApprovalState {
    pub fn reserve_owner_setup_admission_callback(
        &mut self,
        id: &str,
        stage: OwnerSetupStageV1,
        now: u64,
    ) -> Result<()> {
        self.owner_setup_local(id, now)?;
        let mut next = self.clone();
        let o = next
            .owner_setups
            .get_mut(id)
            .and_then(|v| v.stages.get_mut(&stage))
            .ok_or_else(invalid)?;
        if o.admission_callback_claimed_at.is_some() || o.original.is_none() || o.result.is_some() {
            return Err(invalid());
        }
        o.admission_callback_claimed_at = Some(DecimalU64(now));
        next.validate_persistence()?;
        *self = next;
        Ok(())
    }
    pub fn owner_setup_admission_checks(
        &self,
        id: &str,
        stage: OwnerSetupStageV1,
        evidence: &OwnerSetupAdmissionEvidenceV1,
        current: &OwnerSetupCurrent<'_, '_>,
        now: u64,
    ) -> Result<ExactSignatureChecks> {
        let checks = self.owner_setup_access_current_checks(id, stage, current, now)?;
        let o = self.owner_setup_outbox(id, stage).ok_or_else(invalid)?;
        if o.admission_callback_claimed_at.is_none() {
            return Err(invalid());
        }
        self.validate_owner_setup_world_admission(
            id,
            stage,
            evidence,
            now,
            current.provision.approval.authority.service_epoch,
        )?;
        let setup = self.owner_setups.get(id).ok_or_else(invalid)?;
        let checks = append_owner_setup_checks(
            checks,
            OwnerSetupSignatureFact::World(
                &evidence.world,
                &setup.plan.world_identity.service_public_key,
            ),
        )?;
        Ok(append_native_history_checks(
            checks,
            current.provision.approval,
            NativeFact::Status(&evidence.session.status),
            current.native_scope.native_root_key_id,
            current.native_scope.native_root_public_key,
            &digest(&("oasis7.custody.owner-setup-admission.v1", id, stage))?,
        )?)
    }
    pub fn record_owner_setup_admission(
        &mut self,
        id: &str,
        stage: OwnerSetupStageV1,
        evidence: OwnerSetupAdmissionEvidenceV1,
        current: &OwnerSetupCurrent<'_, '_>,
        now: u64,
        verifier: &mut impl ApprovalSignatureVerifier,
    ) -> Result<OwnerSetupOutboxV1> {
        verifier.verify(&self.owner_setup_admission_checks(id, stage, &evidence, current, now)?)?;
        let mut next = self.clone();
        let o = next
            .owner_setups
            .get_mut(id)
            .and_then(|v| v.stages.get_mut(&stage))
            .ok_or_else(invalid)?;
        if o.world.is_some() {
            return if o.world.as_ref() == Some(&evidence.world)
                && o.admission_session.as_ref() == Some(&evidence.session)
            {
                Ok(o.clone())
            } else {
                Err(ProvisionApprovalError::Conflict)
            };
        }
        o.world = Some(evidence.world);
        o.admission_session = Some(evidence.session);
        o.admitted_at = Some(DecimalU64(now));
        let result = o.clone();
        if stage == OwnerSetupStageV1::CreateTask
            && next.provision.stage() == ProvisionStage::ParticipantBound
        {
            let ready = next.owner_setup_ready_phase(id)?;
            next.provision.confirm_owner_setup_ready(ready)?;
        }
        next.validate_persistence()?;
        *self = next;
        Ok(result)
    }
    pub(super) fn owner_setup_ready_phase(&self, id: &str) -> Result<PhaseOriginal> {
        let o = self
            .owner_setup_outbox(id, OwnerSetupStageV1::CreateTask)
            .ok_or_else(invalid)?;
        let evidence = OwnerSetupAdmissionEvidenceV1 {
            world: o.world.clone().ok_or_else(invalid)?,
            session: o.admission_session.clone().ok_or_else(invalid)?,
        };
        let original = o.original.as_ref().ok_or_else(invalid)?;
        let operation_id = digest(&("oasis7.custody.owner-setup-ready.v1", id))?;
        Ok(PhaseOriginal {
            operation_id,
            body_digest: digest(&(original, o.observation.as_ref().ok_or_else(invalid)?))?,
            receipt_digest: Some(digest(&evidence)?),
        })
    }
    pub fn owner_setup_stored_admission_checks(
        &self,
        id: &str,
        stage: OwnerSetupStageV1,
        current: &OwnerSetupCurrent<'_, '_>,
        now: u64,
    ) -> Result<ExactSignatureChecks> {
        let o = self.owner_setup_outbox(id, stage).ok_or_else(invalid)?;
        if o.admitted_at.is_none_or(|t| t.0 > now || now - t.0 > 5000) {
            return Err(invalid());
        }
        self.owner_setup_admission_checks(
            id,
            stage,
            &OwnerSetupAdmissionEvidenceV1 {
                world: o.world.clone().ok_or_else(invalid)?,
                session: o.admission_session.clone().ok_or_else(invalid)?,
            },
            current,
            now,
        )
    }
    pub(super) fn validate_owner_setup_world_admission(
        &self,
        id: &str,
        stage: OwnerSetupStageV1,
        evidence: &OwnerSetupAdmissionEvidenceV1,
        now: u64,
        service_epoch: DecimalU64,
    ) -> Result<()> {
        let setup = self.owner_setups.get(id).ok_or_else(invalid)?;
        let o = setup.stages.get(&stage).ok_or_else(invalid)?;
        self.validate_owner_setup_access(
            id,
            stage,
            o.access.as_ref().ok_or_else(invalid)?,
            now,
            service_epoch,
        )?;
        if o.observation.as_ref() != Some(&evidence.world.request) {
            return Err(invalid());
        }
        self.validate_owner_setup_world_evidence(id, stage, &evidence.world)?;
        let response = &evidence.world.response.payload;
        if response.control.view.game_task_control
            != self.owner_setup_expected_control(id, stage)?
        {
            return Err(invalid());
        }
        match stage {
            OwnerSetupStageV1::Bind | OwnerSetupStageV1::FinalObservation => {
                self.owner_setup_delegation(id, response.signer_delegation.as_ref())?
            }
            _ => {
                if response.signer_delegation.is_some() {
                    return Err(invalid());
                }
            }
        }
        if let Some(min) = &evidence.world.request.read.request.min_commit {
            if !response
                .control
                .version
                .commit
                .satisfies_minimum(min)
                .map_err(|_| invalid())?
            {
                return Err(invalid());
            }
        }
        if evidence.session.status.status.observed_at.0 < o.published_at.ok_or_else(invalid)?.0 {
            return Err(invalid());
        }
        self.validate_owner_setup_session(
            id,
            &ledger::preflight_nonce(id, stage, &o.session_nonce)?,
            &evidence.session,
            now,
            service_epoch,
        )
    }
    pub(super) fn validate_owner_setup_session(
        &self,
        id: &str,
        nonce: &str,
        evidence: &ParticipantSessionEvidenceV1,
        now: u64,
        service_epoch: DecimalU64,
    ) -> Result<()> {
        let setup = self.owner_setups.get(id).ok_or_else(invalid)?;
        let participant = self
            .participant_provisions
            .values()
            .find(|p| p.result.is_some())
            .ok_or_else(invalid)?;
        let session = setup.session.as_ref().ok_or_else(invalid)?;
        evidence
            .status
            .status
            .validate_for(
                &participant.descriptor.native_binding,
                nonce,
                session.native_binding_generation,
                service_epoch,
                now,
            )
            .map_err(|_| invalid())?;
        let status = &evidence.status.status;
        if !status.active
            || status.pending
            || status.frozen
            || status.session_ref != session.session_ref
            || evidence.private_sid_generation != session.private_sid_generation
        {
            return Err(invalid());
        }
        Ok(())
    }
}
