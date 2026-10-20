//! Receipt validation and process artifact readback for package operations.

use super::super::LocalRuntimeArtifactV1;

use super::super::local_io as io;
use super::{
    ABSENT_API, LOCAL_NATIVE_PROFILE, LocalActionRecord, LocalInstance, PROFILE, PROMOTION_API,
    PackageOperationState, PackagePromotionReceiptV1, PackageRollbackReceiptV1,
    PackageStageReceiptV1, PackageUpgradeProgressV1, PackageUpgradeV1, ROLLBACK_API, STAGE_API,
    action_id, is_digest,
};
use serde::Serialize;
use std::{
    fs,
    path::{Path, PathBuf},
    process::{Command, Output},
};
pub(super) fn validate_failed_start(
    request: &PackageUpgradeV1,
    operation_id: &str,
    operation_request_digest: &str,
    expected: &LocalInstance,
    stage: &PackageStageReceiptV1,
    promotion: &PackagePromotionReceiptV1,
    start: &LocalActionRecord,
) -> Result<(), String> {
    let runtime_artifact = verified_runtime_artifact(
        request,
        operation_id,
        operation_request_digest,
        stage,
        promotion,
    )?;
    let start_action_id = format!("op:{operation_id}:start");
    if start.api_version != "oasis7.local-node-action/v1"
        || start.action_id != start_action_id
        || start.kind != "start"
        || start.expected_generation != expected.generation
        || start.target.api_version != "oasis7.local-node-instance/v1"
        || start.target.action_id != start_action_id
        || start.target.generation
            != expected
                .generation
                .checked_add(1)
                .ok_or("package generation overflow")?
        || start.target.config_digest
            != promotion
                .candidate_node_config_digest
                .as_deref()
                .unwrap_or(expected.config_digest.as_str())
        || !matches!(start.target.state.as_str(), "failed" | "stopped")
        || start.target.runtime_pid.is_some()
        || start.runtime_artifact.as_ref() != Some(&runtime_artifact)
    {
        return Err("package rollback requires the exact failed promoted-start receipt".into());
    }
    let expected_payload = io::digest(&(
        "start_package",
        start.target.config_digest.clone(),
        start.expected_generation,
        runtime_artifact,
    ))?;
    if start.payload_digest != expected_payload {
        return Err("failed promoted-start payload digest is invalid".into());
    }
    Ok(())
}

pub(crate) fn verified_runtime_artifact(
    request: &PackageUpgradeV1,
    operation_id: &str,
    operation_request_digest: &str,
    stage: &PackageStageReceiptV1,
    promotion: &PackagePromotionReceiptV1,
) -> Result<LocalRuntimeArtifactV1, String> {
    validate_progress(
        &PackageUpgradeProgressV1 {
            api_version: "oasis7.local-package-upgrade-progress/v1".into(),
            profile: request.profile.clone(),
            operation_id: operation_id.into(),
            operation_request_digest: operation_request_digest.into(),
            stage_receipt: Some(stage.clone()),
            promotion_receipt: Some(promotion.clone()),
            rollback_receipt: None,
        },
        request,
        operation_id,
        operation_request_digest,
    )?;
    let runtime_path = request
        .node_root
        .join("releases")
        .join(&request.package_version)
        .join("bin/oasis7_chain_runtime");
    let buildinfo_path = runtime_path
        .parent()
        .and_then(Path::parent)
        .ok_or("promoted package runtime parent missing")?
        .join("BUILDINFO");
    let runtime_path = io::regular(&runtime_path)?;
    let buildinfo_path = io::regular(&buildinfo_path)?;
    if promotion.runtime_path != runtime_path
        || promotion.promoted_current != request.node_root.join("current")
        || io::sha256(&runtime_path)? != promotion.runtime_sha256
        || io::sha256(&buildinfo_path)? != promotion.buildinfo_sha256
    {
        return Err("promoted package files differ from their durable promotion receipt".into());
    }
    let buildinfo = fs::read_to_string(&buildinfo_path).map_err(|error| error.to_string())?;
    let fields = buildinfo
        .lines()
        .filter_map(|line| line.split_once('='))
        .collect::<std::collections::BTreeMap<_, _>>();
    let expected_platform = if request.profile == LOCAL_NATIVE_PROFILE {
        "local-native"
    } else {
        "linux-x64"
    };
    if fields.get("package_version") != Some(&request.package_version.as_str())
        || fields.get("commit") != Some(&request.commit.as_str())
        || fields.get("run_id") != Some(&request.run_id.as_str())
        || fields.get("runtime_sha256") != Some(&promotion.runtime_sha256.as_str())
        || fields.get("platform") != Some(&expected_platform)
    {
        return Err("promoted BUILDINFO does not bind the immutable package request".into());
    }
    let artifact = LocalRuntimeArtifactV1 {
        api_version: "oasis7.local-package-runtime-artifact/v1".into(),
        profile: request.profile.clone(),
        operation_id: operation_id.into(),
        operation_request_digest: operation_request_digest.into(),
        package_version: request.package_version.clone(),
        commit: request.commit.clone(),
        runtime_path,
        runtime_sha256: promotion.runtime_sha256.clone(),
        buildinfo_sha256: promotion.buildinfo_sha256.clone(),
    };
    validate_runtime_artifact_binding(
        request,
        operation_id,
        operation_request_digest,
        stage,
        promotion,
        &artifact,
    )?;
    Ok(artifact)
}

