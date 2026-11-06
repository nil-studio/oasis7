//! Account is the only profile revocation/version authority. Vault cannot
//! overwrite the exact profile, clear its once journal or roll its revision.
use super::*;
impl AccountProvisionApprovalState {
    pub fn owner_termination_profile_v3(
        &self,
        subject: &str,
        now: UnixMillis,
    ) -> Result<OwnerTerminationProfileV3> {
        let saved = self.termination_v3.get(subject).ok_or_else(invalid)?;
        self.termination_v3_subject(&saved.plan, now)?;
        if saved.installed_at.is_none() || saved.profile_revoked_at.is_some() {
            return Err(invalid());
        }
        let mut actual = self.role_key_results.values().filter(|r| {
            r.scope.context.identity.role == crate::http::Role::Owner
                && r.scope.provision_id == saved.plan.provision_id
                && r.scope.context.identity.account_id == saved.plan.custody_account_id
                && r.public_ref.key_id == saved.plan.owner_key_ref
                && r.public_ref.public_key == saved.plan.initial_fence.owner_public_key
                && self.role_key_tasks.values().any(|task| {
                    task.state == ProvisionRoleKeyTaskStateV1::Pending
                        && crate::role_key_slot::RoleKeySlotScopeV1::from_task(task).as_ref()
                            == Ok(&r.scope)
                })
        });
        let result = actual.next().ok_or_else(invalid)?;
        if actual.next().is_some() {
            return Err(invalid());
        }
        let profile = OwnerTerminationProfileV3 {
            plan: saved.plan.clone(),
            slot: result.scope.clone(),
            profile_revision: DecimalU64(1),
        };
        profile.validate()?;
        Ok(profile)
    }
    pub fn record_owner_termination_profile_v3(
        &mut self,
        subject: &str,
        receipt: OwnerTerminationProfileReceiptV3,
        current: &ProvisionApprovalContext<'_>,
        now: UnixMillis,
    ) -> Result<()> {
        let exact = self.owner_termination_profile_v3(subject, now)?;
        receipt.verify()?;
        if receipt.profile != exact {
            return Err(ProvisionApprovalError::Conflict);
        }
        let saved = self.termination_v3.get(subject).ok_or_else(invalid)?;
        saved
            .book
            .original(&saved.input.operation_id)
            .ok_or_else(invalid)?
            .check_recorded_context(current.approval)?;
        if let Some(old) = &saved.owner_profile_receipt {
            return if old == &receipt {
                Ok(())
            } else {
                Err(ProvisionApprovalError::Conflict)
            };
        }
        let mut next = self.clone();
        next.termination_v3
            .get_mut(subject)
            .ok_or_else(invalid)?
            .owner_profile_receipt = Some(receipt);
        next.validate_termination_v3_persistence()?;
        *self = next;
        Ok(())
    }

    pub fn revoke_owner_termination_profile_v3(
        &mut self,
        subject: &str,
        now: UnixMillis,
    ) -> Result<()> {
        let saved = self.termination_v3.get(subject).ok_or_else(invalid)?;
        if saved.installed_at.is_none() {
            return Err(invalid());
        }
        if saved.profile_revoked_at.is_some() {
            return Ok(());
        }
        let mut next = self.clone();
        let saved = next.termination_v3.get_mut(subject).ok_or_else(invalid)?;
        saved.profile_revoked_at = Some(now);
        saved.version_anchor = saved
            .version_anchor
            .0
            .checked_add(1)
            .map(DecimalU64)
            .ok_or_else(invalid)?;
        next.validate_termination_v3_persistence()?;
        *self = next;
        Ok(())
    }
}
