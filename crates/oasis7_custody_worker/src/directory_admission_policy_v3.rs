//! Closed ordinary Directory adapter: full historical metadata validation only.
//! No Current value, first handle or replacement business record is produced.
use crate::directory_config::{DirectoryConfig, denied, digest};
use oasis7_custody_core::{provisioning::ManagerPrincipal, read_directory_admission_v3::*};
use serde::Deserialize;
use worker::{Env, Result};
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Record {
    id: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Row {
    id: String,
    payload: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Profile {
    birth: Vec<Row>,
    header: Vec<Row>,
    once: Vec<Row>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Inventory {
    records: Vec<Record>,
    execution: Option<serde_json::Value>,
    read: Option<Profile>,
}
pub(crate) struct DirectoryAdmissionPolicyV3 {
    pub(crate) execution_capacity: u32,
    pub(crate) read: Option<ReadDirectoryAdmissionConfigV3>,
    pub(crate) row_bound: usize,
}
impl DirectoryAdmissionPolicyV3 {
    pub(crate) fn load(env: &Env) -> Result<Self> {
        let execution_capacity = match env.var("CUSTODY_EXECUTION_DIRECTORY_ADMISSION_CAPACITY") {
            Ok(raw) => {
                let raw = raw.to_string();
                let parsed = raw.parse::<u32>().map_err(|_| denied())?;
                if parsed == 0 || parsed.to_string() != raw {
                    return Err(denied());
                }
                parsed
            }
            Err(_) => 0,
        };
        let configured = [
            "CUSTODY_READ_DIRECTORY_ADMISSION_CAPACITY",
            "CUSTODY_READ_DIRECTORY_ADMISSION_VERIFIER_HISTORY",
            "CUSTODY_READ_DIRECTORY_ADMISSION_ACTIVE_VERIFIER",
        ]
        .iter()
        .any(|key| env.var(key).is_ok());
        let enabled = env
            .var("CUSTODY_READ_DIRECTORY_V3_ENABLED")
            .ok()
            .is_some_and(|v| v.to_string() == "true");
        let read = if configured || enabled {
            Some(crate::read_directory_config_v3::configuration(env)?)
        } else {
            None
        };
        Ok(Self {
            execution_capacity,
            read,
            row_bound: read_directory_scan_row_bound_v3().map_err(|_| denied())?,
        })
    }
    pub(crate) fn read_capacity(&self) -> u32 {
        self.read.as_ref().map(|c| c.capacity.0 as u32).unwrap_or(0)
    }
    pub(crate) fn verify(
        &self,
        env: &Env,
        actual_object: &str,
        raw: &str,
    ) -> Result<Option<ReadDirectoryRowsV3>> {
        let bound = usize::try_from(self.execution_capacity)
            .map_err(|_| denied())?
            .checked_mul(32768 * 2 + 512)
            .and_then(|n| {
                usize::try_from(self.read_capacity())
                    .ok()
                    .and_then(|c| c.checked_mul(self.row_bound.checked_mul(2)?.checked_add(512)?))
                    .and_then(|m| n.checked_add(m))
            })
            .and_then(|n| n.checked_add(self.row_bound.checked_mul(8)?))
            .ok_or_else(denied)?;
        if raw.len() > bound {
            return Err(denied());
        }
        let inventory: Inventory = serde_json::from_str(raw).map_err(|_| denied())?;
        if inventory.records.len() > 3
            || inventory.records.iter().any(|r| {
                !matches!(
                    r.id.as_str(),
                    "manager-directory-v1"
                        | "manager-directory-pending-birth-v2"
                        | READ_DIRECTORY_BIRTH_KEY_V3
                )
            })
        {
            return Err(denied());
        }
        if let Some(execution) = inventory.execution {
            if self.execution_capacity == 0 {
                return Err(denied());
            }
            crate::directory_execution_history_v2::historical_inventory(
                env,
                actual_object,
                &serde_json::to_string(&execution)?,
            )
            .map_err(|_| denied())?;
        }
        let Some(profile) = inventory.read else {
            return Ok(None);
        };
        let config = self.read.as_ref().ok_or_else(denied)?;
        if profile.birth.len() != 1
            || profile.birth[0].id != READ_DIRECTORY_BIRTH_KEY_V3
            || profile.header.len() != 1
            || profile.header[0].id != "singleton"
        {
            return Err(denied());
        }
        let birth = ReadDirectoryBirthV3::decode_storage(profile.birth[0].payload.as_bytes())
            .map_err(|_| denied())?;
        let scope = &birth.scope;
        let directory = DirectoryConfig::from_env(env)?.ok_or_else(denied)?;
        if scope.object_id != actual_object
            || scope.environment != directory.environment
            || scope.manager_identity_namespace != directory.namespace
            || scope.manager_identity_instance_epoch != directory.epoch
        {
            return Err(denied());
        }
        let principal = ManagerPrincipal {
            identity_namespace: scope.manager_identity_namespace.clone(),
            native_account_id: scope.manager_account_id.clone(),
        };
        let name = digest(
            "oasis7.custody.manager-directory.v1",
            &(&directory.environment, &principal),
        )?;
        let actual = env
            .durable_object("MANAGER_ASSOCIATIONS")?
            .id_from_name(&name)?
            .to_string();
        if actual != actual_object {
            return Err(denied());
        }
        let claims = profile
            .once
            .into_iter()
            .map(|r| (r.id, r.payload))
            .collect::<Vec<_>>();
        let rows = ReadDirectoryRowsV3::decode_storage_rows(
            profile.birth[0].payload.as_bytes(),
            profile.header[0].payload.as_bytes(),
            &claims,
            scope,
            config,
        )
        .map_err(|_| denied())?;
        rows.verify(config, scope, |account| {
            env.durable_object("ACCOUNTS")
                .and_then(|n| n.id_from_name(account).map(|id| id.to_string()))
                .map_err(|_| "read directory account binding rejected".into())
        })
        .map_err(|_| denied())?;
        Ok(Some(rows))
    }
}