pub(crate) fn verified_rollback_runtime_artifact(
    request: &PackageUpgradeV1,
    operation_id: &str,
    operation_request_digest: &str,
    rollback: &PackageRollbackReceiptV1,
) -> Result<LocalRuntimeArtifactV1, String> {
    validate_progress(
        &PackageUpgradeProgressV1 {
            api_version: "oasis7.local-package-upgrade-progress/v1".into(),
            profile: request.profile.clone(),
            operation_id: operation_id.into(),
            operation_request_digest: operation_request_digest.into(),
            stage_receipt: Some(rollback.stage.clone()),
            promotion_receipt: Some(rollback.promotion.clone()),
            rollback_receipt: Some(rollback.clone()),
        },
        request,
        operation_id,
        operation_request_digest,
    )?;
    let current = request.node_root.join("current");
    let current_target = fs::read_link(&current).map_err(|error| error.to_string())?;
    let current_resolved = fs::canonicalize(&current).map_err(|error| error.to_string())?;
    let runtime_path = io::regular(&rollback.runtime_path)?;
    let buildinfo_path = io::regular(
        &runtime_path
            .parent()
            .and_then(Path::parent)
            .ok_or("rollback runtime parent missing")?
            .join("BUILDINFO"),
    )?;
    if current_target != rollback.restored_current_target
        || current_resolved != rollback.restored_current_resolved
        || runtime_path
            != rollback
                .restored_current_resolved
                .join("bin/oasis7_chain_runtime")
        || io::sha256(&runtime_path)? != rollback.runtime_sha256
        || io::sha256(&buildinfo_path)? != rollback.buildinfo_sha256
    {
        return Err("restored package files differ from their durable rollback receipt".into());
    }
    let buildinfo = fs::read_to_string(&buildinfo_path).map_err(|error| error.to_string())?;
    let fields = buildinfo
        .lines()
        .filter_map(|line| line.split_once('='))
        .collect::<std::collections::BTreeMap<_, _>>();
    if fields.get("package_version") != Some(&rollback.package_version.as_str())
        || fields.get("commit") != Some(&rollback.commit.as_str())
        || fields.get("runtime_sha256") != Some(&rollback.runtime_sha256.as_str())
        || fields.get("platform") != Some(&rollback.platform.as_str())
    {
        return Err("restored BUILDINFO differs from the exact rollback runtime".into());
    }
    let artifact = LocalRuntimeArtifactV1 {
        api_version: "oasis7.local-package-runtime-artifact/v1".into(),
        profile: request.profile.clone(),
        operation_id: operation_id.into(),
        operation_request_digest: operation_request_digest.into(),
        package_version: rollback.package_version.clone(),
        commit: rollback.commit.clone(),
        runtime_path,
        runtime_sha256: rollback.runtime_sha256.clone(),
        buildinfo_sha256: rollback.buildinfo_sha256.clone(),
    };
    validate_rollback_runtime_artifact_binding(
        request,
        operation_id,
        operation_request_digest,
        rollback,
        &artifact,
    )?;
    Ok(artifact)
}

