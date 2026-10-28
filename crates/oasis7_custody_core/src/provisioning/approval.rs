//! Atomic approval/plan state. Role tasks are inert pending work, never key permits.
use super::*;
use crate::native_history::NativeHistoryEntryV1;
use crate::{
    DecimalU64,
    approval::{ApprovedWorldV1, CustodyIntentV1, SignedApprovalVerificationV1},
    approval_admission::*,
    http::Role,
    role_key_slot::RoleKeySlotScopeV1,
};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
#[path = "origin_inspection.rs"]
mod origin_inspection;
pub use origin_inspection::*;

#[path = "native_stages.rs"]
mod native_stages;
pub use native_stages::*;
#[path = "participant.rs"]
mod participant;
pub use participant::*;
#[path = "owner_setup.rs"]
mod owner_setup;
pub use owner_setup::*;
#[path = "pairing_invites.rs"]
mod pairing_invites;
pub use pairing_invites::*;

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

/// The adapter accepts this only from the fixed, authenticated role RPC.
/// This receipt is not a signature, a permit, or proof of key possession itself.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProvisionRoleKeyResultV1 {
    pub scope: RoleKeySlotScopeV1,
    pub public_ref: PublicRoleKey,
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
    role_key_results: BTreeMap<String, ProvisionRoleKeyResultV1>,
    cancellation_operation_id: Option<String>,
    origin_inspections: BTreeMap<String, ProvisionOriginInspectionV1>,
    role_key_dispatches: BTreeMap<String, ProvisionRoleKeyDispatchV1>,
    native_stages: BTreeMap<String, NativeStageOriginalV1>,
    bootstrap_activation: Option<BootstrapActivationV1>,
    managed_logins: BTreeMap<String, ManagedLoginApprovalV1>,
    native_history: BTreeMap<String, NativeHistoryEntryV1>,
    session_status_recoveries: BTreeMap<String, SessionStatusRecoveryOriginalV1>,
    session_recovery_activation: Option<String>,
    participant_provisions: BTreeMap<String, ParticipantOriginalV1>,
    owner_setups: BTreeMap<String, OwnerSetupApprovalV1>,
    owner_setup_reconciles: BTreeMap<String, OwnerSetupReconcileApprovalV1>,
    pairing_invites: BTreeMap<String, PairingInviteRecordV1>,
    session_inventory: SessionInventoryV1,
    test_sessions: BTreeMap<String, TestSessionAuthorityV1>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawAccountProvisionApprovalState {
    draft: AccountDraftV1,
    book: ApprovalAdmissionBook,
    provision: ProvisionRecord,
    accepted_operation_id: Option<String>,
    role_key_tasks: BTreeMap<String, ProvisionRoleKeyTaskV1>,
    // Legacy approved snapshots contain no results. Defaulting empty does not
    // permit a later stage: reconstructed ProvisionRecord must still match.
    #[serde(default)]
    role_key_results: BTreeMap<String, ProvisionRoleKeyResultV1>,
    cancellation_operation_id: Option<String>,
    #[serde(default)]
    origin_inspections: BTreeMap<String, ProvisionOriginInspectionV1>,
    #[serde(default)]
    role_key_dispatches: BTreeMap<String, ProvisionRoleKeyDispatchV1>,
    #[serde(default)]
    native_stages: BTreeMap<String, NativeStageOriginalV1>,
    #[serde(default)]
    bootstrap_activation: Option<BootstrapActivationV1>,
    #[serde(default)]
    managed_logins: BTreeMap<String, ManagedLoginApprovalV1>,
    #[serde(default)]
    native_history: BTreeMap<String, NativeHistoryEntryV1>,
    #[serde(default)]
    session_status_recoveries: BTreeMap<String, SessionStatusRecoveryOriginalV1>,
    #[serde(default)]
    session_recovery_activation: Option<String>,
    #[serde(default)]
    participant_provisions: BTreeMap<String, ParticipantOriginalV1>,
    #[serde(default)]
    owner_setups: BTreeMap<String, OwnerSetupApprovalV1>,
    #[serde(default)]
    owner_setup_reconciles: BTreeMap<String, OwnerSetupReconcileApprovalV1>,
    #[serde(default)]
    pairing_invites: BTreeMap<String, PairingInviteRecordV1>,
    #[serde(default)]
    session_inventory: SessionInventoryV1,
    #[serde(default)]
    test_sessions: BTreeMap<String, TestSessionAuthorityV1>,
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
            role_key_results: raw.role_key_results,
            cancellation_operation_id: raw.cancellation_operation_id,
            origin_inspections: raw.origin_inspections,
            role_key_dispatches: raw.role_key_dispatches,
            native_stages: raw.native_stages,
            bootstrap_activation: raw.bootstrap_activation,
            managed_logins: raw.managed_logins,
            native_history: raw.native_history,
            session_status_recoveries: raw.session_status_recoveries,
            session_recovery_activation: raw.session_recovery_activation,
            participant_provisions: raw.participant_provisions,
            owner_setups: raw.owner_setups,
            owner_setup_reconciles: raw.owner_setup_reconciles,
            pairing_invites: raw.pairing_invites,
            session_inventory: raw.session_inventory,
            test_sessions: raw.test_sessions,
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
            role_key_results: BTreeMap::new(),
            cancellation_operation_id: None,
            origin_inspections: BTreeMap::new(),
            role_key_dispatches: BTreeMap::new(),
            native_stages: BTreeMap::new(),
            bootstrap_activation: None,
            managed_logins: BTreeMap::new(),
            native_history: BTreeMap::new(),
            session_status_recoveries: BTreeMap::new(),
            session_recovery_activation: None,
            participant_provisions: BTreeMap::new(),
            owner_setups: BTreeMap::new(),
            owner_setup_reconciles: BTreeMap::new(),
            pairing_invites: BTreeMap::new(),
            session_inventory: SessionInventoryV1::Known,
            test_sessions: BTreeMap::new(),
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
    pub fn role_key_results(&self) -> &BTreeMap<String, ProvisionRoleKeyResultV1> {
        &self.role_key_results
    }
    /// Current source facts must come from fresh authenticated inspect_origin,
    /// not the old approval proof's thirty-second handoff window.
    pub fn check_role_key_current(
        &self,
        current: &ProvisionApprovalContext<'_>,
        now: u64,
    ) -> Result<()> {
        self.current(current, now)?;
        let id = self
            .accepted_operation_id
            .as_deref()
            .ok_or(ApprovalAdmissionError::Missing)?;
        let original = self.original(id).ok_or(ApprovalAdmissionError::Missing)?;
        self.historical_scope(id, current)?;
        let proof = &original.input().approval.proof;
        let source = &current.approval.source;
        if original.stage() != ApprovalAdmissionStage::Consumed
            || current.approval.authority != proof.context
            || current.approval.frozen
            || current
                .approval
                .fence
                .check_epoch(proof.context.service_epoch.0)
                .is_err()
            || source.manager_session_ref != proof.manager_session_ref
            || source.origin_credential_id != proof.origin_credential_id
            || source.origin_credential_version != proof.origin_credential_version
            || source.passkey_credential_id != proof.passkey_credential_id
            || source.passkey_credential_version != proof.passkey_credential_version
            || source.manager_policy_version != proof.manager_policy_version
            || source.manager_security_version != proof.manager_security_version
            || now >= source.session_expires_at.0
            || now >= source.session_idle_expires_at.0
        {
            return Err(ProvisionApprovalError::BindingMismatch);
        }
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
        Ok(())
    }
    /// Records a fixed-role adapter result in the same persisted Account value.
    /// Exact retries are historical reads; they do not authorize another action.
    pub fn record_role_key_result(
        &mut self,
        result: ProvisionRoleKeyResultV1,
        current: &ProvisionApprovalContext<'_>,
        now: u64,
    ) -> Result<PublicRoleKey> {
        let name = self.validate_role_key_result(&result)?;
        if let Some(original) = self.role_key_results.get(name) {
            let id = self
                .accepted_operation_id
                .as_deref()
                .ok_or(ApprovalAdmissionError::Missing)?;
            self.historical_scope(id, current)?;
            return if original == &result {
                Ok(original.public_ref.clone())
            } else {
                Err(ProvisionApprovalError::Conflict)
            };
        }
        self.check_role_key_current(current, now)?;
        if self
            .role_key_tasks
            .get(name)
            .is_none_or(|t| t.state != ProvisionRoleKeyTaskStateV1::Pending)
        {
            return Err(ProvisionApprovalError::Conflict);
        }
        let mut candidate = self.clone();
        candidate.provision.record_key(
            result.scope.context.identity.role,
            result.public_ref.clone(),
            current.approval.authority.security_version.0,
            now,
        )?;
        candidate
            .role_key_results
            .insert(name.into(), result.clone());
        candidate.validate_persistence()?;
        *self = candidate;
        Ok(result.public_ref)
    }
    fn validate_role_key_result(&self, result: &ProvisionRoleKeyResultV1) -> Result<&'static str> {
        result
            .scope
            .validate()
            .map_err(|_| ProvisionApprovalError::BindingMismatch)?;
        let name = match result.scope.context.identity.role {
            Role::Identity => "identity",
            Role::Owner => "owner",
            Role::Executor => "executor",
            Role::RecoveryFence => return Err(ProvisionApprovalError::BindingMismatch),
        };
        let task = self
            .role_key_tasks
            .get(name)
            .ok_or(ProvisionApprovalError::BindingMismatch)?;
        if result.scope
            != RoleKeySlotScopeV1::from_task(task)
                .map_err(|_| ProvisionApprovalError::BindingMismatch)?
            || result.public_ref.key_id != task.key_id
            || result.public_ref.key_version != task.key_version
            || result.public_ref.public_key.len() != 64
            || !result
                .public_ref
                .public_key
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        {
            return Err(ProvisionApprovalError::BindingMismatch);
        }
        Ok(name)
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
        self.validate_managed_login_persistence()?;
        self.validate_owner_setup_persistence()?;
        self.validate_owner_setup_reconcile_persistence()?;
        self.validate_pairing_invite_persistence()?;
        self.validate_test_session_persistence()?;
        self.validate_history_persistence()?;
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
            if !consumed.is_empty()
                || !self.role_key_tasks.is_empty()
                || !self.role_key_results.is_empty()
            {
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
        let mut keys = Vec::new();
        for (name, result) in &self.role_key_results {
            if self.validate_role_key_result(result)? != name {
                return Err(ProvisionApprovalError::InvalidPersistence);
            }
            keys.push((
                result.scope.context.identity.role,
                result.public_ref.clone(),
            ));
        }
        let mut native_phases = self.native_phase_replay()?;
        let recovered = self.session_recovery_phase_replay()?;
        if !recovered.is_empty()
            && native_phases
                .iter()
                .any(|v| v.0 == ProvisionStage::NativeSessionReady)
        {
            return Err(ProvisionApprovalError::InvalidPersistence);
        }
        native_phases.extend(recovered);
        native_phases.extend(self.participant_phase_replay()?);
        native_phases.extend(self.owner_setup_phase_replay()?);
        self.provision.validate_approval_progress(
            &self.draft,
            nonce,
            &keys,
            self.validate_attempt_persistence()?
                + self.validate_native_stage_persistence()?
                + self.validate_session_recovery_persistence()?
                + self.validate_participant_persistence()?,
            &native_phases,
        )?;
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
