//! Persisted source observations and one-shot transport claims for provisioning.
//! These records never authenticate RPC callers; adapters supply fixed-role context.
use super::*;
use crate::approval::{ApprovalVerifyRequestV1, OriginStateV1, VerificationPurposeV1};
use crate::approval_admission::registered_origin_checks;

pub const MAX_PROVISION_ATTEMPTS: usize = 256;
pub const PROVISION_ORIGIN_FRESHNESS_MS: u64 = 5_000;
pub const PROVISION_ORIGIN_PENDING_MS: u64 = 30_000;
/// Conservative fixed-hop reservations; interruption never refunds them.
pub const PROVISION_ORIGIN_TRANSPORT_COST: u64 = 8;
pub const PROVISION_ROLE_TRANSPORT_COST: u64 = 32;
pub const MAX_PROVISION_ROLE_PERMIT_READS: u8 = 3;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProvisionOriginInspectionV1 {
    pub operation_id: String,
    pub verify_nonce: String,
    pub created_at: DecimalU64,
    pub expires_at: DecimalU64,
    pub claimed_at: Option<DecimalU64>,
    pub request_bytes: Option<String>,
    pub verified_at: Option<DecimalU64>,
    pub verification: Option<SignedApprovalVerificationV1>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProvisionRoleKeyRequestV1 {
    pub api_version: u8,
    pub custody_account_id: String,
    pub provision_id: String,
    pub task_operation_id: String,
    pub permit_id: String,
}
impl ProvisionRoleKeyRequestV1 {
    pub fn canonical_bytes(&self) -> Result<Vec<u8>> {
        self.validate()?;
        oasis7_wasm_abi::encode_canonical_cbor(&(
            "oasis7.custody.provision-role-key-request.v1",
            self,
        ))
        .map_err(|_| ProvisionApprovalError::InvalidPersistence)
    }
    pub fn digest(&self) -> Result<String> {
        Ok(crate::approval::bytes_digest(&self.canonical_bytes()?))
    }
    pub fn validate(&self) -> Result<()> {
        if self.api_version != 1
            || !super::super::types::valid_id(&self.custody_account_id)
            || !super::super::types::valid_id(&self.provision_id)
            || !super::super::types::valid_id(&self.task_operation_id)
            || !digest_hex(&self.permit_id)
        {
            return Err(ProvisionApprovalError::BindingMismatch);
        }
        Ok(())
    }
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProvisionRoleKeyPermitViewV1 {
    pub api_version: u8,
    pub permit_id: String,
    pub attempt_id: String,
    pub request_digest: String,
    pub scope: RoleKeySlotScopeV1,
    pub authority_revision: String,
    pub expires_at: DecimalU64,
    pub service_epoch: DecimalU64,
    pub observed_at: DecimalU64,
}
impl ProvisionRoleKeyPermitViewV1 {
    pub fn validate_for(
        &self,
        request: &ProvisionRoleKeyRequestV1,
        caller_role: Role,
    ) -> Result<()> {
        request.validate()?;
        if self.api_version != 1
            || !super::super::types::valid_id(&self.attempt_id)
            || self.scope.context.identity.role != caller_role
            || self.scope.provision_id != request.provision_id
            || self.scope.context.identity.account_id != request.custody_account_id
            || self.scope.operation_id != request.task_operation_id
            || self.permit_id != request.permit_id
            || self.request_digest != request.digest()?
        {
            return Err(ProvisionApprovalError::BindingMismatch);
        }
        Ok(())
    }
    pub fn validate_at(&self, now: u64) -> Result<()> {
        self.scope
            .validate()
            .map_err(|_| ProvisionApprovalError::BindingMismatch)?;
        if self.api_version != 1
            || !super::super::types::valid_id(&self.attempt_id)
            || !digest_hex(&self.request_digest)
            || !digest_hex(&self.permit_id)
            || !digest_hex(&self.authority_revision)
            || self.service_epoch.0 == 0
            || self.observed_at.0 > now
            || now - self.observed_at.0 > PROVISION_ORIGIN_FRESHNESS_MS
            || now >= self.expires_at.0
        {
            return Err(ProvisionApprovalError::BindingMismatch);
        }
        Ok(())
    }
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProvisionRoleKeyDispatchV1 {
    pub operation_id: String,
    pub inspection_id: String,
    pub scope: RoleKeySlotScopeV1,
    pub permit_id: String,
    pub authority_revision: String,
    pub claimed_at: DecimalU64,
    pub expires_at: DecimalU64,
    pub request_bytes: String,
    pub request_digest: String,
    pub result: Option<PublicRoleKey>,
    pub permit_reads: u8,
}
fn digest_hex(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}
fn hash<T: Serialize>(domain: &str, value: &T) -> Result<String> {
    oasis7_wasm_abi::encode_canonical_cbor(&(domain, value))
        .map(|bytes| crate::approval::bytes_digest(&bytes))
        .map_err(|_| ProvisionApprovalError::InvalidPersistence)
}
fn role_name(role: Role) -> Result<&'static str> {
    match role {
        Role::Identity => Ok("identity"),
        Role::Owner => Ok("owner"),
        Role::Executor => Ok("executor"),
        Role::RecoveryFence => Err(ProvisionApprovalError::BindingMismatch),
    }
}
impl AccountProvisionApprovalState {
    pub fn origin_inspections(&self) -> &BTreeMap<String, ProvisionOriginInspectionV1> {
        &self.origin_inspections
    }
    pub fn role_key_dispatches(&self) -> &BTreeMap<String, ProvisionRoleKeyDispatchV1> {
        &self.role_key_dispatches
    }
    pub fn accepted_approval(&self) -> Result<&ApprovalAdmissionRecord> {
        let id = self
            .accepted_operation_id
            .as_deref()
            .ok_or(ApprovalAdmissionError::Missing)?;
        self.original(id)
            .filter(|r| r.stage() == ApprovalAdmissionStage::Consumed)
            .ok_or(ApprovalAdmissionError::Missing.into())
    }
    /// This local check permits only tightening budget/state. It establishes no
    /// current remote directory, source or Fence authority for a transmission.
    fn check_local_plan(&self, now: u64) -> Result<()> {
        let accepted = self.accepted_approval()?;
        let signed = accepted
            .verification()
            .ok_or(ApprovalAdmissionError::Missing)?;
        self.provision
            .check_current(accepted.input().request.context.security_version.0, now)?;
        if self.cancellation_operation_id.is_some()
            || now
                < accepted
                    .consumed_at()
                    .ok_or(ApprovalAdmissionError::Missing)?
            || now >= signed.verification.source_session_expires_at.0
            || now >= signed.verification.source_session_idle_expires_at.0
        {
            return Err(ProvisionApprovalError::BindingMismatch);
        }
        Ok(())
    }
    /// Retry returns the stored original nonce. This convenience path still
    /// checks trusted current authority; real callers reserve before remote I/O.
    pub fn begin_origin_inspection(
        &mut self,
        operation_id: &str,
        nonce: &str,
        current: &ProvisionApprovalContext<'_>,
        now: u64,
    ) -> Result<ProvisionOriginInspectionV1> {
        let accepted = self.accepted_approval()?.input().operation_id.clone();
        self.historical_scope(&accepted, current)?;
        if let Some(original) = self.origin_inspections.get(operation_id) {
            return Ok(original.clone());
        }
        self.check_role_key_current(current, now)?;
        self.reserve_origin_inspection(operation_id, nonce, now)
    }
    /// Reserve the complete fixed-hop cost and immutable nonce before any
    /// remote preflight. The returned record contains no send permission.
    pub fn reserve_origin_inspection(
        &mut self,
        operation_id: &str,
        nonce: &str,
        now: u64,
    ) -> Result<ProvisionOriginInspectionV1> {
        self.check_local_plan(now)?;
        // The preflight itself may already have used remote hops. Repeating
        // this reservation cannot mint another I/O allowance from the same cost.
        if self.origin_inspections.contains_key(operation_id) {
            return Err(ProvisionApprovalError::Conflict);
        }
        if !super::super::types::valid_id(operation_id)
            || !digest_hex(nonce)
            || self.origin_inspections.len() >= MAX_PROVISION_ATTEMPTS
            || self.role_key_dispatches.contains_key(operation_id)
            || self
                .origin_inspections
                .values()
                .any(|v| v.verify_nonce == nonce)
            || self.book.records().any(|v| v.verify_nonce() == nonce)
        {
            return Err(ProvisionApprovalError::Conflict);
        }
        let approved_source = &self
            .accepted_approval()?
            .verification()
            .ok_or(ApprovalAdmissionError::Missing)?
            .verification;
        let source_until = approved_source
            .source_session_expires_at
            .0
            .min(approved_source.source_session_idle_expires_at.0);
        let original = ProvisionOriginInspectionV1 {
            operation_id: operation_id.into(),
            verify_nonce: nonce.into(),
            created_at: DecimalU64(now),
            expires_at: DecimalU64(
                now.checked_add(PROVISION_ORIGIN_PENDING_MS)
                    .ok_or(ProvisionApprovalError::Conflict)?
                    .min(self.draft.plan.expires_at.0)
                    .min(source_until),
            ),
            claimed_at: None,
            request_bytes: None,
            verified_at: None,
            verification: None,
        };
        let mut next = self.clone();
        let epoch = next
            .accepted_approval()?
            .input()
            .request
            .context
            .security_version
            .0;
        for _ in 0..PROVISION_ORIGIN_TRANSPORT_COST {
            next.provision.charge_transport(epoch, now)?;
        }
        next.origin_inspections
            .insert(operation_id.into(), original.clone());
        next.validate_persistence()?;
        *self = next;
        Ok(original)
    }
    /// A successful return is a committed transport claim. Calling this twice is
    /// rejected even when the first response was lost; a new attempt costs again.
    pub fn claim_origin_inspection(
        &mut self,
        operation_id: &str,
        current: &ProvisionApprovalContext<'_>,
        now: u64,
    ) -> Result<ApprovalVerifyRequestV1> {
        self.check_role_key_current(current, now)?;
        let original = self
            .origin_inspections
            .get(operation_id)
            .ok_or(ApprovalAdmissionError::Missing)?;
        if original.claimed_at.is_some() || now >= original.expires_at.0 {
            return Err(ProvisionApprovalError::Conflict);
        }
        let mut request = self.accepted_approval()?.native_verify_request()?;
        request.purpose = VerificationPurposeV1::InspectOrigin;
        request.verify_nonce = original.verify_nonce.clone();
        let request_bytes = serde_json::to_string(&request)
            .map_err(|_| ProvisionApprovalError::InvalidPersistence)?;
        let mut next = self.clone();
        let stored = next
            .origin_inspections
            .get_mut(operation_id)
            .ok_or(ApprovalAdmissionError::Missing)?;
        stored.claimed_at = Some(DecimalU64(now));
        stored.request_bytes = Some(request_bytes);
        next.validate_persistence()?;
        *self = next;
        Ok(request)
    }
    pub fn origin_inspection_request_bytes(&self, operation_id: &str) -> Result<&str> {
        self.origin_inspections
            .get(operation_id)
            .and_then(|v| v.request_bytes.as_deref())
            .ok_or(ApprovalAdmissionError::Missing.into())
    }
    pub fn origin_inspection_checks(
        &self,
        operation_id: &str,
        signed: &SignedApprovalVerificationV1,
        current: &ProvisionApprovalContext<'_>,
        now: u64,
    ) -> Result<ExactSignatureChecks> {
        self.check_role_key_current(current, now)?;
        let original = self
            .origin_inspections
            .get(operation_id)
            .ok_or(ApprovalAdmissionError::Missing)?;
        self.validate_origin_response(original, signed, now)?;
        if original.verification.as_ref().is_some_and(|v| v != signed) {
            return Err(ProvisionApprovalError::Conflict);
        }
        Ok(registered_origin_checks(
            self.accepted_approval()?.input(),
            operation_id,
            &original.verify_nonce,
            current.approval,
            signed,
        )?)
    }
    pub fn complete_origin_inspection(
        &mut self,
        operation_id: &str,
        signed: SignedApprovalVerificationV1,
        current: &ProvisionApprovalContext<'_>,
        now: u64,
        verifier: &mut impl ApprovalSignatureVerifier,
    ) -> Result<ProvisionOriginInspectionV1> {
        if let Some(original) = self
            .origin_inspections
            .get(operation_id)
            .filter(|v| v.verification.is_some())
        {
            self.historical_scope(&self.accepted_approval()?.input().operation_id, current)?;
            return if original.verification.as_ref() == Some(&signed) {
                Ok(original.clone())
            } else {
                Err(ProvisionApprovalError::Conflict)
            };
        }
        let checks = self.origin_inspection_checks(operation_id, &signed, current, now)?;
        verifier.verify(&checks)?;
        let mut next = self.clone();
        let original = next
            .origin_inspections
            .get_mut(operation_id)
            .ok_or(ApprovalAdmissionError::Missing)?;
        original.verified_at = Some(DecimalU64(now));
        original.verification = Some(signed);
        let result = original.clone();
        next.validate_persistence()?;
        *self = next;
        Ok(result)
    }
    pub(super) fn validate_origin_response(
        &self,
        original: &ProvisionOriginInspectionV1,
        signed: &SignedApprovalVerificationV1,
        now: u64,
    ) -> Result<()> {
        let record = self.accepted_approval()?;
        let input = record.input();
        let proof = &input.approval.proof;
        let v = &signed.verification;
        let claimed = original
            .claimed_at
            .ok_or(ApprovalAdmissionError::Missing)?
            .0;
        if now >= original.expires_at.0
            || v.version != 1
            || v.purpose != VerificationPurposeV1::InspectOrigin
            || v.origin_state != OriginStateV1::Active
            || v.identity_namespace != proof.identity_namespace
            || v.proof_digest
                != input
                    .approval
                    .digest()
                    .map_err(|_| ProvisionApprovalError::BindingMismatch)?
            || v.approval_request_id != proof.approval_request_id
            || v.audience != proof.context.audience
            || v.intent_digest != proof.intent_digest
            || v.verify_nonce != original.verify_nonce
            || v.proof_expires_at != proof.expires_at
            || v.identity_instance_epoch != proof.identity_instance_epoch
            || v.manager_security_version != proof.manager_security_version
            || v.manager_policy_version != proof.manager_policy_version
            || v.credential_version != proof.passkey_credential_version
            || v.origin_credential_version != proof.origin_credential_version
            || v.observed_at.0 < claimed
            || v.observed_at.0 > now
            || now - v.observed_at.0 > PROVISION_ORIGIN_FRESHNESS_MS
            || now >= v.source_session_expires_at.0
            || now >= v.source_session_idle_expires_at.0
        {
            return Err(ProvisionApprovalError::BindingMismatch);
        }
        Ok(())
    }
    fn fresh_inspection(
        &self,
        id: &str,
        current: &ProvisionApprovalContext<'_>,
        now: u64,
    ) -> Result<&ProvisionOriginInspectionV1> {
        self.check_role_key_current(current, now)?;
        let inspection = self
            .origin_inspections
            .get(id)
            .ok_or(ApprovalAdmissionError::Missing)?;
        let signed = inspection
            .verification
            .as_ref()
            .ok_or(ApprovalAdmissionError::Missing)?;
        self.validate_origin_response(inspection, signed, now)?;
        let v = &signed.verification;
        if v.source_session_expires_at != current.approval.source.session_expires_at
            || v.source_session_idle_expires_at != current.approval.source.session_idle_expires_at
        {
            return Err(ProvisionApprovalError::BindingMismatch);
        }
        Ok(inspection)
    }
    /// Re-authenticate persisted native evidence with the current pinned keys
    /// before a new dispatch. Deserialization alone is never source authority.
    pub fn role_key_dispatch_checks(
        &self,
        role: Role,
        inspection_id: &str,
        current: &ProvisionApprovalContext<'_>,
        now: u64,
    ) -> Result<ExactSignatureChecks> {
        role_name(role)?;
        let inspection = self.fresh_inspection(inspection_id, current, now)?;
        self.origin_inspection_checks(
            inspection_id,
            inspection
                .verification
                .as_ref()
                .ok_or(ApprovalAdmissionError::Missing)?,
            current,
            now,
        )
    }
    pub fn role_key_permit_checks(
        &self,
        permit_id: &str,
        caller_role: Role,
        current: &ProvisionApprovalContext<'_>,
        now: u64,
    ) -> Result<ExactSignatureChecks> {
        let dispatch = self
            .role_key_dispatches
            .values()
            .find(|d| d.permit_id == permit_id)
            .ok_or(ApprovalAdmissionError::Missing)?;
        if dispatch.scope.context.identity.role != caller_role {
            return Err(ProvisionApprovalError::BindingMismatch);
        }
        self.role_key_dispatch_checks(caller_role, &dispatch.inspection_id, current, now)
    }
    pub fn claim_role_key_dispatch(
        &mut self,
        operation_id: &str,
        role: Role,
        inspection_id: &str,
        current: &ProvisionApprovalContext<'_>,
        now: u64,
        verifier: &mut impl ApprovalSignatureVerifier,
    ) -> Result<ProvisionRoleKeyPermitViewV1> {
        let name = role_name(role)?;
        if self.role_key_dispatches.contains_key(operation_id)
            || self.origin_inspections.contains_key(operation_id)
            || self.role_key_dispatches.len() >= MAX_PROVISION_ATTEMPTS
            || !super::super::types::valid_id(operation_id)
            || self.role_key_results.contains_key(name)
        {
            return Err(ProvisionApprovalError::Conflict);
        }
        let checks = self.role_key_dispatch_checks(role, inspection_id, current, now)?;
        verifier.verify(&checks)?;
        let inspection = self.fresh_inspection(inspection_id, current, now)?;
        let task = self
            .role_key_tasks
            .get(name)
            .filter(|t| t.state == ProvisionRoleKeyTaskStateV1::Pending)
            .ok_or(ApprovalAdmissionError::Missing)?;
        let scope = RoleKeySlotScopeV1::from_task(task)
            .map_err(|_| ProvisionApprovalError::BindingMismatch)?;
        let signed = inspection
            .verification
            .as_ref()
            .ok_or(ApprovalAdmissionError::Missing)?;
        let expires_at = DecimalU64(
            inspection
                .expires_at
                .0
                .min(signed.verification.source_session_expires_at.0)
                .min(signed.verification.source_session_idle_expires_at.0)
                .min(
                    signed
                        .verification
                        .observed_at
                        .0
                        .checked_add(PROVISION_ORIGIN_FRESHNESS_MS)
                        .ok_or(ProvisionApprovalError::Conflict)?,
                ),
        );
        let authority_revision = hash(
            "oasis7.custody.provision-role-authority.v1",
            &(&scope, signed, &current.approval.authority),
        )?;
        let permit_id = hash(
            "oasis7.custody.provision-role-permit.v1",
            &(
                operation_id,
                inspection_id,
                &scope,
                &authority_revision,
                expires_at,
            ),
        )?;
        let request = ProvisionRoleKeyRequestV1 {
            api_version: 1,
            custody_account_id: scope.context.identity.account_id.clone(),
            provision_id: scope.provision_id.clone(),
            task_operation_id: scope.operation_id.clone(),
            permit_id: permit_id.clone(),
        };
        let request_bytes = serde_json::to_string(&request)
            .map_err(|_| ProvisionApprovalError::InvalidPersistence)?;
        let request_digest = request.digest()?;
        let dispatch = ProvisionRoleKeyDispatchV1 {
            operation_id: operation_id.into(),
            inspection_id: inspection_id.into(),
            scope,
            permit_id,
            authority_revision,
            claimed_at: DecimalU64(now),
            expires_at,
            request_bytes,
            request_digest,
            result: None,
            permit_reads: 0,
        };
        let mut next = self.clone();
        for _ in 0..PROVISION_ROLE_TRANSPORT_COST {
            next.provision
                .charge_transport(current.approval.authority.security_version.0, now)?;
        }
        next.role_key_dispatches
            .insert(operation_id.into(), dispatch.clone());
        next.validate_persistence()?;
        *self = next;
        self.permit_view(&dispatch, now)
    }
    fn permit_view(
        &self,
        dispatch: &ProvisionRoleKeyDispatchV1,
        now: u64,
    ) -> Result<ProvisionRoleKeyPermitViewV1> {
        let view = ProvisionRoleKeyPermitViewV1 {
            api_version: 1,
            permit_id: dispatch.permit_id.clone(),
            attempt_id: dispatch.operation_id.clone(),
            request_digest: dispatch.request_digest.clone(),
            scope: dispatch.scope.clone(),
            authority_revision: dispatch.authority_revision.clone(),
            expires_at: dispatch.expires_at,
            service_epoch: self
                .accepted_approval()?
                .input()
                .request
                .context
                .service_epoch,
            observed_at: DecimalU64(now),
        };
        view.validate_at(now)?;
        Ok(view)
    }
    pub fn validate_role_key_permit(
        &self,
        permit_id: &str,
        caller_role: Role,
        current: &ProvisionApprovalContext<'_>,
        now: u64,
    ) -> Result<ProvisionRoleKeyPermitViewV1> {
        let dispatch = self
            .role_key_dispatches
            .values()
            .find(|d| d.permit_id == permit_id)
            .ok_or(ApprovalAdmissionError::Missing)?;
        if dispatch.scope.context.identity.role != caller_role {
            return Err(ProvisionApprovalError::BindingMismatch);
        }
        self.fresh_inspection(&dispatch.inspection_id, current, now)?;
        self.permit_view(dispatch, now)
    }
    /// Tightening only: consumes one of the pre-reserved read credits before
    /// remote directory/Fence I/O, and returns a local ticket, not a permit view.
    pub fn reserve_role_key_permit_read(
        &mut self,
        permit_id: &str,
        caller_role: Role,
        now: u64,
    ) -> Result<u8> {
        self.check_local_plan(now)?;
        let dispatch = self
            .role_key_dispatches
            .values()
            .find(|d| d.permit_id == permit_id)
            .ok_or(ApprovalAdmissionError::Missing)?;
        if dispatch.scope.context.identity.role != caller_role
            || now >= dispatch.expires_at.0
            || now < dispatch.claimed_at.0
            || dispatch.permit_reads >= MAX_PROVISION_ROLE_PERMIT_READS
        {
            return Err(ProvisionApprovalError::Conflict);
        }
        let inspection = self
            .origin_inspections
            .get(&dispatch.inspection_id)
            .ok_or(ApprovalAdmissionError::Missing)?;
        self.validate_origin_response(
            inspection,
            inspection
                .verification
                .as_ref()
                .ok_or(ApprovalAdmissionError::Missing)?,
            now,
        )?;
        let id = dispatch.operation_id.clone();
        let mut next = self.clone();
        let record = next
            .role_key_dispatches
            .get_mut(&id)
            .ok_or(ApprovalAdmissionError::Missing)?;
        record.permit_reads += 1;
        let ticket = record.permit_reads;
        next.validate_persistence()?;
        *self = next;
        Ok(ticket)
    }
    /// High-level component convenience path. Network adapters use the local
    /// reservation first, then verify remote observations and cryptography.
    pub fn claim_role_key_permit_read(
        &mut self,
        permit_id: &str,
        caller_role: Role,
        current: &ProvisionApprovalContext<'_>,
        now: u64,
        verifier: &mut impl ApprovalSignatureVerifier,
    ) -> Result<ProvisionRoleKeyPermitViewV1> {
        let checks = self.role_key_permit_checks(permit_id, caller_role, current, now)?;
        verifier.verify(&checks)?;
        let view = self.validate_role_key_permit(permit_id, caller_role, current, now)?;
        self.reserve_role_key_permit_read(permit_id, caller_role, now)?;
        Ok(view)
    }
    pub fn record_dispatched_role_key_result(
        &mut self,
        dispatch_id: &str,
        result: ProvisionRoleKeyResultV1,
        current: &ProvisionApprovalContext<'_>,
        now: u64,
    ) -> Result<PublicRoleKey> {
        let original = self
            .role_key_dispatches
            .get(dispatch_id)
            .ok_or(ApprovalAdmissionError::Missing)?;
        if original.scope != result.scope {
            return Err(ProvisionApprovalError::BindingMismatch);
        }
        if let Some(public) = &original.result {
            self.historical_scope(&self.accepted_approval()?.input().operation_id, current)?;
            return if public == &result.public_ref {
                Ok(public.clone())
            } else {
                Err(ProvisionApprovalError::Conflict)
            };
        }
        if original.permit_reads == 0 {
            return Err(ProvisionApprovalError::Conflict);
        }
        self.validate_role_key_permit(
            &original.permit_id,
            result.scope.context.identity.role,
            current,
            now,
        )?;
        let mut next = self.clone();
        let public = next.record_role_key_result(result, current, now)?;
        next.role_key_dispatches
            .get_mut(dispatch_id)
            .ok_or(ApprovalAdmissionError::Missing)?
            .result = Some(public.clone());
        next.validate_persistence()?;
        *self = next;
        Ok(public)
    }
    pub(super) fn validate_attempt_persistence(&self) -> Result<u64> {
        if self.origin_inspections.len() > MAX_PROVISION_ATTEMPTS
            || self.role_key_dispatches.len() > MAX_PROVISION_ATTEMPTS
        {
            return Err(ProvisionApprovalError::InvalidPersistence);
        }
        if self.origin_inspections.is_empty() && self.role_key_dispatches.is_empty() {
            return Ok(0);
        }
        let accepted = self.accepted_approval()?;
        let proof = &accepted.input().approval.proof;
        let source = &accepted
            .verification()
            .ok_or(ProvisionApprovalError::InvalidPersistence)?
            .verification;
        let source_until = source
            .source_session_expires_at
            .0
            .min(source.source_session_idle_expires_at.0);

        let mut nonces = std::collections::BTreeSet::new();
        let mut charged = 0_u64;
        for (id, original) in &self.origin_inspections {
            if id != &original.operation_id
                || !super::super::types::valid_id(id)
                || !digest_hex(&original.verify_nonce)
                || !nonces.insert(&original.verify_nonce)
                || self
                    .book
                    .records()
                    .any(|r| r.verify_nonce() == original.verify_nonce)
                || original.created_at.0
                    < accepted
                        .consumed_at()
                        .ok_or(ProvisionApprovalError::InvalidPersistence)?
                || original.created_at.0 >= original.expires_at.0
                || original.expires_at.0
                    != original
                        .created_at
                        .0
                        .checked_add(PROVISION_ORIGIN_PENDING_MS)
                        .ok_or(ProvisionApprovalError::InvalidPersistence)?
                        .min(self.draft.plan.expires_at.0)
                        .min(source_until)
                || original
                    .claimed_at
                    .is_some_and(|at| at.0 < original.created_at.0 || at.0 >= original.expires_at.0)
                || original.verification.is_some() != original.verified_at.is_some()
            {
                return Err(ProvisionApprovalError::InvalidPersistence);
            }
            if original.claimed_at.is_some() != original.request_bytes.is_some() {
                return Err(ProvisionApprovalError::InvalidPersistence);
            }
            if original.claimed_at.is_some() {
                let mut expected = accepted.native_verify_request()?;
                expected.purpose = VerificationPurposeV1::InspectOrigin;
                expected.verify_nonce = original.verify_nonce.clone();
                let bytes = serde_json::to_string(&expected)
                    .map_err(|_| ProvisionApprovalError::InvalidPersistence)?;
                if original.request_bytes.as_deref() != Some(bytes.as_str()) {
                    return Err(ProvisionApprovalError::InvalidPersistence);
                }
            }
            charged += PROVISION_ORIGIN_TRANSPORT_COST;
            if let Some(signed) = &original.verification {
                self.validate_origin_response(
                    original,
                    signed,
                    original
                        .verified_at
                        .ok_or(ProvisionApprovalError::InvalidPersistence)?
                        .0,
                )?;
            }
        }
        for (id, dispatch) in &self.role_key_dispatches {
            let inspection = self
                .origin_inspections
                .get(&dispatch.inspection_id)
                .ok_or(ProvisionApprovalError::InvalidPersistence)?;
            let signed = inspection
                .verification
                .as_ref()
                .ok_or(ProvisionApprovalError::InvalidPersistence)?;
            let role = dispatch.scope.context.identity.role;
            let name = role_name(role)?;
            let expected_scope = RoleKeySlotScopeV1::from_task(
                self.role_key_tasks
                    .get(name)
                    .ok_or(ProvisionApprovalError::InvalidPersistence)?,
            )
            .map_err(|_| ProvisionApprovalError::InvalidPersistence)?;
            let revision = hash(
                "oasis7.custody.provision-role-authority.v1",
                &(&expected_scope, signed, &proof.context),
            )?;
            let expires = DecimalU64(
                inspection
                    .expires_at
                    .0
                    .min(signed.verification.source_session_expires_at.0)
                    .min(signed.verification.source_session_idle_expires_at.0)
                    .min(
                        signed
                            .verification
                            .observed_at
                            .0
                            .checked_add(PROVISION_ORIGIN_FRESHNESS_MS)
                            .ok_or(ProvisionApprovalError::Conflict)?,
                    ),
            );
            let permit = hash(
                "oasis7.custody.provision-role-permit.v1",
                &(
                    id,
                    &dispatch.inspection_id,
                    &expected_scope,
                    &revision,
                    expires,
                ),
            )?;
            let request = ProvisionRoleKeyRequestV1 {
                api_version: 1,
                custody_account_id: expected_scope.context.identity.account_id.clone(),
                provision_id: expected_scope.provision_id.clone(),
                task_operation_id: expected_scope.operation_id.clone(),
                permit_id: permit.clone(),
            };
            let request_bytes = serde_json::to_string(&request)
                .map_err(|_| ProvisionApprovalError::InvalidPersistence)?;
            if dispatch.request_bytes != request_bytes
                || dispatch.request_digest != request.digest()?
            {
                return Err(ProvisionApprovalError::InvalidPersistence);
            }
            if id != &dispatch.operation_id
                || !super::super::types::valid_id(id)
                || self.origin_inspections.contains_key(id)
                || dispatch.permit_reads > MAX_PROVISION_ROLE_PERMIT_READS
                || dispatch.result.is_some() && dispatch.permit_reads == 0
                || dispatch.scope != expected_scope
                || dispatch.authority_revision != revision
                || dispatch.permit_id != permit
                || dispatch.expires_at != expires
                || dispatch.claimed_at.0
                    < inspection
                        .verified_at
                        .ok_or(ProvisionApprovalError::InvalidPersistence)?
                        .0
                || dispatch.claimed_at.0 >= expires.0
                || dispatch.result.as_ref().is_some_and(|r| {
                    self.role_key_results.get(name).map(|v| &v.public_ref) != Some(r)
                })
            {
                return Err(ProvisionApprovalError::InvalidPersistence);
            }
            charged += PROVISION_ROLE_TRANSPORT_COST;
        }
        Ok(charged)
    }
}