pub(crate) fn validate_rollback_runtime_artifact_binding(
    request: &PackageUpgradeV1,
    operation_id: &str,
    operation_request_digest: &str,
    rollback: &PackageRollbackReceiptV1,
    artifact: &LocalRuntimeArtifactV1,
) -> Result<(), String> {
    if artifact.api_version != "oasis7.local-package-runtime-artifact/v1"
        || artifact.profile != request.profile
        || artifact.operation_id != operation_id
        || artifact.operation_request_digest != operation_request_digest
        || artifact.package_version != rollback.package_version
        || artifact.commit != rollback.commit
        || artifact.runtime_path != rollback.runtime_path
        || artifact.runtime_sha256 != rollback.runtime_sha256
        || artifact.buildinfo_sha256 != rollback.buildinfo_sha256
    {
        return Err(
            "rollback-start runtime artifact differs from the exact restored release".into(),
        );
    }
    Ok(())
}

pub(crate) fn validate_runtime_artifact_binding(
    request: &PackageUpgradeV1,
    operation_id: &str,
    operation_request_digest: &str,
    stage: &PackageStageReceiptV1,
    promotion: &PackagePromotionReceiptV1,
    artifact: &LocalRuntimeArtifactV1,
) -> Result<(), String> {
    let expected_path = request
        .node_root
        .join("releases")
        .join(&request.package_version)
        .join("bin/oasis7_chain_runtime");
    if artifact.api_version != "oasis7.local-package-runtime-artifact/v1"
        || artifact.profile != request.profile
        || artifact.operation_id != operation_id
        || artifact.operation_request_digest != operation_request_digest
        || artifact.package_version != request.package_version
        || artifact.commit != request.commit
        || artifact.runtime_path != expected_path
        || artifact.runtime_path != promotion.runtime_path
        || artifact.runtime_sha256 != promotion.runtime_sha256
        || artifact.buildinfo_sha256 != promotion.buildinfo_sha256
        || promotion.stage != *stage
    {
        return Err("package start runtime artifact differs from its immutable promotion".into());
    }
    Ok(())
}

pub(crate) fn verify_running_artifact(
    action: &LocalActionRecord,
    artifact: &LocalRuntimeArtifactV1,
) -> Result<(), String> {
    if action.target.state != "running" || action.runtime_artifact.as_ref() != Some(artifact) {
        return Err("package start receipt does not bind the verified release artifact".into());
    }
    let mut pids = vec![action.target.supervisor_pid];
    if let Some(runtime_pid) = action.target.runtime_pid {
        pids.push(runtime_pid);
    }
    for pid in pids {
        let executable = process_executable(pid)?;
        if executable != artifact.runtime_path
            || io::sha256(&executable)? != artifact.runtime_sha256
        {
            return Err("running process executable differs from the promoted package".into());
        }
    }
    Ok(())
}

#[cfg(target_os = "linux")]
fn process_executable(pid: u32) -> Result<PathBuf, String> {
    fs::canonicalize(format!("/proc/{pid}/exe")).map_err(|error| error.to_string())
}

#[cfg(target_os = "macos")]
fn process_executable(pid: u32) -> Result<PathBuf, String> {
    use std::{ffi::CStr, os::raw::c_void};

    unsafe extern "C" {
        fn proc_pidpath(pid: i32, buffer: *mut c_void, buffersize: u32) -> i32;
    }
    let mut buffer = [0_i8; 4096];
    let written =
        unsafe { proc_pidpath(pid as i32, buffer.as_mut_ptr().cast(), buffer.len() as u32) };
    if written <= 0 || written as usize >= buffer.len() {
        return Err(format!("cannot read executable path for process {pid}"));
    }
    let path = unsafe { CStr::from_ptr(buffer.as_ptr()) }
        .to_str()
        .map_err(|error| error.to_string())?;
    fs::canonicalize(path).map_err(|error| error.to_string())
}

