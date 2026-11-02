//! Fixed master-paid Source subtree. No generic Source or ClientFive fallback.
use super::*;
use crate::approval::{ApprovalVerifyRequestV1, VerificationPurposeV1};
use crate::{
    execution_pending_source_inspect_v2::*,
    execution_pending_source_result_v2::*,
    native_cleanup_v2::{
        AccountMachineIssuerFloorV2, AccountMachineIssuerPinV2, AccountMachineIssuerV2,
    },
};
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct DispatchRecord {
    request: PendingSourceInspectDispatchRequestV2,
    pin: AccountMachineIssuerPinV2,
    floor: AccountMachineIssuerFloorV2,
    pub(super) recorded_at: UnixMillis,
}
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct NativeLeafRecord {
    proof: SignedPendingSourceNativeLeafV2,
    pin: AccountMachineIssuerPinV2,
    floor: AccountMachineIssuerFloorV2,
    pub(super) recorded_at: UnixMillis,
}
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ResultRecord {
    reply: SignedPendingSourceInspectResultV2,
    pub(super) recorded_at: UnixMillis,
}
/// Public deployment name/window metadata only; never a dispatch authority.
pub struct PendingSourceDispatchTargetV2 {
    pub object_id: String,
    pub handoff_until: UnixMillis,
}
pub struct PendingSourceDispatchAuthorityV2 {
    accepted: AcceptedExecutionPendingStartV2,
    binding: PendingSourceInspectBindingV2,
    request: ApprovalVerifyRequestV1,
}
impl PendingSourceDispatchAuthorityV2 {
    pub fn binding(&self) -> &PendingSourceInspectBindingV2 {
        &self.binding
    }
    pub fn signing_bytes(&self) -> Result<Vec<u8>> {
        SignedPendingSourceInspectDispatchV2::signing_bytes(&self.binding).map_err(|_| invalid())
    }
}
/// This carries the original first-only parent, not a wire/cold authority.
pub struct PendingSourceDispatchV2 {
    accepted: AcceptedExecutionPendingStartV2,
    request: PendingSourceInspectDispatchRequestV2,
}
impl PendingSourceDispatchV2 {
    pub fn request(&self) -> &PendingSourceInspectDispatchRequestV2 {
        &self.request
    }
}
/// Historical original plus the original private parent, only after first archive commit.
/// The next paid PreArchive C3 is still mandatory before Source admission.
pub struct PendingSourceHistoryV2 {
    accepted: AcceptedExecutionPendingStartV2,
    result_digest: String,
}
impl PendingSourceHistoryV2 {
    pub fn session_id(&self) -> &str {
        &self.accepted.session_id
    }
    pub fn result_digest(&self) -> &str {
        &self.result_digest
    }
}
pub struct PendingSourceLeafAuthorityV2 {
    session_id: String,
    claim: PendingSourceNativeLeafV2,
}
impl PendingSourceLeafAuthorityV2 {
    pub fn claim(&self) -> &PendingSourceNativeLeafV2 {
        &self.claim
    }
    pub fn signing_bytes(&self) -> Result<Vec<u8>> {
        SignedPendingSourceNativeLeafV2::signing_bytes(&self.claim).map_err(|_| invalid())
    }
}
fn dispatch_leaf() -> ExecutionPendingLeafV2 {
    ExecutionPendingLeafV2::Business(inventory::StartBusinessLeafV2::SourceVerifyService)
}
fn native_leaf() -> ExecutionPendingLeafV2 {
    ExecutionPendingLeafV2::Business(inventory::StartBusinessLeafV2::SourceVerifyNative)
}
pub(super) fn source_sequence() -> [ExecutionPendingLeafV2; 4] {
    [
        dispatch_leaf(),
        ExecutionPendingLeafV2::AccountReentry {
            target: inventory::StartGateTargetV2::SourceVerifyNative,
            hop: inventory::StartReentryHopV2::OwnerService,
        },
        ExecutionPendingLeafV2::AccountReentry {
            target: inventory::StartGateTargetV2::SourceVerifyNative,
            hop: inventory::StartReentryHopV2::AccountActor,
        },
        native_leaf(),
    ]
}
fn expected_binding(
    s: &TestSessionAuthorityV2,
    issuer: AccountMachineIssuerV2,
    object_id: String,
) -> Result<(PendingSourceInspectBindingV2, ApprovalVerifyRequestV1)> {
    let p = s.execution_pending_start.as_ref().ok_or_else(invalid)?;
    if p.directory_signature
        .as_ref()
        .is_none_or(|r| r.proof.claim.issuer != issuer)
        || p.fence_dispatch
            .as_ref()
            .is_none_or(|r| r.proof.binding.issuer != issuer)
    {
        return Err(invalid());
    }
    let original = s
        .execution_pending_ingresses
        .get(&p.ingress_attempt_id)
        .ok_or_else(invalid)?;
    let source = s.sources.get(&p.source_operation_id).ok_or_else(invalid)?;
    let lifetime = s.native_lifetime.as_ref().ok_or_else(invalid)?;
    let (registration, active_digest) = s.stored_native_ready_identity()?;
    let dispatch = p
        .physical_ports
        .iter()
        .find(|v| v.leaf == dispatch_leaf())
        .ok_or_else(invalid)?;
    let native = p
        .physical_ports
        .iter()
        .find(|v| v.leaf == native_leaf())
        .ok_or_else(invalid)?;
    let request = ApprovalVerifyRequestV1 {
        approval: s.input.approval.clone(),
        proof_digest: s.input.approval.digest().map_err(|_| invalid())?,
        expected_audience: s.input.request.context.audience.clone(),
        expected_intent_digest: s.input.approval.proof.intent_digest.clone(),
        purpose: VerificationPurposeV1::InspectOrigin,
        verify_nonce: p.source_verify_nonce.clone(),
    };
    // The independently reserved Source and Inspect slot remain exact. A new
    // Source request does not mint another clock or change its natural bounds.
    if source.source.verify_nonce != p.source_verify_nonce {
        return Err(invalid());
    }
    let binding = PendingSourceInspectBindingV2 {
        api_version: 2,
        issuer,
        scope: s.session.scope.role_scope.clone(),
        session_id: s.input.operation_id.clone(),
        ingress_attempt_id: p.ingress_attempt_id.clone(),
        signed_attempt_digest: original.signed_attempt_digest.clone(),
        action: original.request.clone(),
        approved_input_digest: s.input_digest()?,
        master_digest: p
            .master_digest(original)?
            .iter()
            .map(|v| format!("{v:02x}"))
            .collect(),
        source_operation_id: p.source_operation_id.clone(),
        source_verify_nonce: p.source_verify_nonce.clone(),
        opaque_registration_ref: registration.into(),
        active_certificate_digest: active_digest,
        native_binding: lifetime.lifetime.status.receipt.clone(),
        native_session_ref: s.session.session_ref.clone(),
        native_binding_generation: s.session.native_binding_generation,
        session: s.session.clone(),
        source_request_digest: crate::approval::bytes_digest(
            &serde_json::to_vec(&request).map_err(|_| invalid())?,
        ),
        expected_service_epoch: s.input.request.context.service_epoch,
        accepted_ordinal: DecimalU64(p.accepted_ordinal),
        dispatcher_claim_ordinal: DecimalU64(dispatch.claimed_ordinal.ok_or_else(invalid)?),
        dispatcher_attempt_id: dispatch.physical_id.clone(),
        native_attempt_id: native.physical_id.clone(),
        dispatcher_object_id: object_id,
        issued_at: DecimalU64(dispatch.claimed_at.ok_or_else(invalid)?.get()),
        expires_at: DecimalU64(
            p.absolute_until
                .get()
                .min(source.source.expires_at.0)
                .min(p.source_inspect_current_until.ok_or_else(invalid)?.get()),
        ),
    };
    binding.check_request(&request).map_err(|_| invalid())?;
    Ok((binding, request))
}
impl AccountProvisionApprovalState {
    /// Called only by the same handler after its actual private first D/F
    /// observation. Consumes the original RF first-return continuation.
    pub fn pending_source_dispatch_authority_v2(
        &mut self,
        first: PendingFenceDispatchV2,
        issuer: AccountMachineIssuerV2,
        target: PendingSourceDispatchTargetV2,
        native_scope: &NativeStageScope<'_>,
        now: UnixMillis,
        verifier: &mut impl ApprovalSignatureVerifier,
    ) -> Result<PendingSourceDispatchAuthorityV2> {
        let PendingSourceDispatchTargetV2 {
            object_id,
            handoff_until,
        } = target;
        let (accepted, rf_proof) = first.into_parts();
        for job in self.test_session_v2_native_restore_checks(&accepted.session_id, native_scope)? {
            verifier.verify(&job)?;
        }
        let s = self.pending_execution_local(&accepted, now)?;
        let p = s.execution_pending_start.as_ref().ok_or_else(invalid)?;
        if p.claimed_leaf_count() != 5
            || p.fence_read.is_none()
            || p.fence_dispatch
                .as_ref()
                .is_none_or(|r| r.proof != rf_proof)
            || p.source_inspect_dispatch.is_some()
        {
            return Err(invalid());
        }
        if handoff_until.get() <= now.get()
            || handoff_until.get() > p.absolute_until.get()
            || p.source_inspect_current_until.is_some()
        {
            return Err(invalid());
        }
        let mut next = self.clone();
        next.test_session_v2_record_mut(&accepted.session_id)?
            .execution_pending_start
            .as_mut()
            .ok_or_else(invalid)?
            .source_inspect_current_until = Some(handoff_until);
        fence::claim_leaf(&mut next, &accepted.session_id, dispatch_leaf(), now)?;
        let (binding, request) = expected_binding(
            next.test_session_v2_record(&accepted.session_id)?,
            issuer,
            object_id,
        )?;
        let source_id = binding.source_operation_id.clone();
        let raw = serde_json::to_string(&request).map_err(|_| invalid())?;
        let saved = next.test_session_v2_record_mut(&accepted.session_id)?;
        let wrapper = saved.sources.get_mut(&source_id).ok_or_else(invalid)?;
        if wrapper.source.claimed_at.is_some()
            || wrapper.source.request_bytes.is_some()
            || wrapper.source.verified_at.is_some()
            || wrapper.source.verification.is_some()
        {
            return Err(invalid());
        }
        wrapper.source.claimed_at = Some(DecimalU64(now.get()));
        wrapper.source.request_bytes = Some(raw);
        append_session_event_v2(
            saved,
            SessionEventKindV2::SourceVerificationClaimed {
                source_operation_id: source_id,
            },
            now,
        )?;
        next.validate_persistence()?;
        *self = next;
        Ok(PendingSourceDispatchAuthorityV2 {
            accepted,
            binding,
            request,
        })
    }
    pub fn record_pending_source_dispatch_v2(
        &mut self,
        authority: PendingSourceDispatchAuthorityV2,
        signature_hex: String,
        pin: AccountMachineIssuerPinV2,
        floor: AccountMachineIssuerFloorV2,
        now: UnixMillis,
    ) -> Result<PendingSourceDispatchV2> {
        let s = self.pending_execution_local(&authority.accepted, now)?;
        let p = s.execution_pending_start.as_ref().ok_or_else(invalid)?;
        if p.source_inspect_dispatch.is_some() {
            return Err(ProvisionApprovalError::Conflict);
        }
        let expected = expected_binding(
            s,
            authority.binding.issuer.clone(),
            authority.binding.dispatcher_object_id.clone(),
        )?;
        if expected != (authority.binding.clone(), authority.request.clone())
            || now.get() < authority.binding.issued_at.0
        {
            return Err(invalid());
        }
        let request = PendingSourceInspectDispatchRequestV2 {
            proof: SignedPendingSourceInspectDispatchV2 {
                binding: authority.binding,
                signature_hex,
            },
            request: authority.request,
        };
        request
            .proof
            .verify(
                &pin,
                &floor,
                &request.proof.binding.dispatcher_object_id,
                now.get(),
            )
            .map_err(|_| invalid())?;
        request.wire_bytes().map_err(|_| invalid())?;
        let mut next = self.clone();
        let s = next.test_session_v2_record_mut(&authority.accepted.session_id)?;
        let request_id = s
            .execution_pending_ingresses
            .get(&authority.accepted.ingress_attempt_id)
            .ok_or_else(invalid)?
            .request
            .request_id
            .clone();
        s.execution_pending_start
            .as_mut()
            .ok_or_else(invalid)?
            .source_inspect_dispatch = Some(DispatchRecord {
            request: request.clone(),
            pin,
            floor,
            recorded_at: now,
        });
        append_session_event_v2(
            s,
            SessionEventKindV2::ExecutionPendingSourceDispatchRecorded { request_id },
            now,
        )?;
        next.validate_persistence()?;
        *self = next;
        Ok(PendingSourceDispatchV2 {
            accepted: authority.accepted,
            request,
        })
    }
}

