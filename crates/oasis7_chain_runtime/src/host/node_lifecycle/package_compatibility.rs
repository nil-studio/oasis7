//! Immutable software/data compatibility evidence for a package operation.
//!
//! The prior and candidate executables produce the descriptors. This module
//! binds each descriptor to the executable bytes and the exact data cut read
//! while staging; it does not grant chain or package authority.

use super::super::local_io as io;
use super::{LOCAL_NATIVE_PROFILE, PackageUpgradeV1};
use oasis7_client_api::world_service_full_sync::FormalFullSyncCutV1;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
#[cfg(unix)]
use std::os::unix::fs::OpenOptionsExt;
#[cfg(not(test))]
use std::process::Command;
use std::{
    fs::{self, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
};

const PLAN_API: &str = "oasis7.package-runtime-compatibility-plan/v1";
const DESCRIPTOR_API: &str = "oasis7.node-runtime-compatibility/v1";

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct RuntimeCompatibilityPlanV1 {
    pub api_version: String,
    pub profile: String,
    pub operation_id: String,
    pub config_path: PathBuf,
    pub data_dir: PathBuf,
    pub prior_runtime_path: PathBuf,
    pub prior_runtime_sha256: String,
    pub prior_descriptor: Value,
    pub prior_descriptor_sha256: String,
    pub candidate_runtime_path: PathBuf,
    pub candidate_runtime_sha256: String,
    pub candidate_descriptor: Value,
    pub candidate_descriptor_sha256: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub minimum_full_sync_cut: Option<FormalFullSyncCutV1>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub minimum_full_sync_cut_sha256: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub minimum_full_sync_cut_bytes: Option<String>,
}

pub(crate) struct PersistedPlan {
    pub path: PathBuf,
    pub sha256: String,
}

struct RuntimePair {
    prior_runtime_path: PathBuf,
    prior_runtime_sha256: String,
    prior_descriptor: Value,
    candidate_runtime_path: PathBuf,
    candidate_runtime_sha256: String,
    candidate_descriptor: Value,
    minimum_full_sync_cut: Option<FormalFullSyncCutV1>,
    minimum_full_sync_cut_sha256: Option<String>,
    minimum_full_sync_cut_bytes: Option<String>,
}

struct MinimumCutFile {
    path: PathBuf,
    sha256: String,
    bytes: String,
}

impl Drop for MinimumCutFile {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
    }
}

/// Check a candidate binary against the currently selected executable and
/// exact persisted data without writing a package plan or changing node state.
pub(crate) fn preflight(
    config_path: &Path,
    data_dir: &Path,
    request: &PackageUpgradeV1,
) -> Result<(), String> {
    inspect_pair(config_path, data_dir, request).map(|_| ())
}

pub(crate) fn capture(
    config_path: &Path,
    data_dir: &Path,
    request: &PackageUpgradeV1,
    operation_id: &str,
) -> Result<PersistedPlan, String> {
    let pair = inspect_pair(config_path, data_dir, request)?;
    let config_path = io::regular(config_path)?;
    let data_dir = fs::canonicalize(data_dir).map_err(|error| error.to_string())?;

    let plan_path = plan_path(&data_dir, operation_id)?;
    if plan_path.exists() {
        let plan: RuntimeCompatibilityPlanV1 = io::read(&plan_path)?;
        validate_plan(&plan)?;
        if plan.operation_id != operation_id
            || plan.config_path != config_path
            || plan.data_dir != data_dir
            || plan.prior_runtime_path != pair.prior_runtime_path
            || plan.prior_runtime_sha256 != pair.prior_runtime_sha256
            || plan.candidate_runtime_path != pair.candidate_runtime_path
            || plan.candidate_runtime_sha256 != pair.candidate_runtime_sha256
            || plan.minimum_full_sync_cut != pair.minimum_full_sync_cut
            || plan.minimum_full_sync_cut_sha256 != pair.minimum_full_sync_cut_sha256
            || plan.minimum_full_sync_cut_bytes != pair.minimum_full_sync_cut_bytes
        {
            return Err(
                "persisted runtime compatibility plan differs from the exact package retry".into(),
            );
        }
        return Ok(PersistedPlan {
            sha256: io::sha256(&plan_path)?,
            path: plan_path,
        });
    }

    let plan = RuntimeCompatibilityPlanV1 {
        api_version: PLAN_API.into(),
        profile: request.profile.clone(),
        operation_id: operation_id.to_string(),
        config_path,
        data_dir: data_dir.clone(),
        prior_runtime_path: pair.prior_runtime_path,
        prior_runtime_sha256: pair.prior_runtime_sha256,
        prior_descriptor_sha256: io::digest(&pair.prior_descriptor)?,
        prior_descriptor: pair.prior_descriptor,
        candidate_runtime_path: pair.candidate_runtime_path,
        candidate_runtime_sha256: pair.candidate_runtime_sha256,
        candidate_descriptor_sha256: io::digest(&pair.candidate_descriptor)?,
        candidate_descriptor: pair.candidate_descriptor,
        minimum_full_sync_cut: pair.minimum_full_sync_cut,
        minimum_full_sync_cut_sha256: pair.minimum_full_sync_cut_sha256,
        minimum_full_sync_cut_bytes: pair.minimum_full_sync_cut_bytes,
    };
    persist(&data_dir, operation_id, &plan)
}