#[cfg(not(any(target_os = "linux", target_os = "macos")))]
fn process_executable(pid: u32) -> Result<PathBuf, String> {
    Err(format!(
        "exact process executable readback unsupported for pid {pid}"
    ))
}

pub(crate) fn validate_progress(
    progress: &PackageUpgradeProgressV1,
    request: &PackageUpgradeV1,
    operation_id: &str,
    operation_request_digest: &str,
) -> Result<(), String> {
    if progress.api_version != "oasis7.local-package-upgrade-progress/v1"
        || progress.profile != request.profile
        || progress.operation_id != operation_id
        || progress.operation_request_digest != operation_request_digest
    {
        return Err("package progress differs from immutable OperationRecord".into());
    }
    if let Some(stage) = &progress.stage_receipt {
        validate_stage_binding(
            stage,
            request,
            operation_id,
            operation_request_digest,
            // The owner is taken from the immutable package-stage receipt;
            // OperationRecord exact target checks happen at promotion time.
            &LocalInstance {
                api_version: "oasis7.local-node-instance/v1".into(),
                action_id: stage.expected_instance_action_id.clone(),
                generation: stage.expected_generation,
                incarnation: stage.expected_incarnation.clone(),
                config_digest: stage.expected_config_digest.clone(),
                supervisor_pid: 0,
                runtime_pid: None,
                started_unix_ms: 0,
                state: "running".into(),
                exit_code: None,
            },
        )?;
    }
    if let Some(promotion) = &progress.promotion_receipt {
        let stage = progress
            .stage_receipt
            .as_ref()
            .ok_or("package promotion receipt has no original stage receipt")?;
        if promotion.stage != *stage
            || promotion.schema_version != PROMOTION_API
            || promotion.operation_id != operation_id
            || promotion.action_id != action_id(operation_id, "promote")
            || promotion.operation_request_digest != operation_request_digest
            || promotion.profile != request.profile
            || promotion.request_digest != stage.request_digest
            || promotion.stop_action_id != format!("op:{operation_id}:stop")
            || !is_digest(&promotion.stop_receipt_sha256)
            || promotion.stopped_owner.incarnation != stage.expected_incarnation
            || promotion.stopped_owner.generation != stage.expected_generation
            || promotion.stopped_owner.config_digest != stage.expected_config_digest
            || !matches!(promotion.stopped_owner.state.as_str(), "stopped" | "failed")
            || promotion.stopped_owner.runtime_pid.is_some()
            || promotion.package_version != stage.package_version
            || promotion.commit != stage.commit
            || promotion.prepared_bundle_sha256 != stage.prepared_bundle_sha256
            || !is_digest(&promotion.promoted_config_digest)
            || !candidate_chain_plan_digest_matches(
                stage.candidate_chain_plan_sha256.as_deref(),
                promotion.candidate_chain_plan_sha256.as_deref(),
            )
            || promotion.candidate_node_config_digest != stage.candidate_node_config_digest
            || promotion.candidate_binding_digest != stage.candidate_binding_digest
            || promotion.candidate_anchor_digest != stage.candidate_anchor_digest
            || !is_digest(&promotion.runtime_sha256)
            || promotion.runtime_path
                != request
                    .node_root
                    .join("releases")
                    .join(&request.package_version)
                    .join("bin/oasis7_chain_runtime")
            || promotion.promoted_current != request.node_root.join("current")
            || !is_digest(&promotion.buildinfo_sha256)
            || !promotion.rollback_transaction.is_absolute()
            || !promotion
                .rollback_transaction
                .starts_with(request.node_root.join("package-upgrade-rollback"))
        {
            return Err("package promotion receipt differs from original stage request".into());
        }
    }
    if let Some(rollback) = &progress.rollback_receipt {
        let stage = progress
            .stage_receipt
            .as_ref()
            .ok_or("package rollback receipt has no original stage receipt")?;
        let promotion = progress
            .promotion_receipt
            .as_ref()
            .ok_or("package rollback receipt has no exact promotion receipt")?;
        let expected_platform = if request.profile == LOCAL_NATIVE_PROFILE {
            "local-native"
        } else {
            "linux-x64"
        };
        if rollback.schema_version != ROLLBACK_API
            || rollback.profile != request.profile
            || rollback.operation_id != operation_id
            || rollback.action_id != action_id(operation_id, "rollback")
            || rollback.operation_request_digest != operation_request_digest
            || rollback.request_digest != stage.request_digest
            || rollback.stage != *stage
            || rollback.promotion != *promotion
            || rollback.promotion_receipt_sha256 != promotion_receipt_digest(promotion)?
            || rollback.stop_action_id != promotion.stop_action_id
            || rollback.stop_receipt_sha256 != promotion.stop_receipt_sha256
            || rollback.failed_start_action_id != format!("op:{operation_id}:start")
            || !is_digest(&rollback.failed_start_receipt_sha256)
            || rollback.rollback_transaction != promotion.rollback_transaction
            || !rollback.restored_current_target.is_absolute()
            || !rollback.restored_current_resolved.is_absolute()
            || !rollback
                .restored_current_resolved
                .starts_with(request.node_root.join("releases"))
            || rollback.package_version.trim().is_empty()
            || rollback.commit.trim().is_empty()
            || rollback.platform != expected_platform
            || rollback.runtime_path
                != rollback
                    .restored_current_resolved
                    .join("bin/oasis7_chain_runtime")
            || !is_digest(&rollback.runtime_sha256)
            || !is_digest(&rollback.buildinfo_sha256)
            || rollback.restored_node_config_digest != stage.prior_node_config_digest
            || rollback.restored_binding_digest != stage.prior_binding_digest
        {
            return Err(
                "package rollback receipt differs from the exact failed-start preimage".into(),
            );
        }
    }
    Ok(())
}

