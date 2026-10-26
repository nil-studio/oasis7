//! Atomic approval/plan state. Role tasks are inert pending work, never key permits.
use super::*;
use crate::{
    DecimalU64,
    approval::{ApprovedWorldV1, CustodyIntentV1, SignedApprovalVerificationV1},
    approval_admission::*,
    http::Role,
};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProvisionApprovalError {
    Admission(ApprovalAdmissionError),
    Provision(ProvisionError),
    BindingMismatch,
    Conflict,
    InvalidPersistence,
}
impl core::fmt::Display for ProvisionApprovalError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "invalid provision approval: {self:?}")
    }
}
impl From<ApprovalAdmissionError> for ProvisionApprovalError {
    fn from(error: ApprovalAdmissionError) -> Self {
        Self::Admission(error)
    }
}
impl From<ProvisionError> for ProvisionApprovalError {
    fn from(error: ProvisionError) -> Self {
        Self::Provision(error)
    }
}
type Result<T> = core::result::Result<T, ProvisionApprovalError>;

/// Transient authority from fixed configuration, current source verification and
/// local directory/fence state. This type is deliberately not deserializable.
pub struct ProvisionApprovalContext<'a> {
    pub approval: &'a ApprovalAdmissionContext,
    pub policy: ProvisionPolicy<'a>,
    pub world_identity: &'a ApprovedWorldV1,
    pub native_identity_namespace: &'a str,
}
#[cfg_attr(test, derive(schemars::JsonSchema))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProvisionRoleKeyTaskStateV1 {
    Pending,
    Cancelled,
}
#[cfg_attr(test, derive(schemars::JsonSchema))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProvisionRoleKeyAlgorithmV1 {
    Ed25519,
}
#[cfg_attr(test, derive(schemars::JsonSchema))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProvisionRoleKeyTaskV1 {
    pub operation_id: String,
    pub provision_id: String,
    pub environment: String,
    pub custody_account_id: String,
    pub role: Role,
    pub key_id: String,
    pub key_version: DecimalU64,
    pub algorithm: ProvisionRoleKeyAlgorithmV1,
    pub state: ProvisionRoleKeyTaskStateV1,
}