fn inspect_pair(
    config_path: &Path,
    data_dir: &Path,
    request: &PackageUpgradeV1,
) -> Result<RuntimePair, String> {
    if request.profile != LOCAL_NATIVE_PROFILE {
        return Err(
            "runtime compatibility staging requires the local-native package adapter".into(),
        );
    }
    let candidate = request
        .native_artifact
        .as_ref()
        .ok_or("local-native runtime compatibility plan is missing its candidate binary")?;
    let config_path = io::regular(config_path)?;
    let data_dir = fs::canonicalize(data_dir).map_err(|error| error.to_string())?;
    if !data_dir.is_absolute() {
        return Err("runtime compatibility data directory must be absolute".into());
    }
    let prior_snapshot = super::preimage::loaded_image_snapshot()?;
    let prior_runtime_path = prior_snapshot.path.clone();
    let prior_runtime_sha256 = prior_snapshot.sha256.clone();
    let candidate_snapshot = super::preimage::file_snapshot(&candidate.runtime_binary)?;
    let candidate_runtime_path = candidate_snapshot.path.clone();
    let candidate_runtime_sha256 = candidate_snapshot.sha256.clone();
    if candidate_runtime_sha256 != candidate.runtime_sha256 {
        return Err("candidate runtime changed before compatibility descriptor capture".into());
    }
    let selected_snapshot = super::preimage::current_runtime_snapshot(request)?;
    if prior_snapshot.path != selected_snapshot.path
        || prior_snapshot.device != selected_snapshot.device
        || prior_snapshot.inode != selected_snapshot.inode
        || prior_snapshot.sha256 != selected_snapshot.sha256
    {
        return Err(
            "compatibility stage executable is not the exact currently selected runtime".into(),
        );
    }
    let prior_descriptor = run_descriptor(
        "prior",
        &prior_runtime_path,
        &config_path,
        &data_dir,
        &prior_runtime_sha256,
        None,
    )?;
    if super::preimage::loaded_image_snapshot()? != prior_snapshot
        || super::preimage::file_snapshot(&prior_runtime_path)? != prior_snapshot
        || super::preimage::current_runtime_snapshot(request)? != selected_snapshot
    {
        return Err("selected prior runtime changed while compatibility was being probed".into());
    }
    let minimum_full_sync_cut = if prior_descriptor["runtime_profile"]
        == super::super::formal_full_sync::PROFILE
    {
        let cut: FormalFullSyncCutV1 =
            serde_json::from_value(prior_descriptor["stored_state_probe"]["current_cut"].clone())
                .map_err(|error| {
                format!("full-sync prior descriptor omitted its verified current cut: {error}")
            })?;
        cut.validate()?;
        Some(cut)
    } else {
        None
    };
    let minimum_cut_file = minimum_full_sync_cut
        .as_ref()
        .map(create_minimum_cut_file)
        .transpose()?;
    let candidate_descriptor = run_descriptor(
        "candidate",
        &candidate_runtime_path,
        &config_path,
        &data_dir,
        &candidate_runtime_sha256,
        minimum_cut_file.as_ref(),
    )?;
    if super::preimage::file_snapshot(&candidate_runtime_path)? != candidate_snapshot
        || super::preimage::loaded_image_snapshot()? != prior_snapshot
        || super::preimage::file_snapshot(&prior_runtime_path)? != prior_snapshot
        || super::preimage::current_runtime_snapshot(request)? != selected_snapshot
    {
        return Err("package runtime path changed while compatibility was being probed".into());
    }
    if let (Some(cut), Some(file)) = (&minimum_full_sync_cut, &minimum_cut_file)
        && (candidate_descriptor["minimum_full_sync_cut_sha256"] != file.sha256
            || candidate_descriptor["stored_state_probe"]["minimum_cut_verified"] != true
            || candidate_descriptor["stored_state_probe"]["current_cut"]
                != serde_json::to_value(cut).map_err(|error| error.to_string())?)
    {
        return Err("candidate executable did not verify the exact staged full-sync cut".into());
    }
    let mut prior_probe = prior_descriptor["stored_state_probe"].clone();
    let mut candidate_probe = candidate_descriptor["stored_state_probe"].clone();
    for probe in [&mut prior_probe, &mut candidate_probe] {
        if let Some(object) = probe.as_object_mut() {
            object.remove("minimum_cut");
            object.remove("minimum_cut_verified");
            if let Some(history) = object.get_mut("history").and_then(Value::as_object_mut) {
                history.remove("minimum_cut");
                history.remove("minimum_cut_verified");
            }
        }
    }
    if prior_descriptor["runtime_profile"] != candidate_descriptor["runtime_profile"]
        || prior_descriptor["world_id"] != candidate_descriptor["world_id"]
        || prior_descriptor["chain_id"] != candidate_descriptor["chain_id"]
        || prior_descriptor["config_digest"] != candidate_descriptor["config_digest"]
        || prior_descriptor["config_sha256"] != candidate_descriptor["config_sha256"]
        || prior_probe != candidate_probe
    {
        return Err(
            "prior and candidate executables did not verify the same fixed config, World identity, and stored state".into(),
        );
    }
    require_descriptor_reads_its_probe(&prior_descriptor)?;
    require_descriptor_reads_its_probe(&candidate_descriptor)?;
    Ok(RuntimePair {
        prior_runtime_path,
        prior_runtime_sha256,
        prior_descriptor,
        candidate_runtime_path,
        candidate_runtime_sha256,
        candidate_descriptor,
        minimum_full_sync_cut,
        minimum_full_sync_cut_sha256: minimum_cut_file.as_ref().map(|file| file.sha256.clone()),
        minimum_full_sync_cut_bytes: minimum_cut_file.as_ref().map(|file| file.bytes.clone()),
    })
}

