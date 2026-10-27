use super::*;
impl AccountProvisionApprovalState {
    pub fn reserve_participant_provision(
        &mut self,
        request: ParticipantProvisionRequestV1,
        nonces: ParticipantNoncesV1,
        scope: &NativeStageScope<'_>,
        now: u64,
    ) -> Result<ParticipantOriginalV1> {
        request.validate()?;
        if !self.participant_provisions.is_empty()
            || self.frozen()
            || self.provision().stage() != ProvisionStage::NativeSessionReady
            || self.provision().security_epoch().0 != 1
            || self.bootstrap_activation().is_none()
            || request.custody_account_id != self.draft().entry.custody_account_id
            || request.provision_id != self.draft().entry.provision_id
        {
            return Err(bad());
        }
        let values = [
            &nonces.source,
            &nonces.policy,
            &nonces.owner_view,
            &nonces.session,
        ];
        let mut unique = std::collections::BTreeSet::new();
        if values.iter().any(|v| !nonce(v) || !unique.insert(*v)) {
            return Err(bad());
        }
        let source = self.accepted_approval()?.verification().ok_or_else(bad)?;
        let expires = now
            .checked_add(30_000)
            .ok_or_else(bad)?
            .min(self.draft().plan.expires_at.0)
            .min(source.verification.source_session_expires_at.0)
            .min(source.verification.source_session_idle_expires_at.0);
        if now >= expires {
            return Err(bad());
        }
        let descriptor = self.participant_descriptor(&request, &nonces, scope)?;
        let o = ParticipantOriginalV1 {
            source_inspection: ProvisionOriginInspectionV1 {
                operation_id: digest(&("oasis7.custody.participant-source.v1", &request))?,
                verify_nonce: nonces.source,
                created_at: DecimalU64(now),
                expires_at: DecimalU64(expires),
                claimed_at: None,
                request_bytes: None,
                verification: None,
                verified_at: None,
            },
            request: request.clone(),
            descriptor,
            created_at: DecimalU64(now),
            expires_at: DecimalU64(expires),
            target_security_epoch: DecimalU64(1),
            oracle_reads: 0,
            workflow_claimed_at: None,
            session_nonce: nonces.session,
            claims: BTreeMap::new(),
            callbacks: BTreeMap::new(),
            policy: None,
            policy_session: None,
            policy_recorded_at: None,
            bundle: None,
            bundle_recorded_at: None,
            admission: None,
            admission_session: None,
            admission_recorded_at: None,
            submit: None,
            submit_bytes: None,
            lookup: None,
            owner_view: None,
            result: None,
            result_session: None,
            completed_at: None,
        };
        let mut next = self.clone();
        for _ in 0..PARTICIPANT_TRANSPORT_COST {
            next.provision.charge_transport(1, now)?;
        }
        next.participant_provisions
            .insert(request.operation_id, o.clone());
        next.validate_persistence()?;
        *self = next;
        Ok(o)
    }
    pub fn claim_participant_workflow(
        &mut self,
        id: &str,
        scope: &NativeStageScope<'_>,
        current: &ProvisionApprovalContext<'_>,
        now: u64,
        verifier: &mut impl ApprovalSignatureVerifier,
    ) -> Result<()> {
        verifier.verify(&self.participant_checks(id, scope, current, now)?)?;
        let mut next = self.clone();
        let o = next.participant_provisions.get_mut(id).ok_or_else(bad)?;
        if o.workflow_claimed_at.is_some() {
            return Err(bad());
        }
        o.workflow_claimed_at = Some(DecimalU64(now));
        next.validate_persistence()?;
        *self = next;
        Ok(())
    }
    pub fn reserve_participant_read(&mut self, id: &str, now: u64) -> Result<u8> {
        if self.frozen() || self.provision.security_epoch().0 != 1 {
            return Err(bad());
        }
        let mut next = self.clone();
        let o = next.participant_provisions.get_mut(id).ok_or_else(bad)?;
        if o.result.is_some()
            || o.oracle_reads >= PARTICIPANT_ORACLE_READS
            || now < o.created_at.0
            || now >= o.expires_at.0
        {
            return Err(bad());
        }
        o.oracle_reads += 1;
        let ticket = o.oracle_reads;
        next.validate_persistence()?;
        *self = next;
        Ok(ticket)
    }
    pub fn reserve_participant_callback(
        &mut self,
        id: &str,
        callback: ParticipantCallbackV1,
        now: u64,
    ) -> Result<()> {
        if self.frozen() || self.provision.security_epoch().0 != 1 {
            return Err(bad());
        }
        let mut next = self.clone();
        let o = next.participant_provisions.get_mut(id).ok_or_else(bad)?;
        let name = callback_name(callback);
        if o.result.is_some()
            || o.callbacks.contains_key(name)
            || o.workflow_claimed_at.is_none()
            || now >= o.expires_at.0
            || now < o.created_at.0
        {
            return Err(bad());
        }
        o.callbacks.insert(name.into(), DecimalU64(now));
        next.validate_persistence()?;
        *self = next;
        Ok(())
    }
    pub fn claim_participant_step(
        &mut self,
        id: &str,
        step: ParticipantStepV1,
        scope: &NativeStageScope<'_>,
        current: &ProvisionApprovalContext<'_>,
        now: u64,
        verifier: &mut impl ApprovalSignatureVerifier,
    ) -> Result<ParticipantPermitViewV1> {
        verifier.verify(&self.participant_checks(id, scope, current, now)?)?;
        self.participant_step_allowed(id, step)?;
        let mut next = self.clone();
        let o = next.participant_provisions.get_mut(id).ok_or_else(bad)?;
        if o.claims.contains_key(step_name(step)) {
            return Err(bad());
        }
        o.claims.insert(step_name(step).into(), DecimalU64(now));
        let view = next.participant_view(id, step, current, now)?;
        next.validate_persistence()?;
        *self = next;
        Ok(view)
    }
    pub fn authorize_participant_read(
        &self,
        id: &str,
        read: ParticipantReadTicket,
        scope: &NativeStageScope<'_>,
        current: &ProvisionApprovalContext<'_>,
        now: u64,
        verifier: &mut impl ApprovalSignatureVerifier,
    ) -> Result<ParticipantPermitViewV1> {
        let ParticipantReadTicket { ticket, step } = read;
        let o = self.participant_provisions.get(id).ok_or_else(bad)?;
        if ticket == 0 || ticket > o.oracle_reads || !o.claims.contains_key(step_name(step)) {
            return Err(bad());
        }
        self.participant_step_allowed(id, step)?;
        verifier.verify(&self.participant_checks(id, scope, current, now)?)?;
        self.participant_view(id, step, current, now)
    }
    fn participant_step_allowed(&self, id: &str, step: ParticipantStepV1) -> Result<()> {
        let o = self.participant_provisions.get(id).ok_or_else(bad)?;
        if o.workflow_claimed_at.is_none()
            || match step {
                ParticipantStepV1::Policy => false,
                ParticipantStepV1::SignBundle => o.policy.is_none(),
                ParticipantStepV1::Authorize => o.bundle.is_none(),
                ParticipantStepV1::Submit => o.submit.is_none(),
                ParticipantStepV1::OwnerView => {
                    !o.claims.contains_key("submit") || o.owner_view.is_none()
                }
            }
        {
            return Err(bad());
        }
        Ok(())
    }
    pub(super) fn participant_view(
        &self,
        id: &str,
        step: ParticipantStepV1,
        current: &ProvisionApprovalContext<'_>,
        now: u64,
    ) -> Result<ParticipantPermitViewV1> {
        let o = self.participant_provisions.get(id).ok_or_else(bad)?;
        let observed = o
            .source_inspection
            .verification
            .as_ref()
            .ok_or_else(bad)?
            .verification
            .observed_at
            .0;
        let expires = observed
            .checked_add(5000)
            .ok_or_else(bad)?
            .min(o.expires_at.0);
        if now >= expires {
            return Err(bad());
        }
        let source_revision = digest(&(
            "oasis7.custody.participant-source-revision.v1",
            &current.approval.source,
        ))?;
        let authority_revision = digest(&(
            "oasis7.custody.participant-authority-revision.v1",
            &o.request,
            &o.descriptor,
            o.target_security_epoch,
            current.approval.authority.service_epoch,
            &source_revision,
        ))?;
        Ok(ParticipantPermitViewV1 {
            request: o.request.clone(),
            descriptor: o.descriptor.clone(),
            step,
            permit_id: id.into(),
            source_revision,
            authority_revision,
            target_security_epoch: o.target_security_epoch,
            service_epoch: current.approval.authority.service_epoch,
            observed_at: DecimalU64(now),
            expires_at: DecimalU64(expires),
            session_observation_nonce: source::participant_session_nonce(o, step)?,
            lookup_session_observation_nonce: if step == ParticipantStepV1::OwnerView {
                Some(source::participant_lookup_nonce(o)?)
            } else {
                None
            },
            bundle: o.bundle.clone(),
            submit: o.submit.clone(),
            lookup: o.lookup.clone(),
            owner_view: o.owner_view.clone(),
        })
    }
}