/// A single persisted value is committed with its audit by the storage adapter.
/// Restoring it checks cross-record consistency, but does not grant dispatch.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "RawAccountProvisionApprovalState")]
pub struct AccountProvisionApprovalState {
    draft: AccountDraftV1,
    book: ApprovalAdmissionBook,
    provision: ProvisionRecord,
    accepted_operation_id: Option<String>,
    role_key_tasks: BTreeMap<String, ProvisionRoleKeyTaskV1>,
    cancellation_operation_id: Option<String>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawAccountProvisionApprovalState {
    draft: AccountDraftV1,
    book: ApprovalAdmissionBook,
    provision: ProvisionRecord,
    accepted_operation_id: Option<String>,
    role_key_tasks: BTreeMap<String, ProvisionRoleKeyTaskV1>,
    cancellation_operation_id: Option<String>,
}
impl TryFrom<RawAccountProvisionApprovalState> for AccountProvisionApprovalState {
    type Error = ProvisionApprovalError;
    fn try_from(raw: RawAccountProvisionApprovalState) -> Result<Self> {
        let state = Self {
            draft: raw.draft,
            book: raw.book,
            provision: raw.provision,
            accepted_operation_id: raw.accepted_operation_id,
            role_key_tasks: raw.role_key_tasks,
            cancellation_operation_id: raw.cancellation_operation_id,
        };
        state.validate_persistence()?;
        Ok(state)
    }
}
impl AccountProvisionApprovalState {
    pub fn new(draft: AccountDraftV1) -> Result<Self> {
        validate_draft(&draft)?;
        let provision = ProvisionRecord::draft(draft.entry.clone(), draft.plan.clone())?;
        Ok(Self {
            draft,
            provision,
            book: ApprovalAdmissionBook::default(),
            accepted_operation_id: None,
            role_key_tasks: BTreeMap::new(),
            cancellation_operation_id: None,
        })
    }
    pub fn draft(&self) -> &AccountDraftV1 {
        &self.draft
    }
    pub fn provision(&self) -> &ProvisionRecord {
        &self.provision
    }
    pub fn original(&self, id: &str) -> Option<&ApprovalAdmissionRecord> {
        self.book.original(id)
    }
    pub fn role_key_tasks(&self) -> &BTreeMap<String, ProvisionRoleKeyTaskV1> {
        &self.role_key_tasks
    }
    pub fn accepted_operation_id(&self) -> Option<&str> {
        self.accepted_operation_id.as_deref()
    }
    pub fn cancellation_operation_id(&self) -> Option<&str> {
        self.cancellation_operation_id.as_deref()
    }
    pub fn frozen(&self) -> bool {
        self.provision.frozen()
    }
    fn historical(&self, id: &str) -> bool {
        self.accepted_operation_id.as_deref() == Some(id)
    }
    fn historical_scope(&self, id: &str, current: &ProvisionApprovalContext<'_>) -> Result<()> {
        let original = self.original(id).ok_or(ApprovalAdmissionError::Missing)?;
        let proof = &original.input().approval.proof;
        let target = &original.input().request.context;
        if current.approval.identity_namespace != proof.identity_namespace
            || current.approval.identity_instance_epoch != proof.identity_instance_epoch
            || current.approval.authority.environment != target.environment
            || current.approval.authority.custody_account_id != target.custody_account_id
            || current.approval.authority.audience != target.audience
            || current.approval.authority.service_epoch != target.service_epoch
            || current.approval.source.manager_account_id
                != self.draft.entry.manager.native_account_id
        {
            return Err(ProvisionApprovalError::BindingMismatch);
        }
        Ok(())
    }
    fn current(&self, current: &ProvisionApprovalContext<'_>, now: u64) -> Result<()> {
        self.provision
            .check_current(current.approval.authority.security_version.0, now)?;
        if current.policy.version != self.draft.entry.bootstrap_config_version.0
            || current.approval.authority.environment != self.draft.entry.environment
            || current.approval.authority.custody_account_id != self.draft.entry.custody_account_id
            || current.approval.identity_namespace != self.draft.entry.manager.identity_namespace
            || current.approval.source.manager_account_id
                != self.draft.entry.manager.native_account_id
            || current.world_identity != &self.draft.world_identity
            || !current.policy.managers.iter().any(|manager| {
                manager.principal == self.draft.entry.manager
                    && manager.environments.contains(&self.draft.entry.environment)
            })
        {
            return Err(ProvisionApprovalError::BindingMismatch);
        }
        self.draft.plan.validate(now, current.policy.routes)?;
        Ok(())
    }
    pub fn begin_checks(
        &self,
        input: &ApprovalAdmissionInput,
        verify_nonce: &str,
        current: &ProvisionApprovalContext<'_>,
        now: u64,
    ) -> Result<ExactSignatureChecks> {
        binding(&self.draft, input)?;
        if self.historical(&input.operation_id) {
            self.historical_scope(&input.operation_id, current)?;
            if self
                .original(&input.operation_id)
                .is_none_or(|old| old.input() != input)
            {
                return Err(ProvisionApprovalError::Conflict);
            }
            return Ok(self.book.consumed_checks(&input.operation_id, None)?);
        }
        if self.accepted_operation_id.is_some() {
            return Err(ProvisionApprovalError::Conflict);
        }
        self.current(current, now)?;
        let CustodyIntentV1::Provision {
            native_identity_namespace,
            ..
        } = &input.request.intent
        else {
            return Err(ProvisionApprovalError::BindingMismatch);
        };
        if native_identity_namespace != current.native_identity_namespace {
            return Err(ProvisionApprovalError::BindingMismatch);
        }
        Ok(self
            .book
            .begin_checks(input, verify_nonce, current.approval, now)?)
    }
    pub fn begin(
        &mut self,
        input: ApprovalAdmissionInput,
        verify_nonce: String,
        current: &ProvisionApprovalContext<'_>,
        now: u64,
        verifier: &mut impl ApprovalSignatureVerifier,
    ) -> Result<ApprovalAdmissionRecord> {
        let checks = self.begin_checks(&input, &verify_nonce, current, now)?;
        if self.historical(&input.operation_id) {
            verifier.verify(&checks)?;
            return self
                .original(&input.operation_id)
                .cloned()
                .ok_or(ProvisionApprovalError::InvalidPersistence);
        }
        let mut candidate = self.clone();
        let record = candidate
            .book
            .begin(input, verify_nonce, current.approval, now, verifier)?;
        candidate.validate_persistence()?;
        *self = candidate;
        Ok(record)
    }
    pub fn commit_checks(
        &self,
        id: &str,
        verification: &SignedApprovalVerificationV1,
        current: &ProvisionApprovalContext<'_>,
        now: u64,
    ) -> Result<ExactSignatureChecks> {
        if self.historical(id) {
            self.historical_scope(id, current)?;
            return Ok(self.book.consumed_checks(id, Some(verification))?);
        }
        if self.accepted_operation_id.is_some() {
            return Err(ProvisionApprovalError::Conflict);
        }
        self.current(current, now)?;
        let original = self.original(id).ok_or(ApprovalAdmissionError::Missing)?;
        binding(&self.draft, original.input())?;
        let CustodyIntentV1::Provision {
            native_identity_namespace,
            ..
        } = &original.input().request.intent
        else {
            return Err(ProvisionApprovalError::BindingMismatch);
        };
        if native_identity_namespace != current.native_identity_namespace {
            return Err(ProvisionApprovalError::BindingMismatch);
        }
        Ok(self
            .book
            .commit_checks(id, verification, current.approval, now)?)
    }
    pub fn commit(
        &mut self,
        id: &str,
        verification: SignedApprovalVerificationV1,
        current: &ProvisionApprovalContext<'_>,
        now: u64,
        verifier: &mut impl ApprovalSignatureVerifier,
    ) -> Result<ApprovalAdmissionRecord> {
        let checks = self.commit_checks(id, &verification, current, now)?;
        if self.historical(id) {
            verifier.verify(&checks)?;
            return self
                .original(id)
                .cloned()
                .ok_or(ProvisionApprovalError::InvalidPersistence);
        }
        let mut candidate = self.clone();
        let record = candidate
            .book
            .commit(id, verification, current.approval, now, verifier)?;
        candidate.provision.approve(
            &candidate.draft.entry.plan_digest,
            &record.input().approval.proof.nonce,
            current.approval.authority.security_version.0,
            now,
        )?;
        candidate.accepted_operation_id = Some(id.into());
        candidate.role_key_tasks = tasks(&candidate.draft, false)?;
        candidate.validate_persistence()?;
        *self = candidate;
        Ok(record)
    }
    /// Cancels only unclaimed work. There is no dispatch/claim API in this slice.
    pub fn freeze(&mut self, epoch: u64) -> Result<()> {
        self.provision.freeze(epoch)?;
        for task in self.role_key_tasks.values_mut() {
            task.state = ProvisionRoleKeyTaskStateV1::Cancelled;
        }
        Ok(())
    }
    pub fn cancel(&mut self, operation_id: &str, epoch: u64, _now: u64) -> Result<()> {
        if !super::types::valid_id(operation_id) {
            return Err(ProvisionError::Invalid.into());
        }
        if let Some(original) = &self.cancellation_operation_id {
            return if original == operation_id {
                Ok(())
            } else {
                Err(ProvisionApprovalError::Conflict)
            };
        }
        self.freeze(epoch)?;
        self.cancellation_operation_id = Some(operation_id.into());
        Ok(())
    }
    fn validate_persistence(&self) -> Result<()> {
        validate_draft(&self.draft)?;
        for record in self.book.records() {
            binding(&self.draft, record.input())?;
        }
        let consumed = self
            .book
            .records()
            .filter(|r| r.stage() == ApprovalAdmissionStage::Consumed)
            .collect::<Vec<_>>();
        let nonce = if let Some(id) = &self.accepted_operation_id {
            if consumed.len() != 1 || consumed[0].input().operation_id != *id {
                return Err(ProvisionApprovalError::InvalidPersistence);
            }
            if self.role_key_tasks != tasks(&self.draft, self.provision.frozen())? {
                return Err(ProvisionApprovalError::InvalidPersistence);
            }
            Some(consumed[0].input().approval.proof.nonce.as_str())
        } else {
            if !consumed.is_empty() || !self.role_key_tasks.is_empty() {
                return Err(ProvisionApprovalError::InvalidPersistence);
            }
            None
        };
        if self
            .cancellation_operation_id
            .as_deref()
            .is_some_and(|id| !super::types::valid_id(id) || !self.provision.frozen())
        {
            return Err(ProvisionApprovalError::InvalidPersistence);
        }
        self.provision.validate_approval_only(&self.draft, nonce)?;
        Ok(())
    }
}
fn validate_draft(draft: &AccountDraftV1) -> Result<()> {
    if !draft.world_identity.valid()
        || draft.entry.environment != draft.plan.environment
        || draft.entry.plan_digest != draft.plan.digest()?
        || draft.plan.world_identity != draft.world_identity.world_id
        || draft.entry.bootstrap_config_version.0 == 0
        || !super::types::valid_id(&draft.entry.provision_id)
        || !super::types::valid_id(&draft.entry.custody_account_id)
        || !super::types::valid_id(&draft.entry.client_request_id)
        || draft.entry.manager.identity_namespace.trim().is_empty()
        || draft.entry.manager.native_account_id.trim().is_empty()
        || draft.plan.expires_at.0 == 0
    {
        return Err(ProvisionApprovalError::BindingMismatch);
    }
    let routes = ProvisionRoutes {
        environment: draft.plan.environment.clone(),
        identity_route: draft.plan.identity_route.clone(),
        world_route: draft.plan.world_route.clone(),
        world_identity: draft.plan.world_identity.clone(),
    };
    draft.plan.validate_scope(&routes)?;
    Ok(())
}
fn binding(draft: &AccountDraftV1, input: &ApprovalAdmissionInput) -> Result<()> {
    let CustodyIntentV1::Provision {
        provision_id,
        purpose,
        plan_digest,
        bootstrap_config_version,
        world_identity,
        identity_route,
        world_route,
        deadline,
        transport_limit,
        ..
    } = &input.request.intent
    else {
        return Err(ProvisionApprovalError::BindingMismatch);
    };
    if provision_id != &draft.entry.provision_id
        || purpose != &draft.plan.purpose
        || plan_digest != &draft.entry.plan_digest
        || *bootstrap_config_version != draft.entry.bootstrap_config_version
        || world_identity != &draft.world_identity
        || identity_route != &draft.plan.identity_route
        || world_route != &draft.plan.world_route
        || *deadline != draft.plan.expires_at
        || *transport_limit != draft.plan.transport_limit
        || input.request.context.environment != draft.entry.environment
        || input.request.context.custody_account_id != draft.entry.custody_account_id
        || input.approval.proof.manager_account_id != draft.entry.manager.native_account_id
        || input.approval.proof.identity_namespace != draft.entry.manager.identity_namespace
    {
        return Err(ProvisionApprovalError::BindingMismatch);
    }
    Ok(())
}
fn tasks(
    draft: &AccountDraftV1,
    cancelled: bool,
) -> Result<BTreeMap<String, ProvisionRoleKeyTaskV1>> {
    let mut tasks = BTreeMap::new();
    for (name, role) in [
        ("identity", Role::Identity),
        ("owner", Role::Owner),
        ("executor", Role::Executor),
    ] {
        let operation_id = super::types::digest(&(
            "oasis7.custody.provision-role-key-task.v1",
            &draft.entry,
            name,
            1_u8,
        ))?;
        tasks.insert(
            name.into(),
            ProvisionRoleKeyTaskV1 {
                key_id: format!("role-key:{name}:{operation_id}"),
                operation_id,
                provision_id: draft.entry.provision_id.clone(),
                environment: draft.entry.environment.clone(),
                custody_account_id: draft.entry.custody_account_id.clone(),
                role,
                key_version: DecimalU64(1),
                algorithm: ProvisionRoleKeyAlgorithmV1::Ed25519,
                state: if cancelled {
                    ProvisionRoleKeyTaskStateV1::Cancelled
                } else {
                    ProvisionRoleKeyTaskStateV1::Pending
                },
            },
        );
    }
    Ok(tasks)
}
