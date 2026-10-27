use super::*;
impl AccountProvisionApprovalState {
    /// Atomic 8-hop source preflight + 56-hop remainder. Neither half commits alone.
    pub fn reserve_native_stage(
        &mut self,
        request: NativeStageRequestV1,
        inspection_id: &str,
        nonce: &str,
        scope: &NativeStageScope<'_>,
        now: u64,
    ) -> Result<NativeStageOriginalV1> {
        request.validate()?;
        if self.native_stages.contains_key(&request.operation_id)
            || self.native_stages.len() >= 16
            || request.custody_account_id != self.draft.entry.custody_account_id
            || request.provision_id != self.draft.entry.provision_id
        {
            return Err(ProvisionApprovalError::Conflict);
        }
        let expected = match request.stage {
            NativeStageKindV1::Registration | NativeStageKindV1::Binding => {
                ProvisionStage::RoleKeysReady
            }
            NativeStageKindV1::Login => ProvisionStage::NativeRegistered,
        };
        if self.provision.stage() != expected {
            return Err(ProvisionApprovalError::BindingMismatch);
        }
        let descriptor = self.native_stage_descriptor(scope)?;
        let mut next = self.clone();
        next.reserve_native_origin(inspection_id, nonce, now)?;
        let epoch = next.provision.security_epoch();
        for _ in 8..NATIVE_STAGE_TRANSPORT_COST {
            next.provision.charge_transport(epoch.0, now)?;
        }
        let original = NativeStageOriginalV1 {
            request: request.clone(),
            descriptor,
            inspection_id: inspection_id.into(),
            nonce: nonce.into(),
            reserved_at: DecimalU64(now),
            target_security_epoch: epoch,
            claimed_at: None,
            request_bytes: None,
            reads: 0,
            result: None,
            binding_statement: None,
            binding_challenge: None,
            private_sid_generation: None,
        };
        next.native_stages
            .insert(request.operation_id, original.clone());
        next.validate_persistence()?;
        *self = next;
        Ok(original)
    }
    pub fn native_stage_checks(
        &self,
        id: &str,
        scope: &NativeStageScope<'_>,
        current: &ProvisionApprovalContext<'_>,
        now: u64,
    ) -> Result<ExactSignatureChecks> {
        let stage = self
            .native_stages
            .get(id)
            .ok_or(ApprovalAdmissionError::Missing)?;
        if stage.target_security_epoch != current.approval.authority.security_version {
            return Err(ProvisionApprovalError::BindingMismatch);
        }
        let checks = self.native_origin_checks(id, current, now)?;
        let historical = match stage.request.stage {
            NativeStageKindV1::Registration => return Ok(checks),
            NativeStageKindV1::Binding => {
                let signed = self
                    .native_stages
                    .values()
                    .find_map(|v| match &v.result {
                        Some(NativeStageResultV1::RegistrationChallenge(v)) => Some(v),
                        _ => None,
                    })
                    .ok_or(ApprovalAdmissionError::Missing)?;

                NativeFact::Challenge(signed)
            }
            NativeStageKindV1::Login => NativeFact::Receipt(
                self.native_stages
                    .values()
                    .find_map(|v| match &v.result {
                        Some(NativeStageResultV1::BindingReceipt(v)) => Some(v),
                        _ => None,
                    })
                    .ok_or(ApprovalAdmissionError::Missing)?,
            ),
        };
        Ok(append_native_history_checks(
            checks,
            current.approval,
            historical,
            scope.native_root_key_id,
            scope.native_root_public_key,
            &stage.request.digest()?,
        )?)
    }
    pub fn claim_native_stage(
        &mut self,
        id: &str,
        scope: &NativeStageScope<'_>,
        current: &ProvisionApprovalContext<'_>,
        now: u64,
        verifier: &mut impl ApprovalSignatureVerifier,
    ) -> Result<NativeStagePermitViewV1> {
        let old = self
            .native_stages
            .get(id)
            .ok_or(ApprovalAdmissionError::Missing)?;
        if old.claimed_at.is_some() || old.result.is_some() {
            return Err(ProvisionApprovalError::Conflict);
        }
        verifier.verify(&self.native_stage_checks(id, scope, current, now)?)?;
        let mut next = self.clone();
        let stage = next
            .native_stages
            .get_mut(id)
            .ok_or(ApprovalAdmissionError::Missing)?;
        stage.claimed_at = Some(DecimalU64(now));
        stage.request_bytes = Some(
            serde_json::to_string(&stage.request)
                .map_err(|_| ProvisionApprovalError::InvalidPersistence)?,
        );
        let view = next.native_stage_view(id, current, now)?;
        next.validate_persistence()?;
        *self = next;
        Ok(view)
    }
    /// Read credit is reserved before any remote I/O. This returns no permission.
    pub fn reserve_native_stage_read(&mut self, id: &str, now: u64) -> Result<u8> {
        self.provision
            .check_current(self.provision.security_epoch().0, now)?;
        let mut next = self.clone();
        let stage = next
            .native_stages
            .get_mut(id)
            .ok_or(ApprovalAdmissionError::Missing)?;
        let inspection = next
            .origin_inspections
            .get(&stage.inspection_id)
            .ok_or(ApprovalAdmissionError::Missing)?;
        if stage.result.is_some()
            || stage.target_security_epoch != next.provision.security_epoch()
            || now < stage.reserved_at.0
            || now >= inspection.expires_at.0
            || stage.reads >= NATIVE_STAGE_MAX_ORACLE_READS
        {
            return Err(ProvisionApprovalError::Conflict);
        }
        stage.reads += 1;
        let ticket = stage.reads;
        next.validate_persistence()?;
        *self = next;
        Ok(ticket)
    }
    pub fn authorize_native_stage_read(
        &self,
        id: &str,
        ticket: u8,
        scope: &NativeStageScope<'_>,
        current: &ProvisionApprovalContext<'_>,
        now: u64,
        verifier: &mut impl ApprovalSignatureVerifier,
    ) -> Result<NativeStagePermitViewV1> {
        let original = self
            .native_stages
            .get(id)
            .ok_or(ApprovalAdmissionError::Missing)?;
        if ticket == 0
            || ticket > original.reads
            || original.claimed_at.is_none()
            || original.result.is_some()
        {
            return Err(ProvisionApprovalError::BindingMismatch);
        }
        verifier.verify(&self.native_stage_checks(id, scope, current, now)?)?;
        self.native_stage_view(id, current, now)
    }
    pub(super) fn native_stage_view(
        &self,
        id: &str,
        current: &ProvisionApprovalContext<'_>,
        now: u64,
    ) -> Result<NativeStagePermitViewV1> {
        let stage = self
            .native_stages
            .get(id)
            .ok_or(ApprovalAdmissionError::Missing)?;
        let inspection = self
            .origin_inspections
            .get(&stage.inspection_id)
            .ok_or(ApprovalAdmissionError::Missing)?;
        let signed = inspection
            .verification
            .as_ref()
            .ok_or(ApprovalAdmissionError::Missing)?;
        let expires = signed
            .verification
            .observed_at
            .0
            .checked_add(5000)
            .ok_or(ProvisionApprovalError::BindingMismatch)?
            .min(inspection.expires_at.0)
            .min(self.draft.plan.expires_at.0);
        if now >= expires {
            return Err(ProvisionApprovalError::BindingMismatch);
        }
        let source_revision = super::super::super::types::digest(&(
            "oasis7-custody-native-source-v1",
            &current.approval.source,
        ))?;
        let authority_revision = super::super::super::types::digest(&(
            "oasis7-custody-native-authority-v1",
            &stage.request,
            stage.target_security_epoch,
            current.approval.authority.service_epoch,
            &self.draft.entry.plan_digest,
            &source_revision,
        ))?;
        Ok(NativeStagePermitViewV1 {
            identity_scope: self
                .role_key_results
                .get("identity")
                .ok_or(ApprovalAdmissionError::Missing)?
                .scope
                .clone(),
            authority_revision,
            source_revision,
            request: stage.request.clone(),
            descriptor: stage.descriptor.clone(),
            attempt_id: id.into(),
            request_bytes: stage
                .request_bytes
                .clone()
                .ok_or(ApprovalAdmissionError::Missing)?,
            request_digest: stage.request.digest()?,
            target_security_epoch: stage.target_security_epoch,
            service_epoch: current.approval.authority.service_epoch,
            nonce: stage.nonce.clone(),
            expires_at: DecimalU64(expires),
            observed_at: DecimalU64(now),
        })
    }
    pub(in crate::provisioning) fn validate_native_stage_persistence(&self) -> Result<u64> {
        if self.native_stages.len() > 16 {
            return Err(ProvisionApprovalError::InvalidPersistence);
        }
        let mut inspections = std::collections::BTreeSet::new();
        for (id, v) in &self.native_stages {
            v.request.validate()?;
            v.descriptor
                .registration
                .validate()
                .map_err(|_| ProvisionApprovalError::InvalidPersistence)?;
            let inspection = self
                .origin_inspections
                .get(&v.inspection_id)
                .ok_or(ProvisionApprovalError::InvalidPersistence)?;
            if id != &v.request.operation_id
                || v.request.custody_account_id != self.draft.entry.custody_account_id
                || v.request.provision_id != self.draft.entry.provision_id
                || !inspections.insert(&v.inspection_id)
                || inspection.verify_nonce != v.nonce
                || inspection.created_at != v.reserved_at
                || v.reads > NATIVE_STAGE_MAX_ORACLE_READS
                || self.provision.role_key(Role::Identity) != Some(&v.descriptor.identity_key)
                || v.descriptor.registration.login_public_key
                    != v.descriptor.identity_key.public_key
                || v.descriptor.http_origin != v.descriptor.registration.audience
                || self.native_descriptor_with_origin(&v.descriptor.http_origin)? != v.descriptor
                || v.target_security_epoch.0
                    != if v.request.stage == NativeStageKindV1::Login {
                        1
                    } else {
                        0
                    }
                || v.result.as_ref().is_some_and(|r| {
                    !matches!(
                        (v.request.stage, r),
                        (
                            NativeStageKindV1::Registration,
                            NativeStageResultV1::RegistrationChallenge(_)
                        ) | (
                            NativeStageKindV1::Binding,
                            NativeStageResultV1::BindingReceipt(_)
                        ) | (
                            NativeStageKindV1::Login,
                            NativeStageResultV1::SessionStatus(_)
                        ) | (
                            NativeStageKindV1::Login,
                            NativeStageResultV1::ExplicitLoginRequired
                        )
                    )
                })
                || v.claimed_at.is_some() != v.request_bytes.is_some()
                || v.claimed_at.is_none() && v.result.is_some()
                || v.request_bytes.as_ref().is_some_and(|bytes| {
                    serde_json::to_string(&v.request).ok().as_ref() != Some(bytes)
                })
            {
                return Err(ProvisionApprovalError::InvalidPersistence);
            }
        }
        Ok(self.native_stages.len() as u64 * (NATIVE_STAGE_TRANSPORT_COST - 8))
    }
}
