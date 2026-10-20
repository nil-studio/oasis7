//! Local immutable management request preimage. This is orchestration intent,
//! not a native effect receipt, governance proof, authority or world trust.
use super::{actions, local_io as io, model::NodeConfig};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ManagementRequestBinding {
    pub api_version: String,
    pub operation_id: String,
    pub step_id: String,
    pub action_id: String,
    pub request_digest: String,
    pub request_json: String,
}
impl ManagementRequestBinding {
    fn validate(&self) -> Result<(), String> {
        if self.api_version != "oasis7.local-management-request/v1"
            || self.operation_id.is_empty()
            || self.operation_id.len() > 128
            || !matches!(self.step_id.as_str(), "start" | "stop")
            || self.action_id != format!("op:{}:{}", self.operation_id, self.step_id)
            || hex::encode(Sha256::digest(self.request_json.as_bytes())) != self.request_digest
        {
            return Err("invalid immutable management request binding".into());
        }
        serde_json::from_str::<serde_json::Value>(&self.request_json).map_err(|e| e.to_string())?;
        Ok(())
    }
}
fn location(data: &Path, id: &str) -> Result<PathBuf, String> {
    if id.is_empty() || id.len() > 256 {
        return Err("management action identity bound".into());
    }
    Ok(data.join("management-requests").join(format!(
        "{}.json",
        hex::encode(Sha256::digest(id.as_bytes()))
    )))
}
pub(crate) fn read(
    config_path: &Path,
    id: &str,
) -> Result<Option<ManagementRequestBinding>, String> {
    let config = NodeConfig::load(config_path)?;
    let path = location(&config.identity.data_dir, id)?;
    if !path.exists() {
        io::reject_symlinks(&path)?;
        return Ok(None);
    }
    super::super::controlled_live_config::require_private_owned(&path, false)?;
    let binding: ManagementRequestBinding = io::read(&path)?;
    binding.validate()?;
    if binding.action_id != id {
        return Err("management binding action identity mismatch".into());
    }
    Ok(Some(binding))
}
pub(crate) fn bind(config_path: &Path, binding: &ManagementRequestBinding) -> Result<(), String> {
    binding.validate()?;
    let config = NodeConfig::load(config_path)?;
    let data = &config.identity.data_dir;
    let _admission = super::process::admission_lock(data)?;
    if let Some(original) = read(config_path, &binding.action_id)? {
        if original != *binding {
            return Err("management action already bound to another immutable request".into());
        }
        return Ok(());
    }
    // Never invent a parent goal after an effect was admitted. Stop and
    // Restart share a stop effect but have different immutable final goals.
    if !actions::admission_absent(config_path, &binding.action_id)? {
        return Err("native admission exists without a management request binding; retroactive adoption refused".into());
    }
    io::private_dir(&data.join("management-requests"))?;
    io::write(&location(data, &binding.action_id)?, binding)
}
