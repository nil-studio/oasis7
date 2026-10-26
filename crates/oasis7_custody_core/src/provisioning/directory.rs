use super::types::{digest, valid_id};
use super::{ManagerPrincipal, ProvisionError, ProvisionPlan, ProvisionPolicy};
use crate::DecimalU64;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
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
#[serde(deny_unknown_fields)]
pub struct ManagerAccountIndex {
    manager: ManagerPrincipal,
    frozen: bool,
    entries: BTreeMap<String, DirectoryEntry>,
}

impl ManagerAccountIndex {
    pub fn new(manager: ManagerPrincipal) -> Result<Self, ProvisionError> {
        if manager.identity_namespace.is_empty() || manager.native_account_id.is_empty() {
            return Err(ProvisionError::Invalid);
        }
        Ok(Self {
            manager,
            frozen: false,
            entries: BTreeMap::new(),
        })
    }
    pub fn freeze(&mut self) {
        self.frozen = true;
    }
    pub fn entries(&self) -> impl Iterator<Item = &DirectoryEntry> {
        self.entries.values()
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
        if !valid_id(client_request_id) {
            return Err(ProvisionError::Invalid);
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
        let entry = DirectoryEntry {
            provision_id: format!("provision-{namespace}"),
            custody_account_id: format!("account-{namespace}"),
            manager: principal.clone(),
            client_request_id: client_request_id.into(),
            environment: plan.environment.clone(),
            plan_digest,
            bootstrap_config_version: DecimalU64(policy.version),
        };
        self.entries.insert(namespace, entry.clone());
        Ok(entry)
    }
}
