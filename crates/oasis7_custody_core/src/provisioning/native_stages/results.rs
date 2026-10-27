use super::*;
/// Created only by the fixed Identity RPC after encrypted SID persistence and online checks.
pub struct PersistedNativeSessionAttestation<'a> {
    pub status_digest: &'a str,
    pub session_ref: &'a str,
    pub private_sid_generation: u64,
}
impl AccountProvisionApprovalState {
    pub(super) fn native_challenge(&self, binding_id: &str) -> Result<&NativeBindingChallengeV1> {
        self.native_stages
            .values()
            .find_map(|v| match &v.result {
                Some(NativeStageResultV1::RegistrationChallenge(v))
                    if v.challenge.challenge_id == binding_id =>
                {
                    Some(&v.challenge)
                }
                _ => None,
            })
            .ok_or(ApprovalAdmissionError::Missing.into())
    }
    fn exact_binding_challenge<'a>(
        &'a self,
        original: &'a NativeStageOriginalV1,
    ) -> Result<&'a NativeBindingChallengeV1> {
        if let Some(c) = &original.binding_challenge {
            return Ok(&c.challenge);
        }
        self.native_challenge(&original.request.binding_operation_id)
    }
    pub(super) fn native_receipt(&self) -> Result<&NativeBindingReceiptV1> {
        self.native_stages
            .values()
            .find_map(|v| match &v.result {
                Some(NativeStageResultV1::BindingReceipt(v)) => Some(&v.receipt),
                _ => None,
            })
            .ok_or(ApprovalAdmissionError::Missing.into())
    }
    pub fn native_result_checks(
        &self,
        id: &str,
        result: &NativeStageResultV1,
        scope: &NativeStageScope<'_>,
        current: &ProvisionApprovalContext<'_>,
        now: u64,
    ) -> Result<ExactSignatureChecks> {
        let stage_checks = self.native_stage_checks(id, scope, current, now)?;
        let original = self
            .native_stages
            .get(id)
            .ok_or(ApprovalAdmissionError::Missing)?;
        if original.claimed_at.is_none() {
            return Err(ProvisionApprovalError::BindingMismatch);
        }
        let invalid = || ProvisionApprovalError::BindingMismatch;
        let fact = match (original.request.stage, result) {
            (NativeStageKindV1::Registration, NativeStageResultV1::RegistrationChallenge(v)) => {
                v.challenge.validate_at(now).map_err(|_| invalid())?;
                let c = &v.challenge;
                let r = &original.descriptor.registration;
                if c.registration_operation_id != r.registration_operation_id
                    || c.registration_request_digest != r.digest().map_err(|_| invalid())?
                    || c.login_public_key != r.login_public_key
                    || c.identity_namespace != r.identity_namespace
                    || c.identity_instance_epoch != r.identity_instance_epoch
                    || c.challenge_id != original.request.binding_operation_id
                {
                    return Err(invalid());
                }
                NativeFact::Challenge(v)
            }
            (NativeStageKindV1::Binding, NativeStageResultV1::BindingReceipt(v)) => {
                let statement = original
                    .binding_statement
                    .as_ref()
                    .ok_or(ApprovalAdmissionError::Missing)?;
                v.receipt
                    .validate_for(&statement.statement)
                    .map_err(|_| invalid())?;
                if &statement.statement.challenge != self.exact_binding_challenge(original)?
                    || v.receipt.custody_account_id != self.draft.entry.custody_account_id
                    || v.receipt.provision_id != self.draft.entry.provision_id
                {
                    return Err(invalid());
                }
                NativeFact::Receipt(v)
            }
            (NativeStageKindV1::Login, NativeStageResultV1::SessionStatus(v)) => {
                v.status
                    .validate_for(
                        self.native_receipt()?,
                        &original.nonce,
                        DecimalU64(1),
                        current.approval.authority.service_epoch,
                        now,
                    )
                    .map_err(|_| invalid())?;
                if !v.status.active || v.status.frozen || v.status.pending {
                    return Err(invalid());
                }
                NativeFact::Status(v)
            }
            _ => return Err(invalid()),
        };
        Ok(append_native_history_checks(
            stage_checks,
            current.approval,
            fact,
            scope.native_root_key_id,
            scope.native_root_public_key,
            &original.request.digest()?,
        )?)
    }
    pub fn native_binding_statement_checks(
        &self,
        id: &str,
        statement: &SignedOwnerNativeBindingStatementV1,
        scope: &NativeStageScope<'_>,
        current: &ProvisionApprovalContext<'_>,
        now: u64,
    ) -> Result<ExactSignatureChecks> {
        let stage_checks = self.native_stage_checks(id, scope, current, now)?;
        let original = self
            .native_stages
            .get(id)
            .ok_or(ApprovalAdmissionError::Missing)?;
        if original.request.stage != NativeStageKindV1::Binding
            || original.claimed_at.is_none()
            || original.binding_statement.is_some()
            || statement.statement.challenge != *self.exact_binding_challenge(original)?
        {
            return Err(ProvisionApprovalError::Conflict);
        }
        let permit = self.native_stage_view(id, current, now)?;
        if statement.statement.service_epoch != permit.service_epoch
            || statement.statement.expires_at.0 > permit.expires_at.0
            || statement.statement.source_revision != permit.source_revision
            || statement.statement.account_authority_revision != permit.authority_revision
            || statement.statement.binding_operation_id != original.request.binding_operation_id
        {
            return Err(ProvisionApprovalError::BindingMismatch);
        }
        scope
            .owner_binding_pin
            .validate_statement(&statement.statement, now)
            .map_err(|_| ProvisionApprovalError::BindingMismatch)?;
        let expected = NativeBindingScopeV1 {
            environment: self.draft.entry.environment.clone(),
            custody_account_id: self.draft.entry.custody_account_id.clone(),
            provision_id: self.draft.entry.provision_id.clone(),
            approved_plan_digest: self.draft.entry.plan_digest.clone(),
            identity_namespace: original.descriptor.registration.identity_namespace.clone(),
            identity_instance_epoch: original.descriptor.registration.identity_instance_epoch,
            registration_operation_id: original
                .descriptor
                .registration
                .registration_operation_id
                .clone(),
            registration_request_digest: original
                .descriptor
                .registration
                .digest()
                .map_err(|_| ProvisionApprovalError::BindingMismatch)?,
            login_key: original.descriptor.identity_key.clone(),
            plan_expires_at: self.draft.plan.expires_at,
            source_expires_at: DecimalU64(
                current
                    .approval
                    .source
                    .session_expires_at
                    .0
                    .min(current.approval.source.session_idle_expires_at.0),
            ),
        };
        statement
            .statement
            .validate_for(&expected, now)
            .map_err(|_| ProvisionApprovalError::BindingMismatch)?;
        Ok(append_native_history_checks(
            stage_checks,
            current.approval,
            NativeFact::Statement(statement),
            &scope.owner_binding_pin.key_id,
            &scope.owner_binding_pin.public_key,
            &original.request.digest()?,
        )?)
    }
    pub fn register_native_binding_statement(
        &mut self,
        id: &str,
        statement: SignedOwnerNativeBindingStatementV1,
        scope: &NativeStageScope<'_>,
        current: &ProvisionApprovalContext<'_>,
        now: u64,
        verifier: &mut impl ApprovalSignatureVerifier,
    ) -> Result<()> {
        verifier
            .verify(&self.native_binding_statement_checks(id, &statement, scope, current, now)?)?;
        let mut next = self.clone();
        next.native_stages
            .get_mut(id)
            .ok_or(ApprovalAdmissionError::Missing)?
            .binding_statement = Some(statement);
        next.validate_persistence()?;
        *self = next;
        Ok(())
    }
    #[expect(
        clippy::too_many_arguments,
        reason = "Keep current authority, exact native result and private SID attestation separate at commit"
    )]
    pub fn complete_native_stage(
        &mut self,
        id: &str,
        result: NativeStageResultV1,
        scope: &NativeStageScope<'_>,
        current: &ProvisionApprovalContext<'_>,
        now: u64,
        session: Option<&PersistedNativeSessionAttestation<'_>>,
        verifier: &mut impl ApprovalSignatureVerifier,
    ) -> Result<NativeStageResultV1> {
        if let Some(old) = self.native_stages.get(id).and_then(|v| v.result.as_ref()) {
            return if old == &result {
                Ok(old.clone())
            } else {
                Err(ProvisionApprovalError::Conflict)
            };
        }
        if matches!(result, NativeStageResultV1::ExplicitLoginRequired) {
            verifier.verify(&self.native_stage_checks(id, scope, current, now)?)?;
            if self.native_stages.get(id).is_none_or(|v| {
                v.request.stage != NativeStageKindV1::Login || v.claimed_at.is_none()
            }) {
                return Err(ProvisionApprovalError::BindingMismatch);
            }
        } else {
            verifier.verify(&self.native_result_checks(id, &result, scope, current, now)?)?;
        }
        let mut next = self.clone();
        match &result {
            NativeStageResultV1::BindingReceipt(v) => {
                let receipt = &v.receipt;
                let digest = receipt
                    .digest()
                    .map_err(|_| ProvisionApprovalError::BindingMismatch)?;
                next.provision.save_original(
                    ProvisionStage::NativeRegistered,
                    PhaseOriginal {
                        operation_id: id.into(),
                        body_digest: next
                            .native_stages
                            .get(id)
                            .ok_or(ApprovalAdmissionError::Missing)?
                            .request
                            .digest()?,
                        receipt_digest: None,
                    },
                    0,
                    now,
                )?;
                next.provision.confirm_phase(
                    ProvisionStage::NativeRegistered,
                    id,
                    &digest,
                    Some(&receipt.native_account_id),
                    0,
                    now,
                )?;
                next.provision.activate_native_binding()?;
                next.bootstrap_activation = Some(BootstrapActivationV1 {
                    binding_operation_id: receipt.binding_operation_id.clone(),
                    receipt_digest: digest,
                    from_epoch: DecimalU64(0),
                    to_epoch: DecimalU64(1),
                });
            }
            NativeStageResultV1::SessionStatus(v) => {
                let evidence = session.ok_or(ApprovalAdmissionError::Missing)?;
                let digest = v
                    .status
                    .digest()
                    .map_err(|_| ProvisionApprovalError::BindingMismatch)?;
                if evidence.status_digest != digest
                    || evidence.session_ref != v.status.session_ref
                    || evidence.private_sid_generation == 0
                {
                    return Err(ProvisionApprovalError::BindingMismatch);
                }
                next.native_stages
                    .get_mut(id)
                    .ok_or(ApprovalAdmissionError::Missing)?
                    .private_sid_generation = Some(DecimalU64(evidence.private_sid_generation));
                next.provision.save_original(
                    ProvisionStage::NativeSessionReady,
                    PhaseOriginal {
                        operation_id: id.into(),
                        body_digest: next
                            .native_stages
                            .get(id)
                            .ok_or(ApprovalAdmissionError::Missing)?
                            .request
                            .digest()?,
                        receipt_digest: None,
                    },
                    1,
                    now,
                )?;
                next.provision.confirm_phase(
                    ProvisionStage::NativeSessionReady,
                    id,
                    &digest,
                    None,
                    1,
                    now,
                )?;
            }
            _ => {}
        }
        next.native_stages
            .get_mut(id)
            .ok_or(ApprovalAdmissionError::Missing)?
            .result = Some(result.clone());
        next.validate_persistence()?;
        *self = next;
        Ok(result)
    }
}