impl AccountProvisionApprovalState {
    /// This is a received fixed control gate, not C3 or a free callback. Both
    /// transport leaves are already in the original master; SQL claims them
    /// and the one Native child atomically before its exact receipt is signed.
    pub fn pending_source_native_leaf_authority_v2(
        &mut self,
        proof: &SignedPendingSourceInspectDispatchV2,
        pin: &AccountMachineIssuerPinV2,
        floor: &AccountMachineIssuerFloorV2,
        now: UnixMillis,
    ) -> Result<PendingSourceLeafAuthorityV2> {
        proof
            .verify(pin, floor, &proof.binding.dispatcher_object_id, now.get())
            .map_err(|_| invalid())?;
        let id = &proof.binding.session_id;
        let s = self.test_session_v2_record(id)?;
        let p = s.execution_pending_start.as_ref().ok_or_else(invalid)?;
        if p.claimed_leaf_count() != 6
            || p.source_inspect_native_leaf.is_some()
            || p.source_inspect_dispatch
                .as_ref()
                .is_none_or(|r| r.request.proof != *proof || r.pin != *pin || r.floor != *floor)
        {
            return Err(invalid());
        }
        let accepted = AcceptedExecutionPendingStartV2 {
            session_id: id.clone(),
            ingress_attempt_id: p.ingress_attempt_id.clone(),
            accepted_ordinal: p.accepted_ordinal,
        };
        self.pending_execution_local(&accepted, now)?;
        let mut next = self.clone();
        for leaf in source_sequence().into_iter().skip(1) {
            fence::claim_leaf(&mut next, id, leaf, now)?;
        }
        let s = next.test_session_v2_record(id)?;
        let p = s.execution_pending_start.as_ref().ok_or_else(invalid)?;
        let claim = PendingSourceNativeLeafV2 {
            dispatch: proof.clone(),
            native_claim_ordinal: DecimalU64(
                p.claimed_leaf_ordinal(native_leaf()).ok_or_else(invalid)?,
            ),
            claimed_at: DecimalU64(now.get()),
            expires_at: DecimalU64(p.absolute_until.get().min(proof.binding.expires_at.0)),
        };
        claim.validate_shape().map_err(|_| invalid())?;
        next.validate_persistence()?;
        *self = next;
        Ok(PendingSourceLeafAuthorityV2 {
            session_id: id.clone(),
            claim,
        })
    }
    pub fn record_pending_source_native_leaf_v2(
        &mut self,
        authority: PendingSourceLeafAuthorityV2,
        signature_hex: String,
        pin: AccountMachineIssuerPinV2,
        floor: AccountMachineIssuerFloorV2,
        now: UnixMillis,
    ) -> Result<SignedPendingSourceNativeLeafV2> {
        let s = self.test_session_v2_record(&authority.session_id)?;
        let p = s.execution_pending_start.as_ref().ok_or_else(invalid)?;
        let accepted = AcceptedExecutionPendingStartV2 {
            session_id: authority.session_id.clone(),
            ingress_attempt_id: p.ingress_attempt_id.clone(),
            accepted_ordinal: p.accepted_ordinal,
        };
        self.pending_execution_local(&accepted, now)?;
        if p.source_inspect_native_leaf.is_some()
            || p.claimed_leaf_count() != 9
            || p.source_inspect_dispatch
                .as_ref()
                .is_none_or(|r| r.request.proof != authority.claim.dispatch)
            || p.claimed_leaf_ordinal(native_leaf()) != Some(authority.claim.native_claim_ordinal.0)
            || p.claimed_leaf_at(native_leaf()).map(|v| v.get())
                != Some(authority.claim.claimed_at.0)
            || now.get() < authority.claim.claimed_at.0
        {
            return Err(invalid());
        }
        let proof = SignedPendingSourceNativeLeafV2 {
            claim: authority.claim,
            signature_hex,
        };
        proof
            .verify(
                &pin,
                &floor,
                &proof.claim.dispatch.binding.issuer.native_audience_origin,
                now.get(),
            )
            .map_err(|_| invalid())?;
        let mut next = self.clone();
        let s = next.test_session_v2_record_mut(&authority.session_id)?;
        let request_id = s
            .execution_pending_ingresses
            .get(&accepted.ingress_attempt_id)
            .ok_or_else(invalid)?
            .request
            .request_id
            .clone();
        s.execution_pending_start
            .as_mut()
            .ok_or_else(invalid)?
            .source_inspect_native_leaf = Some(NativeLeafRecord {
            proof: proof.clone(),
            pin,
            floor,
            recorded_at: now,
        });
        append_session_event_v2(
            s,
            SessionEventKindV2::ExecutionPendingSourceNativeLeafRecorded { request_id },
            now,
        )?;
        next.validate_persistence()?;
        *self = next;
        Ok(proof)
    }
    /// Preserves the actual paid Native outer and its original inner. It is not
    /// Source admission, LF, Book birth, Current or a retryable transport token.
    pub fn archive_pending_source_inspect_result_history_v2(
        &mut self,
        first: PendingSourceDispatchV2,
        reply: SignedPendingSourceInspectResultV2,
        native: &NativeStageScope<'_>,
        now: UnixMillis,
    ) -> Result<PendingSourceHistoryV2> {
        let s = self.pending_execution_local(&first.accepted, now)?;
        let p = s.execution_pending_start.as_ref().ok_or_else(invalid)?;
        if p.source_inspect_result.is_some()
            || p.source_inspect_dispatch
                .as_ref()
                .is_none_or(|r| r.request != first.request)
        {
            return Err(invalid());
        }
        let leaf = &p
            .source_inspect_native_leaf
            .as_ref()
            .ok_or_else(invalid)?
            .proof;
        let q = PendingSourceNativeRequestV2 {
            leaf: leaf.clone(),
            request: first.request.request,
        };
        if leaf.claim.dispatch.binding.issuer.native_audience_origin != native.http_origin
            || now.get() < reply.result.completed_at.0
            || now.get() >= leaf.claim.expires_at.0
        {
            return Err(invalid());
        }
        reply
            .verify_for(&q, native.native_root_key_id, native.native_root_public_key)
            .map_err(|_| invalid())?;
        reply.wire_bytes().map_err(|_| invalid())?;
        let result_digest = reply.digest().map_err(|_| invalid())?;
        let mut next = self.clone();
        let s = next.test_session_v2_record_mut(&first.accepted.session_id)?;
        let request_id = s
            .execution_pending_ingresses
            .get(&first.accepted.ingress_attempt_id)
            .ok_or_else(invalid)?
            .request
            .request_id
            .clone();
        s.execution_pending_start
            .as_mut()
            .ok_or_else(invalid)?
            .source_inspect_result = Some(ResultRecord {
            reply,
            recorded_at: now,
        });
        append_session_event_v2(
            s,
            SessionEventKindV2::ExecutionPendingSourceResultRecorded { request_id },
            now,
        )?;
        next.validate_persistence()?;
        *self = next;
        Ok(PendingSourceHistoryV2 {
            accepted: first.accepted,
            result_digest,
        })
    }
}

