use super::types::{digest, valid_id};
use super::{ManagerPrincipal, ProvisionError, ProvisionPlan, ProvisionPolicy};
use crate::DecimalU64;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

pub const MAX_MANAGER_ASSOCIATIONS: usize = 256;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(test, derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct DirectoryEntry {
    pub provision_id: String,
    pub custody_account_id: String,
    pub manager: ManagerPrincipal,
    pub client_request_id: String,
    pub environment: String,
    pub plan_digest: String,
    pub bootstrap_config_version: DecimalU64,
}

/// Owner-local authoritative association inventory, never a D1 projection.
/// Persist reserve before initializing an account or creating any role key.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "RawManagerAccountIndex")]
pub struct ManagerAccountIndex {
    pub(super) manager: ManagerPrincipal,
    pub(super) frozen: bool,
    pub(super) environment: Option<String>,
    pub(super) entries: BTreeMap<String, DirectoryEntry>,
    pub(super) accounts: BTreeMap<String, super::AssociationAccount>,
    pub(super) revocation: Option<super::ManagerRevocation>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawManagerAccountIndex {
    manager: ManagerPrincipal,
    frozen: bool,
    environment: Option<String>,
    entries: BTreeMap<String, DirectoryEntry>,
    accounts: BTreeMap<String, super::AssociationAccount>,
    revocation: Option<super::ManagerRevocation>,
}
impl TryFrom<RawManagerAccountIndex> for ManagerAccountIndex {
    type Error = &'static str;
    fn try_from(raw: RawManagerAccountIndex) -> Result<Self, Self::Error> {
        let index = Self {
            manager: raw.manager,
            frozen: raw.frozen,
            environment: raw.environment,
            entries: raw.entries,
            accounts: raw.accounts,
            revocation: raw.revocation,
        };
        index
            .validate_persistence()
            .map_err(|_| "invalid manager association directory")?;
        Ok(index)
    }
}

impl ManagerAccountIndex {
    pub fn new(manager: ManagerPrincipal) -> Result<Self, ProvisionError> {
        if manager.identity_namespace.is_empty()
            || manager.identity_namespace.len() > 256
            || !valid_id(&manager.native_account_id)
        {
            return Err(ProvisionError::Invalid);
        }
        Ok(Self {
            manager,
            frozen: false,
            environment: None,
            entries: BTreeMap::new(),
            accounts: BTreeMap::new(),
            revocation: None,
        })
    }
    pub fn new_for_environment(
        manager: ManagerPrincipal,
        environment: String,
    ) -> Result<Self, ProvisionError> {
        if !valid_id(&environment) {
            return Err(ProvisionError::Invalid);
        }
        let mut value = Self::new(manager)?;
        value.environment = Some(environment);
        Ok(value)
    }
    pub fn environment(&self) -> Option<&str> {
        self.environment.as_deref()
    }
    pub fn freeze(&mut self) {
        self.frozen = true;
    }
    pub fn entries(&self) -> impl Iterator<Item = &DirectoryEntry> {
        self.entries.values()
    }
    pub fn manager(&self) -> &ManagerPrincipal {
        &self.manager
    }
    pub fn is_frozen(&self) -> bool {
        self.frozen
    }
    pub fn reserve(
        &mut self,
        principal: &ManagerPrincipal,
        client_request_id: &str,
        plan: &ProvisionPlan,
        policy: &ProvisionPolicy<'_>,
        now: u64,
    ) -> Result<DirectoryEntry, ProvisionError> {
        // Current authentication is supplied by the native identity adapter, not this record.
        if self.frozen {
            return Err(ProvisionError::Frozen);
        }
        if principal != &self.manager
            || !policy.managers.iter().any(|entry| {
                &entry.principal == principal && entry.environments.contains(&plan.environment)
            })
        {
            return Err(ProvisionError::Forbidden);
        }
        if !valid_id(client_request_id) || policy.version == 0 {
            return Err(ProvisionError::Invalid);
        }
        if self
            .environment
            .as_ref()
            .is_some_and(|environment| environment != &plan.environment)
        {
            return Err(ProvisionError::Forbidden);
        }
        plan.validate_scope(policy.routes)?;
        let namespace = digest(&(
            "oasis7.custody.provision-id.v1",
            &plan.environment,
            principal,
            client_request_id,
        ))?;
        let plan_digest = plan.digest()?;
        if let Some(original) = self.entries.get(&namespace) {
            if original.plan_digest != plan_digest {
                return Err(ProvisionError::Conflict);
            }
            // Config version is not caller-changeable identity. Its current allowlist was checked above.
            return Ok(original.clone());
        }
        plan.validate(now, policy.routes)?;
        if self.entries.len() >= MAX_MANAGER_ASSOCIATIONS {
            return Err(ProvisionError::Exhausted);
        }
        let entry = DirectoryEntry {
            provision_id: format!("provision-{namespace}"),
            custody_account_id: format!("account-{namespace}"),
            manager: principal.clone(),
            client_request_id: client_request_id.into(),
            environment: plan.environment.clone(),
            plan_digest,
            bootstrap_config_version: DecimalU64(policy.version),
        };
        self.environment = Some(plan.environment.clone());
        self.accounts.insert(
            entry.custody_account_id.clone(),
            super::AssociationAccount::pending(),
        );
        self.entries.insert(namespace, entry.clone());
        Ok(entry)
    }
}
