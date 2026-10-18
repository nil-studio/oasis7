//! Fixed local tool adapters. No shell, privileged store reads, or caller-selected executable.
pub mod cli;
pub mod jobs;
mod signing;
use signing::compatible_info;
pub use signing::{
    file_prepare_identity, file_prepare_resource, prepare_file, prepare_file_frozen,
    reconcile_file_prepare, reconcile_signer_job, recover_file_prepare, selected_file_digest,
    signer_job_identity, signer_submit_identity, submit_file_frozen, verify_file_signature,
};
pub mod secrets;
use crate::{Error, Result};
use oasis7_local_signer::installation;
use oasis7_signer_protocol::{identity, protocol};
use serde::{Deserialize, Serialize};
use std::{
    fs,
    path::{Path, PathBuf},
};
use ts_rs::TS;
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct ToolObservation {
    pub tool_id: String,
    pub state: String,
    pub path: Option<String>,
    pub reason: Option<String>,
    pub observed_at: String,
    #[ts(type = "unknown")]
    pub info: Option<serde_json::Value>,
}
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct FileSignInput {
    pub job_id: String,
    pub file_path: String,
    pub signer_id: String,
    pub grant_id: String,
    #[ts(type = "unknown")]
    pub context: serde_json::Value,
}
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, ts_rs::TS)]
pub struct FilePrepareRecovery {
    pub operation_id: String,
    pub input: FileSignInput,
    pub prepared: FileSignPrepared,
}
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct FileSignPrepared {
    pub job_id: String,
    pub prepared_fingerprint: String,
    pub file_sha256: String,
    pub payload_sha256: String,
    pub operation_key: String,
    pub work_dir: String,
    #[ts(type = "unknown")]
    pub result: serde_json::Value,
}
/// Only a read-only preflight failure can permit editing and a new operation.
/// Errors after preparing starts, existing operations and interrupted work stay unknown.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum FilePrepareEffect {
    None,
    Unknown,
}
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct FilePrepareError {
    pub message: String,
    pub effect: FilePrepareEffect,
}
impl FilePrepareError {
    pub fn no_effect(message: impl std::fmt::Display) -> Self {
        Self {
            message: message.to_string(),
            effect: FilePrepareEffect::None,
        }
    }
    pub fn unknown(message: impl std::fmt::Display) -> Self {
        Self {
            message: message.to_string(),
            effect: FilePrepareEffect::Unknown,
        }
    }
}
/// Atomically claim a rejected invocation before proving that its ID cannot
/// also start preparing elsewhere. No signer job or files are touched here.
pub fn record_prepare_validation_failure(
    jobs: &jobs::ToolJobs,
    operation_id: &str,
    input: &FileSignInput,
    target_resource: Option<&str>,
    error: impl std::fmt::Display,
) -> FilePrepareError {
    // A valid Job cannot be declared unaffected when its installation identity
    // is unknown. Invalid identifiers cannot refer to a legitimate caller Job.
    let resource = match target_resource {
        Some(resource) => resource.to_owned(),
        None if protocol::validate_id(&input.job_id).is_err() => {
            format!("prepare-validation:{operation_id}")
        }
        None => return FilePrepareError::unknown("无法核对原 Job 的安装身份；请核对原操作"),
    };
    let input_bytes = match serde_json::to_vec(input) {
        Ok(bytes) => bytes,
        Err(error) => return FilePrepareError::unknown(error),
    };
    let fingerprint = format!("prepare-validation:{}", identity::sha256_hex(&input_bytes));
    let admission = jobs.admit_once(operation_id, &fingerprint, &resource, "file_prepare");
    match admission {
        Ok((_, true)) => match jobs.finish(operation_id, "failed", None) {
            Ok(_) => FilePrepareError::no_effect(error),
            Err(error) => FilePrepareError::unknown(error),
        },
        Ok((job, false)) => FilePrepareError::unknown(format!(
            "原操作 {} 状态为 {}；请核对原操作",
            job.id, job.state
        )),
        Err(error) => FilePrepareError::unknown(error),
    }
}
fn invalid(s: impl ToString) -> Error {
    Error::Invalid(s.to_string())
}
fn binding() -> Result<(oasis7_signer_protocol::types::InstallationConfig, PathBuf)> {
    let cfg = installation::load_fixed_installation_config().map_err(invalid)?;
    let uid = installation::current_uid();
    if installation::current_euid() != uid || cfg.caller(uid).is_none() {
        return Err(invalid("App is not the bound ordinary caller"));
    }
    let caller = Path::new(&cfg.worker_executable).with_file_name("oasis7_local_signer");
    trusted_file(&caller)?;
    let manifest_path = caller.with_file_name("manifest.json");
    trusted_file(&manifest_path)?;
    let manifest: serde_json::Value = serde_json::from_slice(&fs::read(manifest_path)?)?;
    if manifest["release_id"] != cfg.release_id {
        return Err(invalid("release identity drift"));
    }
    let entry = manifest["files"]
        .as_array()
        .and_then(|a| a.iter().find(|x| x["name"] == "oasis7_local_signer"))
        .ok_or_else(|| invalid("caller absent from installed release manifest"))?;
    if entry["sha256"] != identity::sha256_hex(&fs::read(&caller)?) {
        return Err(invalid("caller release digest drift"));
    }
    Ok((cfg, caller))
}
fn trusted_file(path: &Path) -> Result<()> {
    if !path.is_absolute() {
        return Err(invalid("absolute installed path required"));
    }
    let mut current = PathBuf::new();
    for part in path.components() {
        current.push(part);
        let m = fs::symlink_metadata(&current)?;
        if m.file_type().is_symlink() {
            return Err(invalid("installed path contains symlink"));
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::{MetadataExt, PermissionsExt};
            if m.uid() != 0 || m.permissions().mode() & 0o022 != 0 {
                return Err(invalid("installed path is not root controlled"));
            }
        }
    }
    if !fs::metadata(path)?.is_file() {
        return Err(invalid("installed program is not a regular file"));
    }
    Ok(())
}
pub fn observe_signer() -> ToolObservation {
    let mut o = ToolObservation {
        tool_id: "oasis7_local_signer".into(),
        state: "not_installed".into(),
        path: None,
        reason: None,
        observed_at: crate::now(),
        info: None,
    };
    match binding() {
        Err(e) => {
            o.state = match fs::symlink_metadata(installation::INSTALLATION_CONFIG_PATH) {
                Err(ref error) if error.kind() == std::io::ErrorKind::NotFound => "not_installed",
                _ => "needs_configuration",
            }
            .into();
            o.reason = Some(e.to_string());
        }
        Ok((_, p)) => {
            o.path = Some(p.display().to_string());
            match crate::process::run(p.to_str().unwrap(), &["info", "--json"], Path::new("/")) {
                Ok(raw) => match serde_json::from_slice::<serde_json::Value>(&raw) {
                    Ok(v) if compatible_info(&v) => {
                        o.state = "available".into();
                        o.info = Some(v)
                    }
                    _ => {
                        o.state = "incompatible".into();
                        o.reason = Some("Unknown capability schema".into())
                    }
                },
                Err(_) => {
                    o.state = "incompatible".into();
                    o.reason = Some("Installed caller has no verified info capability".into())
                }
            }
        }
    }
    o
}
pub fn signer_action(action: &str, job_id: Option<&str>) -> Result<serde_json::Value> {
    let (_, p) = binding()?;
    let info: serde_json::Value = serde_json::from_slice(&crate::process::run(
        p.to_str().unwrap(),
        &["info", "--json"],
        Path::new("/"),
    )?)?;
    if !compatible_info(&info) {
        return Err(invalid(
            "Installed caller protocol or file capability is incompatible",
        ));
    }
    let args = match action {
        "doctor" if job_id.is_none() => vec!["doctor"],
        "prepare" | "submit" | "inspect" => {
            let id = job_id.ok_or_else(|| invalid("job ID required"))?;
            protocol::validate_id(id).map_err(invalid)?;
            vec![action, "--job-id", id]
        }
        _ => return Err(invalid("unsupported caller action")),
    };
    Ok(serde_json::from_slice(&crate::process::run(
        p.to_str().unwrap(),
        &args,
        Path::new("/"),
    )?)?)
}