fn candidate_chain_plan_digest_matches(
    stage_digest: Option<&str>,
    promotion_digest: Option<&str>,
) -> bool {
    stage_digest == promotion_digest
}

pub(super) fn run_helper(
    request: &PackageUpgradeV1,
    operation_args: &[String],
) -> Result<Output, String> {
    request.validate_staged()?;
    let helper_script = request
        .helper_script
        .as_ref()
        .ok_or("package operation helper is missing")?;
    let mut command = Command::new(helper_script);
    command
        .args(operation_args)
        .arg("--node-root")
        .arg(&request.node_root);
    if operation_args
        .windows(2)
        .any(|pair| pair[0] == "--operation-mode" && pair[1] == "stage")
    {
        if request.profile == PROFILE {
            command
                .arg("--package-deb")
                .arg(
                    request
                        .package_deb
                        .as_ref()
                        .ok_or("Debian package path is missing")?,
                )
                .arg("--ops-tools-tar")
                .arg(
                    request
                        .ops_tools_tar
                        .as_ref()
                        .ok_or("ops-tools archive path is missing")?,
                );
        } else {
            let native = request
                .native_artifact
                .as_ref()
                .ok_or("local native runtime artifact is missing")?;
            command
                .arg("--native-runtime")
                .arg(&native.runtime_binary)
                .arg("--native-runtime-sha256")
                .arg(&native.runtime_sha256)
                .arg("--native-buildinfo")
                .arg(&native.buildinfo)
                .arg("--native-buildinfo-sha256")
                .arg(&native.buildinfo_sha256);
        }
        command
            .arg("--package-version")
            .arg(&request.package_version)
            .arg("--commit")
            .arg(&request.commit)
            .arg("--run-id")
            .arg(&request.run_id);
        if let Some(artifact_ref) = &request.artifact_ref {
            command.arg("--artifact-ref").arg(artifact_ref);
        }
    }
    let output = command.output().map_err(|error| {
        format!(
            "cannot execute package helper {}: {error}",
            helper_script.display()
        )
    })?;
    if output.stdout.len() > 1024 * 1024 || output.stderr.len() > 16 * 1024 {
        return Err("package helper output exceeds the bounded receipt limit".into());
    }
    if !output.status.success() {
        let message = String::from_utf8_lossy(&output.stderr)
            .chars()
            .filter(|character| !character.is_control() || *character == '\n')
            .take(2048)
            .collect::<String>();
        return Err(format!("package helper refused operation: {message}"));
    }
    Ok(output)
}

