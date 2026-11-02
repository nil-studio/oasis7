//! The fixed Account issuer signs only this first actual claimed Directory
//! leaf. Neither raw metadata nor cold restoration can construct a dispatch.
pub(super) fn expected_claim(
    s: &TestSessionAuthorityV2,
    issuer: AccountMachineIssuerV2,
) -> Result<ExecutionPendingCurrentLeafV2> {
    let p = s.execution_pending_start.as_ref().ok_or_else(invalid)?;
    let original = s
        .execution_pending_ingresses
        .get(&p.ingress_attempt_id)
        .ok_or_else(invalid)?;
    let leaf = ExecutionPendingLeafV2::Business(inventory::StartBusinessLeafV2::Current {
        phase: inventory::StartCurrentPhaseV2::PreInspect,
        port: inventory::StartCurrentPortV2::ManagerAssociations,
    });
    let port = p
        .physical_ports
        .iter()
        .find(|v| v.leaf == leaf)
        .ok_or_else(invalid)?;
    let signed = s
        .book
        .original(&s.input.operation_id)
        .and_then(|v| v.verification())
        .ok_or_else(invalid)?;
    let v = &signed.verification;
    let proof = &s.input.approval.proof;
    let manager = crate::approval::CustodySessionStatusV1 {
        api_version: 1,
        identity_namespace: proof.identity_namespace.clone(),
        identity_instance_epoch: v.identity_instance_epoch,
        account_id: proof.manager_account_id.clone(),
        session_ref: proof.manager_session_ref.clone(),
        credential_ref: proof.origin_credential_id.clone(),
        credential_version: v.origin_credential_version,
        security_version: v.manager_security_version,
        policy_version: v.manager_policy_version,
        server_time: v.observed_at,
        expires_at: v.source_session_expires_at,
        idle_expires_at: v.source_session_idle_expires_at,
    };
    let until = p
        .absolute_until
        .get()
        .min(manager.expires_at.0)
        .min(manager.idle_expires_at.0);
    let master = p
        .master_digest(original)?
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect();
    let claim = ExecutionPendingCurrentLeafV2 {
        api_version: 2,
        issuer,
        scope: s.session.scope.role_scope.clone(),
        session_id: original.request.session_id.clone(),
        approved_input_digest: original.approved_input_digest.clone(),
        expected_service_epoch: s.input.request.context.service_epoch,
        master_digest: master,
        accepted_ordinal: DecimalU64(p.accepted_ordinal),
        claim_ordinal: DecimalU64(port.claimed_ordinal.ok_or_else(invalid)?),
        physical_attempt_id: port.physical_id.clone(),
        phase: PendingCurrentPhaseV2::PreInspect,
        receiver: PendingCurrentReceiverV2::ManagerAssociationsActor,
        manager,
        issued_at: DecimalU64(port.claimed_at.ok_or_else(invalid)?.get()),
        expires_at: DecimalU64(until),
    };
    claim.validate_shape().map_err(|_| invalid())?;
    Ok(claim)
}
use super::*;
use crate::{
    execution_pending_current_v2::*,
    native_cleanup_v2::{
        AccountMachineIssuerFloorV2, AccountMachineIssuerPinV2, AccountMachineIssuerV2,
    },
};
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct PendingDirectorySignatureV2 {
    pub(super) proof: SignedExecutionPendingCurrentLeafV2,
    pub(super) pin: AccountMachineIssuerPinV2,
    pub(super) floor: AccountMachineIssuerFloorV2,
    pub(super) recorded_at: UnixMillis,
}
pub struct ExecutionPendingDirectorySigningAuthorityV2 {
    attempt: PendingDirectoryAttemptV2,
    claim: ExecutionPendingCurrentLeafV2,
}
impl ExecutionPendingDirectorySigningAuthorityV2 {
    pub fn claim(&self) -> &ExecutionPendingCurrentLeafV2 {
        &self.claim
    }
    pub fn signing_bytes(&self) -> Result<Vec<u8>> {
        self.claim.signing_bytes().map_err(|_| invalid())
    }
}
pub struct ExecutionPendingDirectoryDispatchV2 {
    attempt: PendingDirectoryAttemptV2,
    proof: SignedExecutionPendingCurrentLeafV2,
}
impl ExecutionPendingDirectoryDispatchV2 {
    pub(super) fn into_parts(
        self,
    ) -> (
        PendingDirectoryAttemptV2,
        SignedExecutionPendingCurrentLeafV2,
    ) {
        (self.attempt, self.proof)
    }
    pub fn proof(&self) -> &SignedExecutionPendingCurrentLeafV2 {
        &self.proof
    }
    pub fn session_id(&self) -> &str {
        self.attempt.session_id()
    }
    pub fn scope(&self) -> &crate::role_key_slot::RoleKeySlotScopeV1 {
        self.attempt.scope()
    }
    pub fn expires_at(&self) -> UnixMillis {
        UnixMillis::new(self.proof.claim.expires_at.0)
    }
}
impl AccountProvisionApprovalState {
    pub fn pending_directory_signing_authority_v2(
        &self,
        attempt: PendingDirectoryAttemptV2,
        issuer: AccountMachineIssuerV2,
    ) -> Result<ExecutionPendingDirectorySigningAuthorityV2> {
        let s = self.test_session_v2_record(attempt.session_id())?;
        let p = s.execution_pending_start.as_ref().ok_or_else(invalid)?;
        s.execution_pending_ingresses
            .get(&p.ingress_attempt_id)
            .ok_or_else(invalid)?;
        let leaf = ExecutionPendingLeafV2::Business(inventory::StartBusinessLeafV2::Current {
            phase: inventory::StartCurrentPhaseV2::PreInspect,
            port: inventory::StartCurrentPortV2::ManagerAssociations,
        });
        if p.accepted_ordinal != attempt.accepted.accepted_ordinal
            || p.ingress_attempt_id != attempt.accepted.ingress_attempt_id
            || p.claimed_leaf_ordinal(leaf) != Some(attempt.claim_ordinal)
            || p.claimed_leaf_at(leaf) != Some(attempt.claimed_at)
            || p.directory_signature.is_some()
        {
            return Err(invalid());
        }
        let claim = expected_claim(s, issuer)?;
        if claim.approved_input_digest != attempt.input_digest
            || claim.physical_attempt_id != attempt.physical_id
            || claim.scope != attempt.scope
            || claim.issued_at.0 != attempt.claimed_at.get()
        {
            return Err(invalid());
        }
        Ok(ExecutionPendingDirectorySigningAuthorityV2 { attempt, claim })
    }
    pub fn record_pending_directory_signature_v2(
        &mut self,
        authority: ExecutionPendingDirectorySigningAuthorityV2,
        signature_hex: String,
        pin: AccountMachineIssuerPinV2,
        floor: AccountMachineIssuerFloorV2,
        now: UnixMillis,
    ) -> Result<Option<ExecutionPendingDirectoryDispatchV2>> {
        let expected = self.pending_directory_signing_authority_v2(
            authority.attempt,
            authority.claim.issuer.clone(),
        )?;
        if expected.claim != authority.claim || now.get() < expected.attempt.claimed_at.get() {
            return Err(invalid());
        }
        let proof = SignedExecutionPendingCurrentLeafV2 {
            claim: expected.claim,
            signature_hex,
        };
        proof
            .verify_issuer(
                &pin,
                &floor,
                PendingCurrentReceiverV2::ManagerAssociationsActor,
                proof.claim.issued_at.0,
            )
            .map_err(|_| invalid())?;
        let id = expected.attempt.session_id().to_owned();
        let mut next = self.clone();
        let saved = next.test_session_v2_record_mut(&id)?;
        let p = saved.execution_pending_start.as_mut().ok_or_else(invalid)?;
        p.directory_signature = Some(PendingDirectorySignatureV2 {
            proof: proof.clone(),
            pin,
            floor,
            recorded_at: now,
        });
        let request_id = saved
            .execution_pending_ingresses
            .get(&p.ingress_attempt_id)
            .ok_or_else(invalid)?
            .request
            .request_id
            .clone();
        append_session_event_v2(
            saved,
            SessionEventKindV2::ExecutionPendingDirectorySignatureRecorded { request_id },
            now,
        )?;
        next.validate_persistence()?;
        *self = next;
        if now.get() >= proof.claim.expires_at.0 {
            return Ok(None);
        }
        Ok(Some(ExecutionPendingDirectoryDispatchV2 {
            attempt: expected.attempt,
            proof,
        }))
    }
}

fn invalid() -> ProvisionApprovalError {
    ProvisionApprovalError::BindingMismatch
}
