use super::*;
use crate::native_history::{NativeHistoryPhaseV1, NativeMissingSessionV1};
impl AccountProvisionApprovalState {
    /// Only the fixed Identity named capability authenticates this local fact.
    /// No current grant is consulted because this transition only closes a send.
    pub fn tighten_fixed_identity_missing_session(
        &mut self,
        fact: &NativeMissingSessionV1,
    ) -> Result<()> {
        fact.validate()
            .map_err(|_| ProvisionApprovalError::BindingMismatch)?;
        let identity = self
            .role_key_results
            .get("identity")
            .ok_or(ApprovalAdmissionError::Missing)?;
        let initial = self
            .native_stages
            .values()
            .find(|v| v.request.stage == NativeStageKindV1::Login)
            .ok_or(ApprovalAdmissionError::Missing)?;
        let d = &initial.descriptor;
        if fact.scope.role_scope != identity.scope
            || fact.scope.role_scope.context.identity.account_id
                != self.draft.entry.custody_account_id
            || fact.scope.role_scope.provision_id != self.draft.entry.provision_id
            || fact.scope.identity_namespace != d.registration.identity_namespace
            || fact.scope.identity_instance_epoch != d.registration.identity_instance_epoch
            || fact.scope.registration_operation_id != d.registration.registration_operation_id
            || fact.scope.initial_login_operation_id != d.login_request_id
        {
            return Err(ProvisionApprovalError::BindingMismatch);
        }
        let mut next = self.clone();
        let (claimed, operation, result) = match fact.phase {
            NativeHistoryPhaseV1::InitialLogin => {
                let original = next
                    .native_stages
                    .get_mut(&fact.original_permit_id)
                    .ok_or(ApprovalAdmissionError::Missing)?;
                if original.request.stage != NativeStageKindV1::Login {
                    return Err(ProvisionApprovalError::BindingMismatch);
                }
                (
                    original.claimed_at,
                    original.descriptor.login_request_id.clone(),
                    &mut original.result,
                )
            }
            NativeHistoryPhaseV1::ManagedLogin => {
                let task = next
                    .managed_logins
                    .get_mut(&fact.original_permit_id)
                    .and_then(|v| v.task.as_mut())
                    .ok_or(ApprovalAdmissionError::Missing)?;
                (
                    task.claimed_at,
                    task.login_operation_id.clone(),
                    &mut task.result,
                )
            }
            _ => return Err(ProvisionApprovalError::BindingMismatch),
        };
        if claimed.is_none()
            || operation != fact.native_operation_id
            || result
                .as_ref()
                .is_some_and(|v| !matches!(v, NativeStageResultV1::ExplicitLoginRequired))
        {
            return Err(ProvisionApprovalError::BindingMismatch);
        }
        *result = Some(NativeStageResultV1::ExplicitLoginRequired);
        next.validate_persistence()?;
        *self = next;
        Ok(())
    }
}