pub(super) fn parse_stage_or_promotion(
    bytes: &[u8],
    operation_id: &str,
    operation_request_digest: &str,
    profile: &str,
) -> Result<PackageOperationState, String> {
    let value: serde_json::Value = serde_json::from_slice(bytes)
        .map_err(|error| format!("package helper returned invalid JSON receipt: {error}"))?;
    match value["schema_version"].as_str() {
        Some(ABSENT_API) => Ok(None),
        Some(STAGE_API) => {
            let stage: PackageStageReceiptV1 =
                serde_json::from_value(value).map_err(|error| error.to_string())?;
            if stage.operation_id != operation_id
                || stage.operation_request_digest != operation_request_digest
                || stage.profile != profile
            {
                return Err("package stage receipt request binding mismatch".into());
            }
            Ok(Some((stage, None, None)))
        }
        Some(PROMOTION_API) => {
            let promotion: PackagePromotionReceiptV1 =
                serde_json::from_value(value).map_err(|error| error.to_string())?;
            if promotion.operation_id != operation_id
                || promotion.operation_request_digest != operation_request_digest
                || promotion.profile != profile
            {
                return Err("package promotion receipt request binding mismatch".into());
            }
            Ok(Some((promotion.stage.clone(), Some(promotion), None)))
        }
        Some(ROLLBACK_API) => {
            let rollback: PackageRollbackReceiptV1 =
                serde_json::from_value(value).map_err(|error| error.to_string())?;
            if rollback.operation_id != operation_id
                || rollback.operation_request_digest != operation_request_digest
                || rollback.profile != profile
            {
                return Err("package rollback receipt request binding mismatch".into());
            }
            Ok(Some((
                rollback.stage.clone(),
                Some(rollback.promotion.clone()),
                Some(rollback),
            )))
        }
        _ => Err("package helper returned an unsupported receipt schema".into()),
    }
}

