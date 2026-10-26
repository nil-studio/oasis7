use super::*;
impl AccountProvisionApprovalState {
    pub fn managed_login_dispatch_checks(
        &self,
        id: &str,
        scope: &NativeStageScope<'_>,
        current: &ProvisionApprovalContext<'_>,
        now: u64,
    ) -> Result<ExactSignatureChecks> {
        let record = self
            .managed_logins
            .get(id)
            .ok_or(ApprovalAdmissionError::Missing)?;
        let original = record
            .book
            .original(id)
            .ok_or(ApprovalAdmissionError::Missing)?;
        self.managed_login_scope(original.input(), current, now)?;
        let task = record
            .task
            .as_ref()
            .ok_or(ApprovalAdmissionError::Missing)?;
        let signed = original
            .verification()
            .ok_or(ApprovalAdmissionError::Missing)?;
        let v = &signed.verification;
        if original.stage() != ApprovalAdmissionStage::Consumed
            || v.observed_at.0 > now
            || now - v.observed_at.0 > 5000
            || task.result.is_some()
            || current.approval.source.session_expires_at != v.source_session_expires_at
            || current.approval.source.session_idle_expires_at != v.source_session_idle_expires_at
        {
            return Err(ProvisionApprovalError::BindingMismatch);
        }
        let receipt = self
            .native_stages
            .values()
            .find_map(|v| match &v.result {
                Some(NativeStageResultV1::BindingReceipt(v)) => Some(v),
                _ => None,
            })
            .ok_or(ApprovalAdmissionError::Missing)?;
        Ok(append_native_history_checks(
            record.book.consumed_checks(id, Some(signed))?,
            current.approval,
            NativeFact::Receipt(receipt),
            scope.native_root_key_id,
            scope.native_root_public_key,
            &bytes_digest(task.request_bytes.as_bytes()),
        )?)
    }
    pub fn claim_managed_login(
        &mut self,
        id: &str,
        scope: &NativeStageScope<'_>,
        current: &ProvisionApprovalContext<'_>,
        now: u64,
        verifier: &mut impl ApprovalSignatureVerifier,
    ) -> Result<ManagedLoginTaskV1> {
        verifier.verify(&self.managed_login_dispatch_checks(id, scope, current, now)?)?;
        let mut next = self.clone();
        let task = next
            .managed_logins
            .get_mut(id)
            .and_then(|v| v.task.as_mut())
            .ok_or(ApprovalAdmissionError::Missing)?;
        if task.claimed_at.is_some() {
            return Err(ProvisionApprovalError::Conflict);
        }
        task.claimed_at = Some(DecimalU64(now));
        let result = task.clone();
        next.validate_persistence()?;
        *self = next;
        Ok(result)
    }
    pub fn reserve_managed_login_read(&mut self, id: &str, now: u64) -> Result<u8> {
        if self.frozen() || self.provision.security_epoch().0 != 1 {
            return Err(ProvisionApprovalError::BindingMismatch);
        }
        let mut next = self.clone();
        let task = next
            .managed_logins
            .get_mut(id)
            .and_then(|v| v.task.as_mut())
            .ok_or(ApprovalAdmissionError::Missing)?;
        if task.reads >= 8 || task.result.is_some() || now >= task.deadline.0 {
            return Err(ProvisionApprovalError::Conflict);
        }
        task.reads += 1;
        let ticket = task.reads;
        next.validate_persistence()?;
        *self = next;
        Ok(ticket)
    }
    pub fn authorize_managed_login_read(
        &self,
        id: &str,
        ticket: u8,
        scope: &NativeStageScope<'_>,
        current: &ProvisionApprovalContext<'_>,
        now: u64,
        verifier: &mut impl ApprovalSignatureVerifier,
    ) -> Result<ManagedLoginPermitViewV1> {
        let task = self
            .managed_login_task(id)
            .ok_or(ApprovalAdmissionError::Missing)?;
        if ticket == 0 || ticket > task.reads || task.claimed_at.is_none() {
            return Err(ProvisionApprovalError::BindingMismatch);
        }
        verifier.verify(&self.managed_login_dispatch_checks(id, scope, current, now)?)?;
        self.managed_login_permit_view(id, scope, current, now)
    }
    pub fn managed_login_result_checks(
        &self,
        id: &str,
        status: &SignedManagedNativeSessionStatusV1,
        scope: &NativeStageScope<'_>,
        current: &ProvisionApprovalContext<'_>,
        now: u64,
    ) -> Result<ExactSignatureChecks> {
        let checks = self.managed_login_dispatch_checks(id, scope, current, now)?;
        let task = self
            .managed_login_task(id)
            .ok_or(ApprovalAdmissionError::Missing)?;
        if task.claimed_at.is_none() {
            return Err(ProvisionApprovalError::BindingMismatch);
        }
        status
            .status
            .validate_for(
                self.native_receipt()?,
                &task.status_nonce,
                DecimalU64(1),
                current.approval.authority.service_epoch,
                now,
            )
            .map_err(|_| ProvisionApprovalError::BindingMismatch)?;
        if !status.status.active || status.status.frozen || status.status.pending {
            return Err(ProvisionApprovalError::BindingMismatch);
        }
        Ok(append_native_history_checks(
            checks,
            current.approval,
            NativeFact::Status(status),
            scope.native_root_key_id,
            scope.native_root_public_key,
            &bytes_digest(task.request_bytes.as_bytes()),
        )?)
    }
    #[expect(
        clippy::too_many_arguments,
        reason = "Keep current authority, exact native result and private SID attestation separate at commit"
    )]
    pub fn complete_managed_login(
        &mut self,
        id: &str,
        result: NativeStageResultV1,
        scope: &NativeStageScope<'_>,
        current: &ProvisionApprovalContext<'_>,
        now: u64,
        session: Option<&PersistedNativeSessionAttestation<'_>>,
        verifier: &mut impl ApprovalSignatureVerifier,
    ) -> Result<NativeStageResultV1> {
        if let Some(old) = self.managed_login_task(id).and_then(|v| v.result.as_ref()) {
            return if old == &result {
                Ok(old.clone())
            } else {
                Err(ProvisionApprovalError::Conflict)
            };
        }
        match &result {
            NativeStageResultV1::SessionStatus(v) => {
                verifier.verify(&self.managed_login_result_checks(id, v, scope, current, now)?)?
            }
            NativeStageResultV1::ExplicitLoginRequired => {
                verifier.verify(&self.managed_login_dispatch_checks(id, scope, current, now)?)?;
                if self
                    .managed_login_task(id)
                    .is_none_or(|v| v.claimed_at.is_none())
                {
                    return Err(ProvisionApprovalError::BindingMismatch);
                }
            }
            _ => return Err(ProvisionApprovalError::BindingMismatch),
        }
        let mut next = self.clone();
        if let NativeStageResultV1::SessionStatus(v) = &result {
            let e = session.ok_or(ApprovalAdmissionError::Missing)?;
            let digest = v
                .status
                .digest()
                .map_err(|_| ProvisionApprovalError::BindingMismatch)?;
            if e.status_digest != digest
                || e.session_ref != v.status.session_ref
                || e.private_sid_generation == 0
            {
                return Err(ProvisionApprovalError::BindingMismatch);
            }
            let task = next
                .managed_login_task(id)
                .ok_or(ApprovalAdmissionError::Missing)?;
            next.provision.confirm_managed_session(PhaseOriginal {
                operation_id: id.into(),
                body_digest: bytes_digest(task.request_bytes.as_bytes()),
                receipt_digest: Some(digest),
            })?;
            next.managed_logins
                .get_mut(id)
                .and_then(|v| v.task.as_mut())
                .ok_or(ApprovalAdmissionError::Missing)?
                .private_sid_generation = Some(DecimalU64(e.private_sid_generation));
        }
        next.managed_logins
            .get_mut(id)
            .and_then(|v| v.task.as_mut())
            .ok_or(ApprovalAdmissionError::Missing)?
            .result = Some(result.clone());
        next.validate_persistence()?;
        *self = next;
        Ok(result)
    }
}

