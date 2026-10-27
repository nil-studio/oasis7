use super::*;
impl AccountProvisionApprovalState {
    pub(in crate::provisioning) fn validate_session_recovery_persistence(&self) -> Result<u64> {
        if self.session_status_recoveries.len() > MAX_SESSION_STATUS_RECOVERIES {
            return Err(ProvisionApprovalError::InvalidPersistence);
        }
        let mut nonces = std::collections::BTreeSet::new();
        let mut provision_charged = 0u64;
        for (id, r) in &self.session_status_recoveries {
            r.request.validate()?;
            let h = self
                .native_history
                .get(&r.request.history_digest)
                .ok_or(ProvisionApprovalError::InvalidPersistence)?;
            let NativeStageResultV1::SessionStatus(old) = &h.result else {
                return Err(ProvisionApprovalError::InvalidPersistence);
            };
            let a = h
                .session_attestation
                .as_ref()
                .ok_or(ProvisionApprovalError::InvalidPersistence)?;
            let source = self.recovery_source_record(&r.funding)?;
            let v = &source
                .verification()
                .ok_or(ProvisionApprovalError::InvalidPersistence)?
                .verification;
            let expires = r
                .created_at
                .0
                .checked_add(30_000)
                .ok_or(ProvisionApprovalError::InvalidPersistence)?
                .min(self.recovery_deadline(&r.funding)?)
                .min(v.source_session_expires_at.0)
                .min(v.source_session_idle_expires_at.0);
            if id != &r.request.operation_id
                || r.request.custody_account_id != self.draft.entry.custody_account_id
                || r.request.provision_id != self.draft.entry.provision_id
                || r.funding != self.recovery_funding_for(h)?
                || r.source_permit_id != h.result_permit_id
                || r.private_sid_generation != a.private_sid_generation
                || r.native_binding_generation != old.status.generation
                || r.target_security_epoch.0 != 1
                || self.bootstrap_activation.is_none()
                || !status_recovery_types::recovery_hex(&r.nonce)
                || !status_recovery_types::recovery_hex(&r.source_inspection.verify_nonce)
                || !nonces.insert(&r.nonce)
                || !nonces.insert(&r.source_inspection.verify_nonce)
                || r.reads > 8
                || r.expires_at.0 != expires
                || r.created_at.0 >= expires
                || r.source_inspection.created_at != r.created_at
                || r.source_inspection.expires_at != r.expires_at
                || r.source_inspection.operation_id
                    != super::super::super::types::digest(&(
                        "oasis7.custody.session-recovery-origin.v1",
                        &r.request,
                    ))?
                || self.origin_inspections.values().any(|v| {
                    v.verify_nonce == r.nonce || v.verify_nonce == r.source_inspection.verify_nonce
                })
                || self
                    .native_stages
                    .values()
                    .filter(|v| v.request.stage == NativeStageKindV1::Login)
                    .all(|v| v.descriptor.http_origin != r.http_origin)
            {
                return Err(ProvisionApprovalError::InvalidPersistence);
            }
            let inspection = &r.source_inspection;
            if inspection.claimed_at.is_some() != inspection.request_bytes.is_some()
                || inspection.verification.is_some() != inspection.verified_at.is_some()
            {
                return Err(ProvisionApprovalError::InvalidPersistence);
            }
            if let Some(at) = inspection.claimed_at {
                if at.0 < r.created_at.0 || at.0 >= expires {
                    return Err(ProvisionApprovalError::InvalidPersistence);
                }
                let mut req = source.native_verify_request()?;
                req.purpose = crate::approval::VerificationPurposeV1::InspectOrigin;
                req.verify_nonce = inspection.verify_nonce.clone();
                if inspection.request_bytes.as_deref()
                    != Some(
                        serde_json::to_string(&req)
                            .map_err(|_| ProvisionApprovalError::InvalidPersistence)?
                            .as_str(),
                    )
                {
                    return Err(ProvisionApprovalError::InvalidPersistence);
                }
            }
            if let Some(signed) = &inspection.verification {
                status_recovery_source::validate_recovery_origin_fact(
                    source,
                    inspection,
                    signed,
                    inspection
                        .verified_at
                        .ok_or(ProvisionApprovalError::InvalidPersistence)?
                        .0,
                )?;
            }
            if let Some(at) = r.claimed_at {
                let signed = inspection
                    .verification
                    .as_ref()
                    .ok_or(ProvisionApprovalError::InvalidPersistence)?;
                if at.0 < r.created_at.0 || at.0 >= expires {
                    return Err(ProvisionApprovalError::InvalidPersistence);
                }
                status_recovery_source::validate_recovery_origin_fact(
                    source, inspection, signed, at.0,
                )?;
            }
            match (&r.result, r.completed_at, r.result_private_sid_generation) {
                (None, None, None) => {}
                (Some(signed), Some(at), Some(generation)) => {
                    if r.claimed_at.is_none_or(|v| at.0 < v.0)
                        || generation != r.private_sid_generation
                        || signed.status.session_ref != old.status.session_ref
                        || !signed.status.active
                        || signed.status.frozen
                        || signed.status.pending
                    {
                        return Err(ProvisionApprovalError::InvalidPersistence);
                    }
                    signed
                        .validate()
                        .map_err(|_| ProvisionApprovalError::InvalidPersistence)?;
                    signed
                        .status
                        .validate_for(
                            &old.status.receipt,
                            &r.nonce,
                            r.native_binding_generation,
                            source.input().request.context.service_epoch,
                            at.0,
                        )
                        .map_err(|_| ProvisionApprovalError::InvalidPersistence)?;
                    status_recovery_source::validate_recovery_origin_fact(
                        source,
                        inspection,
                        inspection
                            .verification
                            .as_ref()
                            .ok_or(ProvisionApprovalError::InvalidPersistence)?,
                        at.0,
                    )?;
                }
                _ => return Err(ProvisionApprovalError::InvalidPersistence),
            }
            if matches!(r.funding, SessionStatusRecoveryFundingV1::Provision) {
                provision_charged = provision_charged
                    .checked_add(64)
                    .ok_or(ProvisionApprovalError::InvalidPersistence)?;
            }
        }
        Ok(provision_charged)
    }
    pub(super) fn managed_recovery_transport_cost(&self, id: &str) -> u64 {
        64*self.session_status_recoveries.values().filter(|v|matches!(&v.funding,SessionStatusRecoveryFundingV1::ManagedLogin{operation_id} if operation_id==id)).count() as u64
    }
    pub(in crate::provisioning) fn session_recovery_phase_replay(
        &self,
    ) -> Result<Vec<(ProvisionStage, PhaseOriginal, Option<String>)>> {
        let Some(id) = &self.session_recovery_activation else {
            return Ok(Vec::new());
        };
        let r = self
            .session_status_recoveries
            .get(id)
            .ok_or(ProvisionApprovalError::InvalidPersistence)?;
        let signed = r
            .result
            .as_ref()
            .ok_or(ProvisionApprovalError::InvalidPersistence)?;
        Ok(vec![(
            ProvisionStage::NativeSessionReady,
            PhaseOriginal {
                operation_id: id.clone(),
                body_digest: r.request.digest()?,
                receipt_digest: Some(
                    signed
                        .status
                        .digest()
                        .map_err(|_| ProvisionApprovalError::InvalidPersistence)?,
                ),
            },
            None,
        )])
    }
}
