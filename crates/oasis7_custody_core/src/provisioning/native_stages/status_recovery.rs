use super::*;
use crate::{
    approval::{ApprovalVerifyRequestV1, VerificationPurposeV1},
    native_history::*,
};
pub const MAX_SESSION_STATUS_RECOVERIES: usize = 16;
impl AccountProvisionApprovalState {
    pub fn session_status_recoveries(&self) -> &BTreeMap<String, SessionStatusRecoveryOriginalV1> {
        &self.session_status_recoveries
    }
    pub(super) fn recovery_funding_for(
        &self,
        entry: &NativeHistoryEntryV1,
    ) -> Result<SessionStatusRecoveryFundingV1> {
        match entry.phase {
            NativeHistoryPhaseV1::InitialLogin => Ok(SessionStatusRecoveryFundingV1::Provision),
            NativeHistoryPhaseV1::ManagedLogin => {
                self.managed_login_task(&entry.original_permit_id)
                    .ok_or(ApprovalAdmissionError::Missing)?;
                Ok(SessionStatusRecoveryFundingV1::ManagedLogin {
                    operation_id: entry.original_permit_id.clone(),
                })
            }
            _ => Err(ProvisionApprovalError::BindingMismatch),
        }
    }
    pub(super) fn recovery_source_record(
        &self,
        funding: &SessionStatusRecoveryFundingV1,
    ) -> Result<&ApprovalAdmissionRecord> {
        match funding {
            SessionStatusRecoveryFundingV1::Provision => self.accepted_approval(),
            SessionStatusRecoveryFundingV1::ManagedLogin { operation_id } => self
                .managed_login_original(operation_id)
                .filter(|v| v.stage() == ApprovalAdmissionStage::Consumed)
                .ok_or(ApprovalAdmissionError::Missing.into()),
        }
    }
    pub(super) fn recovery_deadline(
        &self,
        funding: &SessionStatusRecoveryFundingV1,
    ) -> Result<u64> {
        Ok(match funding {
            SessionStatusRecoveryFundingV1::Provision => self.draft.plan.expires_at.0,
            SessionStatusRecoveryFundingV1::ManagedLogin { operation_id } => {
                self.managed_login_task(operation_id)
                    .ok_or(ApprovalAdmissionError::Missing)?
                    .deadline
                    .0
            }
        })
    }
    pub fn reserve_session_status_recovery(
        &mut self,
        request: SessionStatusRecoveryRequestV1,
        nonce: &str,
        origin_nonce: &str,
        scope: &NativeStageScope<'_>,
        now: u64,
    ) -> Result<SessionStatusRecoveryOriginalV1> {
        request.validate()?;
        if self
            .session_status_recoveries
            .contains_key(&request.operation_id)
            || self.session_status_recoveries.len() >= MAX_SESSION_STATUS_RECOVERIES
            || request.custody_account_id != self.draft.entry.custody_account_id
            || request.provision_id != self.draft.entry.provision_id
            || self.frozen()
            || self.cancellation_operation_id.is_some()
            || self.provision.security_epoch().0 != 1
            || self.bootstrap_activation.is_none()
            || !status_recovery_types::recovery_hex(nonce)
            || !status_recovery_types::recovery_hex(origin_nonce)
            || nonce == origin_nonce
        {
            return Err(ProvisionApprovalError::BindingMismatch);
        }
        let history = self
            .native_history
            .get(&request.history_digest)
            .ok_or(ApprovalAdmissionError::Missing)?;
        let NativeStageResultV1::SessionStatus(old) = &history.result else {
            return Err(ProvisionApprovalError::BindingMismatch);
        };
        let attestation = history
            .session_attestation
            .as_ref()
            .ok_or(ApprovalAdmissionError::Missing)?;
        let funding = self.recovery_funding_for(history)?;
        let source = self.recovery_source_record(&funding)?;
        let observed = &source
            .verification()
            .ok_or(ApprovalAdmissionError::Missing)?
            .verification;
        let deadline = self
            .recovery_deadline(&funding)?
            .min(observed.source_session_expires_at.0)
            .min(observed.source_session_idle_expires_at.0);
        if now >= deadline || source.consumed_at().is_none_or(|v| now < v) {
            return Err(ProvisionApprovalError::BindingMismatch);
        }
        let descriptor = self
            .native_stages
            .values()
            .find(|v| v.request.stage == NativeStageKindV1::Login)
            .ok_or(ApprovalAdmissionError::Missing)?
            .descriptor
            .clone();
        if scope.http_origin != descriptor.http_origin {
            return Err(ProvisionApprovalError::BindingMismatch);
        }
        if self
            .origin_inspections
            .values()
            .any(|v| v.verify_nonce == nonce || v.verify_nonce == origin_nonce)
            || self.session_status_recoveries.values().any(|v| {
                v.nonce == nonce
                    || v.nonce == origin_nonce
                    || v.source_inspection.verify_nonce == nonce
                    || v.source_inspection.verify_nonce == origin_nonce
            })
        {
            return Err(ProvisionApprovalError::Conflict);
        }
        let expires_at = DecimalU64(
            now.checked_add(30_000)
                .ok_or(ProvisionApprovalError::BindingMismatch)?
                .min(deadline),
        );
        let original = SessionStatusRecoveryOriginalV1 {
            source_permit_id: history.result_permit_id.clone(),
            private_sid_generation: attestation.private_sid_generation,
            native_binding_generation: old.status.generation,
            target_security_epoch: DecimalU64(1),
            http_origin: descriptor.http_origin,
            created_at: DecimalU64(now),
            expires_at,
            source_inspection: ProvisionOriginInspectionV1 {
                operation_id: super::super::super::types::digest(&(
                    "oasis7.custody.session-recovery-origin.v1",
                    &request,
                ))?,
                verify_nonce: origin_nonce.into(),
                created_at: DecimalU64(now),
                expires_at,
                claimed_at: None,
                request_bytes: None,
                verified_at: None,
                verification: None,
            },
            request: request.clone(),
            funding: funding.clone(),
            nonce: nonce.into(),
            claimed_at: None,
            reads: 0,
            result: None,
            completed_at: None,
            result_private_sid_generation: None,
        };
        let mut next = self.clone();
        match &funding {
            SessionStatusRecoveryFundingV1::Provision => {
                for _ in 0..64 {
                    next.provision.charge_transport(1, now)?;
                }
            }
            SessionStatusRecoveryFundingV1::ManagedLogin { operation_id } => {
                let task = next
                    .managed_logins
                    .get_mut(operation_id)
                    .and_then(|v| v.task.as_mut())
                    .ok_or(ApprovalAdmissionError::Missing)?;
                task.transport
                    .charge(64)
                    .map_err(|_| ProvisionError::Exhausted)?;
            }
        }
        next.session_status_recoveries
            .insert(request.operation_id, original.clone());
        next.validate_persistence()?;
        *self = next;
        Ok(original)
    }
    pub(super) fn recovery_current(
        &self,
        id: &str,
        current: &ProvisionApprovalContext<'_>,
        now: u64,
    ) -> Result<&SessionStatusRecoveryOriginalV1> {
        let original = self
            .session_status_recoveries
            .get(id)
            .ok_or(ApprovalAdmissionError::Missing)?;
        if self.frozen()
            || self.cancellation_operation_id.is_some()
            || self.provision.security_epoch() != original.target_security_epoch
            || current.approval.authority.security_version != original.target_security_epoch
            || now >= original.expires_at.0
            || now < original.created_at.0
        {
            return Err(ProvisionApprovalError::BindingMismatch);
        }
        match &original.funding {
            SessionStatusRecoveryFundingV1::Provision => {
                self.check_native_current(current, 1, now)?
            }
            SessionStatusRecoveryFundingV1::ManagedLogin { operation_id } => {
                let source = self.recovery_source_record(&original.funding)?;
                self.managed_login_scope(source.input(), current, now)?;
                if self.managed_login_task(operation_id).is_none() {
                    return Err(ProvisionApprovalError::BindingMismatch);
                }
            }
        }
        Ok(original)
    }
    pub fn reserve_session_status_recovery_read(&mut self, id: &str, now: u64) -> Result<u8> {
        if self.frozen()
            || self.provision.security_epoch().0 != 1
            || self.cancellation_operation_id.is_some()
        {
            return Err(ProvisionApprovalError::BindingMismatch);
        }
        let mut next = self.clone();
        let r = next
            .session_status_recoveries
            .get_mut(id)
            .ok_or(ApprovalAdmissionError::Missing)?;
        if r.reads >= 8 || r.result.is_some() || now >= r.expires_at.0 || now < r.created_at.0 {
            return Err(ProvisionApprovalError::Conflict);
        }
        r.reads += 1;
        let ticket = r.reads;
        next.validate_persistence()?;
        *self = next;
        Ok(ticket)
    }
    pub fn claim_session_recovery_origin(
        &mut self,
        id: &str,
        current: &ProvisionApprovalContext<'_>,
        now: u64,
    ) -> Result<ApprovalVerifyRequestV1> {
        let r = self.recovery_current(id, current, now)?;
        if r.source_inspection.claimed_at.is_some() {
            return Err(ProvisionApprovalError::Conflict);
        }
        let mut request = self
            .recovery_source_record(&r.funding)?
            .native_verify_request()?;
        request.purpose = VerificationPurposeV1::InspectOrigin;
        request.verify_nonce = r.source_inspection.verify_nonce.clone();
        let bytes = serde_json::to_string(&request)
            .map_err(|_| ProvisionApprovalError::InvalidPersistence)?;
        let mut next = self.clone();
        let stored = &mut next
            .session_status_recoveries
            .get_mut(id)
            .ok_or(ApprovalAdmissionError::Missing)?
            .source_inspection;
        stored.claimed_at = Some(DecimalU64(now));
        stored.request_bytes = Some(bytes);
        next.validate_persistence()?;
        *self = next;
        Ok(request)
    }
}
