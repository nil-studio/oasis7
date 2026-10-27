use super::*;
impl AccountProvisionApprovalState {
    pub fn claim_session_status_recovery(
        &mut self,
        id: &str,
        scope: &NativeStageScope<'_>,
        current: &ProvisionApprovalContext<'_>,
        now: u64,
        verifier: &mut impl ApprovalSignatureVerifier,
    ) -> Result<SessionStatusRecoveryPermitViewV1> {
        verifier.verify(&self.session_status_recovery_checks(id, scope, current, now)?)?;
        let mut next = self.clone();
        let r = next
            .session_status_recoveries
            .get_mut(id)
            .ok_or(ApprovalAdmissionError::Missing)?;
        if r.claimed_at.is_some() {
            return Err(ProvisionApprovalError::Conflict);
        }
        r.claimed_at = Some(DecimalU64(now));
        let result = next.session_status_recovery_view(id, current, now)?;
        next.validate_persistence()?;
        *self = next;
        Ok(result)
    }
    pub fn authorize_session_status_recovery_read(
        &self,
        id: &str,
        ticket: u8,
        scope: &NativeStageScope<'_>,
        current: &ProvisionApprovalContext<'_>,
        now: u64,
        verifier: &mut impl ApprovalSignatureVerifier,
    ) -> Result<SessionStatusRecoveryPermitViewV1> {
        let r = self
            .session_status_recoveries
            .get(id)
            .ok_or(ApprovalAdmissionError::Missing)?;
        if ticket == 0 || ticket > r.reads || r.claimed_at.is_none() {
            return Err(ProvisionApprovalError::BindingMismatch);
        }
        verifier.verify(&self.session_status_recovery_checks(id, scope, current, now)?)?;
        self.session_status_recovery_view(id, current, now)
    }
    fn session_status_recovery_view(
        &self,
        id: &str,
        current: &ProvisionApprovalContext<'_>,
        now: u64,
    ) -> Result<SessionStatusRecoveryPermitViewV1> {
        let r = self.recovery_current(id, current, now)?;
        let history = self
            .native_history
            .get(&r.request.history_digest)
            .ok_or(ApprovalAdmissionError::Missing)?;
        let NativeStageResultV1::SessionStatus(old) = &history.result else {
            return Err(ProvisionApprovalError::BindingMismatch);
        };
        let source = r
            .source_inspection
            .verification
            .as_ref()
            .ok_or(ApprovalAdmissionError::Missing)?;
        let expires = source
            .verification
            .observed_at
            .0
            .checked_add(5000)
            .ok_or(ProvisionApprovalError::BindingMismatch)?
            .min(r.expires_at.0)
            .min(source.verification.source_session_expires_at.0)
            .min(source.verification.source_session_idle_expires_at.0);
        if now >= expires {
            return Err(ProvisionApprovalError::BindingMismatch);
        }
        let source_revision = super::super::super::types::digest(&(
            "oasis7.custody.session-recovery-source.v1",
            &current.approval.source,
        ))?;
        let authority_revision = super::super::super::types::digest(&(
            "oasis7.custody.session-recovery-authority.v1",
            &r.request,
            &r.nonce,
            r.private_sid_generation,
            r.native_binding_generation,
            r.target_security_epoch,
            current.approval.authority.service_epoch,
            &source_revision,
        ))?;
        Ok(SessionStatusRecoveryPermitViewV1 {
            request: r.request.clone(),
            history: history.clone(),
            source_permit_id: r.source_permit_id.clone(),
            nonce: r.nonce.clone(),
            private_sid_generation: r.private_sid_generation,
            native_binding_generation: r.native_binding_generation,
            target_security_epoch: r.target_security_epoch,
            service_epoch: current.approval.authority.service_epoch,
            source_revision,
            authority_revision,
            observed_at: DecimalU64(now),
            expires_at: DecimalU64(expires),
            http_origin: r.http_origin.clone(),
            binding: old.status.receipt.clone(),
        })
    }
    pub fn session_status_recovery_result_checks(
        &self,
        id: &str,
        signed: &SignedManagedNativeSessionStatusV1,
        scope: &NativeStageScope<'_>,
        current: &ProvisionApprovalContext<'_>,
        now: u64,
    ) -> Result<ExactSignatureChecks> {
        let checks = self.session_status_recovery_checks(id, scope, current, now)?;
        let r = self
            .session_status_recoveries
            .get(id)
            .ok_or(ApprovalAdmissionError::Missing)?;
        let view = self.session_status_recovery_view(id, current, now)?;
        if r.claimed_at.is_none() {
            return Err(ProvisionApprovalError::BindingMismatch);
        }
        signed
            .status
            .validate_for(
                &view.binding,
                &r.nonce,
                r.native_binding_generation,
                view.service_epoch,
                now,
            )
            .map_err(|_| ProvisionApprovalError::BindingMismatch)?;
        let NativeStageResultV1::SessionStatus(old) = &view.history.result else {
            return Err(ProvisionApprovalError::BindingMismatch);
        };
        if !signed.status.active
            || signed.status.frozen
            || signed.status.pending
            || signed.status.session_ref != old.status.session_ref
        {
            return Err(ProvisionApprovalError::BindingMismatch);
        }
        Ok(append_native_history_checks(
            checks,
            current.approval,
            NativeFact::Status(signed),
            scope.native_root_key_id,
            scope.native_root_public_key,
            &r.request.digest()?,
        )?)
    }
    #[expect(
        clippy::too_many_arguments,
        reason = "Keep readonly native status, current authority and trusted private SID attestation independent"
    )]
    pub fn complete_session_status_recovery(
        &mut self,
        id: &str,
        signed: SignedManagedNativeSessionStatusV1,
        scope: &NativeStageScope<'_>,
        current: &ProvisionApprovalContext<'_>,
        now: u64,
        session: &PersistedNativeSessionAttestation<'_>,
        verifier: &mut impl ApprovalSignatureVerifier,
    ) -> Result<SignedManagedNativeSessionStatusV1> {
        let original = self
            .session_status_recoveries
            .get(id)
            .ok_or(ApprovalAdmissionError::Missing)?;
        let digest = signed
            .status
            .digest()
            .map_err(|_| ProvisionApprovalError::BindingMismatch)?;
        if session.status_digest != digest
            || session.session_ref != signed.status.session_ref
            || session.private_sid_generation != original.private_sid_generation.0
        {
            return Err(ProvisionApprovalError::BindingMismatch);
        }
        if let Some(old) = &original.result {
            return if old == &signed {
                Ok(old.clone())
            } else {
                Err(ProvisionApprovalError::Conflict)
            };
        }
        verifier.verify(
            &self.session_status_recovery_result_checks(id, &signed, scope, current, now)?,
        )?;
        let mut next = self.clone();
        if next.provision.stage() == ProvisionStage::NativeRegistered {
            next.provision.confirm_managed_session(PhaseOriginal {
                operation_id: id.into(),
                body_digest: original.request.digest()?,
                receipt_digest: Some(digest),
            })?;
            if next.session_recovery_activation.is_some() {
                return Err(ProvisionApprovalError::Conflict);
            }
            next.session_recovery_activation = Some(id.into());
        } else if next.provision.stage() != ProvisionStage::NativeSessionReady {
            return Err(ProvisionApprovalError::BindingMismatch);
        }
        let r = next
            .session_status_recoveries
            .get_mut(id)
            .ok_or(ApprovalAdmissionError::Missing)?;
        r.result = Some(signed.clone());
        r.completed_at = Some(DecimalU64(now));
        r.result_private_sid_generation = Some(DecimalU64(session.private_sid_generation));
        next.validate_persistence()?;
        *self = next;
        Ok(signed)
    }
}
