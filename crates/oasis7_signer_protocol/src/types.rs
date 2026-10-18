use serde::{Deserialize, Serialize};

use crate::protocol::{ExplicitNull, RollbackProtocolContext, validate_id};

#[cfg(target_os = "macos")]
pub const INSTALLATION_CONFIG_PATH: &str = "/private/etc/oasis7/local-signer-installation.json";
#[cfg(target_os = "linux")]
pub const INSTALLATION_CONFIG_PATH: &str = "/etc/oasis7/local-signer-installation.json";
#[cfg(not(any(target_os = "macos", target_os = "linux")))]
pub const INSTALLATION_CONFIG_PATH: &str = "";

pub const INSTALLATION_SCHEMA: &str = "oasis7.local_signer_installation.v3";
pub const POLICY_SCHEMA: &str = "oasis7.local_signer_policy.v2";
pub const GRANT_SCHEMA: &str = "oasis7.local_signing_batch_grant.v1";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InstallationConfig {
    pub schema_version: String,
    pub installation_id: String,
    pub deployment_id: String,
    pub store_dir: String,
    pub store_device_id: u64,
    pub store_inode: u64,
    pub signer_uid: u32,
    pub signer_gid: u32,
    pub callers: Vec<CallerBinding>,
    pub release_id: String,
    pub worker_executable: String,
    pub worker_sha256: String,
    pub control_schema_version: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CallerBinding {
    pub uid: u32,
    pub work_dir: String,
    pub work_device_id: u64,
    pub work_inode: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Policy {
    pub schema_version: String,
    pub installation_id: String,
    pub deployment_id: String,
    pub policy_revision: String,
    pub enabled_purposes: Vec<String>,
    pub key_bindings: Vec<KeyBinding>,
    pub limits: PolicyLimits,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct KeyBinding {
    pub purpose: String,
    pub signer_id: String,
    pub public_key_sha256: String,
    pub protocol_authorities: Vec<String>,
    pub provider_ids: Vec<String>,
    pub deployment_ids: Vec<String>,
    pub network_ids: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PolicyLimits {
    pub max_batch_items: u32,
    pub max_distinct_requests_per_item: u32,
    pub max_payload_bytes: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BatchGrant {
    pub schema_version: String,
    pub grant_id: String,
    pub installation_id: String,
    pub deployment_id: String,
    pub caller_uid: u32,
    pub network_id: String,
    pub task_uid: String,
    pub source_head_oid: String,
    pub policy_revision: String,
    /// UTC Unix time in milliseconds, consistent with the rollback payload contract.
    pub not_before: u64,
    /// UTC Unix time in milliseconds, consistent with the rollback payload contract.
    pub expires_at: u64,
    pub approval_record_ref: String,
    pub items: Vec<GrantItem>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GrantItem {
    pub purpose: String,
    pub provider_id: ExplicitNull<String>,
    pub signer_id: String,
    pub public_key_sha256: String,
    pub operation_key: String,
    pub payload_sha256: String,
    pub protocol_bindings: RollbackProtocolContext,
    pub max_distinct_requests: u32,
}

impl InstallationConfig {
    pub fn validate(&self) -> Result<(), String> {
        if self.schema_version != INSTALLATION_SCHEMA {
            return Err("unsupported installation schema_version".to_owned());
        }
        validate_id(&self.installation_id).map_err(|error| error.to_string())?;
        validate_id(&self.deployment_id).map_err(|error| error.to_string())?;
        validate_id(&self.release_id).map_err(|error| error.to_string())?;
        validate_sha256(&self.worker_sha256)?;
        if self.signer_uid == 0 || self.signer_gid == 0 {
            return Err("signer UID and GID must be non-root".to_owned());
        }
        if self.control_schema_version.is_empty() {
            return Err("control_schema_version must not be empty".to_owned());
        }
        validate_absolute_path(&self.store_dir)?;
        validate_absolute_path(&self.worker_executable)?;
        if self.callers.is_empty() {
            return Err("installation must bind at least one caller".to_owned());
        }
        let mut uids = std::collections::BTreeSet::new();
        for caller in &self.callers {
            if caller.uid == 0 || caller.uid == self.signer_uid || !uids.insert(caller.uid) {
                return Err(
                    "caller UID must be non-root, differ from signer, and be unique".to_owned(),
                );
            }
            validate_absolute_path(&caller.work_dir)?;
            let work = std::path::Path::new(&caller.work_dir);
            let store = std::path::Path::new(&self.store_dir);
            if work.starts_with(store) || store.starts_with(work) {
                return Err("caller work root overlaps private store".to_owned());
            }
            for protected in [
                std::path::Path::new(&self.worker_executable)
                    .parent()
                    .unwrap(),
                std::path::Path::new(INSTALLATION_CONFIG_PATH)
                    .parent()
                    .unwrap(),
            ] {
                if work.starts_with(protected) || protected.starts_with(work) {
                    return Err("caller root overlaps protected code or configuration".to_owned());
                }
            }
            for other in &self.callers {
                if caller.uid != other.uid {
                    let other = std::path::Path::new(&other.work_dir);
                    if work.starts_with(other) || other.starts_with(work) {
                        return Err("caller work roots overlap".to_owned());
                    }
                }
            }
        }
        Ok(())
    }

    pub fn caller(&self, caller_uid: u32) -> Option<&CallerBinding> {
        self.callers.iter().find(|caller| caller.uid == caller_uid)
    }
}

impl Policy {
    pub fn validate(&self, installation: &InstallationConfig) -> Result<(), String> {
        if self.schema_version != POLICY_SCHEMA {
            return Err("unsupported policy schema_version".to_owned());
        }
        if self.installation_id != installation.installation_id
            || self.deployment_id != installation.deployment_id
        {
            return Err("policy installation/deployment binding mismatch".to_owned());
        }
        validate_id(&self.policy_revision).map_err(|error| error.to_string())?;
        if self.key_bindings.is_empty() {
            return Err("policy must contain at least one key binding".to_owned());
        }
        if self.limits.max_batch_items == 0
            || self.limits.max_batch_items > 64
            || self.limits.max_distinct_requests_per_item == 0
            || self.limits.max_distinct_requests_per_item > 16
            || self.limits.max_payload_bytes == 0
            || self.limits.max_payload_bytes > 16 * 1024 * 1024
        {
            return Err("policy limits exceed the M0 bounds".to_owned());
        }
        for purpose in &self.enabled_purposes {
            validate_id(purpose).map_err(|error| error.to_string())?;
        }
        for key in &self.key_bindings {
            validate_id(&key.purpose).map_err(|error| error.to_string())?;
            validate_id(&key.signer_id).map_err(|error| error.to_string())?;
            validate_sha256(&key.public_key_sha256)?;
            for value in key
                .protocol_authorities
                .iter()
                .chain(&key.provider_ids)
                .chain(&key.deployment_ids)
                .chain(&key.network_ids)
            {
                validate_id(value).map_err(|error| error.to_string())?;
            }
        }
        Ok(())
    }

    pub fn key_binding(&self, purpose: &str, signer_id: &str) -> Option<&KeyBinding> {
        self.key_bindings
            .iter()
            .find(|key| key.purpose == purpose && key.signer_id == signer_id)
    }
}

impl BatchGrant {
    pub fn validate(
        &self,
        installation: &InstallationConfig,
        policy: &Policy,
    ) -> Result<(), String> {
        if self.schema_version != GRANT_SCHEMA {
            return Err("unsupported grant schema_version".to_owned());
        }
        validate_id(&self.grant_id).map_err(|error| error.to_string())?;
        if self.installation_id != installation.installation_id
            || self.deployment_id != installation.deployment_id
        {
            return Err("grant installation/deployment binding mismatch".to_owned());
        }
        if installation.caller(self.caller_uid).is_none() {
            return Err("grant caller is not installed".to_owned());
        }
        validate_id(&self.network_id).map_err(|error| error.to_string())?;
        validate_id(&self.task_uid).map_err(|error| error.to_string())?;
        if self.source_head_oid.len() != 40
            || !self
                .source_head_oid
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
        {
            return Err("grant source_head_oid is malformed".to_owned());
        }
        if self.policy_revision != policy.policy_revision {
            return Err("grant policy_revision does not match current policy".to_owned());
        }
        if self.not_before >= self.expires_at {
            return Err("grant validity interval is empty or reversed".to_owned());
        }
        if self.approval_record_ref.trim().is_empty() {
            return Err("approval_record_ref must not be empty".to_owned());
        }
        if self.items.is_empty() || self.items.len() > policy.limits.max_batch_items as usize {
            return Err("grant item count exceeds policy limits".to_owned());
        }
        for item in &self.items {
            validate_id(&item.purpose).map_err(|error| error.to_string())?;
            validate_id(&item.signer_id).map_err(|error| error.to_string())?;
            validate_sha256(&item.public_key_sha256)?;
            validate_sha256(&item.operation_key)?;
            validate_sha256(&item.payload_sha256)?;
            if item.max_distinct_requests == 0
                || item.max_distinct_requests > policy.limits.max_distinct_requests_per_item
            {
                return Err("grant request budget exceeds policy limits".to_owned());
            }
            if let Some(provider_id) = &item.provider_id.0 {
                validate_id(provider_id).map_err(|error| error.to_string())?;
            }
            for value in [
                &item.protocol_bindings.authority_id,
                &item.protocol_bindings.rollback_ticket,
                &item.protocol_bindings.receipt_id,
                &item.protocol_bindings.nonce,
            ] {
                validate_id(value).map_err(|error| error.to_string())?;
            }
        }
        Ok(())
    }
}

pub fn validate_sha256(value: &str) -> Result<(), String> {
    if value.len() != 64
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err("SHA-256 digest must be 64 lowercase hexadecimal characters".to_owned());
    }
    Ok(())
}

pub fn validate_absolute_path(value: &str) -> Result<(), String> {
    if !std::path::Path::new(value).is_absolute()
        || value
            .split('/')
            .skip(1)
            .any(|part| part.is_empty() || part == "." || part == "..")
        || value.contains('\0')
    {
        return Err("path must be absolute without dot or parent components".to_owned());
    }
    Ok(())
}
