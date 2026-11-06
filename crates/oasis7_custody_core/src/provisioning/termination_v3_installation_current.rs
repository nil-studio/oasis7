//! Account-local current installation evidence. No new transport and no ready
//! flag. This is only one input to the actual action/unknown-journal gate.
use super::*;
/// Owned, private, non-serializable proof of installed authority under this
/// transaction's exact already verified Current. It does not prove Vault uptime,
/// World current, action eligibility or whole-graph budget sufficiency.
pub struct TerminationInstallationCurrentV3 {
    subject: String,
    plan_digest: String,
    version_anchor: DecimalU64,
    profile_digest: String,
    source_binding_digest: String,
    checked_at: UnixMillis,
    remaining_transport: DecimalU64,
    development_only: bool,
}
impl TerminationInstallationCurrentV3 {
    pub fn subject(&self) -> &str {
        &self.subject
    }
    pub fn plan_digest(&self) -> &str {
        &self.plan_digest
    }
    pub fn version_anchor(&self) -> DecimalU64 {
        self.version_anchor
    }
    pub fn profile_digest(&self) -> &str {
        &self.profile_digest
    }
    pub fn source_binding_digest(&self) -> &str {
        &self.source_binding_digest
    }
    pub fn checked_at(&self) -> UnixMillis {
        self.checked_at
    }
    pub fn remaining_transport(&self) -> DecimalU64 {
        self.remaining_transport
    }
    pub fn development_only(&self) -> bool {
        self.development_only
    }
}
/// The caller is the fixed Rust Account grant adapter. No HTTP field selects
/// this mode, and a measurement witness can never be ProductionReady.
pub enum TerminationCurrentProfileV3 {
    Production,
    DevelopmentMeasuredFixture,
}
impl AccountProvisionApprovalState {
    pub fn termination_installation_current_v3(
        &self,
        subject: &str,
        purpose: SessionPurposeV1,
        current: &TestSessionLiveCurrent<'_, '_>,
        now: UnixMillis,
        profile: TerminationCurrentProfileV3,
        verifier: &mut impl ApprovalSignatureVerifier,
    ) -> Result<TerminationInstallationCurrentV3> {
        if !matches!(
            purpose,
            SessionPurposeV1::Execution | SessionPurposeV1::ReconcileRead
        ) {
            return Err(invalid());
        }
        let funding = match purpose {
            SessionPurposeV1::Execution => SessionSourceFundingV1::Execution,
            SessionPurposeV1::ReconcileRead => SessionSourceFundingV1::ReconcileRead,
            _ => return Err(invalid()),
        };
        self.require_session_source_funding(subject, current.source_operation_id, funding)?;
        // This recomputes the exact jobs and uses the already-verified cache;
        // no observer/RPC is introduced and no transport is paid by anotherpool.
        let checks = self.test_session_live_checks(subject, purpose, current, now)?;
        verifier.verify(&checks)?;
        let a = self.termination_v3.get(subject).ok_or_else(invalid)?;
        self.termination_v3_subject(&a.plan, now)?;
        if a.installed_at.is_none() || a.profile_revoked_at.is_some() || a.version_anchor.0 != 1 {
            return Err(invalid());
        }
        let record = a.book.original(&a.input.operation_id).ok_or_else(invalid)?;
        if record.stage() != ApprovalAdmissionStage::Consumed {
            return Err(invalid());
        }
        // Exact same manager/passkey/session/source versions or failclosed.
        // A proof for another source is not refreshed behind the caller's back.
        record.check_recorded_context(current.provision.approval)?;
        if NativeSubjectRefV3::from_status(&current.lifetime.lifetime.status)
            .map_err(|_| invalid())?
            != a.plan.native_subject
        {
            return Err(invalid());
        }

        let expected = self.owner_termination_profile_v3(subject, now)?;
        let receipt = a.owner_profile_receipt.as_ref().ok_or_else(invalid)?;
        receipt.verify()?;
        if receipt.profile != expected {
            return Err(invalid());
        }
        let binding = a
            .plan
            .initial_fence
            .executor_binding
            .as_ref()
            .ok_or_else(invalid)?;
        let mut executor = self.role_key_results.values().filter(|r| {
            r.scope.context.identity.role == crate::http::Role::Executor
                && r.scope.provision_id == a.plan.provision_id
                && r.scope.context.identity.account_id == a.plan.custody_account_id
                && r.public_ref.key_id == a.plan.executor_key_ref
                && r.public_ref.public_key == binding.executor_public_key
                && self.role_key_tasks.values().any(|task| {
                    task.state == ProvisionRoleKeyTaskStateV1::Pending
                        && crate::role_key_slot::RoleKeySlotScopeV1::from_task(task).as_ref()
                            == Ok(&r.scope)
                })
        });
        if executor.next().is_none() || executor.next().is_some() {
            return Err(invalid());
        }
        match profile {
            TerminationCurrentProfileV3::Production
                if !a.plan.transport_policy.production_supported() =>
            {
                return Err(invalid());
            }
            _ => (),
        }
        Ok(TerminationInstallationCurrentV3 {
            subject: subject.into(),
            plan_digest: a.plan.digest().map_err(|_| invalid())?,
            version_anchor: a.version_anchor,
            profile_digest: expected.digest()?,
            source_binding_digest: checks.binding_digest()?,
            checked_at: now,
            remaining_transport: DecimalU64(a.transport.remaining()?),
            development_only: !a.plan.transport_policy.production_supported(),
        })
    }
}
