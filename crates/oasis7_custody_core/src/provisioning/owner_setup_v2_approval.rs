//! V2 fresh-MFA approval persistence. No setup replay or model dispatch rights.
use super::*;
#[path = "owner_setup_v2_phase.rs"]
mod phase;
#[path = "owner_setup_v2_prepared.rs"]
mod prepared;
use phase::OwnerSetupSdkPhaseProjectionV2;
pub use phase::derive_owner_setup_move0_sdk_plan_v2;
pub use prepared::*;
fn invalid() -> ProvisionApprovalError {
    ProvisionApprovalError::BindingMismatch
}
fn digest<T: Serialize>(value: &T) -> Result<String> {
    crate::provisioning::types::digest(value).map_err(Into::into)
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OwnerSetupApprovalV2 {
    pub(super) plan: OwnerSetupPlanV2,
    pub(super) book: ApprovalAdmissionBook,
    #[serde(deserialize_with = "required_transport")]
    pub(super) transport: Option<crate::budget::BudgetLedger>,
    #[serde(deserialize_with = "required_phase")]
    phase: Option<OwnerSetupSdkPhaseProjectionV2>,
}
fn required_transport<'de, D>(
    d: D,
) -> core::result::Result<Option<crate::budget::BudgetLedger>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    Option::<crate::budget::BudgetLedger>::deserialize(d)
}
fn required_phase<'de, D>(
    d: D,
) -> core::result::Result<Option<OwnerSetupSdkPhaseProjectionV2>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    Option::<OwnerSetupSdkPhaseProjectionV2>::deserialize(d)
}
pub struct CommitOwnerSetupV2 {
    pub id: String,
    pub signed: SignedApprovalVerificationV1,
    pub source_nonce: String,
}
impl AccountProvisionApprovalState {
    pub fn owner_setup_v2_plan(&self) -> Option<&OwnerSetupPlanV2> {
        self.owner_setup_v2.as_ref().map(|v| &v.plan)
    }
    pub fn owner_setup_v2_original(&self, id: &str) -> Option<&ApprovalAdmissionRecord> {
        self.owner_setup_v2.as_ref()?.book.original(id)
    }
    fn owner_setup_v2_scope(
        &self,
        input: &ApprovalAdmissionInput,
        plan: &OwnerSetupPlanV2,
        current: &ProvisionApprovalContext<'_>,
        now: u64,
    ) -> Result<()> {
        plan.validate().map_err(|_| invalid())?;
        input.request.validate(now).map_err(|_| invalid())?;
        let c = current.approval;
        let participant = self
            .participant_provisions
            .values()
            .find(|v| v.result.is_some())
            .ok_or_else(invalid)?;
        let result = participant.result.as_ref().ok_or_else(invalid)?;
        if !self.owner_setups.is_empty()
            || plan.model_budget.as_ref().is_some_and(|b| {
                b.native_account_id != participant.descriptor.native_binding.native_account_id
            })
            || input.operation_id != plan.setup_id
            || self.frozen()
            || !matches!(
                self.provision.stage(),
                ProvisionStage::ParticipantBound
                    | ProvisionStage::ReadyForSetup
                    | ProvisionStage::Prepared
            )
            || self.provision.security_epoch().0 == 0
            || input.request.context != c.authority
            || c.authority.security_version != self.provision.security_epoch()
            || c.frozen
            || c.fence.check_epoch(c.authority.service_epoch.0).is_err()
            || c.authority.environment != self.draft.entry.environment
            || c.authority.custody_account_id != self.draft.entry.custody_account_id
            || c.identity_namespace != self.draft.entry.manager.identity_namespace
            || c.source.manager_account_id != self.draft.entry.manager.native_account_id
            || current.policy.version != self.draft.entry.bootstrap_config_version.0
            || !current.policy.managers.iter().any(|v| {
                v.principal == self.draft.entry.manager
                    && v.environments.contains(&self.draft.entry.environment)
            })
            || current.native_identity_namespace
                != participant.descriptor.native_binding.identity_namespace
            || plan.provision_id != self.draft.entry.provision_id
            || plan.world_identity != self.draft.world_identity
            || plan.bootstrap_config_version != self.draft.entry.bootstrap_config_version
            || plan.participant_receipt_digest != digest(result)?
            || plan.owner_key_ref != participant.descriptor.owner_key.key_id
            || plan.executor_key_ref
                != self
                    .provision
                    .role_key(Role::Executor)
                    .ok_or_else(invalid)?
                    .key_id
            || plan.executor_public_key
                != self
                    .provision
                    .role_key(Role::Executor)
                    .ok_or_else(invalid)?
                    .public_key
            || input.request.intent != plan.intent().map_err(|_| invalid())?
        {
            return Err(invalid());
        }
        Ok(())
    }

