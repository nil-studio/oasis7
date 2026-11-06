//! Re-derive the signed supplemental binding from actual Account subject state.
//! The stored lifetime is an authenticated historical identity anchor; live
//! Native state is separately checked by the actualCurrent/gateway.
use super::*;
/// Trusted adapter inputs; never decoded as an HTTP authority flag.
pub struct TerminationInstallNativeCurrentV3<'a, 'b> {
    pub scope: &'a NativeStageScope<'b>,
    pub status: &'a crate::native_binding::SignedManagedNativeSessionStatusV1,
    pub private_sid_generation: DecimalU64,
}
impl AccountProvisionApprovalState {
    pub(super) fn require_termination_native_subject_v3(
        &self,
        plan: &TerminationPlanV3,
    ) -> Result<()> {
        use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
        let s = self
            .test_sessions
            .get(&plan.subject_session_id)
            .ok_or_else(invalid)?;
        let origin = &self
            .native_stages
            .values()
            .find(|v| {
                v.descriptor.registration.registration_operation_id
                    == s.session.scope.registration_operation_id
                    && v.descriptor.login_request_id == s.session.scope.initial_login_operation_id
            })
            .ok_or_else(invalid)?
            .descriptor
            .http_origin;
        if !self
            .participant_session_candidates(origin)?
            .iter()
            .any(|(_, _, reference, status)| {
                reference == &s.session
                    && NativeSubjectRefV3::from_status(&status.status)
                        .ok()
                        .as_ref()
                        == Some(&plan.native_subject)
            })
        {
            return Err(invalid());
        }
        let signed = s.native_lifetime.as_ref().ok_or_else(invalid)?;
        let status = &signed.lifetime.status;
        let derived = NativeSubjectRefV3::from_status(status).map_err(|_| invalid())?;
        if derived != plan.native_subject
            || derived.native_session_ref != s.session.session_ref
            || derived.identity_namespace != s.session.scope.identity_namespace
            || derived.identity_instance_epoch != s.session.scope.identity_instance_epoch
            || status.receipt.custody_account_id != plan.custody_account_id
            || status.receipt.provision_id != plan.provision_id
            || status.receipt.environment != s.input.request.context.environment
            || derived.login_key_ref != s.session.scope.role_scope.context.identity.key_id
            || status.generation != s.session.native_binding_generation
        {
            return Err(invalid());
        }
        let record = s.book.original(&s.input.operation_id).ok_or_else(invalid)?;
        let pin = record
            .recorded_context()
            .native_key_pins
            .get(&signed.key_id)
            .ok_or_else(invalid)?;
        if pin.len() != 64
            || !pin
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        {
            return Err(invalid());
        }
        let mut key = [0u8; 32];
        for (i, value) in key.iter_mut().enumerate() {
            *value = u8::from_str_radix(&pin[i * 2..i * 2 + 2], 16).map_err(|_| invalid())?;
        }
        let key = ed25519_dalek::VerifyingKey::from_bytes(&key).map_err(|_| invalid())?;
        if key.is_weak() {
            return Err(invalid());
        }
        let signature = URL_SAFE_NO_PAD
            .decode(&signed.signature)
            .map_err(|_| invalid())?;
        let signature = ed25519_dalek::Signature::from_slice(&signature).map_err(|_| invalid())?;
        key.verify_strict(
            &signed.lifetime.signing_bytes().map_err(|_| invalid())?,
            &signature,
        )
        .map_err(|_| invalid())?;
        let mut actual_identity = self.role_key_results.values().filter(|r| {
            r.scope == s.session.scope.role_scope
                && r.scope.context.identity.role == crate::http::Role::Identity
                && r.public_ref.key_id == derived.login_key_ref
                && r.public_ref.public_key == derived.login_public_key
        });
        if actual_identity.next().is_none() || actual_identity.next().is_some() {
            return Err(invalid());
        }
        Ok(())
    }
    pub(super) fn validate_installation_native_restore_v3(
        &self,
        saved: &TestSessionTerminationAuthorityV3,
    ) -> Result<()> {
        let (Some(at), Some(status)) = (
            saved.installed_at,
            saved.installation_native_status.as_ref(),
        ) else {
            return if saved.installed_at.is_none() && saved.installation_native_status.is_none() {
                Ok(())
            } else {
                Err(invalid())
            };
        };
        let original = saved
            .book
            .original(&saved.input.operation_id)
            .ok_or_else(invalid)?;
        let context = original.recorded_context();
        let reference = &self
            .test_sessions
            .get(&saved.plan.subject_session_id)
            .ok_or_else(invalid)?
            .session;
        let nonce = self.termination_install_native_nonce_v3(&saved.plan.subject_session_id)?;
        status
            .status
            .validate_for(
                &status.status.receipt,
                &nonce,
                reference.native_binding_generation,
                context.authority.service_epoch,
                at.get(),
            )
            .map_err(|_| invalid())?;
        if status.status.session_ref != reference.session_ref
            || !status.status.active
            || status.status.frozen
            || status.status.pending
            || NativeSubjectRefV3::from_status(&status.status).map_err(|_| invalid())?
                != saved.plan.native_subject
        {
            return Err(invalid());
        }
        let checks = saved.book.commit_checks(
            &saved.input.operation_id,
            original.verification().ok_or_else(invalid)?,
            context,
            at.get(),
        )?;
        let pin = context
            .native_key_pins
            .get(&status.key_id)
            .ok_or_else(invalid)?;
        let checks = append_native_history_checks(
            checks,
            context,
            NativeFact::Status(status),
            &status.key_id,
            pin,
            &saved.plan.digest().map_err(|_| invalid())?,
        )?;
        super::origin::verify_exact(&checks)
    }
    /// The caller supplies a real freshly observed signed status and the nonce
    /// reserved by Account for this approval operation, never historical metadata.
    pub(super) fn termination_native_consume_checks_v3(
        &self,
        plan: &TerminationPlanV3,
        status: &crate::native_binding::SignedManagedNativeSessionStatusV1,
        private_sid_generation: DecimalU64,
        expected_nonce: &str,
        scope: &NativeStageScope<'_>,
        current: &ProvisionApprovalContext<'_>,
        now: UnixMillis,
        checks: ExactSignatureChecks,
    ) -> Result<ExactSignatureChecks> {
        self.require_termination_native_subject_v3(plan)?;
        let s = self
            .test_sessions
            .get(&plan.subject_session_id)
            .ok_or_else(invalid)?;
        let candidates = self.participant_session_candidates(scope.http_origin)?;
        let (_, _, _, historical) = candidates
            .iter()
            .find(|(_, _, r, _)| r == &s.session)
            .ok_or_else(invalid)?;
        status
            .status
            .validate_for(
                &historical.status.receipt,
                expected_nonce,
                s.session.native_binding_generation,
                current.approval.authority.service_epoch,
                now.get(),
            )
            .map_err(|_| invalid())?;
        if private_sid_generation != s.session.private_sid_generation
            || status.status.session_ref != s.session.session_ref
            || !status.status.active
            || status.status.frozen
            || status.status.pending
            || NativeSubjectRefV3::from_status(&status.status).map_err(|_| invalid())?
                != plan.native_subject
        {
            return Err(invalid());
        }
        append_native_history_checks(
            checks,
            current.approval,
            NativeFact::Status(status),
            scope.native_root_key_id,
            scope.native_root_public_key,
            &plan.digest().map_err(|_| invalid())?,
        )
        .map_err(Into::into)
    }
}
