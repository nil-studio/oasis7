//! Supplemental approval selection in the same Account transaction as V1 allocation.
//! This module performs no network I/O and never treats Native ACK as installation.
use super::*;
use crate::time_units::UnixMillis;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TestSessionTerminationAuthorityV3 {
    plan: TerminationPlanV3,
    input: ApprovalAdmissionInput,
    book: ApprovalAdmissionBook,
    installed_at: Option<UnixMillis>,
    installation_native_status: Option<crate::native_binding::SignedManagedNativeSessionStatusV1>,
    version_anchor: DecimalU64,
    profile_revoked_at: Option<UnixMillis>,
    transport: TerminationTransportBookV3,
    sources: BTreeMap<String, ProvisionOriginInspectionV1>,
    owner_profile_receipt: Option<OwnerTerminationProfileReceiptV3>,
}
/// Created only by Account's current signing-grant transaction. Never HTTP JSON.
/// No production instance can be created until a measured policy and Owner
/// readiness producer exist. In particular a development approval is insufficient.
pub struct TerminationExecutionReadyV3 {
    _subject_session_id: String,
    _plan_digest: String,
    _profile_revision: DecimalU64,
}
/// Distinct from production readiness; only the trusted measurement dispatcher
/// may consume this claim after actual Owner profile installation is implemented.
pub struct TerminationInstalledForMeasurementV3 {
    _subject_session_id: String,
    _plan_digest: String,
    _profile_revision: DecimalU64,
    _explicit_transport_cap: DecimalU64,
}
#[derive(Clone, Copy)]
enum TerminationInstallProfileV3 {
    Production,
    #[cfg(test)]
    DevelopmentOnlyFixture,
}
impl AccountProvisionApprovalState {
    fn termination_v3_subject(&self, plan: &TerminationPlanV3, now: UnixMillis) -> Result<()> {
        plan.validate_at(now.get()).map_err(|_| invalid())?;
        self.require_termination_native_subject_v3(plan)?;
        let progress = self.test_session_progress(&plan.subject_session_id, now)?;
        if !matches!(
            progress.stage,
            TestSessionPhaseV1::Active | TestSessionPhaseV1::Closing
        ) {
            return Err(invalid());
        }
        self.termination_v3_subject_binding(plan)
    }
    fn termination_v3_subject_binding(&self, plan: &TerminationPlanV3) -> Result<()> {
        self.require_termination_native_subject_v3(plan)?;
        let s = self
            .test_sessions
            .get(&plan.subject_session_id)
            .ok_or_else(invalid)?;
        let legacy = &s.termination;
        if !s.termination_slots.is_empty()
            || s.consumed_at.is_none()
            || s.input.request.intent_digest().map_err(|_| invalid())?
                != plan.subject_v1_approval_digest
            || legacy.digest()? != plan.subject_v1_termination_plan_digest
            || plan.custody_account_id != legacy.custody_account_id
            || plan.provision_id != legacy.provision_id
            || plan.setup_id != legacy.setup_id
            || plan.prepared_result_digest != legacy.prepared_result_digest
            || plan.world != legacy.world
            || plan.owner_key_ref != legacy.owner_key_ref
            || plan.executor_key_ref != legacy.executor_key_ref
            || plan.initial_fence != legacy.initial_fence
            || plan.cleanup_until.get() > legacy.cleanup_until.get()
            || plan.original_delegation.world_id != legacy.original_delegation.world_id
            || plan.original_delegation.branch_id != legacy.original_delegation.branch_id
            || plan.original_delegation.agent_id != legacy.original_delegation.agent_id
            || plan.original_delegation.owner_binding != legacy.original_delegation.owner_binding
            || plan.original_delegation.agent_identity_generation.0
                != legacy.original_delegation.agent_identity_generation
            || plan.original_delegation.generation.0 != legacy.original_delegation.generation
            || plan.original_delegation.nonce.0 != legacy.original_delegation.nonce
            || plan.original_delegation.delegate_public_key
                != legacy.original_delegation.delegate_public_key
            || plan.original_delegation.revoked != legacy.original_delegation.revoked
        {
            return Err(invalid());
        }
        Ok(())
    }
    pub fn begin_termination_v3_install(
        &mut self,
        input: ApprovalAdmissionInput,
        verify_nonce: String,
        current: &ProvisionApprovalContext<'_>,
        now: UnixMillis,
        verifier: &mut impl ApprovalSignatureVerifier,
    ) -> Result<ApprovalAdmissionRecord> {
        self.begin_termination_v3_install_profile(
            input,
            verify_nonce,
            current,
            now,
            verifier,
            TerminationInstallProfileV3::Production,
        )
    }
    #[cfg(test)]
    pub fn begin_termination_v3_install_fixture(
        &mut self,
        input: ApprovalAdmissionInput,
        verify_nonce: String,
        current: &ProvisionApprovalContext<'_>,
        now: UnixMillis,
        verifier: &mut impl ApprovalSignatureVerifier,
    ) -> Result<ApprovalAdmissionRecord> {
        self.begin_termination_v3_install_profile(
            input,
            verify_nonce,
            current,
            now,
            verifier,
            TerminationInstallProfileV3::DevelopmentOnlyFixture,
        )
    }
    fn begin_termination_v3_install_profile(
        &mut self,
        input: ApprovalAdmissionInput,
        verify_nonce: String,
        current: &ProvisionApprovalContext<'_>,
        now: UnixMillis,
        verifier: &mut impl ApprovalSignatureVerifier,
        profile: TerminationInstallProfileV3,
    ) -> Result<ApprovalAdmissionRecord> {
        let plan = match &input.request.intent {
            crate::approval::CustodyIntentV1::TerminationPlanV3 { plan } => plan.as_ref().clone(),
            _ => return Err(invalid()),
        };
        match profile {
            TerminationInstallProfileV3::Production
                if !plan.transport_policy.production_supported() =>
            {
                return Err(invalid());
            }
            _ => (),
        }
        self.termination_v3_subject(&plan, now)?;
        // Native verifies its own subject; Account independently binds that subject
        // to its current authority. No caller-supplied verified/current flag exists.
        if input.request.context != current.approval.authority {
            return Err(invalid());
        }
        if self
            .test_sessions
            .values()
            .any(|s| s.input.approval.proof.nonce == input.approval.proof.nonce)
            || self.termination_v3.iter().any(|(id, a)| {
                id != &plan.subject_session_id
                    && a.input.approval.proof.nonce == input.approval.proof.nonce
            })
        {
            return Err(ProvisionApprovalError::Conflict);
        }
        let mut next = self.clone();
        let saved = next
            .termination_v3
            .entry(plan.subject_session_id.clone())
            .or_insert_with(|| TestSessionTerminationAuthorityV3 {
                plan: plan.clone(),
                input: input.clone(),
                book: ApprovalAdmissionBook::default(),
                installed_at: None,
                installation_native_status: None,
                version_anchor: DecimalU64(1),
                profile_revoked_at: None,
                transport: TerminationTransportBookV3::new(plan.transport_limit),
                sources: BTreeMap::new(),
                owner_profile_receipt: None,
            });
        if saved.plan != plan || saved.input != input {
            return Err(ProvisionApprovalError::Conflict);
        }
        let record =
            saved
                .book
                .begin(input, verify_nonce, current.approval, now.get(), verifier)?;
        next.validate_termination_v3_persistence()?;
        *self = next;
        Ok(record)
    }
    /// Domain-separated nonce for the real Native status request before consume.
    pub fn termination_install_native_nonce_v3(&self, subject: &str) -> Result<String> {
        let saved = self.termination_v3.get(subject).ok_or_else(invalid)?;
        let original = saved
            .book
            .original(&saved.input.operation_id)
            .ok_or_else(invalid)?;
        digest(&(
            "oasis7.custody.termination-install-native-current.v3",
            &saved.input.operation_id,
            original.verify_nonce(),
        ))
    }
    pub fn commit_termination_v3_install(
        &mut self,
        subject: &str,
        verification: crate::approval::SignedApprovalVerificationV1,
        current: &ProvisionApprovalContext<'_>,
        native: &TerminationInstallNativeCurrentV3<'_, '_>,
        now: UnixMillis,
        verifier: &mut impl ApprovalSignatureVerifier,
    ) -> Result<ApprovalAdmissionRecord> {
        self.commit_termination_v3_install_profile(
            subject,
            verification,
            current,
            native,
            now,
            verifier,
            TerminationInstallProfileV3::Production,
        )
    }
    #[cfg(test)]
    pub fn commit_termination_v3_install_fixture(
        &mut self,
        subject: &str,
        verification: crate::approval::SignedApprovalVerificationV1,
        current: &ProvisionApprovalContext<'_>,
        native: &TerminationInstallNativeCurrentV3<'_, '_>,
        now: UnixMillis,
        verifier: &mut impl ApprovalSignatureVerifier,
    ) -> Result<ApprovalAdmissionRecord> {
        self.commit_termination_v3_install_profile(
            subject,
            verification,
            current,
            native,
            now,
            verifier,
            TerminationInstallProfileV3::DevelopmentOnlyFixture,
        )
    }
    fn commit_termination_v3_install_profile(
        &mut self,
        subject: &str,
        verification: crate::approval::SignedApprovalVerificationV1,
        current: &ProvisionApprovalContext<'_>,
        native: &TerminationInstallNativeCurrentV3<'_, '_>,
        now: UnixMillis,
        verifier: &mut impl ApprovalSignatureVerifier,
        profile: TerminationInstallProfileV3,
    ) -> Result<ApprovalAdmissionRecord> {
        let saved = self.termination_v3.get(subject).ok_or_else(invalid)?;
        match profile {
            TerminationInstallProfileV3::Production
                if !saved.plan.transport_policy.production_supported() =>
            {
                return Err(invalid());
            }
            _ => (),
        }
        self.termination_v3_subject(&saved.plan, now)?;
        let original = saved
            .book
            .original(&saved.input.operation_id)
            .ok_or_else(invalid)?;
        let expected_nonce = digest(&(
            "oasis7.custody.termination-install-native-current.v3",
            &saved.input.operation_id,
            original.verify_nonce(),
        ))?;
        let checks = saved.book.commit_checks(
            &saved.input.operation_id,
            &verification,
            current.approval,
            now.get(),
        )?;
        let checks = self.termination_native_consume_checks_v3(
            &saved.plan,
            native.status,
            native.private_sid_generation,
            &expected_nonce,
            native.scope,
            current,
            now,
            checks,
        )?;
        verifier.verify(&checks)?;
        let mut next = self.clone();
        let saved = next.termination_v3.get_mut(subject).ok_or_else(invalid)?;
        let record = saved.book.commit(
            &saved.input.operation_id,
            verification,
            current.approval,
            now.get(),
            verifier,
        )?;
        if saved.installed_at.is_none() {
            saved.installed_at = Some(now);
            saved.installation_native_status = Some(native.status.clone());
        }
        // Same SQL callback writes this selection. V1 allocation checks this map
        // in that callback, so an intervening V1 allocation makes commit fail above.
        next.validate_termination_v3_persistence()?;
        *self = next;
        Ok(record)
    }
    pub(in crate::provisioning) fn validate_termination_v3_persistence(&self) -> Result<()> {
        if self.termination_v3.len() > self.test_sessions.len() {
            return Err(invalid());
        }
        let mut proof_nonces = std::collections::BTreeSet::new();
        for (subject, saved) in &self.termination_v3 {
            saved.plan.validate_at(0).map_err(|_| invalid())?;
            self.termination_v3_subject_binding(&saved.plan)?;
            self.validate_installation_native_restore_v3(saved)?;
            saved.transport.validate(&saved.plan)?;
            if saved.version_anchor.0
                != if saved.profile_revoked_at.is_some() {
                    2
                } else {
                    1
                }
                || saved.profile_revoked_at.is_some_and(|at| {
                    saved
                        .installed_at
                        .is_none_or(|installed| at.get() < installed.get())
                })
            {
                return Err(invalid());
            }

            self.validate_termination_sources_v3(subject)?;
            let s = self.test_sessions.get(subject).ok_or_else(invalid)?;
            if let Some(receipt) = &saved.owner_profile_receipt {
                receipt.verify()?;
                if saved.installed_at.is_none()
                    || receipt.profile.plan != saved.plan
                    || !self.role_key_results.values().any(|r| {
                        r.scope == receipt.profile.slot
                            && r.public_ref.public_key == receipt.subject_public_key
                    })
                {
                    return Err(invalid());
                }
            }

            let record = saved
                .book
                .original(&saved.input.operation_id)
                .ok_or_else(invalid)?;
            if subject != &saved.plan.subject_session_id
                || (saved.installed_at.is_some() && !s.termination_slots.is_empty())
                || record.input() != &saved.input
                || saved.book.records().count() != 1
                || saved.installed_at.map(|t| t.get()) != record.consumed_at()
                || saved.plan.subject_v1_approval_digest
                    != s.input.request.intent_digest().map_err(|_| invalid())?
                || saved.plan.subject_v1_termination_plan_digest != s.termination.digest()?
                || saved.plan.cleanup_until.get() > s.termination.cleanup_until.get()
                || !proof_nonces.insert(&saved.input.approval.proof.nonce)
                || self
                    .test_sessions
                    .values()
                    .any(|old| old.input.approval.proof.nonce == saved.input.approval.proof.nonce)
                || !matches!(&saved.input.request.intent,
                    crate::approval::CustodyIntentV1::TerminationPlanV3 { plan } if plan.as_ref() == &saved.plan)
            {
                return Err(invalid());
            }
        }
        Ok(())
    }
    pub(super) fn require_legacy_termination_selected(&self, subject: &str) -> Result<()> {
        if self
            .termination_v3
            .get(subject)
            .is_some_and(|a| a.installed_at.is_some())
        {
            return Err(ProvisionApprovalError::Conflict);
        }
        Ok(())
    }
    pub fn require_termination_v3_execution_ready(
        &self,
        subject: &str,
        now: UnixMillis,
    ) -> Result<TerminationExecutionReadyV3> {
        let saved = self.termination_v3.get(subject).ok_or_else(invalid)?;
        self.termination_v3_subject(&saved.plan, now)?;
        if saved.installed_at.is_none()
            || saved.owner_profile_receipt.is_none()
            || !saved.plan.transport_policy.production_supported()
        {
            return Err(invalid());
        }
        // A formal policy, actual bounded Owner profile, all-HWM reconciliation,
        // and full worst-case remaining-ledger check are deliberately mandatory.
        // No such production producer exists yet; do not substitute a boolean.
        Err(invalid())
    }
}

#[path = "termination_v3_transport.rs"]
mod transport;
pub use transport::*;
#[path = "termination_v3_source.rs"]
mod origin;

#[path = "termination_v3_profile_anchor.rs"]
mod profile_anchor;

#[path = "termination_v3_installation_current.rs"]
mod installation_current;
pub use installation_current::*;


#[path = "termination_v3_native_subject_consume.rs"]
mod native_subject_consume;

pub use native_subject_consume::TerminationInstallNativeCurrentV3;