pub(super) fn validate_stage_binding(
    stage: &PackageStageReceiptV1,
    request: &PackageUpgradeV1,
    operation_id: &str,
    operation_request_digest: &str,
    expected: &LocalInstance,
) -> Result<(), String> {
    if stage.schema_version != STAGE_API
        || stage.profile != request.profile
        || stage.operation_id != operation_id
        || stage.action_id != action_id(operation_id, "stage")
        || stage.operation_request_digest != operation_request_digest
        || stage.node_root != request.node_root
        || stage.expected_incarnation != expected.incarnation
        || stage.expected_generation != expected.generation
        || stage.expected_config_digest != expected.config_digest
        || stage.expected_instance_action_id != expected.action_id
        || stage.package_version != request.package_version
        || stage.commit != request.commit
        || stage.package_deb_sha256
            != request
                .package_deb_sha256
                .clone()
                .or_else(|| {
                    request
                        .native_artifact
                        .as_ref()
                        .map(|artifact| artifact.runtime_sha256.clone())
                })
                .unwrap_or_default()
        || stage.ops_tools_tar_sha256
            != request
                .ops_tools_tar_sha256
                .clone()
                .or_else(|| {
                    request
                        .native_artifact
                        .as_ref()
                        .map(|artifact| artifact.buildinfo_sha256.clone())
                })
                .unwrap_or_default()
    {
        return Err("package stage receipt does not bind the immutable request and owner".into());
    }
    match request.profile.as_str() {
        LOCAL_NATIVE_PROFILE => {
            let path = stage
                .runtime_compatibility_plan_path
                .as_deref()
                .ok_or("local-native stage receipt omitted its executable compatibility plan")?;
            let sha256 = stage
                .runtime_compatibility_plan_sha256
                .as_deref()
                .ok_or("local-native stage receipt omitted its compatibility plan digest")?;
            if !is_digest(sha256) {
                return Err("local-native compatibility plan digest is malformed".into());
            }
            let plan = super::compatibility::load_exact(path, sha256)?;
            let native = request
                .native_artifact
                .as_ref()
                .ok_or("local-native stage request omitted its binary artifact")?;
            if plan.operation_id != operation_id
                || plan.profile != request.profile
                || plan.candidate_runtime_sha256 != native.runtime_sha256
            {
                return Err(
                    "runtime compatibility plan differs from the immutable package request".into(),
                );
            }
        }
        _ if stage.runtime_compatibility_plan_path.is_some()
            || stage.runtime_compatibility_plan_sha256.is_some() =>
        {
            return Err("package stage contains an unsupported runtime compatibility plan".into());
        }
        _ => {}
    }
    let candidate_fields = [
        stage.candidate_chain_plan_sha256.as_deref(),
        stage.candidate_chain_config_sha256.as_deref(),
        stage.prior_node_config_digest.as_deref(),
        stage.prior_binding_digest.as_deref(),
        stage.candidate_node_config_digest.as_deref(),
        stage.candidate_binding_digest.as_deref(),
        stage.prior_anchor_digest.as_deref(),
        stage.candidate_anchor_digest.as_deref(),
        stage.candidate_manifest_sha256.as_deref(),
        stage.candidate_bundle_sha256.as_deref(),
        stage.candidate_runtime_sha256.as_deref(),
        stage.candidate_buildinfo_sha256.as_deref(),
    ];
    match request.candidate_chain_config.as_ref() {
        Some(input) => {
            if stage.candidate_chain_config_path.as_ref() != Some(&input.path)
                || stage.candidate_chain_config_sha256.as_deref() != Some(input.sha256.as_str())
                || candidate_fields
                    .iter()
                    .any(|value| value.is_none_or(|value| !is_digest(value)))
            {
                return Err("package stage receipt does not bind the operator-pinned candidate chain config".into());
            }
            if stage.prior_node_config_digest.as_deref()
                != Some(stage.expected_config_digest.as_str())
            {
                return Err(
                    "package stage candidate plan does not bind the exact prior node config digest"
                        .into(),
                );
            }
        }
        None => {
            if stage.candidate_chain_config_path.is_some()
                || candidate_fields.iter().any(Option::is_some)
            {
                return Err(
                    "package stage receipt unexpectedly includes a candidate-chain transaction"
                        .into(),
                );
            }
        }
    }
    require_digest(&stage.request_digest)?;
    require_digest(&stage.prepared_bundle_sha256)?;
    require_digest(&stage.current_config_preimage_sha256)?;
    Ok(())
}

pub(super) fn validate_stopped_action(
    action: &LocalActionRecord,
    expected: &LocalInstance,
    stop_action_id: &str,
) -> Result<(), String> {
    if action.api_version != "oasis7.local-node-action/v1"
        || action.kind != "stop"
        || action.action_id != stop_action_id
        || action.expected_generation != expected.generation
        || action.target.incarnation != expected.incarnation
        || action.target.generation != expected.generation
        || action.target.config_digest != expected.config_digest
        || action.target.action_id != expected.action_id
        || !matches!(action.target.state.as_str(), "stopped" | "failed")
        || action.target.runtime_pid.is_some()
    {
        return Err("package promotion requires the original exact stopped action receipt".into());
    }
    let expected_digest = io::digest(&(
        "stop",
        expected.incarnation.clone(),
        expected.generation,
        expected.config_digest.clone(),
    ))?;
    if action.payload_digest != expected_digest {
        return Err("stopped action payload digest changed".into());
    }
    Ok(())
}

pub(super) fn require_owner_matches(
    current: Option<&LocalInstance>,
    expected: &LocalInstance,
) -> Result<(), String> {
    let current = current.ok_or("exact package lifecycle owner is missing")?;
    if current.api_version != "oasis7.local-node-instance/v1"
        || current.incarnation != expected.incarnation
        || current.generation != expected.generation
        || current.config_digest != expected.config_digest
        || current.action_id != expected.action_id
    {
        return Err("package lifecycle owner changed from the immutable request".into());
    }
    Ok(())
}