fn persist(
    data_dir: &Path,
    operation_id: &str,
    plan: &RuntimeCompatibilityPlanV1,
) -> Result<PersistedPlan, String> {
    let root = data_dir.join("package-runtime-compatibility");
    io::private_dir(&root)?;
    let path = root.join(format!("{}.json", safe_token(operation_id)?));
    if path.exists() {
        let existing: RuntimeCompatibilityPlanV1 = io::read(&path)?;
        if existing != *plan {
            return Err(
                "persisted runtime compatibility plan conflicts with this package request".into(),
            );
        }
    } else {
        io::write(&path, plan)?;
    }
    let sha256 = io::sha256(&path)?;
    Ok(PersistedPlan { path, sha256 })
}

pub(crate) fn load_exact(
    path: &Path,
    expected_sha256: &str,
) -> Result<RuntimeCompatibilityPlanV1, String> {
    let canonical = io::regular(path)?;
    if canonical != path {
        return Err("runtime compatibility plan path changed or contains a symlink".into());
    }
    let plan: RuntimeCompatibilityPlanV1 = io::read(path)?;
    validate_plan(&plan)?;
    if io::sha256(path)? != expected_sha256 {
        return Err("runtime compatibility plan digest differs from the immutable stage".into());
    }
    Ok(plan)
}

pub(crate) fn probe_prior_against_current_data(
    plan: &RuntimeCompatibilityPlanV1,
    config_path: &Path,
    data_dir: &Path,
) -> Result<Value, String> {
    validate_plan(plan)?;
    if io::sha256(&plan.prior_runtime_path)? != plan.prior_runtime_sha256 {
        return Err("prior executable bytes differ from the immutable compatibility plan".into());
    }
    let config_path = io::regular(config_path)?;
    let data_dir = fs::canonicalize(data_dir).map_err(|error| error.to_string())?;
    if config_path != plan.config_path || data_dir != plan.data_dir {
        return Err(
            "rollback compatibility probe uses a different config or data directory".into(),
        );
    }
    let minimum_cut_file = plan
        .minimum_full_sync_cut
        .as_ref()
        .zip(plan.minimum_full_sync_cut_sha256.as_deref())
        .zip(plan.minimum_full_sync_cut_bytes.as_deref())
        .map(|((cut, expected_sha), exact_bytes)| {
            create_minimum_cut_file_from_plan(cut, exact_bytes, expected_sha)
        })
        .transpose()?;
    let current = run_descriptor(
        "prior-recheck",
        &plan.prior_runtime_path,
        &config_path,
        &data_dir,
        &plan.prior_runtime_sha256,
        minimum_cut_file.as_ref(),
    )?;
    if let Some(minimum) = &plan.minimum_full_sync_cut {
        let minimum_json = serde_json::to_value(minimum).map_err(|error| error.to_string())?;
        if current["minimum_full_sync_cut_sha256"].as_str()
            != plan.minimum_full_sync_cut_sha256.as_deref()
            || current["stored_state_probe"]["minimum_cut_verified"] != true
            || current["stored_state_probe"]["minimum_cut"] != minimum_json
            || !cut_is_at_or_after(&minimum_json, &current["stored_state_probe"]["current_cut"])?
        {
            return Err("prior executable cannot prove the current full-sync data is at least the immutable staged cut".into());
        }
    }
    if current["runtime_profile"] != plan.prior_descriptor["runtime_profile"]
        || current["world_id"] != plan.prior_descriptor["world_id"]
        || current["chain_id"] != plan.prior_descriptor["chain_id"]
        || current["software_formats"] != plan.prior_descriptor["software_formats"]
    {
        return Err("prior executable compatibility descriptor differs from the staged executable/data identity".into());
    }
    require_descriptor_reads_its_probe(&current)?;
    Ok(current)
}

pub(crate) fn validate_stage_receipt(
    stage: &super::PackageStageReceiptV1,
    path: &Path,
    sha256: &str,
) -> Result<(), String> {
    if stage.runtime_compatibility_plan_path.as_deref() != Some(path)
        || stage.runtime_compatibility_plan_sha256.as_deref() != Some(sha256)
    {
        return Err(
            "package stage receipt differs from the immutable runtime compatibility plan".into(),
        );
    }
    let plan = load_exact(path, sha256)?;
    if stage.profile != LOCAL_NATIVE_PROFILE
        || plan.profile != stage.profile
        || plan.operation_id != stage.operation_id
    {
        return Err(
            "package stage compatibility plan has a different profile or operation identity".into(),
        );
    }
    Ok(())
}

