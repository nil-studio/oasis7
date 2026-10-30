//! Bounded private-body storage provenance, not a context-use or action permit.
use super::*;
use crate::context_body::*;
use oasis7_client_api::world_service::task_context::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ContextBodyPortV1 {
    Write,
    ReadBefore,
    ReadAfter,
    RecoverBody,
    RecoverMetadata,
}
/// The old generic wire remains a purpose string. ContextBody charges use a
/// closed object in the same paid ledger, so stripping a parallel context map
/// cannot turn their provenance into an unrelated legacy charge.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum SessionPurposeTransportV1 {
    GenericOne(SessionPurposeV1),
    ContextBody(Box<ContextBodyTransportV1>),
    LiveContextRead(Box<LiveContextReadTransportV1>),
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ContextBodyTransportKindV1 {
    ContextBody,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ContextBodyTransportV1 {
    pub kind: ContextBodyTransportKindV1,
    pub purpose: SessionPurposeV1,
    pub context_operation_id: String,
    pub slot: ContextBodySlotKindV1,
    pub port: ContextBodyPortV1,
    pub source_operation_id: String,
    pub claimed_at: UnixMillis,
    pub expected_scope: ContextBodyScopeV1,
    pub reference_digest: String,
}
impl From<SessionPurposeV1> for SessionPurposeTransportV1 {
    fn from(purpose: SessionPurposeV1) -> Self {
        Self::GenericOne(purpose)
    }
}
impl SessionPurposeTransportV1 {
    pub fn purpose(&self) -> SessionPurposeV1 {
        match self {
            Self::GenericOne(p) => *p,
            Self::ContextBody(v) => v.purpose,
            Self::LiveContextRead(v) => v.purpose,
        }
    }
}
fn context_transport(
    c: &SessionContextV1,
    a: &ContextBodyAttemptV1,
    reference: &ContextBodyReferenceV1,
) -> Result<SessionPurposeTransportV1> {
    Ok(SessionPurposeTransportV1::ContextBody(Box::new(
        ContextBodyTransportV1 {
            kind: ContextBodyTransportKindV1::ContextBody,
            purpose: a.purpose,
            context_operation_id: c.original.request.client_operation_id.clone(),
            slot: a.slot,
            port: a.port,
            source_operation_id: a.source_operation_id.clone(),
            claimed_at: a.claimed_at,
            expected_scope: reference.scope.clone(),
            reference_digest: digest(&("oasis7.custody.context-body.reference.v1", reference))?,
        },
    )))
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ContextBodyAttemptV1 {
    pub slot: ContextBodySlotKindV1,
    pub port: ContextBodyPortV1,
    pub purpose: SessionPurposeV1,
    pub source_operation_id: String,
    pub claimed_at: UnixMillis,
    pub observation: Option<ContextBodyStorageFactV1>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ContextBodyReferenceV1 {
    pub scope: ContextBodyScopeV1,
    pub evidence: ContextBodyEvidenceV1,
    pub plaintext_digest: String,
    pub plaintext_bytes: DecimalU64,
    pub envelope_digest: String,
    pub record_wire_bytes: DecimalU64,
    pub write_attempt: String,
}
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SessionContextV1 {
    pub original: ws::SignedReadRequest<TaskContextPrepareV1>,
    pub prepared_source: String,
    pub registered_at: UnixMillis,
    pub get: Option<ws::SignedReadRequest<TaskContextGetV1>>,
    pub captured_source: Option<String>,
    pub prefix: Option<ContextPrefixEvidenceV1>,
    pub slots: BTreeMap<ContextBodySlotKindV1, ContextBodyReferenceV1>,
    pub attempts: BTreeMap<String, ContextBodyAttemptV1>,
    #[serde(default)]
    pub live_reads: BTreeMap<String, LiveContextReadRecordV1>,
}
impl core::fmt::Debug for SessionContextV1 {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str("SessionContextV1(<private-reference-not-authority>)")
    }
}
fn context_attempt_id(value: &str) -> bool {
    !value.is_empty() && value.len() <= 256 && !value.chars().any(char::is_control)
}
fn body_error(error: ContextBodyError) -> ProvisionApprovalError {
    match error {
        ContextBodyError::ContextTooLarge => ProvisionApprovalError::ContextTooLarge,
        _ => invalid(),
    }
}
/// Borrowed inputs only; constructing this value proves no verification.
pub struct ContextGetRegistration<'a> {
    pub prepared: &'a ValidatedContextBody,
    pub original: &'a ws::SignedReadRequest<TaskContextGetV1>,
    pub prefix: &'a ContextPrefixProofV1,
}
pub struct ContextBodyWrite<'a> {
    pub attempt: &'a str,
    pub body: &'a ValidatedContextBody,
    pub candidate: &'a ContextBodyRecordV1,
}
pub struct ContextBodyRead<'a> {
    pub slot: ContextBodySlotKindV1,
    pub port: ContextBodyPortV1,
    pub attempt: &'a str,
}
impl AccountProvisionApprovalState {
    fn context_scope(&self, id: &str, slot: ContextBodySlotKindV1) -> Result<ContextBodyScopeV1> {
        let s = self.test_sessions.get(id).ok_or_else(invalid)?;
        let c = s.context.as_ref().ok_or_else(invalid)?;
        let scope = ContextBodyScopeV1 {
            api_version: 1,
            identity_scope: s.session.scope.role_scope.clone(),
            world: s.termination.world.clone(),
            session_id: id.into(),
            context_operation_id: c.original.request.client_operation_id.clone(),
            session_intent_digest: s.input.request.intent_digest().map_err(|_| invalid())?,
            native_binding_digest: s
                .native_lifetime
                .as_ref()
                .ok_or_else(invalid)?
                .lifetime
                .status
                .receipt
                .digest()
                .map_err(|_| invalid())?,
            slot,
            version: DecimalU64(1),
            write_deadline: DecimalU64(
                s.roots
                    .get(&SessionPurposeV1::Execution)
                    .ok_or_else(invalid)?
                    .deadline
                    .get(),
            ),
        };
        scope.validate().map_err(body_error)?;
        Ok(scope)
    }
    /// Registers an immutable original only; it does not permit a Native request.
    pub fn register_test_session_context(
        &mut self,
        id: &str,
        original: ws::SignedReadRequest<TaskContextPrepareV1>,
        current: &TestSessionLiveCurrent<'_, '_>,
        now: UnixMillis,
        verifier: &mut impl ApprovalSignatureVerifier,
    ) -> Result<()> {
        verifier
            .verify(&self.test_session_context_original_checks(id, &original, current, now)?)?;
        verify_prepare(&original).map_err(|_| invalid())?;
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
        if let Some(old) = &s.context {
            return if old.original == original {
                Ok(())
            } else {
                Err(ProvisionApprovalError::Conflict)
            };
        }
        if Self::session_transport_attempt_occupied(s, &original.request.client_operation_id)
            || s.actions
                .contains_key(&original.request.client_operation_id)
        {
            return Err(ProvisionApprovalError::Conflict);
        }
        let mut next = self.clone();
        let s = next.test_sessions.get_mut(id).ok_or_else(invalid)?;
        s.roots
            .get_mut(&SessionPurposeV1::Execution)
            .ok_or_else(invalid)?
            .budgets
            .requests
            .charge(1)
            .map_err(|_| invalid())?;
        s.context = Some(SessionContextV1 {
            original,
            prepared_source: current.source_operation_id.into(),
            registered_at: now,
            get: None,
            captured_source: None,
            prefix: None,
            slots: BTreeMap::new(),
            attempts: BTreeMap::new(),
            live_reads: BTreeMap::new(),
        });
        next.validate_persistence()?;
        *self = next;
        Ok(())
    }
    /// SDK validation gives only temporary storage evidence, never usable context.
    pub fn validate_test_session_prepared_body(
        &self,
        id: &str,
        response: &ws::SignedServiceResponse<PreparedTaskContextV1>,
        current: &TestSessionLiveCurrent<'_, '_>,
        now: UnixMillis,
        verifier: &mut impl ApprovalSignatureVerifier,
    ) -> Result<ValidatedContextBody> {
        let body = self.prepared_context_body(id, response, current)?;
        let checks = self.context_complete_checks(id, &body, current, now)?;
        verifier.verify(&checks)?;
        Ok(body)
    }
    /// The Get original and strong committed Prefix are saved before any Get dispatch.
    pub fn register_test_session_context_get(
        &mut self,
        id: &str,
        input: ContextGetRegistration<'_>,
        current: &TestSessionLiveCurrent<'_, '_>,
        now: UnixMillis,
        verifier: &mut impl ApprovalSignatureVerifier,
    ) -> Result<()> {
        let ContextGetRegistration {
            prepared,
            original,
            prefix,
        } = input;
        verifier.verify(
            &self.test_session_context_get_checks(id, prepared, original, prefix, current, now)?,
        )?;
        self.context_body_matches(id, prepared)?;
        let s = self.test_sessions.get(id).ok_or_else(invalid)?;
        let c = s.context.as_ref().ok_or_else(invalid)?;
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
        let public =
            public_prefix(&prepared.prepared().map_err(body_error)?).map_err(|_| invalid())?;
        let oasis7_client_api::world_service_wire::WorldServicePayloadV1::Scheduler(signed) =
            &prefix.original.signed_payload
        else {
            return Err(invalid());
        };
        if signed.request != public {
            return Err(invalid());
        }
        let evidence = prefix
            .encode(
                &self.context_scope(id, ContextBodySlotKindV1::Captured)?,
                &original.request.context,
            )
            .map_err(body_error)?;
        if let Some(old) = &c.get {
            return if old == original && c.prefix.as_ref() == Some(&evidence) {
                Ok(())
            } else {
                Err(ProvisionApprovalError::Conflict)
            };
        }
        let mut next = self.clone();
        let c = next
            .test_sessions
            .get_mut(id)
            .and_then(|s| s.context.as_mut())
            .ok_or_else(invalid)?;
        c.get = Some(original.clone());
        c.prefix = Some(evidence);
        c.captured_source = Some(current.source_operation_id.into());
        next.validate_persistence()?;
        *self = next;
        Ok(())
    }
    pub fn validate_test_session_captured_body(
        &self,
        id: &str,
        response: &ws::SignedServiceResponse<CapturedTaskContextV1>,
        current: &TestSessionLiveCurrent<'_, '_>,
        now: UnixMillis,
        verifier: &mut impl ApprovalSignatureVerifier,
    ) -> Result<ValidatedContextBody> {
        let body = self.captured_context_body(id, response, current)?;
        let checks = self.context_complete_checks(id, &body, current, now)?;
        verifier.verify(&checks)?;
        Ok(body)
    }
    fn context_body_matches(&self, id: &str, body: &ValidatedContextBody) -> Result<()> {
        if self.context_scope(id, body.scope().slot)? != *body.scope() {
            return Err(invalid());
        }
        let c = self
            .test_sessions
            .get(id)
            .and_then(|s| s.context.as_ref())
            .ok_or_else(invalid)?;
        if body.evidence().prepare() != &c.original {
            return Err(invalid());
        }
        if c.slots.get(&body.scope().slot).is_some_and(|r| {
            r.evidence != *body.evidence()
                || r.plaintext_bytes.0 != body.canonical_bytes().len() as u64
                || r.plaintext_digest != crate::approval::bytes_digest(body.canonical_bytes())
        }) {
            return Err(ProvisionApprovalError::Conflict);
        }
        if let ContextBodyEvidenceV1::Captured {
            original, prefix, ..
        } = body.evidence()
        {
            if c.get.as_ref() != Some(original.as_ref()) || c.prefix.as_ref() != Some(prefix) {
                return Err(invalid());
            }
        }
        ValidatedContextBody::revalidate(
            body.scope().clone(),
            body.evidence(),
            body.canonical_bytes(),
        )
        .map_err(body_error)?;
        Ok(())
    }
}
#[path = "test_session_context_ports.rs"]
mod ports;
#[path = "test_session_context_restore.rs"]
mod restore;

#[path = "test_session_context_checks.rs"]
mod checks;

#[path = "test_session_live_context.rs"]
mod live_context;
pub use live_context::*;