impl AccountProvisionApprovalState {
    pub fn managed_login_permit_view(
        &self,
        id: &str,
        scope: &NativeStageScope<'_>,
        current: &ProvisionApprovalContext<'_>,
        now: u64,
    ) -> Result<ManagedLoginPermitViewV1> {
        let task = self
            .managed_login_task(id)
            .ok_or(ApprovalAdmissionError::Missing)?;
        let original = self
            .managed_logins
            .get(id)
            .and_then(|v| v.book.original(id))
            .ok_or(ApprovalAdmissionError::Missing)?;
        self.managed_login_scope(original.input(), current, now)?;
        let observed = original
            .verification()
            .ok_or(ApprovalAdmissionError::Missing)?
            .verification
            .observed_at;
        let expires = observed
            .0
            .checked_add(5000)
            .ok_or(ProvisionApprovalError::BindingMismatch)?
            .min(task.deadline.0)
            .min(current.approval.source.session_expires_at.0)
            .min(current.approval.source.session_idle_expires_at.0);
        if now >= expires || observed.0 > now {
            return Err(ProvisionApprovalError::BindingMismatch);
        }
        let source_revision = super::super::super::types::digest(&(
            "oasis7-custody-managed-login-source-v1",
            &current.approval.source,
        ))?;
        let authority_revision = super::super::super::types::digest(&(
            "oasis7-custody-managed-login-authority-v1",
            id,
            &task.request_bytes,
            task.target_security_epoch,
            current.approval.authority.service_epoch,
            &source_revision,
        ))?;
        Ok(ManagedLoginPermitViewV1 {
            task: task.clone(),
            initial_login_operation_id: self
                .native_descriptor_with_origin(scope.http_origin)?
                .login_request_id,
            identity_scope: self
                .role_key_results
                .get("identity")
                .ok_or(ApprovalAdmissionError::Missing)?
                .scope
                .clone(),
            authority_revision,
            source_revision,
            service_epoch: current.approval.authority.service_epoch,
            observed_at: DecimalU64(now),
            expires_at: DecimalU64(expires),
            http_origin: scope.http_origin.into(),
            source_audience: original.input().request.context.audience.clone(),
            binding: self.native_receipt()?.clone(),
        })
    }
}
