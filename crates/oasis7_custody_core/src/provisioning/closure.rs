//! Manager-wide conservative closure. Authentication and canonical receipt verification
//! belong to the native/Owner adapters; these transitions never grant authority.
use super::types::{digest, valid_id};
use super::{MAX_MANAGER_ASSOCIATIONS, ManagerAccountIndex, ManagerPrincipal, ProvisionError};
use crate::DecimalU64;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

const MAX_IN_FLIGHT: usize = 256;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AssociationPhase {
    PendingInitialization,
    Initialized,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AssociationAccount {
    phase: AssociationPhase,
    security_epoch: DecimalU64,
}
impl AssociationAccount {
    pub(super) fn pending() -> Self {
        Self {
            phase: AssociationPhase::PendingInitialization,
            security_epoch: DecimalU64(0),
        }
    }
    pub fn phase(&self) -> AssociationPhase {
        self.phase.clone()
    }
    pub fn security_epoch(&self) -> u64 {
        self.security_epoch.0
    }
}

/// An authenticated native revocation event. The adapter must authenticate its
/// namespace/manager and current epoch/version before entering the transaction.
/// Any source revocation conservatively closes this manager's entire directory.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ManagerRevocationNotice {
    pub operation_id: String,
    pub manager: ManagerPrincipal,
    pub environment: String,
    pub identity_instance_epoch: DecimalU64,
    pub source_security_version: DecimalU64,
    pub source_credential_ref: Option<String>,
    pub source_credential_version: Option<DecimalU64>,
}
impl ManagerRevocationNotice {
    pub fn digest(&self) -> Result<String, ProvisionError> {
        digest(&("oasis7.custody.manager-revocation.v1", self))
    }
    fn validate(&self) -> Result<(), ProvisionError> {
        if !valid_id(&self.environment)
            || !valid_id(&self.operation_id)
            || self.identity_instance_epoch.0 == 0
            || self.source_security_version.0 == 0
            || self.source_credential_ref.is_some() != self.source_credential_version.is_some()
            || self
                .source_credential_ref
                .as_ref()
                .is_some_and(|v| !valid_id(v))
            || self.source_credential_version.is_some_and(|v| v.0 == 0)
        {
            return Err(ProvisionError::Invalid);
        }
        Ok(())
    }
}

/// Exact immutable target saved before dispatch, including accounts whose
/// initialization has not run yet. The Account DO must persist a tombstone even
/// if its account record does not exist.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AccountFreezeTarget {
    pub custody_account_id: String,
    pub manager: ManagerPrincipal,
    pub environment: String,
    pub revocation_digest: String,
    pub minimum_security_epoch: DecimalU64,
}

/// Accepted only after the adapter checks the authenticated Account DO response.
/// It attests persistent freeze, closure of all source root/child authorizations,
/// and the complete claimed-operation inventory at that linearization point.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AccountFreezeReceipt {
    pub target: AccountFreezeTarget,
    pub frozen_security_epoch: DecimalU64,
    pub authorization_closure: AccountAuthorizationClosure,
    pub dispatch_barrier: AccountDispatchBarrier,
    pub receipt_digest: String,
    pub claimed_operation_ids: BTreeSet<String>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "snake_case", deny_unknown_fields)]