fn run_descriptor(
    phase: &'static str,
    executable: &Path,
    config_path: &Path,
    data_dir: &Path,
    expected_sha256: &str,
    minimum_cut: Option<&MinimumCutFile>,
) -> Result<Value, String> {
    let config_file_sha256 = io::sha256(config_path)?;
    #[cfg(test)]
    let descriptor =
        test_support::run_descriptor_child(phase, executable, config_path, data_dir, minimum_cut)?;
    #[cfg(not(test))]
    let descriptor = {
        let _ = phase;
        let mut command = Command::new(executable);
        command
            .arg("node")
            .arg("runtime-compatibility")
            .arg("--config")
            .arg(config_path)
            .arg("--data-dir")
            .arg(data_dir);
        if let Some(minimum_cut) = minimum_cut {
            command
                .arg("--minimum-full-sync-cut")
                .arg(&minimum_cut.path)
                .arg("--minimum-full-sync-cut-sha256")
                .arg(&minimum_cut.sha256);
        }
        let output = command
            .output()
            .map_err(|error| format!("run executable compatibility descriptor: {error}"))?;
        if !output.status.success() {
            return Err(format!(
                "executable compatibility probe failed ({}): {}",
                output.status,
                descriptor_failure_diagnostic(&output.stderr, &output.stdout)
            ));
        }
        serde_json::from_slice(&output.stdout)
            .map_err(|error| format!("parse executable compatibility descriptor: {error}"))?
    };
    if io::sha256(config_path)? != config_file_sha256 {
        return Err(
            "node configuration changed while an executable compatibility probe was running".into(),
        );
    }
    let config = super::super::model::NodeConfig::load(config_path)?;
    let config_digest = io::digest(&config)?;
    if io::sha256(config_path)? != config_file_sha256 {
        return Err("node configuration changed while an executable compatibility descriptor was being validated".into());
    }
    validate_descriptor(
        &descriptor,
        expected_sha256,
        config_path,
        &config_digest,
        &config_file_sha256,
        data_dir,
        minimum_cut.map(|cut| cut.sha256.as_str()),
    )?;
    Ok(descriptor)
}

#[cfg(test)]
mod test_support {
    use super::{MinimumCutFile, Value, io};
    use serde::{Deserialize, Serialize};
    use std::{
        fs::{self, OpenOptions},
        io::Write,
        os::unix::fs::OpenOptionsExt,
        path::{Path, PathBuf},
        process::{Command, Stdio},
    };

    const INPUT_ENV: &str = "OASIS7_PACKAGE_COMPATIBILITY_CHILD_INPUT";
    const MANAGER_PROGRESS_ENV: &str = "OASIS7_TEST_PACKAGE_MANAGER_PROGRESS";
    const TEST_ENTRY: &str =
        "node_lifecycle::package::compatibility::test_support::descriptor_child_entry";

    fn mark_manager_progress(phase: &str) {
        let Some(path) = std::env::var_os(MANAGER_PROGRESS_ENV) else {
            return;
        };
        if let Ok(mut file) = OpenOptions::new()
            .create(true)
            .append(true)
            .mode(0o600)
            .open(path)
        {
            let _ = writeln!(file, "{phase}");
            let _ = file.sync_all();
        }
    }

    fn parse_phase(value: &str) -> Result<&'static str, String> {
        match value {
            "prior" => Ok("prior"),
            "candidate" => Ok("candidate"),
            "prior-recheck" => Ok("prior-recheck"),
            _ => Err("descriptor child received an invalid phase label".into()),
        }
    }

    #[derive(Serialize, Deserialize)]
    struct ChildInput {
        phase: String,
        config_path: PathBuf,
        data_dir: PathBuf,
        minimum_cut_path: Option<PathBuf>,
        minimum_cut_sha256: Option<String>,
        output_path: PathBuf,
    }

    #[derive(Serialize, Deserialize)]
    struct ChildOutput {
        descriptor: Option<Value>,
        error: Option<String>,
    }

    /// Tests run inside libtest, so invoking the test binary with production
    /// CLI arguments would only exercise libtest's parser. Re-enter this exact
    /// executable as a dedicated child test and call the production descriptor
    /// decoder there. The executable path, loaded image, and bytes therefore
    /// remain the selected prior/candidate artifact under test.
    pub(super) fn run_descriptor_child(
        phase: &'static str,
        executable: &Path,
        config_path: &Path,
        data_dir: &Path,
        minimum_cut: Option<&MinimumCutFile>,
    ) -> Result<Value, String> {
        let root = fs::canonicalize(std::env::temp_dir()).map_err(|error| error.to_string())?;
        let work = root.join(format!(
            ".oasis7-package-descriptor-child-{}-{}",
            std::process::id(),
            io::random_id()?
        ));
        io::private_dir(&work)?;
        let input_path = work.join("input.json");
        let output_path = work.join("output.json");
        let input = ChildInput {
            phase: phase.to_string(),
            config_path: config_path.to_path_buf(),
            data_dir: data_dir.to_path_buf(),
            minimum_cut_path: minimum_cut.map(|cut| cut.path.clone()),
            minimum_cut_sha256: minimum_cut.map(|cut| cut.sha256.clone()),
            output_path: output_path.clone(),
        };
        io::write(&input_path, &input)?;
        mark_manager_progress(&format!("descriptor_child_started:{phase}"));
        let child = Command::new(executable)
            .args(["--exact", TEST_ENTRY, "--nocapture"])
            .env(INPUT_ENV, &input_path)
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .output()
            .map_err(|error| format!("run selected-image descriptor child: {error}"));
        let output = match child {
            Ok(output) => output,
            Err(error) => {
                let _ = fs::remove_dir_all(&work);
                return Err(error);
            }
        };
        mark_manager_progress(&format!(
            "descriptor_child_finished:{phase}:{}",
            if output.status.success() {
                "success"
            } else {
                "failed"
            }
        ));
        let result = if output_path.is_file() {
            let child_output: ChildOutput = io::read(&output_path)?;
            match (child_output.descriptor, child_output.error) {
                (Some(_), Some(_)) | (None, None) => {
                    Err("descriptor child returned an ambiguous result".to_string())
                }
                (Some(descriptor), None) => {
                    if output.status.success() {
                        let executable_path =
                            fs::canonicalize(executable).map_err(|error| error.to_string())?;
                        if descriptor["executable_path"] != executable_path.display().to_string() {
                            Err(
                                "descriptor child did not execute the selected artifact path"
                                    .into(),
                            )
                        } else {
                            Ok(descriptor)
                        }
                    } else {
                        Err(format!(
                            "selected-image descriptor child exited {} after returning a descriptor",
                            output.status
                        ))
                    }
                }
                (None, Some(error)) => Err(format!("selected-image descriptor failed: {error}")),
            }
        } else {
            Err(format!(
                "selected-image descriptor child produced no private result ({}): {}",
                output.status,
                String::from_utf8_lossy(&output.stderr).trim()
            ))
        };
        let _ = fs::remove_dir_all(&work);
        result
    }

    #[test]
    fn descriptor_child_entry() {
        let Some(input_path) = std::env::var_os(INPUT_ENV) else {
            return;
        };
        mark_manager_progress("descriptor_child_entry_started");
        let outcome = (|| {
            let input: ChildInput = io::read(Path::new(&input_path))?;
            let phase = parse_phase(&input.phase)?;
            mark_manager_progress(&format!("descriptor_input_read:{phase}"));
            mark_manager_progress(&format!("descriptor_probe_started:{phase}"));
            let result = super::super::super::runtime_compatibility::describe_with_minimum(
                &input.config_path,
                Some(&input.data_dir),
                input.minimum_cut_path.as_deref(),
                input.minimum_cut_sha256.as_deref(),
            );
            mark_manager_progress(&format!(
                "descriptor_probe_finished:{phase}:{}",
                if result.is_ok() { "success" } else { "failed" }
            ));
            let output = match result {
                Ok(descriptor) => ChildOutput {
                    descriptor: Some(descriptor),
                    error: None,
                },
                Err(error) => ChildOutput {
                    descriptor: None,
                    error: Some(error),
                },
            };
            mark_manager_progress(&format!("descriptor_result_write_started:{phase}"));
            io::write(&input.output_path, &output)?;
            mark_manager_progress(&format!("descriptor_result_write_finished:{phase}"));
            if let Some(error) = output.error {
                return Err(error);
            }
            Ok(())
        })();
        if let Err(error) = outcome {
            panic!("descriptor child failed: {error}");
        }
    }
}

