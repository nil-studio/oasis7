//! First RF subtree only. The stored claims never reconstruct first sender
//! tokens; each Account callback is a local causal gate, not a fresh C3.
use super::*;
use crate::{
    execution_fence_dispatch_v2::*,
    native_cleanup_v2::{
        AccountMachineIssuerFloorV2, AccountMachineIssuerPinV2, AccountMachineIssuerV2,
    },
};
fn dispatcher_leaf() -> ExecutionPendingLeafV2 {
    ExecutionPendingLeafV2::Business(inventory::StartBusinessLeafV2::Current {
        phase: inventory::StartCurrentPhaseV2::PreInspect,
        port: inventory::StartCurrentPortV2::RecoveryFenceDispatcherActor,
    })
}
fn fence_leaf() -> ExecutionPendingLeafV2 {
    ExecutionPendingLeafV2::Business(inventory::StartBusinessLeafV2::Current {
        phase: inventory::StartCurrentPhaseV2::PreInspect,
        port: inventory::StartCurrentPortV2::RecoveryFenceActor,
    })
}
pub(super) fn first_rf_sequence() -> [ExecutionPendingLeafV2; 5] {
    [
        ExecutionPendingLeafV2::Business(inventory::StartBusinessLeafV2::Current {
            phase: inventory::StartCurrentPhaseV2::PreInspect,
            port: inventory::StartCurrentPortV2::ManagerAssociationsActor,
        }),
        dispatcher_leaf(),
        ExecutionPendingLeafV2::AccountReentry {
            target: inventory::StartGateTargetV2::CurrentFenceActor(
                inventory::StartCurrentPhaseV2::PreInspect,
            ),
            hop: inventory::StartReentryHopV2::OwnerService,
        },
        ExecutionPendingLeafV2::AccountReentry {
            target: inventory::StartGateTargetV2::CurrentFenceActor(
                inventory::StartCurrentPhaseV2::PreInspect,
            ),
            hop: inventory::StartReentryHopV2::AccountActor,
        },
        fence_leaf(),
    ]
}
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct FenceDispatchRecord {
    pub(super) proof: SignedExecutionFenceDispatchV2,
    pub(super) pin: AccountMachineIssuerPinV2,
    pub(super) floor: AccountMachineIssuerFloorV2,
    pub(super) recorded_at: UnixMillis,
}
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct FenceReadRecord {
    pub(super) proof: SignedExecutionFenceReadV2,
    pub(super) pin: AccountMachineIssuerPinV2,
    pub(super) floor: AccountMachineIssuerFloorV2,
    pub(super) recorded_at: UnixMillis,
}
pub struct PendingFenceDispatchAuthorityV2 {
    accepted: AcceptedExecutionPendingStartV2,
    binding: ExecutionFenceBindingV2,
}
impl PendingFenceDispatchAuthorityV2 {
    pub fn binding(&self) -> &ExecutionFenceBindingV2 {
        &self.binding
    }
    pub fn signing_bytes(&self) -> Result<Vec<u8>> {
        SignedExecutionFenceDispatchV2::signing_bytes(&self.binding).map_err(|_| invalid())
    }
}
/// The sole first-return transport token. Cold metadata and raw IDs cannot mint it.
pub struct PendingFenceDispatchV2 {
    proof: SignedExecutionFenceDispatchV2,
}
impl PendingFenceDispatchV2 {
    pub fn proof(&self) -> &SignedExecutionFenceDispatchV2 {
        &self.proof
    }
}
pub struct PendingFenceReadAuthorityV2 {
    session_id: String,
    claim: ExecutionFenceReadV2,
}
impl PendingFenceReadAuthorityV2 {
    pub fn claim(&self) -> &ExecutionFenceReadV2 {
        &self.claim
    }
    pub fn signing_bytes(&self) -> Result<Vec<u8>> {
        self.claim.signing_bytes().map_err(|_| invalid())
    }
}
fn request_id(s: &TestSessionAuthorityV2) -> Result<String> {
    let p = s.execution_pending_start.as_ref().ok_or_else(invalid)?;
    Ok(s.execution_pending_ingresses
        .get(&p.ingress_attempt_id)
        .ok_or_else(invalid)?
        .request
        .request_id
        .clone())
}
fn claim_leaf(
    state: &mut AccountProvisionApprovalState,
    id: &str,
    leaf: ExecutionPendingLeafV2,
    now: UnixMillis,
) -> Result<()> {
    let s = state.test_session_v2_record_mut(id)?;
    let request_id = request_id(s)?;
    let ordinal = next_session_ordinal_v2(s)?;
    let port = s
        .execution_pending_start
        .as_mut()
        .ok_or_else(invalid)?
        .physical_ports
        .iter_mut()
        .find(|p| p.leaf == leaf)
        .ok_or_else(invalid)?;
    if port.claimed_ordinal.is_some() || port.claimed_at.is_some() {
        return Err(ProvisionApprovalError::Conflict);
    }
    port.claimed_ordinal = Some(ordinal);
    port.claimed_at = Some(now);
    append_session_event_v2(
        s,
        SessionEventKindV2::ExecutionPendingLeafClaimed { request_id, leaf },
        now,
    )
    .map(|_| ())
}
pub(super) fn expected_binding(
    s: &TestSessionAuthorityV2,
    issuer: AccountMachineIssuerV2,
    dispatcher_object_id: String,
    fence_object_id: String,
) -> Result<ExecutionFenceBindingV2> {
    let p = s.execution_pending_start.as_ref().ok_or_else(invalid)?;
    let original = s
        .execution_pending_ingresses
        .get(&p.ingress_attempt_id)
        .ok_or_else(invalid)?;
    let dispatch = p
        .physical_ports
        .iter()
        .find(|v| v.leaf == dispatcher_leaf())
        .ok_or_else(invalid)?;
    let fence = p
        .physical_ports
        .iter()
        .find(|v| v.leaf == fence_leaf())
        .ok_or_else(invalid)?;
    let directory = &p.directory_signature.as_ref().ok_or_else(invalid)?.proof;
    if issuer != directory.claim.issuer {
        return Err(invalid());
    }
    let body = serde_json::to_vec(&ExecutionFenceQueryV2 {
        api_version: 2,
        expected_service_epoch: s.input.request.context.service_epoch,
    })
    .map_err(|_| invalid())?;
    let b = ExecutionFenceBindingV2 {
        api_version: 2,
        issuer,
        scope: s.session.scope.role_scope.clone(),
        session_id: s.input.operation_id.clone(),
        approved_input_digest: s.input_digest()?,
        master_digest: p
            .master_digest(original)?
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect(),
        source_operation_id: p.source_operation_id.clone(),
        accepted_ordinal: DecimalU64(p.accepted_ordinal),
        dispatcher_claim_ordinal: DecimalU64(dispatch.claimed_ordinal.ok_or_else(invalid)?),
        phase: ExecutionFencePhaseV2::PendingPreInspect,
        dispatcher_attempt_id: dispatch.physical_id.clone(),
        fence_attempt_id: fence.physical_id.clone(),
        dispatcher_object_id,
        fence_object_id,
        expected_service_epoch: s.input.request.context.service_epoch,
        business_body_digest: crate::approval::bytes_digest(&body),
        issued_at: DecimalU64(dispatch.claimed_at.ok_or_else(invalid)?.get()),
        expires_at: DecimalU64(p.absolute_until.get().min(directory.claim.expires_at.0)),
    };
    b.validate_shape().map_err(|_| invalid())?;
    Ok(b)
}
impl AccountProvisionApprovalState {
    pub fn pending_first_fence_dispatch_authority_v2(
        &mut self,
        directory: ExecutionPendingDirectoryDispatchV2,
        issuer: AccountMachineIssuerV2,
        dispatcher_object_id: String,
        fence_object_id: String,
        now: UnixMillis,
    ) -> Result<PendingFenceDispatchAuthorityV2> {
        let (attempt, proof) = directory.into_parts();
        let accepted = attempt.accepted;
        let s = self.pending_execution_local(&accepted, now)?;
        let p = s.execution_pending_start.as_ref().ok_or_else(invalid)?;
        if p.claimed_leaf_count() != 1
            || p.directory_signature
                .as_ref()
                .is_none_or(|v| v.proof != proof)
            || p.fence_dispatch.is_some()
        {
            return Err(invalid());
        }
        let id = accepted.session_id.clone();
        let mut next = self.clone();
        claim_leaf(&mut next, &id, dispatcher_leaf(), now)?;
        let binding = expected_binding(
            next.test_session_v2_record(&id)?,
            issuer,
            dispatcher_object_id,
            fence_object_id,
        )?;
        next.validate_persistence()?;
        *self = next;
        Ok(PendingFenceDispatchAuthorityV2 { accepted, binding })
    }
    pub fn record_pending_first_fence_dispatch_v2(
        &mut self,
        authority: PendingFenceDispatchAuthorityV2,
        signature_hex: String,
        pin: AccountMachineIssuerPinV2,
        floor: AccountMachineIssuerFloorV2,
        now: UnixMillis,
    ) -> Result<Option<PendingFenceDispatchV2>> {
        let s = self.pending_execution_local(&authority.accepted, now)?;
        let p = s.execution_pending_start.as_ref().ok_or_else(invalid)?;
        if p.fence_dispatch.is_some() {
            return Err(ProvisionApprovalError::Conflict);
        }
        let expected = expected_binding(
            s,
            authority.binding.issuer.clone(),
            authority.binding.dispatcher_object_id.clone(),
            authority.binding.fence_object_id.clone(),
        )?;
        if expected != authority.binding || now.get() < expected.issued_at.0 {
            return Err(invalid());
        }
        let proof = SignedExecutionFenceDispatchV2 {
            binding: expected,
            signature_hex,
        };
        proof
            .verify(&pin, &floor, &proof.binding.dispatcher_object_id, now.get())
            .map_err(|_| invalid())?;
        let id = authority.accepted.session_id;
        let mut next = self.clone();
        let s = next.test_session_v2_record_mut(&id)?;
        let request_id = request_id(s)?;
        s.execution_pending_start
            .as_mut()
            .ok_or_else(invalid)?
            .fence_dispatch = Some(FenceDispatchRecord {
            proof: proof.clone(),
            pin,
            floor,
            recorded_at: now,
        });
        append_session_event_v2(
            s,
            SessionEventKindV2::ExecutionPendingFenceDispatchRecorded { request_id },
            now,
        )?;
        next.validate_persistence()?;
        *self = next;
        Ok(Some(PendingFenceDispatchV2 { proof }))
    }
    pub fn pending_first_fence_read_authority_v2(
        &mut self,
        proof: &SignedExecutionFenceDispatchV2,
        pin: &AccountMachineIssuerPinV2,
        floor: &AccountMachineIssuerFloorV2,
        now: UnixMillis,
    ) -> Result<PendingFenceReadAuthorityV2> {
        proof
            .verify(pin, floor, &proof.binding.dispatcher_object_id, now.get())
            .map_err(|_| invalid())?;
        let id = &proof.binding.session_id;
        let s = self.test_session_v2_record(id)?;
        let p = s.execution_pending_start.as_ref().ok_or_else(invalid)?;
        if p.fence_dispatch
            .as_ref()
            .is_none_or(|v| v.proof != *proof || v.pin != *pin || v.floor != *floor)
            || p.fence_read.is_some()
            || p.claimed_leaf_count() != 2
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
        for leaf in first_rf_sequence().into_iter().skip(2) {
            claim_leaf(&mut next, id, leaf, now)?;
        }
        let s = next.test_session_v2_record(id)?;
        let p = s.execution_pending_start.as_ref().ok_or_else(invalid)?;
        let claim = ExecutionFenceReadV2 {
            binding: proof.binding.clone(),
            dispatch_digest: proof.digest().map_err(|_| invalid())?,
            claim_ordinal: DecimalU64(p.claimed_leaf_ordinal(fence_leaf()).ok_or_else(invalid)?),
            claimed_at: DecimalU64(now.get()),
        };
        claim.validate_shape().map_err(|_| invalid())?;
        next.validate_persistence()?;
        *self = next;
        Ok(PendingFenceReadAuthorityV2 {
            session_id: id.clone(),
            claim,
        })
    }
    pub fn record_pending_first_fence_read_v2(
        &mut self,
        authority: PendingFenceReadAuthorityV2,
        signature_hex: String,
        pin: AccountMachineIssuerPinV2,
        floor: AccountMachineIssuerFloorV2,
        now: UnixMillis,
    ) -> Result<SignedExecutionFenceReadV2> {
        let s = self.test_session_v2_record(&authority.session_id)?;
        let p = s.execution_pending_start.as_ref().ok_or_else(invalid)?;
        let b = &authority.claim.binding;
        let accepted = AcceptedExecutionPendingStartV2 {
            session_id: authority.session_id.clone(),
            ingress_attempt_id: p.ingress_attempt_id.clone(),
            accepted_ordinal: p.accepted_ordinal,
        };
        self.pending_execution_local(&accepted, now)?;
        if p.fence_read.is_some()
            || p.fence_dispatch.as_ref().is_none_or(|v| {
                v.proof.binding != *b
                    || v.proof.digest().ok().as_ref() != Some(&authority.claim.dispatch_digest)
            })
            || p.claimed_leaf_ordinal(fence_leaf()) != Some(authority.claim.claim_ordinal.0)
            || p.claimed_leaf_at(fence_leaf()).map(|v| v.get())
                != Some(authority.claim.claimed_at.0)
            || now.get() < authority.claim.claimed_at.0
        {
            return Err(invalid());
        }
        let proof = SignedExecutionFenceReadV2 {
            claim: authority.claim,
            signature_hex,
        };
        proof
            .verify(&pin, &floor, now.get())
            .map_err(|_| invalid())?;
        let mut next = self.clone();
        let s = next.test_session_v2_record_mut(&authority.session_id)?;
        let request_id = request_id(s)?;
        s.execution_pending_start
            .as_mut()
            .ok_or_else(invalid)?
            .fence_read = Some(FenceReadRecord {
            proof: proof.clone(),
            pin,
            floor,
            recorded_at: now,
        });
        append_session_event_v2(
            s,
            SessionEventKindV2::ExecutionPendingFenceReadRecorded { request_id },
            now,
        )?;
        next.validate_persistence()?;
        *self = next;
        Ok(proof)
    }
}

