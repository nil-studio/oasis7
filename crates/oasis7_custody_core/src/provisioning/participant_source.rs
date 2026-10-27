use super::*;
use crate::approval::{
    ApprovalVerifyRequestV1, SignedApprovalVerificationV1, VerificationPurposeV1,
};
impl AccountProvisionApprovalState {
    pub fn participant_original(&self, id: &str) -> Option<&ParticipantOriginalV1> {
        self.participant_provisions.get(id)
    }
    pub fn participant_provisions(&self) -> &BTreeMap<String, ParticipantOriginalV1> {
        &self.participant_provisions
    }
    pub(super) fn participant_current(
        &self,
        id: &str,
        scope: &NativeStageScope<'_>,
        current: &ProvisionApprovalContext<'_>,
        now: u64,
    ) -> Result<&ParticipantOriginalV1> {
        self.check_native_current(current, 1, now)?;
        let original = self.participant_provisions.get(id).ok_or_else(bad)?;
        if original.result.is_some()
            || now < original.created_at.0
            || now >= original.expires_at.0
            || original.target_security_epoch.0 != 1
        {
            return Err(bad());
        }
        let nonces = ParticipantNoncesV1 {
            source: original.source_inspection.verify_nonce.clone(),
            policy: original.descriptor.policy_nonce.clone(),
            owner_view: original.descriptor.owner_view_nonce.clone(),
            session: original.session_nonce.clone(),
        };
        if self.participant_descriptor(&original.request, &nonces, scope)? != original.descriptor {
            return Err(bad());
        }
        Ok(original)
    }
    pub fn claim_participant_source(
        &mut self,
        id: &str,
        current: &ProvisionApprovalContext<'_>,
        now: u64,
    ) -> Result<ApprovalVerifyRequestV1> {
        self.check_native_current(current, 1, now)?;
        let o = self.participant_provisions.get(id).ok_or_else(bad)?;
        if o.source_inspection.claimed_at.is_some() || now >= o.source_inspection.expires_at.0 {
            return Err(bad());
        }
        let mut request = self.accepted_approval()?.native_verify_request()?;
        request.purpose = VerificationPurposeV1::InspectOrigin;
        request.verify_nonce = o.source_inspection.verify_nonce.clone();
        let mut next = self.clone();
        let i = &mut next
            .participant_provisions
            .get_mut(id)
            .ok_or_else(bad)?
            .source_inspection;
        i.claimed_at = Some(DecimalU64(now));
        i.request_bytes = Some(serde_json::to_string(&request).map_err(|_| bad())?);
        next.validate_persistence()?;
        *self = next;
        Ok(request)
    }
    pub fn participant_source_checks(
        &self,
        id: &str,
        signed: &SignedApprovalVerificationV1,
        current: &ProvisionApprovalContext<'_>,
        now: u64,
    ) -> Result<ExactSignatureChecks> {
        self.check_native_current(current, 1, now)?;
        let o = self.participant_provisions.get(id).ok_or_else(bad)?;
        self.validate_origin_response(&o.source_inspection, signed, now)?;
        let source = &current.approval.source;
        let v = &signed.verification;
        if o.source_inspection
            .verification
            .as_ref()
            .is_some_and(|v| v != signed)
            || v.source_session_expires_at != source.session_expires_at
            || v.source_session_idle_expires_at != source.session_idle_expires_at
        {
            return Err(bad());
        }
        Ok(registered_origin_checks(
            self.accepted_approval()?.input(),
            &o.source_inspection.operation_id,
            &o.source_inspection.verify_nonce,
            current.approval,
            signed,
        )?)
    }
    pub fn complete_participant_source(
        &mut self,
        id: &str,
        signed: SignedApprovalVerificationV1,
        current: &ProvisionApprovalContext<'_>,
        now: u64,
        verifier: &mut impl ApprovalSignatureVerifier,
    ) -> Result<()> {
        verifier.verify(&self.participant_source_checks(id, &signed, current, now)?)?;
        let mut next = self.clone();
        let i = &mut next
            .participant_provisions
            .get_mut(id)
            .ok_or_else(bad)?
            .source_inspection;
        i.verification = Some(signed);
        i.verified_at = Some(DecimalU64(now));
        next.validate_persistence()?;
        *self = next;
        Ok(())
    }
    pub fn participant_checks(
        &self,
        id: &str,
        scope: &NativeStageScope<'_>,
        current: &ProvisionApprovalContext<'_>,
        now: u64,
    ) -> Result<ExactSignatureChecks> {
        let o = self.participant_current(id, scope, current, now)?;
        let source = o.source_inspection.verification.as_ref().ok_or_else(bad)?;
        let mut checks = self.participant_source_checks(id, source, current, now)?;
        let (_, baseline) = self.participant_session(scope)?;
        checks = append_native_history_checks(
            checks,
            current.approval,
            NativeFact::Status(&baseline),
            scope.native_root_key_id,
            scope.native_root_public_key,
            &o.request.digest()?,
        )?;
        if let Some(policy) = &o.policy {
            validation::policy(o, policy)?;
            checks = append_participant_signature_checks(
                checks,
                ParticipantSignatureFact::Policy(policy, &o.descriptor.world.service_public_key),
            )?;
            if let Some(session) = &o.policy_session {
                checks = self.participant_session_checks(
                    id,
                    ParticipantStepV1::Policy,
                    ParticipantSessionObservation {
                        evidence: session,
                        checked_at: now,
                    },
                    scope,
                    current,
                    checks,
                )?;
            } else {
                return Err(bad());
            }
        }
        if let Some(bundle) = &o.bundle {
            validation::bundle(o, bundle)?;
            checks = append_participant_signature_checks(
                checks,
                ParticipantSignatureFact::Bundle(bundle),
            )?;
        }
        if let Some(admission) = &o.admission {
            validation::admission(o, admission)?;
            let issuer = validation::issuer(o)?;
            checks = append_participant_signature_checks(
                checks,
                ParticipantSignatureFact::Admission(admission, issuer),
            )?;
            checks = self.participant_session_checks(
                id,
                ParticipantStepV1::Authorize,
                ParticipantSessionObservation {
                    evidence: o.admission_session.as_ref().ok_or_else(bad)?,
                    checked_at: now,
                },
                scope,
                current,
                checks,
            )?;
        }
        Ok(checks)
    }
    pub(super) fn participant_session_checks(
        &self,
        id: &str,
        step: ParticipantStepV1,
        observation: ParticipantSessionObservation<'_>,
        scope: &NativeStageScope<'_>,
        current: &ProvisionApprovalContext<'_>,
        checks: ExactSignatureChecks,
    ) -> Result<ExactSignatureChecks> {
        let ParticipantSessionObservation {
            evidence,
            checked_at: now,
        } = observation;
        let o = self.participant_provisions.get(id).ok_or_else(bad)?;
        self.participant_session_checks_nonce(
            id,
            &participant_session_nonce(o, step)?,
            ParticipantSessionObservation {
                evidence,
                checked_at: now,
            },
            scope,
            current,
            checks,
        )
    }
    pub(super) fn participant_session_checks_nonce(
        &self,
        id: &str,
        expected: &str,
        observation: ParticipantSessionObservation<'_>,
        scope: &NativeStageScope<'_>,
        current: &ProvisionApprovalContext<'_>,
        checks: ExactSignatureChecks,
    ) -> Result<ExactSignatureChecks> {
        let ParticipantSessionObservation {
            evidence,
            checked_at: now,
        } = observation;
        let o = self.participant_provisions.get(id).ok_or_else(bad)?;
        evidence
            .status
            .status
            .validate_for(
                &o.descriptor.native_binding,
                expected,
                o.descriptor.session.native_binding_generation,
                current.approval.authority.service_epoch,
                now,
            )
            .map_err(|_| bad())?;
        let s = &evidence.status.status;
        if !s.active
            || s.frozen
            || s.pending
            || s.session_ref != o.descriptor.session.session_ref
            || evidence.private_sid_generation != o.descriptor.session.private_sid_generation
        {
            return Err(bad());
        }
        Ok(append_native_history_checks(
            checks,
            current.approval,
            NativeFact::Status(&evidence.status),
            scope.native_root_key_id,
            scope.native_root_public_key,
            &o.request.digest()?,
        )?)
    }
}
pub(super) fn participant_session_nonce(
    o: &ParticipantOriginalV1,
    step: ParticipantStepV1,
) -> Result<String> {
    digest(&(
        "oasis7.custody.participant-session-nonce.v1",
        &o.request,
        &o.session_nonce,
        step_name(if step == ParticipantStepV1::SignBundle {
            ParticipantStepV1::Policy
        } else {
            step
        }),
    ))
}

pub(super) fn participant_lookup_nonce(o: &ParticipantOriginalV1) -> Result<String> {
    digest(&(
        "oasis7.custody.participant-session-nonce.v1",
        &o.request,
        &o.session_nonce,
        "lookup",
    ))
}