fn validate_descriptor(
    descriptor: &Value,
    expected_sha256: &str,
    config_path: &Path,
    config_digest: &str,
    config_file_sha256: &str,
    data_dir: &Path,
    minimum_cut_sha256: Option<&str>,
) -> Result<(), String> {
    if descriptor["api_version"] != DESCRIPTOR_API
        || descriptor["executable_sha256"] != expected_sha256
        || descriptor["config_path"] != config_path.display().to_string()
        || descriptor["config_digest"] != config_digest
        || descriptor["config_sha256"] != config_file_sha256
        || descriptor["data_dir"] != data_dir.display().to_string()
        || descriptor["chain_authority_claim"] != false
        || descriptor["process_started"] != false
        || !matches!(
            descriptor["stored_state_probe"]["state"].as_str(),
            Some("verified" | "uninitialized")
        )
        || descriptor["minimum_full_sync_cut_sha256"]
            != minimum_cut_sha256.map(Value::from).unwrap_or(Value::Null)
    {
        return Err("executable compatibility descriptor is incomplete or not bound to the exact binary/data".into());
    }
    Ok(())
}

fn create_minimum_cut_file(cut: &FormalFullSyncCutV1) -> Result<MinimumCutFile, String> {
    cut.validate()?;
    let bytes = serde_json::to_vec_pretty(cut).map_err(|error| error.to_string())?;
    create_minimum_cut_file_bytes(bytes)
}

fn create_minimum_cut_file_from_plan(
    cut: &FormalFullSyncCutV1,
    exact_bytes: &str,
    expected_sha256: &str,
) -> Result<MinimumCutFile, String> {
    cut.validate()?;
    let decoded: FormalFullSyncCutV1 = serde_json::from_str(exact_bytes)
        .map_err(|error| format!("decode immutable full-sync minimum cut bytes: {error}"))?;
    let bytes = exact_bytes.as_bytes();
    if decoded != *cut
        || serde_json::to_vec_pretty(&decoded).map_err(|error| error.to_string())? != bytes
        || sha256_bytes(bytes) != expected_sha256
    {
        return Err(
            "immutable full-sync minimum cut bytes differ from the exact staged value or digest"
                .into(),
        );
    }
    create_minimum_cut_file_bytes(bytes.to_vec())
}

fn create_minimum_cut_file_bytes(bytes: Vec<u8>) -> Result<MinimumCutFile, String> {
    let directory = fs::canonicalize(std::env::temp_dir()).map_err(|error| error.to_string())?;
    let path = directory.join(format!(
        ".oasis7-full-sync-stage-cut-{}-{}.json",
        std::process::id(),
        io::random_id()?
    ));
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    options.mode(0o600);
    let mut file = options.open(&path).map_err(|error| error.to_string())?;
    file.write_all(&bytes).map_err(|error| error.to_string())?;
    file.sync_all().map_err(|error| error.to_string())?;
    Ok(MinimumCutFile {
        path,
        sha256: hex::encode(Sha256::digest(bytes.as_slice())),
        bytes: String::from_utf8(bytes).map_err(|error| error.to_string())?,
    })
}

fn sha256_bytes(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}

fn cut_is_at_or_after(previous: &Value, current: &Value) -> Result<bool, String> {
    let previous: FormalFullSyncCutV1 = serde_json::from_value(previous.clone())
        .map_err(|error| format!("decode previous full-sync cut: {error}"))?;
    let current: FormalFullSyncCutV1 = serde_json::from_value(current.clone())
        .map_err(|error| format!("decode current full-sync cut: {error}"))?;
    previous.validate()?;
    current.validate()?;
    Ok(previous.world == current.world
        && previous.chain_id == current.chain_id
        && (current.position > previous.position
            || (current.position == previous.position && current == previous)))
}

