use super::super::super::participant::validation::{
    admission_for_descriptor, policy_for_descriptor,
};
use super::*;
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SessionPreflightStepV1 {
    Lifetime,
    FreshAccessPair,
    OwnerRead,
}
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TestSessionPreflightDescriptorV1 {
    pub api_version: u8,
    pub session_id: String,
    pub session: ParticipantSessionRefV1,
    pub native_binding: crate::native_binding::NativeBindingReceiptV1,
    pub world: crate::approval::ApprovedWorldV1,
    pub owner_key: PublicRoleKey,
    pub participant_owner: ws::SignedReadRequest<oasis7_agent_api::game::GameParticipantRegisterV1>,
    pub owner_scope: RoleKeySlotScopeV1,
    pub identity_scope: RoleKeySlotScopeV1,
    pub source_operation_id: String,
    pub source_nonce: String,
    pub lifetime_nonce: String,
    pub access_nonces: OwnerSetupAccessNoncesV1,
    pub owner_read: ws::ReadWorldViewRequest,
    pub expires_at: UnixMillis,
}
impl core::fmt::Debug for TestSessionPreflightDescriptorV1 {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str("TestSessionPreflightDescriptorV1(<private>)")
    }
}
pub struct TestSessionPreflightCurrent<'a, 'b> {
    pub provision: &'a ProvisionApprovalContext<'b>,
    pub native_scope: &'a NativeStageScope<'b>,
    pub source_operation_id: &'a str,
    pub source_ticket: u8,
    pub oracle_ticket: u8,
    pub observed_at: UnixMillis,
}
impl AccountProvisionApprovalState {
    pub fn test_session_preflight_descriptor(
        &self,
        id: &str,
    ) -> Result<TestSessionPreflightDescriptorV1> {
        let s = self.test_sessions.get(id).ok_or_else(invalid)?;
        let (_, _, write, _, _) = admission::pairing(&s.input.request.intent)?;
        let proof = s.input.approval.digest().map_err(|_| invalid())?;
        let nonce = |domain: &str| digest(&(domain, id, &proof));
        Ok(TestSessionPreflightDescriptorV1 {
            api_version: 1,
            session_id: id.into(),
            session: s.session.clone(),
            native_binding: self
                .participant_provisions
                .values()
                .find(|p| p.result.is_some())
                .ok_or_else(invalid)?
                .descriptor
                .native_binding
                .clone(),
            world: s.termination.world.clone(),
            owner_key: self
                .provision
                .role_key(Role::Owner)
                .ok_or_else(invalid)?
                .clone(),
            participant_owner: self
                .participant_provisions
                .values()
                .find(|p| p.result.is_some())
                .and_then(|p| p.bundle.as_ref())
                .ok_or_else(invalid)?
                .owner
                .clone(),
            owner_scope: self
                .participant_provisions
                .values()
                .find(|p| p.result.is_some())
                .ok_or_else(invalid)?
                .descriptor
                .owner_scope
                .clone(),
            identity_scope: s.session.scope.role_scope.clone(),
            source_operation_id: nonce("oasis7.custody.test-session-preflight-source.v1")?,
            source_nonce: s.preflight_source_nonce.clone().ok_or_else(invalid)?,
            lifetime_nonce: nonce("oasis7.custody.test-session-native-lifetime.v1")?,
            access_nonces: OwnerSetupAccessNoncesV1 {
                policy: nonce("oasis7.custody.test-session-access-policy.v1")?,
                policy_session: nonce("oasis7.custody.test-session-access-policy-session.v1")?,
                authorize_session: nonce(
                    "oasis7.custody.test-session-access-authorize-session.v1",
                )?,
            },
            owner_read: self.test_session_world_read(&s.input)?,
            expires_at: UnixMillis::new(write),
        })
    }
    /// Once-only workflow reservation; its descriptor is read-only, never a purpose grant.
    pub fn claim_test_session_preflight(
        &mut self,
        id: &str,
        now: UnixMillis,
    ) -> Result<TestSessionPreflightDescriptorV1> {
        let s = self.test_sessions.get(id).ok_or_else(invalid)?;
        let d = self.test_session_preflight_descriptor(id)?;
        if self.frozen()
            || s.consumed_at.is_none()
            || s.closing.is_some()
            || s.approved_at.is_some()
            || s.preflight_claimed_at.is_some()
            || now.get() >= d.expires_at.get()
        {
            return Err(invalid());
        }
        let mut next = self.clone();
        next.reserve_test_session_source(
            id,
            SessionSourceReservation {
                operation_id: &d.source_operation_id,
                nonce: &d.source_nonce,
                funding: SessionSourceFundingV1::Control,
            },
            now,
        )?;
        let source = next
            .test_sessions
            .get_mut(id)
            .and_then(|s| s.sources.get_mut(&d.source_operation_id))
            .ok_or_else(invalid)?;
        source.expires_at.0 = source.expires_at.0.min(d.expires_at.get());
        next.test_sessions
            .get_mut(id)
            .ok_or_else(invalid)?
            .preflight_claimed_at = Some(now);
        next.validate_persistence()?;
        *self = next;
        Ok(d)
    }
    pub fn reserve_test_session_preflight_read(
        &mut self,
        id: &str,
        purpose: OwnerSetupReadPurposeV1,
        role: Role,
        now: UnixMillis,
    ) -> Result<u8> {
        let mut next = self.clone();
        let s = next.test_sessions.get_mut(id).ok_or_else(invalid)?;
        let (_, _, deadline, _, _) = admission::pairing(&s.input.request.intent)?;
        if self.frozen()
            || s.closing.is_some()
            || s.preflight_claimed_at.is_none()
            || s.approved_at.is_some()
            || now.get() >= deadline
        {
            return Err(invalid());
        }
        let ticket = s
            .preflight_oracle_reads
            .checked_add(1)
            .ok_or_else(invalid)?;
        if !match ticket {
            1 | 2 => purpose == OwnerSetupReadPurposeV1::Session && role == Role::Identity,
            3 | 4 | 5 => purpose == OwnerSetupReadPurposeV1::Sign && role == Role::Owner,
            _ => false,
        } {
            return Err(invalid());
        }
        s.preflight_oracle_reads = ticket;
        next.validate_persistence()?;
        *self = next;
        Ok(ticket)
    }
    pub fn test_session_preflight_checks(
        &self,
        id: &str,
        current: &TestSessionPreflightCurrent<'_, '_>,
        now: UnixMillis,
    ) -> Result<ExactSignatureChecks> {
        let s = self.test_sessions.get(id).ok_or_else(invalid)?;
        let c = current.provision.approval;
        let (_, _, deadline, _, _) = admission::pairing(&s.input.request.intent)?;
        let record = s
            .book
            .original(id)
            .filter(|r| r.stage() == ApprovalAdmissionStage::Consumed)
            .ok_or_else(invalid)?;
        record.check_recorded_context(c)?;
        if self.frozen()
            || s.closing.is_some()
            || s.preflight_claimed_at.is_none()
            || s.approved_at.is_some()
            || now.get() >= deadline
            || c.authority != s.input.request.context
            || c.authority.security_version != self.provision.security_epoch()
            || c.authority.security_version.0 == 0
            || c.frozen
            || c.fence.check_epoch(c.authority.service_epoch.0).is_err()
            || now.get() >= c.source.session_expires_at.0
            || now.get() >= c.source.session_idle_expires_at.0
            || current.provision.world_identity != &s.termination.world
            || current.provision.policy.version != self.draft.entry.bootstrap_config_version.0
            || !current.provision.policy.managers.iter().any(|m| {
                m.principal == self.draft.entry.manager
                    && m.environments.contains(&self.draft.entry.environment)
            })
            || current.observed_at.get() > now.get()
            || now.get() - current.observed_at.get() > 5000
            || current.source_ticket == 0
            || current.source_ticket
                > *s.source_oracle_reads
                    .get(current.source_operation_id)
                    .ok_or_else(invalid)?
            || current.oracle_ticket == 0
            || current.oracle_ticket > s.preflight_oracle_reads
        {
            return Err(invalid());
        }
        let source = s
            .sources
            .get(current.source_operation_id)
            .ok_or_else(invalid)?;
        super::source::validate_inspection_record(
            record,
            source,
            source.verification.as_ref().ok_or_else(invalid)?,
            c,
            now.get(),
        )
    }
    /// Reserve all four native HTTP attempts before I/O: policy status, policy,
    /// authorize status and authorize. A partial/unknown pair never refunds.
    pub fn claim_test_session_preflight_step(
        &mut self,
        id: &str,
        step: SessionPreflightStepV1,
        now: UnixMillis,
    ) -> Result<TestSessionPreflightDescriptorV1> {
        let mut next = self.clone();
        let s = next.test_sessions.get_mut(id).ok_or_else(invalid)?;
        let (_, _, deadline, _, _) = admission::pairing(&s.input.request.intent)?;
        if self.frozen()
            || s.closing.is_some()
            || s.preflight_claimed_at.is_none()
            || s.approved_at.is_some()
            || now.get() >= deadline
            || s.preflight_steps.contains_key(&step)
        {
            return Err(invalid());
        }
        if step == SessionPreflightStepV1::FreshAccessPair && s.preflight_oracle_reads != 2
            || matches!(
                step,
                SessionPreflightStepV1::OwnerRead | SessionPreflightStepV1::Lifetime
            ) && s.preflight_read.is_none()
            || step == SessionPreflightStepV1::OwnerRead && s.preflight_lifetime.is_none()
        {
            return Err(invalid());
        }
        s.identity_transport
            .as_mut()
            .ok_or_else(invalid)?
            .charge(if step == SessionPreflightStepV1::FreshAccessPair {
                4
            } else {
                1
            })
            .map_err(|_| invalid())?;
        s.preflight_steps.insert(step, now);
        let d = next.test_session_preflight_descriptor(id)?;
        next.validate_persistence()?;
        *self = next;
        Ok(d)
    }
    pub fn test_session_preflight_access_checks(
        &self,
        id: &str,
        e: &OwnerSetupAccessEvidenceV1,
        current: &TestSessionPreflightCurrent<'_, '_>,
        now: UnixMillis,
    ) -> Result<ExactSignatureChecks> {
        let checks = self.test_session_preflight_checks(id, current, now)?;
        let s = self.test_sessions.get(id).ok_or_else(invalid)?;
        let claimed = s
            .preflight_steps
            .get(&SessionPreflightStepV1::FreshAccessPair)
            .ok_or_else(invalid)?;
        if current.oracle_ticket < 3 || s.preflight_access.as_ref().is_some_and(|old| old != e) {
            return Err(invalid());
        }
        let participant = self
            .participant_provisions
            .values()
            .find(|p| p.result.is_some())
            .ok_or_else(invalid)?;
        let d = self.test_session_preflight_descriptor(id)?;
        policy_for_descriptor(&participant.descriptor, &d.access_nonces.policy, &e.policy)?;
        admission_for_descriptor(&participant.descriptor, &e.policy, &e.admission)?;
        for (nonce, status) in [
            (&d.access_nonces.policy_session, &e.policy_session),
            (&d.access_nonces.authorize_session, &e.authorize_session),
        ] {
            status
                .status
                .status
                .validate_for(
                    &participant.descriptor.native_binding,
                    nonce,
                    s.session.native_binding_generation,
                    current.provision.approval.authority.service_epoch,
                    now.get(),
                )
                .map_err(|_| invalid())?;
            if !status.status.status.active
                || status.status.status.frozen
                || status.status.status.pending
                || status.status.status.observed_at.0 < claimed.get()
                || status.status.status.session_ref != s.session.session_ref
                || status.private_sid_generation != s.session.private_sid_generation
            {
                return Err(invalid());
            }
        }
        let checks = append_participant_signature_checks(
            checks,
            ParticipantSignatureFact::Policy(&e.policy, &s.termination.world.service_public_key),
        )?;
        let checks = append_participant_signature_checks(
            checks,
            ParticipantSignatureFact::Admission(
                &e.admission,
                e.policy.response.payload["policy"]["issuer_public_key"]
                    .as_str()
                    .ok_or_else(invalid)?,
            ),
        )?;
        let checks = append_native_history_checks(
            checks,
            current.provision.approval,
            NativeFact::Status(&e.policy_session.status),
            current.native_scope.native_root_key_id,
            current.native_scope.native_root_public_key,
            &digest(&(id, "preflight-policy"))?,
        )?;
        Ok(append_native_history_checks(
            checks,
            current.provision.approval,
            NativeFact::Status(&e.authorize_session.status),
            current.native_scope.native_root_key_id,
            current.native_scope.native_root_public_key,
            &digest(&(id, "preflight-authorize"))?,
        )?)
    }
    pub fn record_test_session_preflight_access(
        &mut self,
        id: &str,
        e: OwnerSetupAccessEvidenceV1,
        current: &TestSessionPreflightCurrent<'_, '_>,
        now: UnixMillis,
        verifier: &mut impl ApprovalSignatureVerifier,
    ) -> Result<()> {
        verifier.verify(&self.test_session_preflight_access_checks(id, &e, current, now)?)?;
        let mut next = self.clone();
        next.test_sessions
            .get_mut(id)
            .ok_or_else(invalid)?
            .preflight_access = Some(e);
        next.validate_persistence()?;
        *self = next;
        Ok(())
    }
    pub fn test_session_preflight_signing_request(
        &self,
        id: &str,
        current: &TestSessionPreflightCurrent<'_, '_>,
        now: UnixMillis,
        verifier: &mut impl ApprovalSignatureVerifier,
    ) -> Result<ws::ReadWorldViewRequest> {
        let e = self
            .test_sessions
            .get(id)
            .and_then(|s| s.preflight_access.as_ref())
            .ok_or_else(invalid)?;
        if !matches!(current.oracle_ticket, 3 | 4) {
            return Err(invalid());
        }
        verifier.verify(&self.test_session_preflight_access_checks(id, e, current, now)?)?;
        Ok(self.test_session_preflight_descriptor(id)?.owner_read)
    }
    pub fn test_session_preflight_publish_checks(
        &self,
        id: &str,
        original: &ws::participant_discovery::ParticipantOwnerViewRequestV1,
        current: &TestSessionPreflightCurrent<'_, '_>,
        now: UnixMillis,
    ) -> Result<ExactSignatureChecks> {
        let s = self.test_sessions.get(id).ok_or_else(invalid)?;
        let e = s.preflight_access.as_ref().ok_or_else(invalid)?;
        let checks = self.test_session_preflight_access_checks(id, e, current, now)?;
        let participant = self
            .participant_provisions
            .values()
            .find(|p| p.result.is_some())
            .ok_or_else(invalid)?;
        if current.oracle_ticket != 5
            || original.read.request != self.test_session_preflight_descriptor(id)?.owner_read
            || original.read.subject_public_key != s.termination.initial_fence.owner_public_key
            || original.owner != participant.bundle.as_ref().ok_or_else(invalid)?.owner
            || original.admission != e.admission
            || original.response_profile
                != Some(ws::participant_discovery::ParticipantOwnerViewProfileV1::OwnerSetupV1)
            || s.preflight_read
                .as_ref()
                .is_some_and(|old| old.as_ref() != original)
        {
            return Err(invalid());
        }
        Ok(append_owner_setup_checks(
            checks,
            OwnerSetupSignatureFact::Read(original, &s.termination.initial_fence.owner_public_key),
        )?)
    }
    pub fn publish_test_session_preflight_read(
        &mut self,
        id: &str,
        original: ws::participant_discovery::ParticipantOwnerViewRequestV1,
        current: &TestSessionPreflightCurrent<'_, '_>,
        now: UnixMillis,
        verifier: &mut impl ApprovalSignatureVerifier,
    ) -> Result<()> {
        verifier
            .verify(&self.test_session_preflight_publish_checks(id, &original, current, now)?)?;
        let mut next = self.clone();
        next.test_sessions
            .get_mut(id)
            .ok_or_else(invalid)?
            .preflight_read = Some(Box::new(original));
        next.validate_persistence()?;
        *self = next;
        Ok(())
    }
}

#[path = "test_session_preflight_lifetime.rs"]
mod lifetime;

#[path = "test_session_preflight_restore.rs"]
mod restore;

#[path = "test_session_preflight_permit.rs"]
mod permit;
pub use permit::*;