pub enum AccountAuthorizationClosure {
    AllManagerRootsAndChildrenRevoked { inventory_digest: String },
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "snake_case", deny_unknown_fields)]
pub enum AccountDispatchBarrier {
    AllUnclaimedBlocked { barrier_digest: String },
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConfirmedOriginalOutcome {
    Applied,
    NotApplied,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InFlightResult {
    pub outcome: ConfirmedOriginalOutcome,
    pub receipt_digest: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct ClosureAccount {
    target: AccountFreezeTarget,
    freeze_receipt: Option<AccountFreezeReceipt>,
    reconciled: BTreeMap<String, InFlightResult>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ManagerRevocation {
    notice: ManagerRevocationNotice,
    accounts: BTreeMap<String, ClosureAccount>,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(test, derive(schemars::JsonSchema))]
#[serde(rename_all = "snake_case")]
pub enum ManagerClosureStatus {
    Open,
    Pending,
    Closed,
}

impl ManagerAccountIndex {
    pub fn account(&self, id: &str) -> Option<&AssociationAccount> {
        self.accounts.get(id)
    }
    /// Commit only after the stable directory reservation was persisted. A late
    /// response cannot initialize an account after this manager was frozen.
    pub fn mark_initialized(
        &mut self,
        account_id: &str,
        security_epoch: u64,
    ) -> Result<(), ProvisionError> {
        if self.frozen {
            return Err(ProvisionError::Frozen);
        }
        if security_epoch == u64::MAX {
            return Err(ProvisionError::Overflow);
        }
        let account = self
            .accounts
            .get_mut(account_id)
            .ok_or(ProvisionError::Missing)?;
        if account.phase == AssociationPhase::Initialized {
            return if account.security_epoch.0 == security_epoch {
                Ok(())
            } else {
                Err(ProvisionError::Conflict)
            };
        }
        account.phase = AssociationPhase::Initialized;
        account.security_epoch = DecimalU64(security_epoch);
        Ok(())
    }
    pub fn revocation(&self) -> Option<&ManagerRevocationNotice> {
        self.revocation.as_ref().map(|v| &v.notice)
    }
    /// Atomically persist the closed directory and every account target before
    /// making any fan-out request. Same notice retries never replace targets.
    pub fn begin_revocation(
        &mut self,
        notice: ManagerRevocationNotice,
    ) -> Result<(), ProvisionError> {
        notice.validate()?;
        if notice.manager != self.manager
            || self.environment.as_deref() != Some(&notice.environment)
        {
            return Err(ProvisionError::Forbidden);
        }
        if let Some(original) = &self.revocation {
            return if original.notice == notice {
                Ok(())
            } else {
                Err(ProvisionError::Conflict)
            };
        }
        let revocation_digest = notice.digest()?;
        let mut accounts = BTreeMap::new();
        for (id, account) in &self.accounts {
            let epoch = account
                .security_epoch
                .0
                .checked_add(1)
                .ok_or(ProvisionError::Overflow)?;
            accounts.insert(
                id.clone(),
                ClosureAccount {
                    target: AccountFreezeTarget {
                        custody_account_id: id.clone(),
                        manager: self.manager.clone(),
                        environment: notice.environment.clone(),
                        revocation_digest: revocation_digest.clone(),
                        minimum_security_epoch: DecimalU64(epoch),
                    },
                    freeze_receipt: None,
                    reconciled: BTreeMap::new(),
                },
            );
        }
        self.frozen = true;
        self.revocation = Some(ManagerRevocation { notice, accounts });
        Ok(())
    }
    pub fn freeze_targets(&self) -> impl Iterator<Item = &AccountFreezeTarget> {
        self.revocation
            .iter()
            .flat_map(|v| v.accounts.values().map(|a| &a.target))
    }
    pub fn pending_freeze_targets(&self) -> impl Iterator<Item = &AccountFreezeTarget> {
        self.revocation
            .iter()
            .flat_map(|closure| closure.accounts.values())
            .filter(|account| account.freeze_receipt.is_none())
            .map(|account| &account.target)
    }
    pub fn pending_originals(&self) -> impl Iterator<Item = (&AccountFreezeTarget, &str)> {
        self.revocation
            .iter()
            .flat_map(|closure| closure.accounts.values())
            .flat_map(|account| {
                account
                    .freeze_receipt
                    .iter()
                    .flat_map(|receipt| receipt.claimed_operation_ids.iter())
                    .filter(|id| !account.reconciled.contains_key(*id))
                    .map(|id| (&account.target, id.as_str()))
            })
    }
    pub fn closure_status(&self) -> ManagerClosureStatus {
        let Some(revocation) = &self.revocation else {
            return if self.frozen {
                ManagerClosureStatus::Pending
            } else {
                ManagerClosureStatus::Open
            };
        };
        if revocation.accounts.values().all(|account| {
            account.freeze_receipt.as_ref().is_some_and(|receipt| {
                receipt
                    .claimed_operation_ids
                    .iter()
                    .all(|id| account.reconciled.contains_key(id))
            })
        }) {
            ManagerClosureStatus::Closed
        } else {
            ManagerClosureStatus::Pending
        }
    }
    pub fn record_freeze_receipt(
        &mut self,
        receipt: AccountFreezeReceipt,
    ) -> Result<(), ProvisionError> {
        validate_receipt(&receipt)?;
        let closure = self.revocation.as_mut().ok_or(ProvisionError::Missing)?;
        let account = closure
            .accounts
            .get_mut(&receipt.target.custody_account_id)
            .ok_or(ProvisionError::Missing)?;
        if account.target != receipt.target {
            return Err(ProvisionError::Conflict);
        }
        if let Some(original) = &account.freeze_receipt {
            return if original == &receipt {
                Ok(())
            } else {
                Err(ProvisionError::Conflict)
            };
        }
        account.freeze_receipt = Some(receipt);
        Ok(())
    }
    /// A canonical original-operation lookup result, verified by the adapter;
    /// transport success, missing history and unknown outcomes are not results.
    pub fn record_inflight_result(
        &mut self,
        revocation_digest: &str,
        account_id: &str,
        operation_id: &str,
        result: InFlightResult,
    ) -> Result<(), ProvisionError> {
        if !valid_digest(&result.receipt_digest) {
            return Err(ProvisionError::Invalid);
        }
        let closure = self.revocation.as_mut().ok_or(ProvisionError::Missing)?;
        let account = closure
            .accounts
            .get_mut(account_id)
            .ok_or(ProvisionError::Missing)?;
        if account.target.revocation_digest != revocation_digest {
            return Err(ProvisionError::Conflict);
        }
        let receipt = account
            .freeze_receipt
            .as_ref()
            .ok_or(ProvisionError::InvalidTransition)?;
        if !receipt.claimed_operation_ids.contains(operation_id) {
            return Err(ProvisionError::Forbidden);
        }
        if let Some(original) = account.reconciled.get(operation_id) {
            return if original == &result {
                Ok(())
            } else {
                Err(ProvisionError::Conflict)
            };
        }
        account.reconciled.insert(operation_id.into(), result);
        Ok(())
    }
    pub(super) fn validate_persistence(&self) -> Result<(), ProvisionError> {
        if self.manager.identity_namespace.is_empty()
            || self.manager.identity_namespace.len() > 256
            || !valid_id(&self.manager.native_account_id)
            || self.environment.as_ref().is_some_and(|v| !valid_id(v))
            || (!self.entries.is_empty() && self.environment.is_none())
            || self.entries.len() > MAX_MANAGER_ASSOCIATIONS
            || self.entries.len() != self.accounts.len()
        {
            return Err(ProvisionError::Invalid);
        }
        let mut ids = BTreeSet::new();
        for (key, entry) in &self.entries {
            let expected = digest(&(
                "oasis7.custody.provision-id.v1",
                &entry.environment,
                &self.manager,
                &entry.client_request_id,
            ))?;
            if key != &expected
                || entry.provision_id != format!("provision-{expected}")
                || entry.custody_account_id != format!("account-{expected}")
                || entry.manager != self.manager
                || self.environment.as_ref() != Some(&entry.environment)
                || !valid_id(&entry.client_request_id)
                || !valid_digest(&entry.plan_digest)
                || entry.bootstrap_config_version.0 == 0
                || !ids.insert(&entry.custody_account_id)
                || !self.accounts.contains_key(&entry.custody_account_id)
            {
                return Err(ProvisionError::Invalid);
            }
        }
        for account in self.accounts.values() {
            if account.security_epoch.0 == u64::MAX
                || (account.phase == AssociationPhase::PendingInitialization
                    && account.security_epoch.0 != 0)
            {
                return Err(ProvisionError::Invalid);
            }
        }
        if let Some(closure) = &self.revocation {
            closure.notice.validate()?;
            if !self.frozen
                || closure.notice.manager != self.manager
                || self.environment.as_ref() != Some(&closure.notice.environment)
                || closure.accounts.len() != self.accounts.len()
            {
                return Err(ProvisionError::Invalid);
            }
            let digest = closure.notice.digest()?;
            for (id, account) in &closure.accounts {
                let baseline = self.accounts.get(id).ok_or(ProvisionError::Invalid)?;
                if account.target.custody_account_id != *id
                    || account.target.manager != self.manager
                    || account.target.environment != closure.notice.environment
                    || account.target.revocation_digest != digest
                    || Some(account.target.minimum_security_epoch.0)
                        != baseline.security_epoch.0.checked_add(1)
                {
                    return Err(ProvisionError::Invalid);
                }
                if let Some(receipt) = &account.freeze_receipt {
                    validate_receipt(receipt)?;
                    if receipt.target != account.target
                        || account.reconciled.iter().any(|(id, d)| {
                            !receipt.claimed_operation_ids.contains(id)
                                || !valid_digest(&d.receipt_digest)
                        })
                    {
                        return Err(ProvisionError::Invalid);
                    }
                } else if !account.reconciled.is_empty() {
                    return Err(ProvisionError::Invalid);
                }
            }
        }
        Ok(())
    }
}
fn valid_digest(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}
fn validate_receipt(receipt: &AccountFreezeReceipt) -> Result<(), ProvisionError> {
    if receipt.frozen_security_epoch.0 < receipt.target.minimum_security_epoch.0
        || !valid_digest(match &receipt.authorization_closure {
            AccountAuthorizationClosure::AllManagerRootsAndChildrenRevoked { inventory_digest } => {
                inventory_digest
            }
        })
        || !valid_digest(match &receipt.dispatch_barrier {
            AccountDispatchBarrier::AllUnclaimedBlocked { barrier_digest } => barrier_digest,
        })
        || !valid_digest(&receipt.receipt_digest)
        || receipt.claimed_operation_ids.len() > MAX_IN_FLIGHT
        || receipt.claimed_operation_ids.iter().any(|id| !valid_id(id))
    {
        return Err(ProvisionError::Invalid);
    }
    Ok(())
}