fn require_descriptor_reads_its_probe(descriptor: &Value) -> Result<(), String> {
    let formats = descriptor
        .get("software_formats")
        .ok_or("compatibility descriptor omitted software format support")?;
    let probe = descriptor
        .get("stored_state_probe")
        .ok_or("compatibility descriptor omitted stored-state probe")?;
    match descriptor["runtime_profile"].as_str() {
        Some(profile) if profile == super::super::model::PROFILE => {
            if probe["state"] == "verified" {
                let version = probe["world_format_version"]
                    .as_u64()
                    .ok_or("verified runtime probe omitted the stored World format version")?;
                require_read_version(
                    &formats["world_persistence"]["read_versions"],
                    version,
                    "World persistence",
                )?;
                let history_verified = probe["execution_history_verified"]
                    .as_bool()
                    .ok_or("verified runtime probe omitted its execution-history result")?;
                if history_verified == probe["execution_head"].is_null() {
                    return Err("runtime probe execution-head presence differs from its verification result".into());
                }
            }
            if !probe["execution_head"].is_null() {
                let version = probe["execution_head"]["schema_version"]
                    .as_u64()
                    .ok_or("runtime probe omitted its stored execution record schema")?;
                require_read_version(
                    &formats["execution_bridge_record"]["read_versions"],
                    version,
                    "execution record",
                )?;
            }
        }
        Some(profile) if profile == super::super::formal_observer::PROFILE => {
            let api = formats["local_status"]["api_version"]
                .as_str()
                .ok_or("observer descriptor omitted its status format")?;
            if probe["status_loaded"] == true && api != "oasis7.formal-public-observer-status/v1" {
                return Err("observer binary does not support the persisted status API".into());
            }
        }
        Some(profile) if profile == super::super::formal_full_sync::PROFILE => {
            let api = formats["local_status"]["api_version"]
                .as_str()
                .ok_or("full-sync descriptor omitted its status format")?;
            if probe["status_loaded"] == true && api != "oasis7.formal-full-sync-status/v1" {
                return Err("full-sync binary does not support the persisted status API".into());
            }
            if probe["history_head_verified"] != true
                || !matches!(probe["state"].as_str(), Some("verified" | "uninitialized"))
            {
                return Err(
                    "full-sync descriptor did not replay the fixed-origin local history closure"
                        .into(),
                );
            }
            let history_head =
                formats["external_checkpoint_and_replay_history"]["history_head_schema"]
                    .as_u64()
                    .ok_or("full-sync descriptor omitted its history-head schema")?;
            if history_head != 1
                || formats["external_checkpoint_and_replay_history"]["history_manifest_schema"] != 1
                || formats["external_checkpoint_and_replay_history"]["page_api"]
                    != "oasis7.formal-world-read-history-page/v1"
            {
                return Err(
                    "full-sync binary does not declare the persisted proof-history formats".into(),
                );
            }
            let current: FormalFullSyncCutV1 = serde_json::from_value(probe["current_cut"].clone())
                .map_err(|error| {
                    format!("full-sync probe omitted its verified current cut: {error}")
                })?;
            current.validate()?;
            let origin: FormalFullSyncCutV1 = serde_json::from_value(probe["origin_cut"].clone())
                .map_err(|error| {
                format!("full-sync probe omitted its external C0 cut: {error}")
            })?;
            origin.validate()?;
            if !cut_is_at_or_after(
                &serde_json::to_value(&origin).map_err(|error| error.to_string())?,
                &serde_json::to_value(&current).map_err(|error| error.to_string())?,
            )? {
                return Err(
                    "full-sync current cut is not on or after the fixed external C0".into(),
                );
            }
            let minimum_verified = probe["minimum_cut_verified"]
                .as_bool()
                .ok_or("full-sync descriptor omitted minimum-cut verification state")?;
            if !probe["minimum_cut"].is_null() {
                let cut: FormalFullSyncCutV1 = serde_json::from_value(probe["minimum_cut"].clone())
                    .map_err(|error| format!("full-sync minimum cut is malformed: {error}"))?;
                cut.validate()?;
                if !minimum_verified
                    || !cut_is_at_or_after(
                        &serde_json::to_value(&cut).map_err(|error| error.to_string())?,
                        &serde_json::to_value(&current).map_err(|error| error.to_string())?,
                    )?
                {
                    return Err("full-sync descriptor did not verify the exact minimum cut in its current history".into());
                }
            } else if minimum_verified {
                return Err(
                    "full-sync descriptor claims minimum-cut verification without a cut".into(),
                );
            }
        }
        Some(_) => return Err("runtime descriptor contains an unsupported profile".into()),
        None => return Err("runtime descriptor omitted its profile".into()),
    }
    Ok(())
}

fn require_read_version(values: &Value, version: u64, description: &str) -> Result<(), String> {
    if !values
        .as_array()
        .is_some_and(|versions| versions.iter().any(|item| item.as_u64() == Some(version)))
    {
        return Err(format!(
            "runtime does not declare support for stored {description} schema {version}"
        ));
    }
    Ok(())
}