pub(super) fn validate_records(s: &TestSessionAuthorityV2) -> Result<()> {
    let Some(p) = &s.execution_pending_start else {
        return Ok(());
    };
    if let Some(record) = &p.fence_dispatch {
        let b = &record.proof.binding;
        let expected = expected_binding(
            s,
            b.issuer.clone(),
            b.dispatcher_object_id.clone(),
            b.fence_object_id.clone(),
        )?;
        if expected != *b
            || record.recorded_at.get() < b.issued_at.0
            || record.recorded_at.get() >= b.expires_at.0
            || session_event_ordinal_v2(
                s,
                &SessionEventKindV2::ExecutionPendingFenceDispatchRecorded {
                    request_id: request_id(s)?,
                },
            )?
            .is_none()
        {
            return Err(invalid());
        }
        record
            .proof
            .verify(
                &record.pin,
                &record.floor,
                &b.dispatcher_object_id,
                b.issued_at.0,
            )
            .map_err(|_| invalid())?;
    }
    if let Some(record) = &p.fence_read {
        let dispatch = &p.fence_dispatch.as_ref().ok_or_else(invalid)?.proof;
        let c = &record.proof.claim;
        if c.binding != dispatch.binding
            || c.dispatch_digest != dispatch.digest().map_err(|_| invalid())?
            || p.claimed_leaf_ordinal(fence_leaf()) != Some(c.claim_ordinal.0)
            || p.claimed_leaf_at(fence_leaf()).map(|v| v.get()) != Some(c.claimed_at.0)
            || record.recorded_at.get() < c.claimed_at.0
            || record.recorded_at.get() >= c.binding.expires_at.0
            || session_event_ordinal_v2(
                s,
                &SessionEventKindV2::ExecutionPendingFenceReadRecorded {
                    request_id: request_id(s)?,
                },
            )?
            .is_none()
        {
            return Err(invalid());
        }
        record
            .proof
            .verify(&record.pin, &record.floor, c.claimed_at.0)
            .map_err(|_| invalid())?;
    }
    Ok(())
}

impl AccountProvisionApprovalState {
    /// Read-only receiver-name metadata; no Current, first token or dispatch grant.
    pub fn pending_first_fence_receiver_name_v2(
        &self,
        id: &str,
        issuer: &AccountMachineIssuerV2,
    ) -> Result<String> {
        let s = self.test_session_v2_record(id)?;
        let p = s.execution_pending_start.as_ref().ok_or_else(invalid)?;
        let port = p
            .physical_ports
            .iter()
            .find(|v| v.leaf == dispatcher_leaf())
            .ok_or_else(invalid)?;
        if issuer.environment != s.session.scope.role_scope.context.identity.environment {
            return Err(invalid());
        }
        execution_fence_receiver_name_v2(issuer, &port.physical_id).map_err(|_| invalid())
    }
}

fn invalid() -> ProvisionApprovalError {
    ProvisionApprovalError::BindingMismatch
}
