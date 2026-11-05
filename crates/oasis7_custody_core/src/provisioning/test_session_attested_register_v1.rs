//! Registers the exact unsigned Prepare candidate after real V1 Move admission.
//! Registration allocates no signature, Native send, or usable ContextRef.
use super::*;
use oasis7_client_api::world_service::task_context::TaskContextPrepareV1;

fn context_attempt_id(value: &str) -> bool {
    !value.is_empty() && value.len() <= 256 && !value.chars().any(char::is_control)
}
fn candidate_bytes(value: &AttestedExecutorCandidateV1) -> Result<String> {
    let bytes = oasis7_wasm_abi::encode_canonical_cbor(value).map_err(|_| invalid())?;
    Ok(bytes.iter().map(|b| format!("{b:02x}")).collect())
}
fn registered_prepare(ledger: &AttestedMoveContextLedgerV1) -> Result<TaskContextPrepareV1> {
    let slot = ledger
        .originals
        .get(&AttestedMoveStageV1::Prepare)
        .ok_or_else(invalid)?;
    if slot.canonical_cbor_hex.len() > 32 * 1024 || slot.canonical_cbor_hex.len() % 2 != 0 {
        return Err(invalid());
    }
    let bytes = slot.canonical_cbor_hex.as_bytes().chunks_exact(2)
        .map(|pair| u8::from_str_radix(core::str::from_utf8(pair).map_err(|_| invalid())?, 16).map_err(|_| invalid()))
        .collect::<Result<Vec<_>>>()?;
    let candidate: AttestedExecutorCandidateV1 =
        serde_cbor::from_slice(&bytes).map_err(|_| invalid())?;
    if candidate_bytes(&candidate)? != slot.canonical_cbor_hex {
        return Err(invalid());
    }
    match candidate {
        AttestedExecutorCandidateV1::PrepareContext(v) => Ok(v),
        _ => Err(invalid()),
    }
}
impl AccountProvisionApprovalState {
    pub(in crate::provisioning) fn attested_move_context_nonce_reserved_v1(
        ledger: &AttestedMoveContextLedgerV1,
        nonce: &str,
    ) -> bool {
        use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
        fn bytes(value: &str) -> Option<Vec<u8>> {
            if hex64(value) {
                value
                    .as_bytes()
                    .chunks_exact(2)
                    .map(|p| u8::from_str_radix(core::str::from_utf8(p).ok()?, 16).ok())
                    .collect()
            } else if value.len() == 43 {
                let bytes = URL_SAFE_NO_PAD.decode(value).ok()?;
                (bytes.len() == 32 && URL_SAFE_NO_PAD.encode(&bytes) == value).then_some(bytes)
            } else {
                None
            }
        }
        registered_prepare(ledger).map_or(true, |v| {
            v.client_nonce == nonce
                || bytes(&v.client_nonce)
                    .zip(bytes(nonce))
                    .is_some_and(|(a, b)| a == b)
        })
    }

    pub(in crate::provisioning) fn attested_move_context_attempt_occupied_v1(
        ledger: &AttestedMoveContextLedgerV1,
        attempt: &str,
    ) -> bool {
        ledger.context_operation_id == attempt
            || ledger.paid_rows.contains_key(attempt)
            || ledger
                .originals
                .values()
                .any(|v| v.send_claim.as_deref() == Some(attempt))
    }

