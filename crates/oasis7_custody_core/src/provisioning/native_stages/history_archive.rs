use super::*;
use crate::native_history::*;
pub const MAX_NATIVE_HISTORY_ENTRIES: usize = 64;
impl AccountProvisionApprovalState {
    pub fn native_history(&self) -> &BTreeMap<String, NativeHistoryEntryV1> {
        &self.native_history
    }
    pub fn native_history_archive_checks(
        &self,
        entry: &NativeHistoryEntryV1,
        scope: &NativeHistoryVerificationContext<'_>,
    ) -> Result<ExactSignatureChecks> {
        self.validate_known_native_history(entry)?;
        if self
            .native_stages
            .values()
            .all(|v| v.descriptor.http_origin != scope.http_origin)
        {
            return Err(ProvisionApprovalError::BindingMismatch);
        }
        Ok(native_history_signature_checks(
            entry,
            scope.native_root_key_id,
            scope.native_root_public_key,
        )?)
    }
    /// Only the fixed Identity history RPC supplies this entry. This mutates facts, never authority.
    pub fn archive_native_history(
        &mut self,
        entry: NativeHistoryEntryV1,
        scope: &NativeHistoryVerificationContext<'_>,
        verifier: &mut impl ApprovalSignatureVerifier,
    ) -> Result<NativeHistoryEntryV1> {
        let digest = entry
            .digest()
            .map_err(|_| ProvisionApprovalError::BindingMismatch)?;
        if let Some(old) = self.native_history.get(&digest) {
            return if old == &entry {
                Ok(old.clone())
            } else {
                Err(ProvisionApprovalError::Conflict)
            };
        }
        if self.native_history.len() >= MAX_NATIVE_HISTORY_ENTRIES {
            return Err(ProvisionApprovalError::Conflict);
        }
        verifier.verify(&self.native_history_archive_checks(&entry, scope)?)?;
        let mut next = self.clone();
        next.native_history.insert(digest, entry.clone());
        next.validate_persistence()?;
        *self = next;
        Ok(entry)
    }
    pub(in crate::provisioning) fn validate_history_persistence(&self) -> Result<()> {
        if self.native_history.len() > MAX_NATIVE_HISTORY_ENTRIES {
            return Err(ProvisionApprovalError::InvalidPersistence);
        }
        for (digest, entry) in &self.native_history {
            if entry
                .digest()
                .map_err(|_| ProvisionApprovalError::InvalidPersistence)?
                != *digest
            {
                return Err(ProvisionApprovalError::InvalidPersistence);
            }
            self.validate_known_native_history(entry)?;
        }
        Ok(())
    }
    pub(super) fn validate_known_native_history(&self, entry: &NativeHistoryEntryV1) -> Result<()> {
        entry
            .validate()
            .map_err(|_| ProvisionApprovalError::BindingMismatch)?;
        let identity = self
            .role_key_results
            .get("identity")
            .ok_or(ApprovalAdmissionError::Missing)?;
        if entry.environment != self.draft.entry.environment
            || entry.custody_account_id != self.draft.entry.custody_account_id
            || entry.provision_id != self.draft.entry.provision_id
            || entry.scope.role_scope != identity.scope
        {
            return Err(ProvisionApprovalError::BindingMismatch);
        }
        let (descriptor, native_operation, nonce) = match entry.phase {
            NativeHistoryPhaseV1::ManagedLogin => {
                let task = self
                    .managed_login_task(&entry.original_permit_id)
                    .ok_or(ApprovalAdmissionError::Missing)?;
                let d = self
                    .native_stages
                    .values()
                    .find(|v| v.request.stage == NativeStageKindV1::Login)
                    .ok_or(ApprovalAdmissionError::Missing)?
                    .descriptor
                    .clone();
                let nonce = if entry.result_permit_id == entry.original_permit_id {
                    if task.claimed_at.is_none() {
                        return Err(ProvisionApprovalError::BindingMismatch);
                    }
                    task.status_nonce.clone()
                } else {
                    let recovery = self
                        .session_status_recoveries
                        .get(&entry.result_permit_id)
                        .ok_or(ApprovalAdmissionError::Missing)?;
                    if recovery.claimed_at.is_none()
                        || recovery.source_permit_id != entry.original_permit_id
                    {
                        return Err(ProvisionApprovalError::BindingMismatch);
                    }
                    recovery.nonce.clone()
                };
                (d, task.login_operation_id.clone(), Some(nonce))
            }
            _ => {
                let original = self
                    .native_stages
                    .get(&entry.original_permit_id)
                    .ok_or(ApprovalAdmissionError::Missing)?;
                if original.claimed_at.is_none() {
                    return Err(ProvisionApprovalError::BindingMismatch);
                }
                let expected = match entry.phase {
                    NativeHistoryPhaseV1::Register => NativeStageKindV1::Registration,
                    NativeHistoryPhaseV1::Binding => NativeStageKindV1::Binding,
                    _ => NativeStageKindV1::Login,
                };
                if original.request.stage != expected {
                    return Err(ProvisionApprovalError::BindingMismatch);
                }
                let op = match entry.phase {
                    NativeHistoryPhaseV1::Register => original
                        .descriptor
                        .registration
                        .registration_operation_id
                        .clone(),
                    NativeHistoryPhaseV1::Binding => original.request.binding_operation_id.clone(),
                    _ => original.descriptor.login_request_id.clone(),
                };
                let nonce = if let Some(result) = self.native_stages.get(&entry.result_permit_id) {
                    if result.claimed_at.is_none()
                        || result.request.stage != expected
                        || result.descriptor != original.descriptor
                    {
                        return Err(ProvisionApprovalError::BindingMismatch);
                    }
                    if result.result.as_ref().is_some_and(|v| v != &entry.result) {
                        return Err(ProvisionApprovalError::Conflict);
                    }
                    Some(result.nonce.clone())
                } else {
                    let recovery = self
                        .session_status_recoveries
                        .get(&entry.result_permit_id)
                        .ok_or(ApprovalAdmissionError::Missing)?;
                    if recovery.claimed_at.is_none()
                        || recovery.source_permit_id != entry.original_permit_id
                        || expected != NativeStageKindV1::Login
                    {
                        return Err(ProvisionApprovalError::BindingMismatch);
                    }
                    Some(recovery.nonce.clone())
                };
                (original.descriptor.clone(), op, nonce)
            }
        };
        if entry.native_operation_id != native_operation
            || entry.scope.identity_namespace != descriptor.registration.identity_namespace
            || entry.scope.identity_instance_epoch
                != descriptor.registration.identity_instance_epoch
            || entry.scope.registration_operation_id
                != descriptor.registration.registration_operation_id
            || entry.scope.initial_login_operation_id != descriptor.login_request_id
        {
            return Err(ProvisionApprovalError::BindingMismatch);
        }
        match &entry.result {
            NativeStageResultV1::RegistrationChallenge(v) => {
                let c = &v.challenge;
                if c.registration_operation_id != descriptor.registration.registration_operation_id
                    || c.registration_request_digest
                        != descriptor
                            .registration
                            .digest()
                            .map_err(|_| ProvisionApprovalError::BindingMismatch)?
                    || c.identity_namespace != entry.scope.identity_namespace
                    || c.identity_instance_epoch != entry.scope.identity_instance_epoch
                    || c.login_public_key != descriptor.identity_key.public_key
                {
                    return Err(ProvisionApprovalError::BindingMismatch);
                }
            }
            NativeStageResultV1::BindingReceipt(v) => {
                self.validate_history_binding_receipt(&v.receipt, &descriptor)?;
            }
            NativeStageResultV1::SessionStatus(v) => {
                self.validate_history_binding_receipt(&v.status.receipt, &descriptor)?;
                if Some(&v.status.nonce) != nonce.as_ref() {
                    return Err(ProvisionApprovalError::BindingMismatch);
                }
            }
            _ => return Err(ProvisionApprovalError::BindingMismatch),
        }
        Ok(())
    }
    fn validate_history_binding_receipt(
        &self,
        receipt: &NativeBindingReceiptV1,
        descriptor: &NativeStageDescriptorV1,
    ) -> Result<()> {
        let statement = self
            .native_stages
            .values()
            .filter_map(|v| v.binding_statement.as_ref())
            .find(|v| v.statement.digest().ok().as_ref() == Some(&receipt.statement_digest))
            .ok_or(ApprovalAdmissionError::Missing)?;
        receipt
            .validate_for(&statement.statement)
            .map_err(|_| ProvisionApprovalError::BindingMismatch)?;
        if receipt.environment != self.draft.entry.environment
            || receipt.custody_account_id != self.draft.entry.custody_account_id
            || receipt.provision_id != self.draft.entry.provision_id
            || receipt.login_key_ref != descriptor.identity_key.key_id
            || receipt.login_public_key != descriptor.identity_key.public_key
            || receipt.registration_operation_id
                != descriptor.registration.registration_operation_id
            || receipt.identity_namespace != descriptor.registration.identity_namespace
            || receipt.identity_instance_epoch != descriptor.registration.identity_instance_epoch
        {
            return Err(ProvisionApprovalError::BindingMismatch);
        }
        Ok(())
    }
}
