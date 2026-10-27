use super::*;
impl AccountProvisionApprovalState {
    pub fn participant_policy_checks(
        &self,
        id: &str,
        original: &ParticipantPolicyOriginalV1,
        session: &ParticipantSessionEvidenceV1,
        scope: &NativeStageScope<'_>,
        current: &ProvisionApprovalContext<'_>,
        now: u64,
    ) -> Result<ExactSignatureChecks> {
        let o = self.participant_current(id, scope, current, now)?;
        if !o.claims.contains_key("policy")
            || !o.callbacks.contains_key("policy")
            || o.policy.as_ref().is_some_and(|v| v != original)
        {
            return Err(bad());
        }
        validation::policy(o, original)?;
        let checks = self.participant_session_checks(
            id,
            ParticipantStepV1::Policy,
            ParticipantSessionObservation {
                evidence: session,
                checked_at: now,
            },
            scope,
            current,
            self.participant_checks(id, scope, current, now)?,
        )?;
        Ok(append_participant_signature_checks(
            checks,
            ParticipantSignatureFact::Policy(original, &o.descriptor.world.service_public_key),
        )?)
    }
    #[expect(
        clippy::too_many_arguments,
        reason = "Keep fixed World policy, native session evidence and current authority independent"
    )]
    pub fn record_participant_policy(
        &mut self,
        id: &str,
        original: ParticipantPolicyOriginalV1,
        session: ParticipantSessionEvidenceV1,
        scope: &NativeStageScope<'_>,
        current: &ProvisionApprovalContext<'_>,
        now: u64,
        verifier: &mut impl ApprovalSignatureVerifier,
    ) -> Result<()> {
        if let Some(old) = self.participant_original(id).filter(|v| v.policy.is_some()) {
            return if old.policy.as_ref() == Some(&original)
                && old.policy_session.as_ref() == Some(&session)
            {
                Ok(())
            } else {
                Err(bad())
            };
        }
        verifier.verify(
            &self.participant_policy_checks(id, &original, &session, scope, current, now)?,
        )?;
        let mut next = self.clone();
        let o = next.participant_provisions.get_mut(id).ok_or_else(bad)?;
        o.policy = Some(original);
        o.policy_session = Some(session);
        o.policy_recorded_at = Some(DecimalU64(now));
        next.validate_persistence()?;
        *self = next;
        Ok(())
    }
    pub fn participant_bundle_checks(
        &self,
        id: &str,
        bundle: &ParticipantSignBundleV1,
        scope: &NativeStageScope<'_>,
        current: &ProvisionApprovalContext<'_>,
        now: u64,
    ) -> Result<ExactSignatureChecks> {
        let o = self.participant_current(id, scope, current, now)?;
        if !o.claims.contains_key("sign_bundle")
            || o.policy.is_none()
            || o.bundle.as_ref().is_some_and(|v| v != bundle)
        {
            return Err(bad());
        }
        validation::bundle(o, bundle)?;
        Ok(append_participant_signature_checks(
            self.participant_checks(id, scope, current, now)?,
            ParticipantSignatureFact::Bundle(bundle),
        )?)
    }
    #[expect(
        clippy::too_many_arguments,
        reason = "Publish is the prepaid signer refresh, not a second callback"
    )]
    pub fn publish_participant_bundle(
        &mut self,
        id: &str,
        ticket: u8,
        bundle: ParticipantSignBundleV1,
        scope: &NativeStageScope<'_>,
        current: &ProvisionApprovalContext<'_>,
        now: u64,
        verifier: &mut impl ApprovalSignatureVerifier,
    ) -> Result<ParticipantSignBundleV1> {
        let o = self.participant_original(id).ok_or_else(bad)?;
        if let Some(old) = &o.bundle {
            return if old == &bundle {
                Ok(old.clone())
            } else {
                Err(bad())
            };
        }
        if ticket == 0 || ticket > o.oracle_reads {
            return Err(bad());
        }
        verifier.verify(&self.participant_bundle_checks(id, &bundle, scope, current, now)?)?;
        let mut next = self.clone();
        let o = next.participant_provisions.get_mut(id).ok_or_else(bad)?;
        o.bundle = Some(bundle.clone());
        o.bundle_recorded_at = Some(DecimalU64(now));
        next.validate_persistence()?;
        *self = next;
        Ok(bundle)
    }
    pub fn participant_admission_checks(
        &self,
        id: &str,
        admission: &SignedReadRequest<GameParticipantAdmissionV1>,
        session: &ParticipantSessionEvidenceV1,
        scope: &NativeStageScope<'_>,
        current: &ProvisionApprovalContext<'_>,
        now: u64,
    ) -> Result<ExactSignatureChecks> {
        let o = self.participant_current(id, scope, current, now)?;
        if !o.claims.contains_key("authorize")
            || !o.callbacks.contains_key("admission")
            || o.bundle.is_none()
            || o.admission.as_ref().is_some_and(|v| v != admission)
        {
            return Err(bad());
        }
        validation::admission(o, admission)?;
        let checks = self.participant_session_checks(
            id,
            ParticipantStepV1::Authorize,
            ParticipantSessionObservation {
                evidence: session,
                checked_at: now,
            },
            scope,
            current,
            self.participant_checks(id, scope, current, now)?,
        )?;
        Ok(append_participant_signature_checks(
            checks,
            ParticipantSignatureFact::Admission(admission, validation::issuer(o)?),
        )?)
    }
    #[expect(
        clippy::too_many_arguments,
        reason = "Native admission and signed current native session are distinct authorities"
    )]
    pub fn record_participant_admission(
        &mut self,
        id: &str,
        admission: SignedReadRequest<GameParticipantAdmissionV1>,
        session: ParticipantSessionEvidenceV1,
        scope: &NativeStageScope<'_>,
        current: &ProvisionApprovalContext<'_>,
        now: u64,
        verifier: &mut impl ApprovalSignatureVerifier,
    ) -> Result<()> {
        if let Some(o) = self
            .participant_original(id)
            .filter(|o| o.admission.is_some())
        {
            return if o.admission.as_ref() == Some(&admission)
                && o.admission_session.as_ref() == Some(&session)
            {
                Ok(())
            } else {
                Err(bad())
            };
        }
        verifier.verify(
            &self.participant_admission_checks(id, &admission, &session, scope, current, now)?,
        )?;
        let mut next = self.clone();
        let o = next.participant_provisions.get_mut(id).ok_or_else(bad)?;
        let (submit, lookup, owner_view) = validation::wire(o, &admission)?;
        o.submit_bytes = Some(serde_json::to_string(&submit).map_err(|_| bad())?);
        o.submit = Some(submit);
        o.lookup = Some(lookup);
        o.owner_view = Some(owner_view);
        o.admission = Some(admission);
        o.admission_session = Some(session);
        o.admission_recorded_at = Some(DecimalU64(now));
        next.validate_persistence()?;
        *self = next;
        Ok(())
    }
    pub fn participant_result_checks(
        &self,
        id: &str,
        result: &ParticipantFinalResultV1,
        session: &ParticipantSessionEvidenceV1,
        scope: &NativeStageScope<'_>,
        current: &ProvisionApprovalContext<'_>,
        now: u64,
    ) -> Result<ExactSignatureChecks> {
        let o = self.participant_current(id, scope, current, now)?;
        if !o.claims.contains_key("submit")
            || !o.claims.contains_key("owner_view")
            || !o.callbacks.contains_key("final")
        {
            return Err(bad());
        }
        validation::result(o, result)?;
        let checks = self.participant_session_checks(
            id,
            ParticipantStepV1::Submit,
            ParticipantSessionObservation {
                evidence: &result.submit_session,
                checked_at: now,
            },
            scope,
            current,
            self.participant_checks(id, scope, current, now)?,
        )?;
        let checks = match (&result.response_source, &result.lookup_session) {
            (ParticipantResultSourceV1::Submit, None) => checks,
            (ParticipantResultSourceV1::Lookup, Some(evidence)) => self
                .participant_session_checks_nonce(
                    id,
                    &source::participant_lookup_nonce(o)?,
                    ParticipantSessionObservation {
                        evidence,
                        checked_at: now,
                    },
                    scope,
                    current,
                    checks,
                )?,
            _ => return Err(bad()),
        };
        let checks = self.participant_session_checks(
            id,
            ParticipantStepV1::OwnerView,
            ParticipantSessionObservation {
                evidence: session,
                checked_at: now,
            },
            scope,
            current,
            checks,
        )?;
        Ok(append_participant_signature_checks(
            checks,
            ParticipantSignatureFact::Final(result, &o.descriptor.world.service_public_key),
        )?)
    }
    #[expect(
        clippy::too_many_arguments,
        reason = "Only canonical receipt plus independent owner and native session observations settle"
    )]
    pub fn complete_participant(
        &mut self,
        id: &str,
        result: ParticipantFinalResultV1,
        session: ParticipantSessionEvidenceV1,
        scope: &NativeStageScope<'_>,
        current: &ProvisionApprovalContext<'_>,
        now: u64,
        verifier: &mut impl ApprovalSignatureVerifier,
    ) -> Result<GameParticipantReceiptV1> {
        let old = self.participant_original(id).ok_or_else(bad)?;
        if let Some(saved) = &old.result {
            return if saved == &result && old.result_session.as_ref() == Some(&session) {
                validation::result(old, saved)
            } else {
                Err(bad())
            };
        }
        verifier
            .verify(&self.participant_result_checks(id, &result, &session, scope, current, now)?)?;
        let receipt = validation::result(old, &result)?;
        let mut next = self.clone();
        let o = next.participant_provisions.get_mut(id).ok_or_else(bad)?;
        o.result = Some(result);
        o.result_session = Some(session);
        o.completed_at = Some(DecimalU64(now));
        let body = digest(&o.submit.as_ref().ok_or_else(bad)?)?;
        let result_digest = digest(&o.result.as_ref().ok_or_else(bad)?)?;
        next.provision.save_original(
            ProvisionStage::ParticipantBound,
            PhaseOriginal {
                operation_id: id.into(),
                body_digest: body,
                receipt_digest: None,
            },
            1,
            now,
        )?;
        next.provision.confirm_phase(
            ProvisionStage::ParticipantBound,
            id,
            &result_digest,
            None,
            1,
            now,
        )?;
        next.validate_persistence()?;
        *self = next;
        Ok(receipt)
    }
}