    /// This is a new profile. An old registered private ContextRef never converts.
    pub fn register_attested_move_context_v1(
        &mut self,
        id: &str,
        action_request_id: &str,
        context_operation_id: &str,
        client_nonce: &str,
        current: &TestSessionLiveCurrent<'_, '_>,
        now: UnixMillis,
        verifier: &mut impl ApprovalSignatureVerifier,
    ) -> Result<TaskContextPrepareV1> {
        verifier.verify(&self.test_session_live_checks(
            id,
            SessionPurposeV1::Execution,
            current,
            now,
        )?)?;
        let s = self.test_sessions.get(id).ok_or_else(invalid)?;
        tool_nonce::native_registration(s)?;
        if s.source_funding.get(current.source_operation_id)
            != Some(&SessionSourceFundingV1::Execution)
            || !context_attempt_id(context_operation_id)
            || !context_attempt_id(client_nonce)
        {
            return Err(invalid());
        }
        let action = s.actions.get(action_request_id).ok_or_else(invalid)?;
        let CustodyIntentV1::TestSession { actions, .. } = &s.input.request.intent else {
            return Err(invalid());
        };
        let SessionToolActionV1::Execute(crate::approval::ApprovedActionV1::Move { position_cm }) =
            &action.request.action
        else {
            return Err(invalid());
        };
        if actions.len() != 1
            || actions.first()
                != Some(match &action.request.action {
                    SessionToolActionV1::Execute(a) => a,
                    _ => return Err(invalid()),
                })
            || action.purpose != SessionPurposeV1::Execution
            || action.business.0 != 0
            || action.request.session_id != id
            || action.request_id != action_request_id
        {
            return Err(invalid());
        }
        let prepare = TaskContextPrepareV1 {
            authority_mode: Some(
                oasis7_agent_api::game::TaskContextAuthorityModeV1::NativeExecutorV1,
            ),
            schema_revision: 1,
            fence: s.termination.initial_fence.clone(),
            client_operation_id: context_operation_id.into(),
            client_nonce: client_nonce.into(),
        };
        prepare.fence.validate().map_err(|_| invalid())?;
        if let Some(existing) = &s.context {
            let SessionContextRecordV1::Attested(ledger) = existing else {
                return Err(ProvisionApprovalError::Conflict);
            };
            if ledger.action_request_id != action_request_id
                || registered_prepare(ledger)? != prepare
            {
                return Err(ProvisionApprovalError::Conflict);
            }
            return Ok(prepare);
        }
        if Self::session_transport_attempt_occupied(s, context_operation_id)
            || s.actions.contains_key(context_operation_id)
            || Self::live_context_nonce_occupied(id, s, client_nonce)?
        {
            return Err(ProvisionApprovalError::Conflict);
        }
        let deadline = s
            .roots
            .get(&SessionPurposeV1::Execution)
            .ok_or_else(invalid)?
            .deadline;
        let read_until = s
            .roots
            .get(&SessionPurposeV1::ReconcileRead)
            .ok_or_else(invalid)?
            .deadline;
        let ledger = AttestedMoveContextLedgerV1 {
            profile: AttestedMoveProfileV1::AttestedMoveV1,
            session_id: id.into(),
            action_request_id: action_request_id.into(),
            action_request_digest: action.body_digest.clone(),
            context_operation_id: context_operation_id.into(),
            approved_position_cm: position_cm.map(|v| v.0),
            original_fence: prepare.fence.clone(),
            configured_world_public_key: self.draft.world_identity.service_public_key.clone(),
            registered_at: now,
            write_deadline: deadline,
            read_until,
            paid_rows: BTreeMap::new(),
            public_facts: BTreeMap::new(),
            originals: BTreeMap::from([(
                AttestedMoveStageV1::Prepare,
                AttestedMoveOriginalSlotV1 {
                    canonical_cbor_hex: candidate_bytes(
                        &AttestedExecutorCandidateV1::PrepareContext(prepare.clone()),
                    )?,
                    source_operation_id: current.source_operation_id.into(),
                    allocated_at: now,
                    send_claim: None,
                    archived_response_cbor_hex: None,
                    admitted_at: None,
                },
            )]),
        };
        let mut next = self.clone();
        let s = next.test_sessions.get_mut(id).ok_or_else(invalid)?;
        s.roots
            .get_mut(&SessionPurposeV1::Execution)
            .ok_or_else(invalid)?
            .budgets
            .requests
            .charge(1)
            .map_err(|_| invalid())?;
        s.context = Some(SessionContextRecordV1::Attested(Box::new(ledger)));
        next.validate_persistence()?;
        *self = next;
        Ok(prepare)
    }
    // Initial registered stage only. Later stage extension must validate its real
    // signed producer, immutable claim and public World proof in the same patch.
    pub(in crate::provisioning) fn validate_attested_move_registration_v1(
        &self,
        id: &str,
        s: &TestSessionAuthorityV1,
        ledger: &AttestedMoveContextLedgerV1,
    ) -> Result<()> {
        tool_nonce::native_registration(s)?;
        let a = s
            .actions
            .get(&ledger.action_request_id)
            .ok_or_else(invalid)?;
        let SessionToolActionV1::Execute(crate::approval::ApprovedActionV1::Move { position_cm }) =
            &a.request.action
        else {
            return Err(invalid());
        };
        let CustodyIntentV1::TestSession { actions, .. } = &s.input.request.intent else {
            return Err(invalid());
        };
        let prepare = registered_prepare(ledger)?;
        let slot = ledger
            .originals
            .get(&AttestedMoveStageV1::Prepare)
            .ok_or_else(invalid)?;
        if actions.len() != 1
            || !actions.contains(match &a.request.action {
                SessionToolActionV1::Execute(v) => v,
                _ => return Err(invalid()),
            })
            || ledger.session_id != id
            || a.request_id != ledger.action_request_id
            || a.request.session_id != id
            || ledger.action_request_digest != a.body_digest
            || a.purpose != SessionPurposeV1::Execution
            || a.business.0 != 0
            || ledger.approved_position_cm != position_cm.map(|v| v.0)
            || ledger.original_fence != s.termination.initial_fence
            || prepare.fence != ledger.original_fence
            || prepare.authority_mode
                != Some(oasis7_agent_api::game::TaskContextAuthorityModeV1::NativeExecutorV1)
            || Self::live_context_legacy_nonce(id, s, &prepare.client_nonce)?
            || prepare.schema_revision != 1
            || prepare.client_operation_id != ledger.context_operation_id
            || !context_attempt_id(&prepare.client_operation_id)
            || !context_attempt_id(&prepare.client_nonce)
            || ledger.configured_world_public_key != self.draft.world_identity.service_public_key
            || ledger.write_deadline
                != s.roots
                    .get(&SessionPurposeV1::Execution)
                    .ok_or_else(invalid)?
                    .deadline
            || ledger.read_until
                != s.roots
                    .get(&SessionPurposeV1::ReconcileRead)
                    .ok_or_else(invalid)?
                    .deadline
            || ledger.registered_at.get() < s.approved_at.ok_or_else(invalid)?.get()
            || ledger.registered_at.get() >= ledger.write_deadline.get()
            || ledger.registered_at.get() < a.admitted_at.get()
            || s.sources.contains_key(&ledger.context_operation_id)
            || s.purpose_transports
                .contains_key(&ledger.context_operation_id)
            || s.purpose_http_admissions
                .contains_key(&ledger.context_operation_id)
            || s.control_transports
                .contains_key(&ledger.context_operation_id)
            || s.client_http_admissions
                .contains_key(&ledger.context_operation_id)
            || s.actions.contains_key(&ledger.context_operation_id)
            || s.actions
                .values()
                .any(|v| v.transport_attempts.contains(&ledger.context_operation_id))
            || slot.allocated_at != ledger.registered_at
            || s.source_funding.get(&slot.source_operation_id)
                != Some(&SessionSourceFundingV1::Execution)
            || s.sources
                .get(&slot.source_operation_id)
                .is_none_or(|source| source.verification.is_none())
            || slot.send_claim.is_some()
            || slot.archived_response_cbor_hex.is_some()
            || slot.admitted_at.is_some()
            || ledger.originals.len() != 1
            || !ledger.public_facts.is_empty()
            || !ledger.paid_rows.is_empty()
        {
            return Err(invalid());
        }
        Ok(())
    }
}