pub(super) fn validate_records(s: &TestSessionAuthorityV2) -> Result<()> {
    let Some(p) = &s.execution_pending_start else {
        return Ok(());
    };
    let count = p.claimed_leaf_count();
    if !matches!(count, 0 | 1 | 2 | 5 | 6 | 9) {
        return Err(invalid());
    }
    let original = s
        .execution_pending_ingresses
        .get(&p.ingress_attempt_id)
        .ok_or_else(invalid)?;
    let rid = &original.request.request_id;
    let dispatch_event = session_event_ordinal_v2(
        s,
        &SessionEventKindV2::ExecutionPendingSourceDispatchRecorded {
            request_id: rid.clone(),
        },
    )?;
    let native_event = session_event_ordinal_v2(
        s,
        &SessionEventKindV2::ExecutionPendingSourceNativeLeafRecorded {
            request_id: rid.clone(),
        },
    )?;
    let result_event = session_event_ordinal_v2(
        s,
        &SessionEventKindV2::ExecutionPendingSourceResultRecorded {
            request_id: rid.clone(),
        },
    )?;
    if dispatch_event.is_some() != p.source_inspect_dispatch.is_some()
        || native_event.is_some() != p.source_inspect_native_leaf.is_some()
        || result_event.is_some() != p.source_inspect_result.is_some()
        || p.source_inspect_dispatch.is_some() && count < 6
        || p.source_inspect_native_leaf.is_some() && count != 9
        || p.source_inspect_result.is_some() && p.source_inspect_native_leaf.is_none()
    {
        return Err(invalid());
    }
    if count < 6 {
        if p.source_inspect_current_until.is_some() {
            return Err(invalid());
        }
    } else {
        let until = p.source_inspect_current_until.ok_or_else(invalid)?;
        let claimed_at = p.claimed_leaf_at(dispatch_leaf()).ok_or_else(invalid)?;
        if until.get() <= claimed_at.get() || until.get() > p.absolute_until.get() {
            return Err(invalid());
        }
    }
    let source = &s
        .sources
        .get(&p.source_operation_id)
        .ok_or_else(invalid)?
        .source;
    if count < 6 {
        if source.claimed_at.is_some() || source.request_bytes.is_some() {
            return Err(invalid());
        }
    } else {
        let at = p.claimed_leaf_at(dispatch_leaf()).ok_or_else(invalid)?;
        let request = ApprovalVerifyRequestV1 {
            approval: s.input.approval.clone(),
            proof_digest: s.input.approval.digest().map_err(|_| invalid())?,
            expected_audience: s.input.request.context.audience.clone(),
            expected_intent_digest: s.input.approval.proof.intent_digest.clone(),
            purpose: VerificationPurposeV1::InspectOrigin,
            verify_nonce: p.source_verify_nonce.clone(),
        };
        let original_claim_ordinal = session_event_ordinal_v2(
            s,
            &SessionEventKindV2::SourceVerificationClaimed {
                source_operation_id: p.source_operation_id.clone(),
            },
        )?
        .ok_or_else(invalid)?;
        if source.claimed_at != Some(DecimalU64(at.get()))
            || source.request_bytes.as_deref()
                != Some(
                    serde_json::to_string(&request)
                        .map_err(|_| invalid())?
                        .as_str(),
                )
            || original_claim_ordinal
                <= p.claimed_leaf_ordinal(dispatch_leaf())
                    .ok_or_else(invalid)?
            || dispatch_event.is_some_and(|signed| original_claim_ordinal >= signed)
        {
            return Err(invalid());
        }
    }
    // This first subtree has not reached PreArchive C3 or Source admission.
    if source.verification.is_some() || source.verified_at.is_some() {
        return Err(invalid());
    }
    let sequence = source_sequence();
    let mut previous_ordinal = p.claimed_leaf_ordinal(fence::first_rf_sequence()[4]);
    let mut previous_at = p.claimed_leaf_at(fence::first_rf_sequence()[4]);
    for (index, leaf) in sequence.into_iter().enumerate() {
        let ordinal = p.claimed_leaf_ordinal(leaf);
        let at = p.claimed_leaf_at(leaf);
        if index >= count.saturating_sub(5) {
            if ordinal.is_some() || at.is_some() {
                return Err(invalid());
            }
            continue;
        }
        let ordinal = ordinal.ok_or_else(invalid)?;
        let at = at.ok_or_else(invalid)?;
        if ordinal <= previous_ordinal.ok_or_else(invalid)?
            || at.get() < previous_at.ok_or_else(invalid)?.get()
        {
            return Err(invalid());
        }
        if index == 0 {
            let rf = p.fence_read.as_ref().ok_or_else(invalid)?;
            let event = session_event_ordinal_v2(
                s,
                &SessionEventKindV2::ExecutionPendingFenceReadRecorded {
                    request_id: rid.clone(),
                },
            )?
            .ok_or_else(invalid)?;
            if event >= ordinal
                || event <= previous_ordinal.ok_or_else(invalid)?
                || rf.recorded_at.get() > at.get()
            {
                return Err(invalid());
            }
        }
        if index == 1 {
            let record = p.source_inspect_dispatch.as_ref().ok_or_else(invalid)?;
            let event = dispatch_event.ok_or_else(invalid)?;
            if event >= ordinal
                || event <= previous_ordinal.ok_or_else(invalid)?
                || record.recorded_at.get() > at.get()
            {
                return Err(invalid());
            }
        }
        if index >= 2 && Some(at) != previous_at {
            return Err(invalid());
        }
        previous_ordinal = Some(ordinal);
        previous_at = Some(at);
    }
    if let Some(r) = &p.source_inspect_dispatch {
        let (expected, request) = expected_binding(
            s,
            r.request.proof.binding.issuer.clone(),
            r.request.proof.binding.dispatcher_object_id.clone(),
        )?;
        if expected != r.request.proof.binding
            || request != r.request.request
            || r.recorded_at.get() < expected.issued_at.0
            || r.recorded_at.get() >= expected.expires_at.0
            || dispatch_event.ok_or_else(invalid)? <= expected.dispatcher_claim_ordinal.0
        {
            return Err(invalid());
        }
        r.request.wire_bytes().map_err(|_| invalid())?;
        r.request
            .proof
            .verify(
                &r.pin,
                &r.floor,
                &expected.dispatcher_object_id,
                expected.issued_at.0,
            )
            .map_err(|_| invalid())?;
    }
    if let Some(r) = &p.source_inspect_native_leaf {
        let dispatch = &p
            .source_inspect_dispatch
            .as_ref()
            .ok_or_else(invalid)?
            .request
            .proof;
        let c = &r.proof.claim;
        if c.dispatch != *dispatch
            || p.claimed_leaf_ordinal(native_leaf()) != Some(c.native_claim_ordinal.0)
            || p.claimed_leaf_at(native_leaf()).map(|v| v.get()) != Some(c.claimed_at.0)
            || r.recorded_at.get() < c.claimed_at.0
            || r.recorded_at.get() >= c.expires_at.0
            || native_event.ok_or_else(invalid)? <= c.native_claim_ordinal.0
        {
            return Err(invalid());
        }
        r.proof
            .verify(
                &r.pin,
                &r.floor,
                &c.dispatch.binding.issuer.native_audience_origin,
                c.claimed_at.0,
            )
            .map_err(|_| invalid())?;
    }
    if let Some(r) = &p.source_inspect_result {
        let leaf = p.source_inspect_native_leaf.as_ref().ok_or_else(invalid)?;
        let dispatch = p.source_inspect_dispatch.as_ref().ok_or_else(invalid)?;
        let q = PendingSourceNativeRequestV2 {
            leaf: leaf.proof.clone(),
            request: dispatch.request.request.clone(),
        };
        r.reply.result.check_request(&q).map_err(|_| invalid())?;
        r.reply.wire_bytes().map_err(|_| invalid())?;
        if r.recorded_at.get() < r.reply.result.completed_at.0
            || r.recorded_at.get() >= q.leaf.claim.expires_at.0
            || result_event.ok_or_else(invalid)? <= native_event.ok_or_else(invalid)?
        {
            return Err(invalid());
        }
    }
    Ok(())
}
impl AccountProvisionApprovalState {
    pub(in crate::provisioning) fn pending_source_native_history_v2(
        &self,
        id: &str,
        native: &NativeStageScope<'_>,
    ) -> Result<()> {
        let s = self.test_session_v2_record(id)?;
        let Some(p) = &s.execution_pending_start else {
            return Ok(());
        };
        let Some(r) = &p.source_inspect_result else {
            return Ok(());
        };
        let dispatch = p.source_inspect_dispatch.as_ref().ok_or_else(invalid)?;
        let leaf = p.source_inspect_native_leaf.as_ref().ok_or_else(invalid)?;
        let q = PendingSourceNativeRequestV2 {
            leaf: leaf.proof.clone(),
            request: dispatch.request.request.clone(),
        };
        if q.leaf.claim.dispatch.binding.issuer.native_audience_origin != native.http_origin {
            return Err(invalid());
        }
        r.reply
            .verify_for(&q, native.native_root_key_id, native.native_root_public_key)
            .map_err(|_| invalid())
    }
}
