//! Actual Account-owned historical floor consumer. It is not a dispatch permit.
use super::*;
use crate::native_cleanup_v2::{NativeExternalFloorSnapshotV2, NativeFloorEvidenceV2};
impl AccountProvisionApprovalState {
    /// Called by the fixed Native-response receiver in the same Account SQL
    /// transition as its corresponding Ack/current record. It performs no I/O
    /// and does not return Root credentials, renew Source, or admit World facts.
    pub fn record_test_session_v2_native_floor(
        &mut self,
        id: &str,
        evidence: NativeFloorEvidenceV2,
        native: &NativeStageScope<'_>,
    ) -> Result<NativeExternalFloorSnapshotV2> {
        let session = self.test_session_v2_record(id)?;
        let original = session
            .book
            .original(id)
            .ok_or_else(|| ProvisionApprovalError::BindingMismatch)?;
        let (evidence_session, input_digest) = evidence.session_binding();
        if evidence_session != id
            || input_digest != session.input_digest()?
            || session.consumed_at.is_none()
            || original.stage() != ApprovalAdmissionStage::Consumed
            || original.input() != &session.input
        {
            return Err(ProvisionApprovalError::BindingMismatch);
        }
        let namespace = &session.input.approval.proof.identity_namespace;
        if namespace != &session.session.scope.identity_namespace {
            return Err(ProvisionApprovalError::BindingMismatch);
        }
        let mut next = self.clone();
        let store = next
            .native_machine_floor_v2
            .get_or_insert_with(crate::native_cleanup_v2::AccountNativeFloorStoreV2::empty);
        let floor = store
            .consume_history(native, namespace, evidence)
            .map_err(|_| ProvisionApprovalError::BindingMismatch)?;
        next.validate_persistence()?;
        *self = next;
        Ok(floor)
    }
    /// Native scope is fixed trusted configuration, never a key supplied by the
    /// stored evidence. All receive/send paths must call this after cold restore.
    pub fn test_session_v2_native_floor_scope(
        &self,
        id: &str,
        native: &NativeStageScope<'_>,
    ) -> Result<()> {
        let session = self.test_session_v2_record(id)?;
        self.native_machine_floor_v2
            .as_ref()
            .ok_or(ProvisionApprovalError::BindingMismatch)?
            .validate_native_scope(native, &session.input.approval.proof.identity_namespace)
            .map_err(|_| ProvisionApprovalError::BindingMismatch)
    }
}