    pub fn begin_owner_setup_v2_checks(
        &self,
        input: &ApprovalAdmissionInput,
        plan: &OwnerSetupPlanV2,
        nonce: &str,
        current: &ProvisionApprovalContext<'_>,
        now: u64,
    ) -> Result<ExactSignatureChecks> {
        self.check_owner_setup_proof_unique(input)?;
        self.owner_setup_v2_scope(input, plan, current, now)?;
        let empty = ApprovalAdmissionBook::default();
        if self
            .owner_setup_v2
            .as_ref()
            .is_some_and(|v| &v.plan != plan)
        {
            return Err(ProvisionApprovalError::Conflict);
        }
        Ok(self
            .owner_setup_v2
            .as_ref()
            .map_or(&empty, |v| &v.book)
            .begin_checks(input, nonce, current.approval, now)?)
    }
    pub fn commit_owner_setup_v2_checks(
        &self,
        id: &str,
        signed: &SignedApprovalVerificationV1,
        current: &ProvisionApprovalContext<'_>,
        now: u64,
    ) -> Result<ExactSignatureChecks> {
        let v = self.owner_setup_v2.as_ref().ok_or_else(invalid)?;
        let r = v.book.original(id).ok_or_else(invalid)?;
        self.owner_setup_v2_scope(r.input(), &v.plan, current, now)?;
        Ok(v.book.commit_checks(id, signed, current.approval, now)?)
    }
    pub fn owner_setup_v2_progress(&self, id: &str) -> Result<OwnerSetupApprovalProgressV2> {
        let v = self.owner_setup_v2.as_ref().ok_or_else(invalid)?;
        let r = v.book.original(id).ok_or_else(invalid)?;
        Ok(OwnerSetupApprovalProgressV2 {
            api_version: 2,
            custody_account_id: self.draft.entry.custody_account_id.clone(),
            provision_id: self.draft.entry.provision_id.clone(),
            setup_id: v.plan.setup_id.clone(),
            phase: if r.stage() == ApprovalAdmissionStage::Consumed {
                crate::approval::ApprovalPhaseV1::Completed
            } else {
                crate::approval::ApprovalPhaseV1::Pending
            },
            model_budget_digest: v
                .plan
                .model_budget
                .as_ref()
                .map(|b| b.digest(v.plan.deadline.0))
                .transpose()
                .map_err(|_| invalid())?,
        })
    }
    pub fn begin_owner_setup_v2(
        &mut self,
        input: ApprovalAdmissionInput,
        plan: OwnerSetupPlanV2,
        nonce: String,
        current: &ProvisionApprovalContext<'_>,
        now: u64,
        verifier: &mut impl ApprovalSignatureVerifier,
    ) -> Result<ApprovalAdmissionRecord> {
        verifier.verify(&self.begin_owner_setup_v2_checks(&input, &plan, &nonce, current, now)?)?;
        let mut next = self.clone();
        let v = next
            .owner_setup_v2
            .get_or_insert_with(|| OwnerSetupApprovalV2 {
                plan: plan.clone(),
                book: ApprovalAdmissionBook::default(),
                transport: None,
                phase: None,
            });
        if v.plan != plan {
            return Err(ProvisionApprovalError::Conflict);
        }
        let record = v
            .book
            .begin(input, nonce, current.approval, now, verifier)?;
        next.validate_persistence()?;
        *self = next;
        Ok(record)
    }
    pub fn commit_owner_setup_v2(
        &mut self,
        commit: CommitOwnerSetupV2,
        native_scope: &NativeStageScope<'_>,
        current: &ProvisionApprovalContext<'_>,
        now: u64,
        verifier: &mut impl ApprovalSignatureVerifier,
    ) -> Result<ApprovalAdmissionRecord> {
        let CommitOwnerSetupV2 {
            id,
            signed,
            source_nonce,
        } = commit;
        let id = id.as_str();
        verifier.verify(&self.commit_owner_setup_v2_checks(id, &signed, current, now)?)?;
        if source_nonce.len() != 64
            || !source_nonce
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        {
            return Err(invalid());
        }
        let (session, status) = self.participant_session(native_scope)?;
        let participant = self
            .participant_provisions
            .values()
            .find(|p| p.result.is_some())
            .ok_or_else(invalid)?;
        if status.status.receipt != participant.descriptor.native_binding {
            return Err(invalid());
        }
        let mut next = self.clone();
        let record = next
            .owner_setup_v2
            .as_mut()
            .ok_or_else(invalid)?
            .book
            .commit(id, signed, current.approval, now, verifier)?;
        let v = next.owner_setup_v2.as_mut().ok_or_else(invalid)?;
        if v.transport.is_none() {
            // Same actual approval observe/verify/observe path as V1; these are
            // setup approval transports, not inference/Execution graph credit.
            let mut transport = crate::budget::BudgetLedger::new(v.plan.transport_limit.0);
            transport.charge(8).map_err(|_| invalid())?;
            v.transport = Some(transport);
        }
        if v.phase.is_none() && v.plan.selection.is_exact_move0() {
            v.phase = Some(OwnerSetupSdkPhaseProjectionV2::from_consumed(
                &v.plan,
                &v.book,
                v.transport.as_ref().ok_or_else(invalid)?,
                session,
                source_nonce,
                current,
                now,
            )?);
        }
        next.validate_persistence()?;
        *self = next;
        Ok(record)
    }
    pub(super) fn validate_owner_setup_v2_persistence(&self) -> Result<()> {
        let Some(v) = &self.owner_setup_v2 else {
            return Ok(());
        };
        v.plan.validate().map_err(|_| invalid())?;
        let r = v.book.original(&v.plan.setup_id).ok_or_else(invalid)?;
        self.check_owner_setup_proof_unique(r.input())?;
        let expected_transport = if r.stage() == ApprovalAdmissionStage::Consumed {
            let mut ledger = crate::budget::BudgetLedger::new(v.plan.transport_limit.0);
            ledger.charge(8).map_err(|_| invalid())?;
            Some(ledger)
        } else {
            None
        };
        if v.transport != expected_transport {
            return Err(invalid());
        }
        match (
            &v.phase,
            r.stage() == ApprovalAdmissionStage::Consumed,
            v.plan.selection.is_exact_move0(),
        ) {
            (Some(phase), true, true) => phase.validate_header(&v.plan, &v.book)?,
            (None, false, _) | (None, true, false) => {}
            _ => return Err(invalid()),
        }
        if !self.owner_setups.is_empty()
            || v.book.records().count() != 1
            || r.input().request.intent != v.plan.intent().map_err(|_| invalid())?
            || r.input().request.context.custody_account_id != self.draft.entry.custody_account_id
            || r.input().request.context.environment != self.draft.entry.environment
            || v.plan.provision_id != self.draft.entry.provision_id
            || v.plan.world_identity != self.draft.world_identity
            || v.plan.bootstrap_config_version != self.draft.entry.bootstrap_config_version
            || self
                .provision
                .role_key(Role::Owner)
                .is_none_or(|k| k.key_id != v.plan.owner_key_ref)
            || self.provision.role_key(Role::Executor).is_none_or(|k| {
                k.key_id != v.plan.executor_key_ref || k.public_key != v.plan.executor_public_key
            })
            || self
                .participant_provisions
                .values()
                .find(|p| p.result.is_some())
                .and_then(|p| p.result.as_ref())
                .is_none_or(|result| {
                    digest(result).ok().as_ref() != Some(&v.plan.participant_receipt_digest)
                })
        {
            return Err(invalid());
        }
        Ok(())
    }
}