fn validate_plan(plan: &RuntimeCompatibilityPlanV1) -> Result<(), String> {
    let mut prior_probe = plan.prior_descriptor["stored_state_probe"].clone();
    let mut candidate_probe = plan.candidate_descriptor["stored_state_probe"].clone();
    for probe in [&mut prior_probe, &mut candidate_probe] {
        if let Some(object) = probe.as_object_mut() {
            object.remove("minimum_cut");
            object.remove("minimum_cut_verified");
            if let Some(history) = object.get_mut("history").and_then(Value::as_object_mut) {
                history.remove("minimum_cut");
                history.remove("minimum_cut_verified");
            }
        }
    }
    if plan.api_version != PLAN_API
        || plan.profile != LOCAL_NATIVE_PROFILE
        || safe_token(&plan.operation_id).is_err()
        || !plan.config_path.is_absolute()
        || !plan.data_dir.is_absolute()
        || !plan.prior_runtime_path.is_absolute()
        || !plan.candidate_runtime_path.is_absolute()
        || plan.prior_descriptor["api_version"] != DESCRIPTOR_API
        || plan.candidate_descriptor["api_version"] != DESCRIPTOR_API
        || io::digest(&plan.prior_descriptor)? != plan.prior_descriptor_sha256
        || io::digest(&plan.candidate_descriptor)? != plan.candidate_descriptor_sha256
        || plan.prior_descriptor["executable_sha256"] != plan.prior_runtime_sha256
        || plan.candidate_descriptor["executable_sha256"] != plan.candidate_runtime_sha256
        || plan.prior_descriptor["config_path"] != plan.config_path.display().to_string()
        || plan.candidate_descriptor["config_path"] != plan.config_path.display().to_string()
        || plan.prior_descriptor["config_digest"] != plan.candidate_descriptor["config_digest"]
        || plan.prior_descriptor["config_sha256"] != plan.candidate_descriptor["config_sha256"]
        || plan.prior_descriptor["data_dir"] != plan.data_dir.display().to_string()
        || plan.candidate_descriptor["data_dir"] != plan.data_dir.display().to_string()
        || plan.prior_descriptor["runtime_profile"] != plan.candidate_descriptor["runtime_profile"]
        || plan.prior_descriptor["world_id"] != plan.candidate_descriptor["world_id"]
        || plan.prior_descriptor["chain_id"] != plan.candidate_descriptor["chain_id"]
        || plan.prior_descriptor["config_digest"] != plan.candidate_descriptor["config_digest"]
        || plan.prior_descriptor["config_sha256"] != plan.candidate_descriptor["config_sha256"]
        || prior_probe != candidate_probe
    {
        return Err("runtime compatibility plan is malformed or lost an executable binding".into());
    }
    require_descriptor_reads_its_probe(&plan.prior_descriptor)?;
    require_descriptor_reads_its_probe(&plan.candidate_descriptor)?;
    match (
        plan.minimum_full_sync_cut.as_ref(),
        plan.minimum_full_sync_cut_sha256.as_deref(),
        plan.minimum_full_sync_cut_bytes.as_deref(),
    ) {
        (Some(cut), Some(sha256), Some(bytes)) => {
            cut.validate()?;
            let decoded: FormalFullSyncCutV1 = serde_json::from_str(bytes).map_err(|error| {
                format!("decode immutable full-sync minimum cut bytes: {error}")
            })?;
            if decoded != *cut
                || serde_json::to_vec_pretty(&decoded).map_err(|error| error.to_string())?
                    != bytes.as_bytes()
                || sha256_bytes(bytes.as_bytes()) != *sha256
                || plan.prior_descriptor["runtime_profile"]
                    != super::super::formal_full_sync::PROFILE
                || plan.prior_descriptor["stored_state_probe"]["current_cut"]
                    != serde_json::to_value(cut).map_err(|error| error.to_string())?
                || plan.candidate_descriptor["stored_state_probe"]["minimum_cut"]
                    != serde_json::to_value(cut).map_err(|error| error.to_string())?
                || plan.candidate_descriptor["stored_state_probe"]["minimum_cut_verified"] != true
                || plan.candidate_descriptor["minimum_full_sync_cut_sha256"] != sha256
            {
                return Err(
                    "full-sync compatibility plan lost its exact staged minimum cut binding".into(),
                );
            }
        }
        (None, None, None)
            if plan.prior_descriptor["runtime_profile"]
                != super::super::formal_full_sync::PROFILE => {}
        _ => return Err("full-sync compatibility plan is missing its bounded minimum cut".into()),
    }
    Ok(())
}

fn plan_path(data_dir: &Path, operation_id: &str) -> Result<PathBuf, String> {
    let root = data_dir.join("package-runtime-compatibility");
    io::private_dir(&root)?;
    Ok(root.join(format!("{}.json", safe_token(operation_id)?)))
}

fn safe_token(value: &str) -> Result<&str, String> {
    if value.is_empty()
        || value.len() > 128
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-'))
    {
        return Err("package version is not a safe compatibility-plan filename".into());
    }
    Ok(value)
}

fn descriptor_failure_diagnostic(stderr: &[u8], stdout: &[u8]) -> String {
    let stderr = String::from_utf8_lossy(stderr);
    let stdout = String::from_utf8_lossy(stdout);
    let combined = format!("{}\n{}", stderr.trim(), stdout.trim());
    let mut diagnostic = String::new();
    for character in combined.trim().chars() {
        if diagnostic.len() + character.len_utf8() > 1024 {
            break;
        }
        diagnostic.push(character);
    }
    diagnostic
}