impl AccountProvisionApprovalState {
    pub(in crate::provisioning) fn native_phase_replay(
        &self,
    ) -> Result<Vec<(ProvisionStage, PhaseOriginal, Option<String>)>> {
        let mut phases = Vec::new();
        let bindings: Vec<_> = self
            .native_stages
            .iter()
            .filter_map(|(id, v)| match &v.result {
                Some(NativeStageResultV1::BindingReceipt(r)) => Some((id, v, r)),
                _ => None,
            })
            .collect();
        if bindings.len() > 1 {
            return Err(ProvisionApprovalError::InvalidPersistence);
        }
        if let Some((id, v, r)) = bindings.first() {
            let statement = v
                .binding_statement
                .as_ref()
                .ok_or(ProvisionApprovalError::InvalidPersistence)?;
            r.receipt
                .validate_for(&statement.statement)
                .map_err(|_| ProvisionApprovalError::InvalidPersistence)?;
            if statement.statement.challenge != *self.exact_binding_challenge(v)?
                || r.receipt.environment != self.draft.entry.environment
                || r.receipt.custody_account_id != self.draft.entry.custody_account_id
                || r.receipt.provision_id != self.draft.entry.provision_id
                || r.receipt.login_public_key != v.descriptor.identity_key.public_key
            {
                return Err(ProvisionApprovalError::InvalidPersistence);
            }
            let digest = r
                .receipt
                .digest()
                .map_err(|_| ProvisionApprovalError::InvalidPersistence)?;
            let expected = BootstrapActivationV1 {
                binding_operation_id: r.receipt.binding_operation_id.clone(),
                receipt_digest: digest.clone(),
                from_epoch: DecimalU64(0),
                to_epoch: DecimalU64(1),
            };
            if self.bootstrap_activation.as_ref() != Some(&expected) {
                return Err(ProvisionApprovalError::InvalidPersistence);
            }
            phases.push((
                ProvisionStage::NativeRegistered,
                PhaseOriginal {
                    operation_id: (*id).clone(),
                    body_digest: v.request.digest()?,
                    receipt_digest: Some(digest),
                },
                Some(r.receipt.native_account_id.clone()),
            ));
        } else if self.bootstrap_activation.is_some() {
            return Err(ProvisionApprovalError::InvalidPersistence);
        }
        let statuses: Vec<_> = self
            .native_stages
            .iter()
            .filter_map(|(id, v)| match &v.result {
                Some(NativeStageResultV1::SessionStatus(r)) => Some((id, v, r)),
                _ => None,
            })
            .collect();
        if statuses.len() > 1 {
            return Err(ProvisionApprovalError::InvalidPersistence);
        }
        if let Some((id, v, r)) = statuses.first() {
            r.status
                .validate()
                .map_err(|_| ProvisionApprovalError::InvalidPersistence)?;
            if &r.status.receipt != self.native_receipt()?
                || r.status.nonce != v.nonce
                || r.status.generation.0 != 1
                || !r.status.active
                || r.status.frozen
                || r.status.pending
                || v.target_security_epoch.0 != 1
                || v.private_sid_generation.is_none_or(|v| v.0 == 0)
            {
                return Err(ProvisionApprovalError::InvalidPersistence);
            }
            phases.push((
                ProvisionStage::NativeSessionReady,
                PhaseOriginal {
                    operation_id: (*id).clone(),
                    body_digest: v.request.digest()?,
                    receipt_digest: Some(
                        r.status
                            .digest()
                            .map_err(|_| ProvisionApprovalError::InvalidPersistence)?,
                    ),
                },
                None,
            ));
        }
        for id in self.managed_logins.keys() {
            if let Some(task) = self.managed_login_task(id) {
                if let Some(NativeStageResultV1::SessionStatus(v)) = &task.result {
                    v.validate()
                        .map_err(|_| ProvisionApprovalError::InvalidPersistence)?;
                    if phases
                        .iter()
                        .any(|v| v.0 == ProvisionStage::NativeSessionReady)
                        || v.status.receipt != *self.native_receipt()?
                        || v.status.nonce != task.status_nonce
                        || v.status.generation.0 != 1
                        || !v.status.active
                        || v.status.frozen
                        || v.status.pending
                        || task.private_sid_generation.is_none_or(|v| v.0 == 0)
                    {
                        return Err(ProvisionApprovalError::InvalidPersistence);
                    }
                    phases.push((
                        ProvisionStage::NativeSessionReady,
                        PhaseOriginal {
                            operation_id: id.clone(),
                            body_digest: bytes_digest(task.request_bytes.as_bytes()),
                            receipt_digest: Some(
                                v.status
                                    .digest()
                                    .map_err(|_| ProvisionApprovalError::InvalidPersistence)?,
                            ),
                        },
                        None,
                    ));
                }
            }
        }
        Ok(phases)
    }
}