pub(super) fn require_live_owner(
    status: &serde_json::Value,
    expected: &LocalInstance,
) -> Result<(), String> {
    if expected.state != "running"
        || expected.runtime_pid.is_none()
        || !status["blocker"].is_null()
        || status["instance"]["incarnation"].as_str() != Some(&expected.incarnation)
        || status["instance"]["generation"].as_u64() != Some(expected.generation)
        || status["instance"]["config_digest"].as_str() != Some(&expected.config_digest)
        || status["instance"]["action_id"].as_str() != Some(&expected.action_id)
        || status["instance"]["state"].as_str() != Some("running")
        || status["instance"]["runtime_pid"].as_u64().is_none()
    {
        return Err("package stage requires the exact live local lifecycle owner".into());
    }
    Ok(())
}

pub(super) fn validate_operation_id(value: &str) -> Result<(), String> {
    if value.is_empty()
        || value.len() > 96
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b"._-".contains(&byte))
        || value.contains("..")
    {
        return Err("package operation ID must be a safe portable path token".into());
    }
    Ok(())
}
pub(super) fn require_digest(value: &str) -> Result<(), String> {
    if is_digest(value) {
        Ok(())
    } else {
        Err("package operation digest must be canonical SHA256".into())
    }
}

fn promotion_receipt_digest<T: Serialize>(value: &T) -> Result<String, String> {
    let mut value = serde_json::to_value(value).map_err(|error| error.to_string())?;
    let object = value
        .as_object_mut()
        .ok_or("package promotion receipt is not an object")?;
    // The shell helper's canonical receipt digest is computed over its JSON
    // object, which writes these optional fields explicitly as null. Rust's
    // typed receipt omits `None` during serialization, so restore those null
    // keys before hashing to keep the shared receipt contract byte-equivalent.
    for field in [
        "candidate_chain_plan_sha256",
        "candidate_node_config_digest",
        "candidate_binding_digest",
        "candidate_anchor_digest",
    ] {
        object
            .entry(field.to_owned())
            .or_insert(serde_json::Value::Null);
    }
    io::digest(&value)
}

#[cfg(test)]
mod tests {
    use super::{candidate_chain_plan_digest_matches, promotion_receipt_digest};
    use serde::Serialize;

    #[test]
    fn promotion_must_retain_the_exact_staged_candidate_plan_digest() {
        let digest = "a".repeat(64);
        let other_digest = "b".repeat(64);

        assert!(candidate_chain_plan_digest_matches(None, None));
        assert!(candidate_chain_plan_digest_matches(
            Some(&digest),
            Some(&digest)
        ));
        assert!(!candidate_chain_plan_digest_matches(Some(&digest), None));
        assert!(!candidate_chain_plan_digest_matches(
            Some(&digest),
            Some(&other_digest)
        ));
        assert!(!candidate_chain_plan_digest_matches(None, Some(&digest)));
    }

    #[test]
    fn promotion_receipt_digest_retains_explicit_null_candidate_fields() {
        #[derive(Serialize)]
        struct Receipt {
            schema_version: &'static str,
            #[serde(skip_serializing_if = "Option::is_none")]
            candidate_chain_plan_sha256: Option<String>,
            #[serde(skip_serializing_if = "Option::is_none")]
            candidate_node_config_digest: Option<String>,
            #[serde(skip_serializing_if = "Option::is_none")]
            candidate_binding_digest: Option<String>,
            #[serde(skip_serializing_if = "Option::is_none")]
            candidate_anchor_digest: Option<String>,
        }

        let receipt = Receipt {
            schema_version: "oasis7.package_upgrade_promotion_receipt.v1",
            candidate_chain_plan_sha256: None,
            candidate_node_config_digest: None,
            candidate_binding_digest: None,
            candidate_anchor_digest: None,
        };
        let helper_shape = serde_json::json!({
            "schema_version": "oasis7.package_upgrade_promotion_receipt.v1",
            "candidate_chain_plan_sha256": null,
            "candidate_node_config_digest": null,
            "candidate_binding_digest": null,
            "candidate_anchor_digest": null,
        });

        assert_eq!(
            promotion_receipt_digest(&receipt).unwrap(),
            super::super::super::local_io::digest(&helper_shape).unwrap()
        );
    }
}
