use super::types::valid_id;
use super::{DirectoryEntry, ProvisionError, ProvisionPlan};
use crate::{DecimalU64, budget::BudgetLedger, http::Role};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[cfg_attr(test, derive(schemars::JsonSchema))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProvisionStage {
    Draft,
    Approved,
    RoleKeysReady,
    NativeRegistered,
    NativeSessionReady,
    ParticipantBound,
    ReadyForSetup,
    Prepared,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PublicRoleKey {
    pub key_id: String,
    pub key_version: DecimalU64,
    pub public_key: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PhaseOriginal {
    pub operation_id: String,
    pub body_digest: String,
    pub receipt_digest: Option<String>,
}

/// Phase methods accept only adapter-verified native/canonical receipts. They
/// never themselves verify signatures, authenticate callers or perform dispatch.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProvisionRecord {
    directory: DirectoryEntry,
    plan: ProvisionPlan,
    stage: ProvisionStage,
    frozen: bool,
    security_epoch: DecimalU64,
    approval_nonce: Option<String>,
    keys: BTreeMap<String, PublicRoleKey>,
    originals: BTreeMap<String, PhaseOriginal>,
    native_account_id: Option<String>,
    transport: BudgetLedger,
}

impl ProvisionRecord {
    pub fn draft(directory: DirectoryEntry, plan: ProvisionPlan) -> Result<Self, ProvisionError> {
        if directory.environment != plan.environment || directory.plan_digest != plan.digest()? {
            return Err(ProvisionError::Conflict);
        }
        let transport = BudgetLedger::new(plan.transport_limit.0);
        Ok(Self {
            directory,
            plan,
            stage: ProvisionStage::Draft,
            frozen: false,
            security_epoch: DecimalU64(0),
            approval_nonce: None,
            keys: BTreeMap::new(),
            originals: BTreeMap::new(),
            native_account_id: None,
            transport,
        })
    }
    pub(super) fn frozen(&self) -> bool {
        self.frozen
    }
    pub(super) fn validate_approval_progress(
        &self,
        draft: &super::AccountDraftV1,
        nonce: Option<&str>,
        keys: &[(Role, PublicRoleKey)],
        claimed_transports: u64,
        native_phases: &[(ProvisionStage, PhaseOriginal, Option<String>)],
    ) -> Result<(), ProvisionError> {
        let mut expected = Self::draft(draft.entry.clone(), draft.plan.clone())?;
        if let Some(nonce) = nonce {
            expected.approve(&draft.entry.plan_digest, nonce, 0, 0)?;
        }
        for (role, key) in keys {
            expected.record_key(*role, key.clone(), 0, 0)?;
        }
        for _ in 0..claimed_transports {
            expected.charge_transport(0, 0)?;
        }
        for (phase, original, native_account) in native_phases {
            let epoch = expected.security_epoch.0;
            let mut pending = original.clone();
            let digest = pending
                .receipt_digest
                .take()
                .ok_or(ProvisionError::Invalid)?;
            expected.save_original(*phase, pending, epoch, 0)?;
            expected.confirm_phase(
                *phase,
                &original.operation_id,
                &digest,
                native_account.as_deref(),
                epoch,
                0,
            )?;
            if *phase == ProvisionStage::NativeRegistered {
                expected.activate_native_binding()?;
            }
        }
        if self.frozen {
            expected.freeze(self.security_epoch.0)?;
        }
        if self != &expected {
            return Err(ProvisionError::Invalid);
        }
        Ok(())
    }
    pub(super) fn confirm_managed_session(
        &mut self,
        original: PhaseOriginal,
    ) -> Result<(), ProvisionError> {
        if self.frozen
            || self.security_epoch.0 != 1
            || self.stage != ProvisionStage::NativeRegistered
            || !valid_id(&original.operation_id)
            || original.body_digest.is_empty()
            || original.receipt_digest.is_none()
        {
            return Err(ProvisionError::InvalidTransition);
        }
        if self.originals.contains_key("login") {
            return Err(ProvisionError::Conflict);
        }
        self.originals.insert("login".into(), original);
        self.stage = ProvisionStage::NativeSessionReady;
        Ok(())
    }
    pub fn transport_remaining(&self) -> Result<u64, ProvisionError> {
        self.transport
            .remaining()
            .map_err(|_| ProvisionError::Exhausted)
    }
    pub fn security_epoch(&self) -> DecimalU64 {
        self.security_epoch
    }
    pub(super) fn activate_native_binding(&mut self) -> Result<(), ProvisionError> {
        if self.frozen
            || self.stage != ProvisionStage::NativeRegistered
            || self.security_epoch.0 != 0
        {
            return Err(ProvisionError::Conflict);
        }
        self.security_epoch = DecimalU64(1);
        Ok(())
    }
    pub fn stage(&self) -> ProvisionStage {
        self.stage
    }
    pub fn native_account_id(&self) -> Option<&str> {
        self.native_account_id.as_deref()
    }
    pub fn role_key(&self, role: Role) -> Option<&PublicRoleKey> {
        self.keys.get(role_name(role))
    }
    pub fn freeze(&mut self, epoch: u64) -> Result<(), ProvisionError> {
        if epoch < self.security_epoch.0 {
            return Err(ProvisionError::Conflict);
        }
        self.security_epoch = DecimalU64(epoch);
        self.frozen = true;
        Ok(())
    }
    pub fn check_current(&self, expected_epoch: u64, now: u64) -> Result<(), ProvisionError> {
        if self.frozen {
            return Err(ProvisionError::Frozen);
        }
        if self.security_epoch.0 != expected_epoch {
            return Err(ProvisionError::Conflict);
        }
        if now >= self.plan.expires_at.0 {
            return Err(ProvisionError::Expired);
        }
        Ok(())
    }
    /// Caller consumes a verified fresh proof in the same transaction as this transition.
    pub fn approve(
        &mut self,
        plan_digest: &str,
        nonce: &str,
        epoch: u64,
        now: u64,
    ) -> Result<(), ProvisionError> {
        self.check_current(epoch, now)?;
        if !valid_id(nonce) {
            return Err(ProvisionError::Invalid);
        }
        if plan_digest != self.directory.plan_digest {
            return Err(ProvisionError::Conflict);
        }
        if self.approval_nonce.as_deref() == Some(nonce) {
            return Ok(());
        }
        if self.stage != ProvisionStage::Draft {
            return Err(ProvisionError::Conflict);
        }
        self.approval_nonce = Some(nonce.into());
        self.stage = ProvisionStage::Approved;
        Ok(())
    }
    /// A role slot is immutable. Concurrent/late ensure-key responses cannot replace it.
    pub fn record_key(
        &mut self,
        role: Role,
        key: PublicRoleKey,
        epoch: u64,
        now: u64,
    ) -> Result<(), ProvisionError> {
        self.check_current(epoch, now)?;
        if role == Role::RecoveryFence || !valid_id(&key.key_id) || key.public_key.is_empty() {
            return Err(ProvisionError::Invalid);
        }
        let name = role_name(role);
        if let Some(original) = self.keys.get(name) {
            return if original == &key {
                Ok(())
            } else {
                Err(ProvisionError::Conflict)
            };
        }
        if self.stage != ProvisionStage::Approved {
            return Err(ProvisionError::InvalidTransition);
        }
        if self
            .keys
            .values()
            .any(|other| other.public_key == key.public_key || other.key_id == key.key_id)
        {
            return Err(ProvisionError::Conflict);
        }
        self.keys.insert(name.into(), key);
        if self.keys.len() == 3 {
            self.stage = ProvisionStage::RoleKeysReady;
        }
        Ok(())
    }
    /// Save exact phase identity before calling the network. No phase may replace an unknown original.
    pub fn save_original(
        &mut self,
        phase: ProvisionStage,
        original: PhaseOriginal,
        epoch: u64,
        now: u64,
    ) -> Result<(), ProvisionError> {
        self.check_current(epoch, now)?;
        if !valid_id(&original.operation_id)
            || original.body_digest.is_empty()
            || original.receipt_digest.is_some()
        {
            return Err(ProvisionError::Invalid);
        }
        let name = phase_name(phase).ok_or(ProvisionError::InvalidTransition)?;
        if let Some(saved) = self.originals.get(name) {
            return if saved.operation_id == original.operation_id
                && saved.body_digest == original.body_digest
            {
                Ok(())
            } else {
                Err(ProvisionError::Conflict)
            };
        }
        if next(self.stage) != Some(phase) {
            return Err(ProvisionError::InvalidTransition);
        }
        self.originals.insert(name.into(), original);
        Ok(())
    }
    pub fn charge_transport(&mut self, epoch: u64, now: u64) -> Result<(), ProvisionError> {
        self.check_current(epoch, now)?;
        self.transport
            .charge(1)
            .map_err(|_| ProvisionError::Exhausted)
    }
    /// Only after verifying the fixed native/world pin, original correlation and receipt.
    pub fn confirm_phase(
        &mut self,
        phase: ProvisionStage,
        operation_id: &str,
        receipt_digest: &str,
        native_account_id: Option<&str>,
        epoch: u64,
        now: u64,
    ) -> Result<(), ProvisionError> {
        self.check_current(epoch, now)?;
        if receipt_digest.is_empty() {
            return Err(ProvisionError::Invalid);
        }
        let name = phase_name(phase).ok_or(ProvisionError::InvalidTransition)?;
        let original = self.originals.get(name).ok_or(ProvisionError::Missing)?;
        if original.operation_id != operation_id {
            return Err(ProvisionError::Conflict);
        }
        if let Some(receipt) = &original.receipt_digest {
            if receipt != receipt_digest
                || (native_account_id.is_some() && native_account_id != self.native_account_id())
            {
                return Err(ProvisionError::Conflict);
            }
            return Ok(());
        }
        if next(self.stage) != Some(phase) {
            return Err(ProvisionError::InvalidTransition);
        }
        if phase == ProvisionStage::NativeRegistered {
            let account = native_account_id
                .filter(|id| valid_id(id))
                .ok_or(ProvisionError::Invalid)?;
            if self
                .native_account_id
                .as_deref()
                .is_some_and(|id| id != account)
            {
                return Err(ProvisionError::Conflict);
            }
            self.native_account_id = Some(account.into());
        } else if native_account_id.is_some() {
            return Err(ProvisionError::Invalid);
        }
        self.originals
            .get_mut(name)
            .ok_or(ProvisionError::Missing)?
            .receipt_digest = Some(receipt_digest.into());
        self.stage = phase;
        Ok(())
    }
}
fn role_name(role: Role) -> &'static str {
    match role {
        Role::Identity => "identity",
        Role::Owner => "owner",
        Role::Executor => "executor",
        Role::RecoveryFence => "recovery-fence",
    }
}
fn phase_name(stage: ProvisionStage) -> Option<&'static str> {
    match stage {
        ProvisionStage::NativeRegistered => Some("registration"),
        ProvisionStage::NativeSessionReady => Some("login"),
        ProvisionStage::ParticipantBound => Some("participant"),
        ProvisionStage::ReadyForSetup => Some("owner-confirmation"),
        ProvisionStage::Prepared => Some("setup"),
        _ => None,
    }
}
fn next(stage: ProvisionStage) -> Option<ProvisionStage> {
    match stage {
        ProvisionStage::RoleKeysReady => Some(ProvisionStage::NativeRegistered),
        ProvisionStage::NativeRegistered => Some(ProvisionStage::NativeSessionReady),
        ProvisionStage::NativeSessionReady => Some(ProvisionStage::ParticipantBound),
        ProvisionStage::ParticipantBound => Some(ProvisionStage::ReadyForSetup),
        // Setup requires separately approved owner intents, not this provision plan.
        _ => None,
    }
}
