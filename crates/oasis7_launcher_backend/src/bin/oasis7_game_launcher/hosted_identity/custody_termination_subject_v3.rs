//! Exact managed machine-subject gate, not ordinary browser session auth.
//! Mounted as a child of custody_managed_binding to reuse its real predicates.
use super::*;
use oasis7_custody_core::provisioning::NativeSubjectRefV3;

/// Rust-only observation witness. No serialization, public constructor, or effect authority.
/// This alone does not attest a completed UV approval or CF transport payment.
pub(in super::super) struct ManagedTerminationSubjectWitnessV3 {
    _subject: NativeSubjectRefV3,
}

// Private operation mode. Neither result carries CF selected-book, payment,
// signing or dispatch authority; inspection can be reconstructed after restart.
enum SubjectObservationMode { FreshConsume, CompletedOrigin }

impl IdentityService {
    pub(in super::super) fn inspect_completed_managed_subject_v3(
        &mut self, sid: Option<&str>, approval_id: &str,
        profile: termination_v3::TerminationApprovalProfile,
        expected_request: &ApprovalOptionsRequestV1, now: u64,
    ) -> Result<ManagedTerminationSubjectWitnessV3, StoreError> {
        self.observe_managed_termination_subject_v3(sid, approval_id, profile,
            expected_request, now, SubjectObservationMode::CompletedOrigin)
    }
}

impl IdentityService {
    pub(in super::super) fn require_managed_termination_subject_v3(
        &mut self,
        sid: Option<&str>,
        approval_id: &str,
        profile: termination_v3::TerminationApprovalProfile,
        expected_request: &ApprovalOptionsRequestV1,
        now: u64,
    ) -> Result<ManagedTerminationSubjectWitnessV3, StoreError> {
        self.observe_managed_termination_subject_v3(sid, approval_id, profile,
            expected_request, now, SubjectObservationMode::FreshConsume)
    }

    fn observe_managed_termination_subject_v3(
        &mut self, sid: Option<&str>, approval_id: &str,
        profile: termination_v3::TerminationApprovalProfile,
        expected_request: &ApprovalOptionsRequestV1,
        now: u64, mode: SubjectObservationMode,
    ) -> Result<ManagedTerminationSubjectWitnessV3, StoreError> {
        if !self.clock.observe(now) {
            return Err(reject("clock_unhealthy"));
        }
        // Require actual persisted completed approval, never caller completion metadata.
        let CustodyIntentV1::TerminationPlanV3 { plan } = &expected_request.intent else {
            return Err(reject("invalid_termination_approval"));
        };
        let saved = self.store.transaction(|tx| {
            tx.get::<ApprovalRecord>(&approval_key(approval_id))?
                .map(|r| r.value).ok_or_else(|| reject("approval_history_unavailable"))
        })?;
        let CustodyIntentV1::TerminationPlanV3 { plan: _approved } = &saved.request.intent else {
            return Err(reject("invalid_termination_approval"));
        };
        if saved.proof_bytes.is_none() || saved.complete_digest.is_none()
            || serde_json::to_value(&saved.request).map_err(|_| StoreError::Corrupt)?
                != serde_json::to_value(expected_request).map_err(|_| StoreError::Corrupt)? {
            return Err(reject("approval_original_mismatch"));
        }
        // Profile is a Rust-only explicit enum; production cannot admit dev policy.
        termination_v3::enforce_profile(&saved.request, profile, now)?;
        let token = sid.ok_or_else(|| reject("session_expired"))?;
        let key = session_key(&self.config.session_key, token);
        let expected = &plan.native_subject;
        // Reject managerWebAuthn and importedpublic_key credentials before
        // touching any effect/lookup journal or leaving Native for World.
        self.store.transaction(|tx| {
            let (account, session) =
                valid_session(tx, &key, self.config.rp.identity_instance_epoch, now)?;
            let credential = tx
                .get::<CredentialRecord>(&credential_key(&session.origin_credential_id))?
                .ok_or_else(|| reject("authentication_failed"))?
                .value;
            if session.auth_method != "native_key"
                || credential.registered_via != "machine_registration"
                || session.account_id != expected.native_account_id
                || account.account_id != expected.native_account_id
                || session.session_id != expected.native_session_ref
                || session.origin_credential_id != expected.native_credential_ref
                || session.origin_credential_version != expected.native_credential_version.0
                || credential.version != expected.native_credential_version.0
                || account.security_epoch != expected.native_account_security_epoch.0
                || !matches!(&credential.public_material,CredentialMaterial::NativeKey{public_key}
                    if hex::encode(public_key)==expected.login_public_key)
            {
                return Err(reject("termination_native_subject_mismatch"));
            }
            Ok(())
        })?;
        // This is a local Native call, not another HTTP. It reuses the existing
        // binding_is_current / active / noPending / exactcredentialNativeKey
        // implementation; it does not create a parallel weaker predicate.
        let nonce = blake3::hash(random_token().as_bytes()).to_hex().to_string();
        let value =
            self.managed_session_observation(sid, &serde_json::json!({"nonce":nonce}), now, false)?;
        let signed: SignedManagedNativeSessionStatusV1 =
            serde_json::from_value(value).map_err(|_| StoreError::Corrupt)?;
        let status = &signed.status;
        let actual = NativeSubjectRefV3::from_status(status)
            .map_err(|_| reject("termination_native_subject_mismatch"))?;
        if !status.active
            || status.frozen
            || status.pending
            || actual != *expected
            || status.receipt.custody_account_id != plan.custody_account_id
            || status.receipt.provision_id != plan.provision_id
            || status.receipt.environment != saved.request.context.environment
        {
            return Err(reject("termination_native_subject_mismatch"));
        }
        let approval: SignedCustodyApprovalV1 = serde_json::from_slice(
            saved.proof_bytes.as_deref().ok_or(StoreError::Corrupt)?
        ).map_err(|_| StoreError::Corrupt)?;
        let request = ApprovalVerifyRequestV1 {
            proof_digest: approval.digest().map_err(|_| StoreError::Corrupt)?,
            expected_audience: approval.proof.context.audience.clone(),
            expected_intent_digest: saved.intent_digest.clone(),
            approval, purpose: match mode {
                SubjectObservationMode::FreshConsume => VerificationPurposeV1::ConsumeApproval,
                SubjectObservationMode::CompletedOrigin => VerificationPurposeV1::InspectOrigin,
            },
            verify_nonce: nonce,
        };
        let secret = self.custody.as_ref().ok_or_else(|| reject("custody_disabled"))?.adapter_secret;
        let headers = format!("POST / HTTP/1.1\r\nAuthorization: Bearer {}\r\nContent-Type: application/json\r\n\r\n", hex::encode(secret));
        let verified = self.custody_verify_route_profile("POST", &headers,
            &serde_json::to_value(request).map_err(|_| StoreError::Corrupt)?,
            "termination-subject-local", now, profile)?;
        let verified: SignedApprovalVerificationV1 = serde_json::from_value(verified)
            .map_err(|_| StoreError::Corrupt)?;
        // InspectOrigin reports revoked/unavailable rather than rejecting it.
        // Gateway observation must require actual current origin explicitly.
        if verified.verification.origin_state != OriginStateV1::Active {
            return Err(reject("approval_expired_or_revoked"));
        }
        Ok(ManagedTerminationSubjectWitnessV3 { _subject: actual })
    }
}
