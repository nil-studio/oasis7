//! Strong historical LF archive and separate fresh admission. No dispatch.
use super::super::test_session::{SessionEventKindV2, append_session_event_v2};
use super::*;
fn invalid() -> ProvisionApprovalError {
    ProvisionApprovalError::BindingMismatch
}
pub(super) fn fact_shape(
    state: &AccountProvisionApprovalState,
    s: &TestSessionAuthorityV2,
    b: &SessionExecutionBookV2,
    fact: &crate::native_binding::SignedManagedNativeSessionLifetimeV1,
) -> Result<()> {
    let initial = s.native_lifetime.as_ref().ok_or_else(invalid)?;
    let observed = fact.lifetime.status.observed_at.0;
    if fact.lifetime.status.session_ref != s.session.session_ref
        || fact.lifetime.status.receipt.identity_namespace != s.session.scope.identity_namespace
    {
        return Err(invalid());
    }
    fact.validate().map_err(|_| invalid())?;
    fact.lifetime
        .status
        .validate_for(
            &initial.lifetime.status.receipt,
            &state.test_session_v2_lifetime_nonce(&b.session_id, &b.source_operation_id)?,
            s.session.native_binding_generation,
            s.input.request.context.service_epoch,
            observed,
        )
        .map_err(|_| invalid())?;
    Ok(())
}
impl AccountProvisionApprovalState {
    pub fn test_session_v2_execution_lifetime_archive_checks(
        &self,
        session_id: &str,
        fact: &crate::native_binding::SignedManagedNativeSessionLifetimeV1,
        native: &NativeStageScope<'_>,
    ) -> Result<ExactSignatureChecks> {
        let s = self.test_session_v2_record(session_id)?;
        let b = s.execution_book.as_ref().ok_or_else(invalid)?;
        fact_shape(self, s, b, fact)?;
        // An independent fixed pin also authenticates the saved initial root
        // fact; a packet-provided key is never accepted as a trust source.
        s.native_lifetime
            .as_ref()
            .ok_or_else(invalid)?
            .verify_root(native.native_root_key_id, native.native_root_public_key)
            .map_err(|_| invalid())?;
        registered_native_fact_checks(
            &b.action_request_id,
            &self.test_session_v2_lifetime_nonce(session_id, &b.source_operation_id)?,
            s.book
                .original(session_id)
                .ok_or_else(invalid)?
                .recorded_context(),
            NativeFact::Lifetime(fact),
            native.native_root_key_id,
            native.native_root_public_key,
            &digest(&(
                "oasis7.custody.execution-lifetime-fact.v2",
                &b.approved_input_digest,
                &b.source_operation_id,
                fact,
            ))?,
        )
        .map_err(Into::into)
    }
    pub fn archive_test_session_v2_execution_lifetime(
        &mut self,
        session_id: &str,
        fact: &crate::native_binding::SignedManagedNativeSessionLifetimeV1,
        native: &NativeStageScope<'_>,
        now: UnixMillis,
        verifier: &mut impl ApprovalSignatureVerifier,
    ) -> Result<()> {
        // This deliberately has no local/Source/LF/current-clock admission.
        // Late authentic facts remain historical and cannot re-open the book.
        let checks =
            self.test_session_v2_execution_lifetime_archive_checks(session_id, fact, native)?;
        verifier.verify(&checks)?;
        let s = self.test_session_v2_record(session_id)?;
        let b = s.execution_book.as_ref().ok_or_else(invalid)?;
        if let Some(old) = &b.archived_lifetime {
            return if old == fact {
                Ok(())
            } else {
                Err(ProvisionApprovalError::Conflict)
            };
        }
        let request_id = b.action_request_id.clone();
        let source_operation_id = b.source_operation_id.clone();
        let mut next = self.clone();
        let saved = next.test_session_v2_record_mut(session_id)?;
        let b = saved.execution_book.as_mut().ok_or_else(invalid)?;
        b.archived_lifetime = Some(fact.clone());
        b.lifetime_archived_at = Some(now);
        append_session_event_v2(
            saved,
            SessionEventKindV2::ExecutionLifetimeArchived {
                request_id,
                source_operation_id,
            },
            now,
        )?;
        next.validate_persistence()?;
        *self = next;
        Ok(())
    }
    pub fn admit_test_session_v2_execution_lifetime(
        &mut self,
        session_id: &str,
        current: &TestSessionLiveCurrent<'_, '_>,
        now: UnixMillis,
        verifier: &mut impl ApprovalSignatureVerifier,
    ) -> Result<()> {
        let s = self.test_session_v2_record(session_id)?;
        let b = s.execution_book.as_ref().ok_or_else(invalid)?;
        if s.roots
            .get(&SessionPurposeV1::Execution)
            .ok_or_else(invalid)?
            .credential
            .as_ref()
            .ok_or_else(invalid)?
            .generation
            != b.credential_generation
            || b.lifetime_admitted_at.is_some()
            || current.source_operation_id != b.source_operation_id
            || current.source_ticket != 4
            || b.archived_lifetime.as_ref() != Some(current.lifetime)
            || b.lifetime_archived_at.is_none_or(|at| at.get() > now.get())
        {
            return Err(invalid());
        }
        let archive = self.test_session_v2_execution_lifetime_archive_checks(
            session_id,
            current.lifetime,
            current.native_scope,
        )?;
        verifier.verify(&archive)?;
        verifier.verify(&self.test_session_v2_live_scope(
            session_id,
            SessionPurposeV1::Execution,
            current,
            now,
        )?)?;
        let request_id = b.action_request_id.clone();
        let source_operation_id = b.source_operation_id.clone();
        let mut next = self.clone();
        let saved = next.test_session_v2_record_mut(session_id)?;
        saved
            .execution_book
            .as_mut()
            .ok_or_else(invalid)?
            .lifetime_admitted_at = Some(now);
        append_session_event_v2(
            saved,
            SessionEventKindV2::ExecutionLifetimeAdmitted {
                request_id,
                source_operation_id,
            },
            now,
        )?;
        next.validate_persistence()?;
        *self = next;
        Ok(())
    }
}
