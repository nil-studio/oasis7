//! Complete fixed Native-profile manifests for asynchronous crypto adapters.
use super::*;
use crate::approval_admission::{
    ContextSignatureFact, append_context_signature_checks, bind_context_body_checks,
    bind_context_reference_checks,
};
use zeroize::Zeroizing;
impl AccountProvisionApprovalState {
    fn context_base_checks(
        &self,
        id: &str,
        current: &TestSessionLiveCurrent<'_, '_>,
        now: UnixMillis,
    ) -> Result<ExactSignatureChecks> {
        let s = self.test_sessions.get(id).ok_or_else(invalid)?;
        tool_nonce::native_registration(s)?;
        if s.source_funding.get(current.source_operation_id)
            != Some(&SessionSourceFundingV1::Execution)
        {
            return Err(invalid());
        }
        self.test_session_live_checks(id, SessionPurposeV1::Execution, current, now)
    }
    pub fn test_session_context_original_checks(
        &self,
        id: &str,
        original: &ws::SignedReadRequest<TaskContextPrepareV1>,
        current: &TestSessionLiveCurrent<'_, '_>,
        now: UnixMillis,
    ) -> Result<ExactSignatureChecks> {
        let checks = self.context_base_checks(id, current, now)?;
        verify_prepare(original).map_err(|_| invalid())?;
        let s = self.test_sessions.get(id).ok_or_else(invalid)?;
        if original.request.fence != s.termination.initial_fence
            || original.request.authority_mode
                != Some(oasis7_agent_api::game::TaskContextAuthorityModeV1::NativeExecutorV1)
            || original.subject_public_key
                != s.termination
                    .initial_fence
                    .executor_binding
                    .as_ref()
                    .ok_or_else(invalid)?
                    .executor_public_key
        {
            return Err(invalid());
        }
        Ok(append_context_signature_checks(
            checks,
            ContextSignatureFact::Prepare(original),
        )?)
    }
    pub(super) fn prepared_context_body(
        &self,
        id: &str,
        response: &ws::SignedServiceResponse<PreparedTaskContextV1>,
        current: &TestSessionLiveCurrent<'_, '_>,
    ) -> Result<ValidatedContextBody> {
        let c = self
            .test_sessions
            .get(id)
            .and_then(|s| s.context.as_ref())
            .ok_or_else(invalid)?;
        if c.prepared_source != current.source_operation_id {
            return Err(invalid());
        }
        ValidatedContextBody::from_prepared(
            self.context_scope(id, ContextBodySlotKindV1::Prepared)?,
            &c.original,
            response,
        )
        .map_err(body_error)
    }
    pub(super) fn captured_context_body(
        &self,
        id: &str,
        response: &ws::SignedServiceResponse<CapturedTaskContextV1>,
        current: &TestSessionLiveCurrent<'_, '_>,
    ) -> Result<ValidatedContextBody> {
        let c = self
            .test_sessions
            .get(id)
            .and_then(|s| s.context.as_ref())
            .ok_or_else(invalid)?;
        if c.captured_source.as_deref() != Some(current.source_operation_id) {
            return Err(invalid());
        }
        ValidatedContextBody::from_captured(
            self.context_scope(id, ContextBodySlotKindV1::Captured)?,
            &c.original,
            c.get.as_ref().ok_or_else(invalid)?,
            c.prefix.as_ref().ok_or_else(invalid)?,
            response,
        )
        .map_err(body_error)
    }
    pub fn test_session_prepared_context_checks(
        &self,
        id: &str,
        response: &ws::SignedServiceResponse<PreparedTaskContextV1>,
        current: &TestSessionLiveCurrent<'_, '_>,
        now: UnixMillis,
    ) -> Result<ExactSignatureChecks> {
        let body = self.prepared_context_body(id, response, current)?;
        self.context_complete_checks(id, &body, current, now)
    }
    pub fn test_session_captured_context_checks(
        &self,
        id: &str,
        response: &ws::SignedServiceResponse<CapturedTaskContextV1>,
        current: &TestSessionLiveCurrent<'_, '_>,
        now: UnixMillis,
    ) -> Result<ExactSignatureChecks> {
        let body = self.captured_context_body(id, response, current)?;
        self.context_complete_checks(id, &body, current, now)
    }
    /// The returned manifest authenticates a full body; it is not a usable context permit.
    pub fn test_session_context_body_checks(
        &self,
        id: &str,
        body: &ValidatedContextBody,
        current: &TestSessionLiveCurrent<'_, '_>,
        now: UnixMillis,
    ) -> Result<ExactSignatureChecks> {
        self.context_complete_checks(id, body, current, now)
    }
    pub(super) fn context_complete_checks(
        &self,
        id: &str,
        body: &ValidatedContextBody,
        current: &TestSessionLiveCurrent<'_, '_>,
        now: UnixMillis,
    ) -> Result<ExactSignatureChecks> {
        let checks = self.context_base_checks(id, current, now)?;
        self.context_body_matches(id, body)?;
        self.append_context_body_manifest(id, checks, body)
    }
    pub(super) fn context_historical_body_checks(
        &self,
        id: &str,
        body: &ValidatedContextBody,
    ) -> Result<ExactSignatureChecks> {
        self.context_body_matches(id, body)?;
        let reference = self
            .test_sessions
            .get(id)
            .and_then(|s| s.context.as_ref())
            .and_then(|c| c.slots.get(&body.scope().slot))
            .ok_or_else(invalid)?;
        let checks = crate::approval_admission::context_historical_checks(id, reference)?;
        self.append_context_body_manifest(id, checks, body)
    }
    fn append_context_body_manifest(
        &self,
        id: &str,
        mut checks: ExactSignatureChecks,
        body: &ValidatedContextBody,
    ) -> Result<ExactSignatureChecks> {
        let key = &body.scope().world.service_public_key;
        match body.evidence() {
            ContextBodyEvidenceV1::Prepared { original, .. } => {
                let response = body.prepared_response().map_err(body_error)?;
                checks = append_context_signature_checks(
                    checks,
                    ContextSignatureFact::Prepared {
                        original,
                        response: &response,
                        world_key: key,
                    },
                )?;
            }
            ContextBodyEvidenceV1::Captured {
                original, prefix, ..
            } => {
                let response = body.captured_response().map_err(body_error)?;
                let prepared = self.context_prepared_dependency(id, &response.payload.prepared)?;
                checks = self.append_context_body_manifest(id, checks, &prepared)?;
                let prefix = prefix.decode().map_err(body_error)?;
                checks = append_context_signature_checks(
                    checks,
                    ContextSignatureFact::Get {
                        original,
                        prefix: &prefix,
                        world_key: key,
                    },
                )?;
                checks = append_context_signature_checks(
                    checks,
                    ContextSignatureFact::Captured {
                        response: &response,
                        world_key: key,
                    },
                )?;
            }
        }
        Ok(bind_context_body_checks(
            checks,
            body.scope(),
            body.evidence(),
            body.canonical_bytes(),
        )?)
    }
    fn context_prepared_dependency(
        &self,
        id: &str,
        prepared: &PreparedTaskContextV1,
    ) -> Result<ValidatedContextBody> {
        let c = self
            .test_sessions
            .get(id)
            .and_then(|s| s.context.as_ref())
            .ok_or_else(invalid)?;
        let r = c
            .slots
            .get(&ContextBodySlotKindV1::Prepared)
            .ok_or_else(invalid)?;
        let bytes = Zeroizing::new(
            oasis7_wasm_abi::encode_canonical_cbor(prepared).map_err(|_| invalid())?,
        );
        if bytes.len() as u64 != r.plaintext_bytes.0
            || crate::approval::bytes_digest(&bytes) != r.plaintext_digest
            || r.scope != self.context_scope(id, ContextBodySlotKindV1::Prepared)?
            || !matches!(r.evidence, ContextBodyEvidenceV1::Prepared { .. })
        {
            return Err(ProvisionApprovalError::Conflict);
        }
        let body = ValidatedContextBody::revalidate(r.scope.clone(), &r.evidence, &bytes)
            .map_err(body_error)?;
        self.context_body_matches(id, &body)?;
        Ok(body)
    }
    pub fn test_session_context_get_checks(
        &self,
        id: &str,
        prepared: &ValidatedContextBody,
        original: &ws::SignedReadRequest<TaskContextGetV1>,
        prefix: &ContextPrefixProofV1,
        current: &TestSessionLiveCurrent<'_, '_>,
        now: UnixMillis,
    ) -> Result<ExactSignatureChecks> {
        let checks = self.context_complete_checks(id, prepared, current, now)?;
        let c = self
            .test_sessions
            .get(id)
            .and_then(|s| s.context.as_ref())
            .ok_or_else(invalid)?;
        if prepared.scope().slot != ContextBodySlotKindV1::Prepared
            || !c.slots.contains_key(&ContextBodySlotKindV1::Prepared)
            || !c.attempts.values().any(|a| {
                a.slot == ContextBodySlotKindV1::Prepared
                    && a.observation.as_ref().is_some_and(|f| f.stored.is_some())
            })
            || current.source_operation_id == c.prepared_source
        {
            return Err(invalid());
        }
        verify_get(original).map_err(|_| invalid())?;
        if original.request.context != *prepared.evidence().identity()
            || original.request.prepare_operation_id != c.original.request.client_operation_id
            || original.subject_public_key != c.original.subject_public_key
        {
            return Err(invalid());
        }
        let expected =
            public_prefix(&prepared.prepared().map_err(body_error)?).map_err(|_| invalid())?;
        let oasis7_client_api::world_service_wire::WorldServicePayloadV1::Scheduler(signed) =
            &prefix.original.signed_payload
        else {
            return Err(invalid());
        };
        if signed.request != expected {
            return Err(invalid());
        }
        prefix
            .validate(
                &self.context_scope(id, ContextBodySlotKindV1::Captured)?,
                &original.request.context,
            )
            .map_err(body_error)?;
        Ok(append_context_signature_checks(
            checks,
            ContextSignatureFact::Get {
                original,
                prefix,
                world_key: &prepared.scope().world.service_public_key,
            },
        )?)
    }
    /// Ref-only authorization of one fixed storage read. No missing body is
    /// treated as verified: full outer/nested jobs run after the actual read.
    pub fn test_session_context_read_checks(
        &self,
        id: &str,
        slot: ContextBodySlotKindV1,
        port: ContextBodyPortV1,
        current: &TestSessionLiveCurrent<'_, '_>,
        now: UnixMillis,
    ) -> Result<ExactSignatureChecks> {
        if port == ContextBodyPortV1::Write {
            return Err(invalid());
        }
        let purpose = if port == ContextBodyPortV1::RecoverMetadata {
            SessionPurposeV1::ReconcileRead
        } else {
            SessionPurposeV1::Execution
        };
        let s = self.test_sessions.get(id).ok_or_else(invalid)?;
        tool_nonce::native_registration(s)?;
        if s.source_funding
            .get(current.source_operation_id)
            .and_then(|v| v.purpose())
            != Some(purpose)
        {
            return Err(invalid());
        }
        let mut checks = self.test_session_live_checks(id, purpose, current, now)?;
        let c = s.context.as_ref().ok_or_else(invalid)?;
        let r = c.slots.get(&slot).ok_or_else(invalid)?;
        r.evidence.validate_metadata(&r.scope).map_err(body_error)?;
        if r.scope != self.context_scope(id, slot)? {
            return Err(invalid());
        }
        checks =
            append_context_signature_checks(checks, ContextSignatureFact::Prepare(&c.original))?;
        if let ContextBodyEvidenceV1::Captured {
            original, prefix, ..
        } = &r.evidence
        {
            let prefix = prefix.decode().map_err(body_error)?;
            checks = append_context_signature_checks(
                checks,
                ContextSignatureFact::Get {
                    original,
                    prefix: &prefix,
                    world_key: &r.scope.world.service_public_key,
                },
            )?;
        }
        Ok(bind_context_reference_checks(checks, r)?)
    }
}