#[cfg(test)]
mod tests {
    #[test]
    fn failed_descriptor_reports_stdout_and_bounds_both_streams() {
        assert_eq!(
            super::descriptor_failure_diagnostic(b"", b"unproven World"),
            "unproven World"
        );
        assert_eq!(
            super::descriptor_failure_diagnostic(b"stderr", b"stdout"),
            "stderr\nstdout"
        );
        let large = "世".repeat(2048);
        let bounded = super::descriptor_failure_diagnostic(large.as_bytes(), b"stdout");
        assert!(bounded.len() <= 1024);
        assert!(!bounded.is_empty());
    }

    use super::*;
    use oasis7_client_api::world_service::WorldIdentity;

    fn cut(position: u64, seed: u8) -> FormalFullSyncCutV1 {
        let hash = hex::encode([seed; 32]);
        FormalFullSyncCutV1 {
            world: WorldIdentity {
                world_id: "world-a".into(),
                genesis_digest: hex::encode([1; 32]),
            },
            chain_id: "chain-a".into(),
            position,
            record_id: hash.clone(),
            after_state_root: hex::encode([2; 32]),
            active_authority_id: hex::encode([3; 32]),
            finalizing_authority_id: hex::encode([4; 32]),
            authority_epoch: 1,
        }
    }

    fn descriptor(
        cut: &FormalFullSyncCutV1,
        executable_sha256: &str,
        config_path: &Path,
        data_dir: &Path,
        minimum_cut: Option<&FormalFullSyncCutV1>,
        minimum_sha256: Option<&str>,
    ) -> Value {
        serde_json::json!({
            "api_version": DESCRIPTOR_API,
            "executable_sha256": executable_sha256,
            "config_path": config_path.display().to_string(),
            "config_digest": "node-config-digest",
            "config_sha256": "node-config-file-sha256",
            "data_dir": data_dir.display().to_string(),
            "runtime_profile": "formal_full_state_sync_read_only_v1",
            "world_id": cut.world.world_id,
            "chain_id": cut.chain_id,
            "minimum_full_sync_cut_sha256": minimum_sha256,
            "software_formats": {
                "local_status": {
                    "api_version": "oasis7.formal-full-sync-status/v1",
                    "read_versions": [1],
                    "write_version": 1
                },
                "external_checkpoint_and_replay_history": {
                    "history_head_schema": 1,
                    "history_manifest_schema": 1,
                    "page_api": "oasis7.formal-world-read-history-page/v1"
                }
            },
            "stored_state_probe": {
                "state": "verified",
                "history_head_verified": true,
                "origin_cut": cut,
                "current_cut": cut,
                "minimum_cut": minimum_cut,
                "minimum_cut_verified": minimum_cut.is_some()
            }
        })
    }

    fn plan_with_minimum() -> RuntimeCompatibilityPlanV1 {
        let cut = cut(7, 7);
        let encoded = serde_json::to_vec_pretty(&cut).unwrap();
        let minimum_sha256 = hex::encode(Sha256::digest(&encoded));
        let config_path = PathBuf::from("/tmp/node.json");
        let data_dir = PathBuf::from("/tmp/node-data");
        let prior = descriptor(
            &cut,
            &hex::encode([5; 32]),
            &config_path,
            &data_dir,
            None,
            None,
        );
        let candidate = descriptor(
            &cut,
            &hex::encode([6; 32]),
            &config_path,
            &data_dir,
            Some(&cut),
            Some(&minimum_sha256),
        );
        RuntimeCompatibilityPlanV1 {
            api_version: PLAN_API.into(),
            profile: LOCAL_NATIVE_PROFILE.into(),
            operation_id: "stage-7".into(),
            config_path,
            data_dir,
            prior_runtime_path: PathBuf::from("/tmp/prior-runtime"),
            prior_runtime_sha256: hex::encode([5; 32]),
            prior_descriptor_sha256: io::digest(&prior).unwrap(),
            prior_descriptor: prior,
            candidate_runtime_path: PathBuf::from("/tmp/candidate-runtime"),
            candidate_runtime_sha256: hex::encode([6; 32]),
            candidate_descriptor_sha256: io::digest(&candidate).unwrap(),
            candidate_descriptor: candidate,
            minimum_full_sync_cut: Some(cut),
            minimum_full_sync_cut_sha256: Some(minimum_sha256),
            minimum_full_sync_cut_bytes: Some(
                String::from_utf8(encoded).expect("minimum cut JSON is UTF-8"),
            ),
        }
    }

    #[test]
    fn immutable_minimum_cut_is_bound_to_prior_and_candidate_descriptors() {
        let mut plan = plan_with_minimum();
        validate_plan(&plan).expect("exact minimum cut is a valid compatibility plan");

        let substituted = cut(7, 8);
        let bytes = serde_json::to_vec_pretty(&substituted).unwrap();
        plan.minimum_full_sync_cut = Some(substituted);
        plan.minimum_full_sync_cut_sha256 = Some(hex::encode(Sha256::digest(bytes)));
        assert!(
            validate_plan(&plan).is_err(),
            "changing the staged cut cannot be reconciled with executable descriptors"
        );
    }

    #[test]
    fn same_position_requires_the_exact_formal_cut() {
        let first = cut(4, 1);
        let conflict = cut(4, 2);
        let later = cut(5, 3);
        let first = serde_json::to_value(first).unwrap();
        let conflict = serde_json::to_value(conflict).unwrap();
        let later = serde_json::to_value(later).unwrap();

        assert!(!cut_is_at_or_after(&first, &conflict).unwrap());
        assert!(cut_is_at_or_after(&first, &first).unwrap());
        assert!(cut_is_at_or_after(&first, &later).unwrap());
    }
}
