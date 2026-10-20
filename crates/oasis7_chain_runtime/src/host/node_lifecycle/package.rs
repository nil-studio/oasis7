//! Local package deployment adapters for Linux and native local runtimes.
//!
//! This is a separate deployment profile from the observer/full-sync runtime
//! profiles. The package helper owns artifact validation, release promotion,
//! and rollback; this adapter binds those effects to the local lifecycle's
//! exact incarnation/generation receipts. It deliberately has no SSH path.
use super::{LocalActionRecord, LocalInstance, actions, local_io as io};
use oasis7::network_tier_manifest::LoadedNetworkTierManifest;
#[path = "package_artifact.rs"]
mod artifact;
use artifact::{
    parse_stage_or_promotion, require_digest, require_live_owner, require_owner_matches,
    run_helper, validate_failed_start, validate_operation_id, validate_stage_binding,
    validate_stopped_action,
};
pub(crate) use artifact::{
    validate_progress, validate_rollback_runtime_artifact_binding,
    validate_runtime_artifact_binding, verified_rollback_runtime_artifact,
    verified_runtime_artifact, verify_running_artifact,
};

use serde::{Deserialize, Serialize};
use std::{
    fs,
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
};

#[path = "package_candidate_chain.rs"]
pub(super) mod candidate_chain;
#[path = "package_compatibility.rs"]
mod compatibility;
#[path = "package_preimage.rs"]
mod preimage;

pub(crate) const PROFILE: &str = "public_testnet_linux_package_v1";
pub(crate) const LOCAL_NATIVE_PROFILE: &str = "local_native_package_v1";
const REQUEST_API: &str = "oasis7.local-package-upgrade-request/v1";
const STAGE_API: &str = "oasis7.package_upgrade_operation_receipt.v2";
const PROMOTION_API: &str = "oasis7.package_upgrade_promotion_receipt.v1";
const ROLLBACK_API: &str = "oasis7.package_upgrade_rollback_receipt.v1";
const ABSENT_API: &str = "oasis7.package_upgrade_operation_absent.v1";

pub(crate) fn supports_profile(profile: &str) -> bool {
    matches!(profile, PROFILE | LOCAL_NATIVE_PROFILE)
}

fn is_digest(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct PackageUpgradeV1 {
    pub api_version: String,
    pub profile: String,
    pub node_root: PathBuf,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub helper_script: Option<PathBuf>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub helper_sha256: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub package_deb: Option<PathBuf>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub package_deb_sha256: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ops_tools_tar: Option<PathBuf>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ops_tools_tar_sha256: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub native_artifact: Option<LocalNativePackageArtifactV1>,
    /// Operator-pinned candidate chain config for a local-native runtime
    /// upgrade. The complete derived plan is frozen into the helper's durable
    /// stage request; the OperationRecord digest binds this input path/SHA.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub candidate_chain_config: Option<candidate_chain::CandidateChainConfigInputV1>,
    pub package_version: String,
    pub commit: String,
    pub run_id: String,
    pub artifact_ref: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct LocalNativePackageArtifactV1 {
    pub runtime_binary: PathBuf,
    pub runtime_sha256: String,
    pub buildinfo: PathBuf,
    pub buildinfo_sha256: String,
}

impl PackageUpgradeV1 {
    pub(crate) fn validate(&self) -> Result<(), String> {
        self.validate_request(true)
    }

    pub(crate) fn validate_staged(&self) -> Result<(), String> {
        self.validate_request(false)
    }

    /// Require an exact prior release identity before admitting a package
    /// operation. Operation-mode rollback restores only a symlink preimage, so
    /// unsupported current shapes must fail before a durable operation record
    /// or a stage intent can be created.
    pub(crate) fn validate_current_preimage(&self) -> Result<(), String> {
        preimage::validate(self)
    }

    /// A different runtime hash requires a fully validated operator-pinned
    /// candidate chain config. The chain anchor itself must preserve the
    /// existing world/genesis/registry/peer identity and the candidate bundle
    /// may differ only in its BUILDINFO-bound runtime_build subtree.
    pub(crate) fn validate_current_runtime_pin(&self, config_path: &Path) -> Result<(), String> {
        if self.profile != LOCAL_NATIVE_PROFILE {
            return Ok(());
        }
        let native = self
            .native_artifact
            .as_ref()
            .ok_or("local native runtime artifact is missing")?;
        let node = super::model::NodeConfig::load(config_path)?;
        node.verify_identity()?;
        node.identity.chain.verify()?;
        let runtime_profile = node.identity.chain.runtime_profile();
        if runtime_profile == super::formal_observer::PROFILE
            || runtime_profile == super::formal_full_sync::PROFILE
        {
            if self.candidate_chain_config.is_some() {
                return Err("formal read-only package upgrades preserve the exact externally pinned chain config, grant and C0; candidate chain configs are unsupported".into());
            }
            // The request's immutable artifact identity is the operator's
            // explicit candidate image selection. Each binary independently
            // verifies the same current config and stored data before an
            // OperationRecord is admitted; stage repeats this check and
            // persists the executable-derived descriptor pair.
            return compatibility::preflight(config_path, &node.identity.data_dir, self);
        }
        if runtime_profile != super::model::PROFILE {
            return Err(format!(
                "package upgrade does not support runtime profile {runtime_profile}"
            ));
        }
        if let Some(input) = self.candidate_chain_config.as_ref() {
            let plan = candidate_chain::prepare_candidate_chain(config_path, input, native, self)?;
            if plan.candidate_runtime_sha256 != native.runtime_sha256 {
                return Err(
                    "candidate chain plan does not bind the requested runtime SHA256".into(),
                );
            }
            return Ok(());
        }
        let compatibility = node.identity.chain.compatibility()?;
        let loaded = LoadedNetworkTierManifest::load(&compatibility.manifest.path)?;
        loaded.validate_runtime_support()?;
        let (bundle_path, approved_sha256) =
            super::super::cli::approved_runtime_sha256_for_network_tier_bundle(
                &compatibility.manifest.path,
                &loaded,
            )?;
        if native.runtime_sha256 != approved_sha256 {
            return Err(format!(
                "local-native candidate runtime SHA256 {} differs from current approved bundle {} SHA256 {}; a candidate chain-config transaction is required",
                native.runtime_sha256,
                bundle_path.display(),
                approved_sha256
            ));
        }
        Ok(())
    }

    fn validate_request(&self, require_source_files: bool) -> Result<(), String> {
        if self.api_version != REQUEST_API || !supports_profile(&self.profile) {
            return Err("unsupported package deployment profile".into());
        }
        if self.profile == PROFILE && self.candidate_chain_config.is_some() {
            return Err(
                "Linux package upgrades cannot include a local candidate chain config".into(),
            );
        }
        let helper_script = self
            .helper_script
            .as_ref()
            .ok_or("package operation helper is missing")?;
        let helper_sha256 = self
            .helper_sha256
            .as_ref()
            .ok_or("package operation helper SHA256 is missing")?;
        for (name, path) in [
            ("node root", &self.node_root),
            ("package helper", helper_script),
        ] {
            if !path.is_absolute() {
                return Err(format!("package {name} path must be absolute"));
            }
        }
        if !is_digest(helper_sha256) {
            return Err("package helper SHA256 must be canonical SHA256".into());
        }
        if self.package_version.trim().is_empty()
            || self.package_version.len() > 128
            || self.commit.trim().is_empty()
            || self.commit.len() > 128
            || self.run_id.trim().is_empty()
            || self.run_id.len() > 128
            || self
                .artifact_ref
                .as_ref()
                .is_some_and(|value| value.trim().is_empty() || value.len() > 2048)
        {
            return Err("package provenance fields are empty or exceed their bounds".into());
        }
        let node_root = io::regular(helper_script)?;
        let metadata = fs::metadata(&node_root).map_err(|error| error.to_string())?;
        if metadata.permissions().mode() & 0o111 == 0 {
            return Err("package helper is not executable".into());
        }
        if io::sha256(&node_root)? != *helper_sha256 {
            return Err("package helper digest changed after request admission".into());
        }
        if !fs::metadata(&self.node_root)
            .map_err(|error| error.to_string())?
            .is_dir()
        {
            return Err("package node root is not a directory".into());
        }
        if fs::canonicalize(&self.node_root).map_err(|error| error.to_string())? != self.node_root {
            return Err("package node root must use its canonical absolute path".into());
        }
        if require_source_files {
            if self.profile == PROFILE {
                let package_deb = self
                    .package_deb
                    .as_ref()
                    .ok_or("Debian package path is missing")?;
                let package_sha = self
                    .package_deb_sha256
                    .as_ref()
                    .ok_or("Debian package SHA256 is missing")?;
                let ops_tools = self
                    .ops_tools_tar
                    .as_ref()
                    .ok_or("ops-tools archive path is missing")?;
                let ops_sha = self
                    .ops_tools_tar_sha256
                    .as_ref()
                    .ok_or("ops-tools archive SHA256 is missing")?;
                if !package_deb.is_absolute()
                    || !ops_tools.is_absolute()
                    || !is_digest(package_sha)
                    || !is_digest(ops_sha)
                {
                    return Err(
                        "Linux package inputs must use absolute paths and canonical SHA256 values"
                            .into(),
                    );
                }
                if io::sha256(package_deb)? != *package_sha || io::sha256(ops_tools)? != *ops_sha {
                    return Err("package source digest changed after request admission".into());
                }
                if self.native_artifact.is_some() {
                    return Err(
                        "Linux package request cannot include a local native artifact".into(),
                    );
                }
            } else {
                if self.package_deb.is_some()
                    || self.package_deb_sha256.is_some()
                    || self.ops_tools_tar.is_some()
                    || self.ops_tools_tar_sha256.is_some()
                {
                    return Err("local native package cannot include Linux package inputs".into());
                }
                let native = self
                    .native_artifact
                    .as_ref()
                    .ok_or("local native runtime artifact is missing")?;
                if let Some(candidate) = self.candidate_chain_config.as_ref() {
                    candidate.validate_shape()?;
                }
                if !native.runtime_binary.is_absolute()
                    || !native.buildinfo.is_absolute()
                    || !is_digest(&native.runtime_sha256)
                    || !is_digest(&native.buildinfo_sha256)
                {
                    return Err("local native artifact inputs must use absolute paths and canonical SHA256 values".into());
                }
                if io::sha256(&native.runtime_binary)? != native.runtime_sha256
                    || io::sha256(&native.buildinfo)? != native.buildinfo_sha256
                {
                    return Err(
                        "local native artifact digest changed after request admission".into(),
                    );
                }
                let metadata =
                    fs::metadata(&native.runtime_binary).map_err(|error| error.to_string())?;
                if !metadata.is_file() || metadata.permissions().mode() & 0o111 == 0 {
                    return Err(
                        "local native runtime artifact must be an executable regular file".into(),
                    );
                }
            }
        } else if self.profile == PROFILE {
            if self.candidate_chain_config.is_some() {
                return Err(
                    "Linux package upgrades cannot include a local candidate chain config".into(),
                );
            }
            for (name, path, digest) in [
                (
                    "Debian package",
                    self.package_deb.as_ref(),
                    self.package_deb_sha256.as_ref(),
                ),
                (
                    "ops-tools archive",
                    self.ops_tools_tar.as_ref(),
                    self.ops_tools_tar_sha256.as_ref(),
                ),
            ] {
                if path.is_none_or(|path| !path.is_absolute())
                    || digest.is_none_or(|value| !is_digest(value))
                {
                    return Err(format!(
                        "staged Linux request is missing its {name} binding"
                    ));
                }
            }
        } else {
            let native = self
                .native_artifact
                .as_ref()
                .ok_or("staged local native request is missing its runtime artifact binding")?;
            if self.package_deb.is_some()
                || self.package_deb_sha256.is_some()
                || self.ops_tools_tar.is_some()
                || self.ops_tools_tar_sha256.is_some()
                || !native.runtime_binary.is_absolute()
                || !native.buildinfo.is_absolute()
                || !is_digest(&native.runtime_sha256)
                || !is_digest(&native.buildinfo_sha256)
            {
                return Err("staged local native artifact binding is invalid".into());
            }
            if let Some(candidate) = self.candidate_chain_config.as_ref() {
                candidate.validate_shape()?;
            }
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct PackageStageReceiptV1 {
    pub schema_version: String,
    pub profile: String,
    pub operation_id: String,
    pub action_id: String,
    pub operation_request_digest: String,
    pub request_digest: String,
    pub node_root: PathBuf,
    pub expected_incarnation: String,
    pub expected_generation: u64,
    pub expected_config_digest: String,
    pub expected_instance_action_id: String,
    pub package_version: String,
    pub commit: String,
    pub package_deb_sha256: String,
    pub ops_tools_tar_sha256: String,
    pub prepared_bundle_sha256: String,
    pub current_config_preimage_sha256: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub candidate_chain_plan_sha256: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub candidate_chain_config_path: Option<PathBuf>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub candidate_chain_config_sha256: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prior_node_config_digest: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prior_binding_digest: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub candidate_node_config_digest: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub candidate_binding_digest: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prior_anchor_digest: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub candidate_anchor_digest: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub candidate_manifest_sha256: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub candidate_bundle_sha256: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub candidate_runtime_sha256: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub candidate_buildinfo_sha256: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub runtime_compatibility_plan_path: Option<PathBuf>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub runtime_compatibility_plan_sha256: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct PackagePromotionReceiptV1 {
    pub schema_version: String,
    pub profile: String,
    pub operation_id: String,
    pub action_id: String,
    pub operation_request_digest: String,
    pub request_digest: String,
    pub stage: PackageStageReceiptV1,
    pub stop_action_id: String,
    pub stop_receipt_sha256: String,
    pub stopped_owner: LocalInstance,
    pub package_version: String,
    pub commit: String,
    pub prepared_bundle_sha256: String,
    pub runtime_sha256: String,
    pub runtime_path: PathBuf,
    pub buildinfo_sha256: String,
    pub promoted_current: PathBuf,
    pub promoted_config_digest: String,
    pub rollback_transaction: PathBuf,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub candidate_chain_plan_sha256: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub candidate_node_config_digest: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub candidate_binding_digest: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub candidate_anchor_digest: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct PackageRollbackReceiptV1 {
    pub schema_version: String,
    pub profile: String,
    pub operation_id: String,
    pub action_id: String,
    pub operation_request_digest: String,
    pub request_digest: String,
    pub stage: PackageStageReceiptV1,
    pub promotion: PackagePromotionReceiptV1,
    pub promotion_receipt_sha256: String,
    pub stop_action_id: String,
    pub stop_receipt_sha256: String,
    pub failed_start_action_id: String,
    pub failed_start_receipt_sha256: String,
    pub rollback_transaction: PathBuf,
    pub restored_current_target: PathBuf,
    pub restored_current_resolved: PathBuf,
    pub package_version: String,
    pub commit: String,
    pub platform: String,
    pub runtime_path: PathBuf,
    pub runtime_sha256: String,
    pub buildinfo_sha256: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub restored_node_config_digest: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub restored_binding_digest: Option<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct PackageUpgradeProgressV1 {
    pub api_version: String,
    pub profile: String,
    pub operation_id: String,
    pub operation_request_digest: String,
    pub stage_receipt: Option<PackageStageReceiptV1>,
    pub promotion_receipt: Option<PackagePromotionReceiptV1>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rollback_receipt: Option<PackageRollbackReceiptV1>,
}

pub(crate) type PackageOperationState = Option<(
    PackageStageReceiptV1,
    Option<PackagePromotionReceiptV1>,
    Option<PackageRollbackReceiptV1>,
)>;

pub(crate) fn action_id(operation_id: &str, step: &str) -> String {
    // The helper accepts portable path tokens only. This is a deterministic
    // projection of the immutable OperationRecord ID, never a caller alias.
    format!("pkg-{operation_id}-{step}")
}

pub(crate) fn stage(
    config_path: &Path,
    request: &PackageUpgradeV1,
    operation_id: &str,
    operation_request_digest: &str,
    expected: &LocalInstance,
) -> Result<PackageStageReceiptV1, String> {
    request.validate()?;
    request.validate_current_preimage()?;
    if request.profile != LOCAL_NATIVE_PROFILE {
        return Err("operation-record package stage requires the local-native executable compatibility probe; the Linux/systemd adapter has no verified rollback probe input".into());
    }
    validate_operation_id(operation_id)?;
    require_digest(operation_request_digest)?;
    let initial_binding = actions::binding(config_path)?;
    let initial_plan =
        prepare_request_candidate_plan(config_path, request, operation_id, &initial_binding)?;
    // The supervisor intentionally holds process.lock for its entire lifetime.
    // Stage only takes the transient admission fence, then proves the exact
    // owner through its control/status path; this excludes concurrent start,
    // stop, and management admissions without stopping the live process.
    let _admission = super::process::admission_lock(&initial_binding.data_dir)?;
    let binding = actions::binding(config_path)?;
    if binding.data_dir != initial_binding.data_dir
        || binding.config_digest != initial_binding.config_digest
    {
        return Err("package stage lifecycle configuration changed before admission".into());
    }
    let candidate_plan =
        prepare_request_candidate_plan(config_path, request, operation_id, &binding)?;
    if candidate_plan.as_ref().map(|item| &item.plan)
        != initial_plan.as_ref().map(|item| &item.plan)
    {
        return Err(
            "candidate chain config or package plan changed while waiting for lifecycle admission"
                .into(),
        );
    }
    require_owner_matches(binding.current.as_ref(), expected)?;
    require_live_owner(&super::process::status(&binding.data_dir)?, expected)?;
    let compatibility_plan = if request.profile == LOCAL_NATIVE_PROFILE {
        let plan = compatibility::capture(config_path, &binding.data_dir, request, operation_id)?;
        Some(plan)
    } else {
        None
    };
    let mut args = vec![
        "--operation-mode".into(),
        "stage".into(),
        "--operation-id".into(),
        operation_id.into(),
        "--action-id".into(),
        action_id(operation_id, "stage"),
        "--operation-record-digest".into(),
        operation_request_digest.into(),
        "--operation-profile".into(),
        request.profile.clone(),
        "--lifecycle-data-dir".into(),
        binding.data_dir.display().to_string(),
        "--expected-instance-id".into(),
        expected.incarnation.clone(),
        "--expected-incarnation".into(),
        expected.incarnation.clone(),
        "--expected-generation".into(),
        expected.generation.to_string(),
        "--expected-config-digest".into(),
        expected.config_digest.clone(),
        "--expected-instance-action-id".into(),
        expected.action_id.clone(),
    ];
    if let Some(compatibility) = &compatibility_plan {
        if let Some(candidate) = &candidate_plan {
            args.extend([
                "--candidate-chain-plan".into(),
                candidate.path.display().to_string(),
                "--candidate-chain-plan-sha256".into(),
                candidate.sha256.clone(),
            ]);
        } else {
            args.extend([
                "--candidate-chain-plan".into(),
                String::new(),
                "--candidate-chain-plan-sha256".into(),
                String::new(),
            ]);
        }
        args.extend([
            "--runtime-compatibility-plan".into(),
            compatibility.path.display().to_string(),
            "--runtime-compatibility-plan-sha256".into(),
            compatibility.sha256.clone(),
        ]);
    }
    let output = run_helper(request, &args)?;
    let readback = actions::binding(config_path)?;
    if readback.data_dir != binding.data_dir || readback.config_digest != binding.config_digest {
        return Err("package stage changed the local lifecycle configuration".into());
    }
    require_owner_matches(readback.current.as_ref(), expected)?;
    require_live_owner(&super::process::status(&binding.data_dir)?, expected)?;
    let (stage_receipt, promotion_receipt, rollback_receipt) = parse_stage_or_promotion(
        &output.stdout,
        operation_id,
        operation_request_digest,
        &request.profile,
    )?
    .ok_or("package helper returned no stage receipt")?;
    if promotion_receipt.is_some() || rollback_receipt.is_some() {
        return Err("package stage receipt differs from exact OperationRecord owner".into());
    }
    validate_stage_binding(
        &stage_receipt,
        request,
        operation_id,
        operation_request_digest,
        expected,
    )?;
    if let Some(candidate) = candidate_plan.as_ref() {
        validate_stage_candidate_plan(&stage_receipt, &candidate.plan, &candidate.sha256)?;
    } else if stage_receipt.candidate_chain_plan_sha256.is_some() {
        return Err("package stage returned an unexpected candidate-chain plan receipt".into());
    }
    match compatibility_plan.as_ref() {
        Some(plan) => {
            compatibility::validate_stage_receipt(&stage_receipt, &plan.path, &plan.sha256)?
        }
        None if stage_receipt.runtime_compatibility_plan_path.is_some()
            || stage_receipt.runtime_compatibility_plan_sha256.is_some() =>
        {
            return Err("package stage returned an unexpected runtime compatibility plan".into());
        }
        None => {}
    }
    Ok(stage_receipt)
}

struct PersistedCandidatePlan {
    plan: candidate_chain::CandidateChainPlanV1,
    path: PathBuf,
    sha256: String,
}

fn prepare_request_candidate_plan(
    config_path: &Path,
    request: &PackageUpgradeV1,
    operation_id: &str,
    binding: &super::actions::LocalBinding,
) -> Result<Option<PersistedCandidatePlan>, String> {
    let Some(input) = request.candidate_chain_config.as_ref() else {
        return Ok(None);
    };
    if request.profile != LOCAL_NATIVE_PROFILE {
        return Err(
            "candidate chain config is supported only for local-native package upgrades".into(),
        );
    }
    let artifact = request
        .native_artifact
        .as_ref()
        .ok_or("candidate chain config has no native runtime artifact")?;
    let plan = candidate_chain::prepare_candidate_chain(config_path, input, artifact, request)?;
    let (path, sha256) =
        candidate_chain::persist_immutable_plan(&binding.data_dir, operation_id, &plan)?;
    Ok(Some(PersistedCandidatePlan { plan, path, sha256 }))
}

fn validate_stage_candidate_plan(
    stage: &PackageStageReceiptV1,
    plan: &candidate_chain::CandidateChainPlanV1,
    plan_sha256: &str,
) -> Result<(), String> {
    candidate_chain::validate_captured_anchor(
        &plan.input,
        &plan.input_bytes,
        &plan.candidate_anchor,
    )?;
    let candidate_node_config: super::model::NodeConfig =
        serde_json::from_str(&plan.candidate_node_config_bytes)
            .map_err(|error| format!("decode immutable candidate node config: {error}"))?;
    let candidate_node_binding: super::model::IdentityBinding =
        serde_json::from_str(&plan.candidate_node_binding_bytes)
            .map_err(|error| format!("decode immutable candidate node binding: {error}"))?;
    if io::digest(&plan.candidate_anchor)? != plan.candidate_anchor_digest
        || candidate_node_config != plan.candidate_node_config
        || candidate_node_binding != plan.candidate_node_binding
        || plan.candidate_node_config.identity.chain != plan.candidate_anchor
        || plan.candidate_node_binding.chain != plan.candidate_anchor
        || io::digest(&plan.candidate_node_config)? != plan.candidate_node_config_digest
        || io::digest(&plan.candidate_node_binding)? != plan.candidate_binding_digest
        || candidate_chain::sha256_bytes(plan.candidate_node_config_bytes.as_bytes())
            != plan.candidate_node_config_bytes_sha256
        || candidate_chain::sha256_bytes(plan.candidate_node_binding_bytes.as_bytes())
            != plan.candidate_node_binding_bytes_sha256
    {
        return Err(
            "immutable candidate-chain plan bytes do not reproduce their bound anchors and config"
                .into(),
        );
    }
    let expected = [
        (
            "candidate_chain_plan_sha256",
            stage.candidate_chain_plan_sha256.as_deref(),
            plan_sha256,
        ),
        (
            "prior_node_config_digest",
            stage.prior_node_config_digest.as_deref(),
            &plan.prior_node_config_digest,
        ),
        (
            "prior_binding_digest",
            stage.prior_binding_digest.as_deref(),
            &plan.prior_binding_digest,
        ),
        (
            "candidate_node_config_digest",
            stage.candidate_node_config_digest.as_deref(),
            &plan.candidate_node_config_digest,
        ),
        (
            "candidate_binding_digest",
            stage.candidate_binding_digest.as_deref(),
            &plan.candidate_binding_digest,
        ),
        (
            "prior_anchor_digest",
            stage.prior_anchor_digest.as_deref(),
            &plan.prior_anchor_digest,
        ),
        (
            "candidate_anchor_digest",
            stage.candidate_anchor_digest.as_deref(),
            &plan.candidate_anchor_digest,
        ),
        (
            "candidate_manifest_sha256",
            stage.candidate_manifest_sha256.as_deref(),
            &plan.candidate_manifest_sha256,
        ),
        (
            "candidate_bundle_sha256",
            stage.candidate_bundle_sha256.as_deref(),
            &plan.candidate_bundle_sha256,
        ),
        (
            "candidate_runtime_sha256",
            stage.candidate_runtime_sha256.as_deref(),
            &plan.candidate_runtime_sha256,
        ),
        (
            "candidate_buildinfo_sha256",
            stage.candidate_buildinfo_sha256.as_deref(),
            &plan.candidate_buildinfo_sha256,
        ),
    ];
    if stage.candidate_chain_config_path.as_ref() != Some(&plan.input.path)
        || stage.candidate_chain_config_sha256.as_deref() != Some(plan.input.sha256.as_str())
    {
        return Err(
            "package stage receipt candidate-chain input pin differs from the immutable request"
                .into(),
        );
    }
    if let Some((field, _, _)) = expected
        .iter()
        .find(|(_, actual, expected)| *actual != Some(*expected))
    {
        return Err(format!(
            "package stage receipt {field} differs from the immutable candidate plan"
        ));
    }
    Ok(())
}

pub(crate) fn query(
    config_path: &Path,
    request: &PackageUpgradeV1,
    operation_id: &str,
    operation_request_digest: &str,
) -> Result<PackageOperationState, String> {
    request.validate_staged()?;
    validate_operation_id(operation_id)?;
    require_digest(operation_request_digest)?;
    // Query reconciles durable promotion/rollback intents. Hold the same
    // transient admission fence used by start and stop so recovery writes
    // cannot race a new local generation. Read node.json directly because a
    // crash may have installed only one member of a config/binding pair;
    // data_dir is invariant across the candidate-chain update.
    let config_path = io::regular(config_path)?;
    let config = super::model::NodeConfig::load(&config_path)?;
    let _admission = super::process::admission_lock(&config.identity.data_dir)?;
    let args = vec![
        "--operation-mode".into(),
        "query".into(),
        "--operation-id".into(),
        operation_id.into(),
        "--action-id".into(),
        action_id(operation_id, "stage"),
        "--operation-record-digest".into(),
        operation_request_digest.into(),
        "--operation-profile".into(),
        request.profile.clone(),
    ];
    let output = run_helper(request, &args)?;
    let state = parse_stage_or_promotion(
        &output.stdout,
        operation_id,
        operation_request_digest,
        &request.profile,
    )?;
    if let Some((stage, promotion, rollback)) = &state {
        validate_progress(
            &PackageUpgradeProgressV1 {
                api_version: "oasis7.local-package-upgrade-progress/v1".into(),
                profile: request.profile.clone(),
                operation_id: operation_id.into(),
                operation_request_digest: operation_request_digest.into(),
                stage_receipt: Some(stage.clone()),
                promotion_receipt: promotion.clone(),
                rollback_receipt: rollback.clone(),
            },
            request,
            operation_id,
            operation_request_digest,
        )?;
    }
    Ok(state)
}

pub(crate) fn promote(
    config_path: &Path,
    request: &PackageUpgradeV1,
    operation_id: &str,
    operation_request_digest: &str,
    expected: &LocalInstance,
    stage: &PackageStageReceiptV1,
    stop_action: &LocalActionRecord,
) -> Result<PackagePromotionReceiptV1, String> {
    request.validate_staged()?;
    validate_operation_id(operation_id)?;
    require_digest(operation_request_digest)?;
    validate_stage_binding(
        stage,
        request,
        operation_id,
        operation_request_digest,
        expected,
    )?;
    let stop_action_id = format!("op:{operation_id}:stop");
    validate_stopped_action(stop_action, expected, &stop_action_id)?;

    // A completed or interrupted helper promotion may already have installed
    // the candidate node config. Read its durable transaction before deriving
    // a new plan from the now-promoted config; a retry must resume the original
    // stage plan, never reinterpret the candidate as a fresh preimage.
    if let Some((returned_stage, promotion, rollback)) =
        query(config_path, request, operation_id, operation_request_digest)?
    {
        if returned_stage != *stage {
            return Err(
                "package promotion retry returned a different immutable stage receipt".into(),
            );
        }
        if rollback.is_some() {
            return Err("package promotion transaction is already rolled back".into());
        }
        if let Some(receipt) = promotion {
            return Ok(receipt);
        }
    }

    let binding = actions::binding(config_path)?;
    // Acquire the lifecycle ownership lock before reading current owner or
    // stop evidence. This keeps the proof and filesystem promotion in one
    // exact local-owner critical section.
    let _admission = super::process::admission_lock(&binding.data_dir)?;
    let _owner_lock = super::process::lock(&binding.data_dir)?;
    require_owner_matches(binding.current.as_ref(), expected)?;
    let current = binding
        .current
        .as_ref()
        .ok_or("stopped package owner missing")?;
    if !matches!(current.state.as_str(), "stopped" | "failed") || current.runtime_pid.is_some() {
        return Err("package promotion requires exact stopped owner with no runtime pid".into());
    }
    let actual_stop = actions::query(&binding.data_dir, &stop_action_id)?
        .ok_or("exact stopped action receipt is missing")?;
    validate_stopped_action(&actual_stop, expected, &stop_action_id)?;
    if io::digest(&actual_stop)? != io::digest(stop_action)? {
        return Err("stop receipt changed before package promotion".into());
    }

    // The package helper independently rereads the owner and stop receipt on
    // this same target host while the native process lock remains held.
    let output = run_helper(
        request,
        &[
            "--operation-mode".into(),
            "promote".into(),
            "--operation-id".into(),
            operation_id.into(),
            "--action-id".into(),
            action_id(operation_id, "stage"),
            "--promotion-action-id".into(),
            action_id(operation_id, "promote"),
            "--operation-request-digest".into(),
            stage.request_digest.clone(),
            "--operation-record-digest".into(),
            operation_request_digest.into(),
            "--operation-profile".into(),
            request.profile.clone(),
            "--lifecycle-data-dir".into(),
            binding.data_dir.display().to_string(),
            "--expected-instance-id".into(),
            expected.incarnation.clone(),
            "--expected-incarnation".into(),
            expected.incarnation.clone(),
            "--expected-generation".into(),
            expected.generation.to_string(),
            "--expected-config-digest".into(),
            expected.config_digest.clone(),
            "--expected-instance-action-id".into(),
            expected.action_id.clone(),
            "--stop-action-id".into(),
            stop_action_id.clone(),
        ],
    )?;
    let Some((returned_stage, Some(receipt), None)) = parse_stage_or_promotion(
        &output.stdout,
        operation_id,
        operation_request_digest,
        &request.profile,
    )?
    else {
        return Err("package promotion helper returned no durable promoted receipt".into());
    };
    if returned_stage != *stage
        || receipt.action_id != action_id(operation_id, "promote")
        || receipt.stop_action_id != stop_action_id
        || receipt.stop_receipt_sha256 != io::digest(stop_action)?
        || io::digest(&receipt.stopped_owner)? != io::digest(&stop_action.target)?
    {
        return Err("package promotion receipt differs from the exact stopped owner".into());
    }
    let readback = actions::binding(config_path)?;
    require_owner_matches(readback.current.as_ref(), expected)?;
    if let Some(candidate_config_digest) = stage.candidate_node_config_digest.as_deref() {
        if readback.config_digest != candidate_config_digest
            || receipt.candidate_node_config_digest.as_deref() != Some(candidate_config_digest)
            || receipt.candidate_binding_digest != stage.candidate_binding_digest
            || receipt.candidate_anchor_digest != stage.candidate_anchor_digest
        {
            return Err(
                "promoted lifecycle config/binding differs from the exact candidate chain plan"
                    .into(),
            );
        }
    } else if readback.config_digest != binding.config_digest
        || receipt.candidate_node_config_digest.is_some()
        || receipt.candidate_binding_digest.is_some()
        || receipt.candidate_anchor_digest.is_some()
    {
        return Err("package promotion unexpectedly changed the lifecycle chain config".into());
    }
    validate_progress(
        &PackageUpgradeProgressV1 {
            api_version: "oasis7.local-package-upgrade-progress/v1".into(),
            profile: request.profile.clone(),
            operation_id: operation_id.into(),
            operation_request_digest: operation_request_digest.into(),
            stage_receipt: Some(stage.clone()),
            promotion_receipt: Some(receipt.clone()),
            rollback_receipt: None,
        },
        request,
        operation_id,
        operation_request_digest,
    )?;
    Ok(receipt)
}

pub(crate) struct RollbackContext<'a> {
    pub(crate) operation_id: &'a str,
    pub(crate) operation_request_digest: &'a str,
    pub(crate) expected: &'a LocalInstance,
    pub(crate) stage: &'a PackageStageReceiptV1,
    pub(crate) promotion: &'a PackagePromotionReceiptV1,
    pub(crate) failed_start: &'a LocalActionRecord,
}

pub(crate) fn rollback(
    config_path: &Path,
    request: &PackageUpgradeV1,
    context: RollbackContext<'_>,
) -> Result<PackageRollbackReceiptV1, String> {
    let RollbackContext {
        operation_id,
        operation_request_digest,
        expected,
        stage,
        promotion,
        failed_start,
    } = context;
    request.validate_staged()?;
    if request.profile != LOCAL_NATIVE_PROFILE {
        return Err("operation-record rollback requires an executable-bound compatibility probe; the Linux/systemd adapter has no verified rollback probe input".into());
    }
    validate_operation_id(operation_id)?;
    require_digest(operation_request_digest)?;
    validate_failed_start(
        request,
        operation_id,
        operation_request_digest,
        expected,
        stage,
        promotion,
        failed_start,
    )?;
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

    let binding = actions::binding(config_path)?;
    let _admission = super::process::admission_lock(&binding.data_dir)?;
    let _owner_lock = super::process::lock(&binding.data_dir)?;
    let current = super::process::read_instance(&binding.data_dir)?;
    require_owner_matches(current.as_ref(), &failed_start.target)?;
    let stored: LocalActionRecord = io::read(&super::process::action_path(
        &binding.data_dir,
        &failed_start.action_id,
    )?)?;
    validate_failed_start(
        request,
        operation_id,
        operation_request_digest,
        expected,
        stage,
        promotion,
        &stored,
    )?;
    if io::digest(&stored)? != io::digest(failed_start)? {
        return Err("failed promoted start receipt changed before package rollback".into());
    }

    let compatibility_path = stage
        .runtime_compatibility_plan_path
        .as_deref()
        .ok_or("package rollback has no immutable executable compatibility plan")?;
    let compatibility_sha256 = stage
        .runtime_compatibility_plan_sha256
        .as_deref()
        .ok_or("package rollback has no immutable compatibility plan digest")?;
    let compatibility_plan = compatibility::load_exact(compatibility_path, compatibility_sha256)?;
    if compatibility_plan.operation_id != operation_id
        || compatibility_plan.profile != request.profile
        || compatibility_plan.candidate_runtime_sha256 != promotion.runtime_sha256
    {
        return Err(
            "rollback compatibility plan differs from the exact staged/promoted artifact".into(),
        );
    }
    // Run the exact prior binary's read-only decoder against the current data
    // before asking the helper to write rollback intent or restore any file.
    compatibility::probe_prior_against_current_data(
        &compatibility_plan,
        config_path,
        &binding.data_dir,
    )?;

    let failed_start_receipt_sha256 = io::digest(failed_start)?;
    let output = run_helper(
        request,
        &[
            "--operation-mode".into(),
            "rollback".into(),
            "--operation-id".into(),
            operation_id.into(),
            "--action-id".into(),
            action_id(operation_id, "stage"),
            "--operation-record-digest".into(),
            operation_request_digest.into(),
            "--operation-request-digest".into(),
            stage.request_digest.clone(),
            "--operation-profile".into(),
            request.profile.clone(),
            "--lifecycle-data-dir".into(),
            binding.data_dir.display().to_string(),
            "--expected-instance-id".into(),
            expected.incarnation.clone(),
            "--expected-incarnation".into(),
            expected.incarnation.clone(),
            "--expected-generation".into(),
            expected.generation.to_string(),
            "--expected-config-digest".into(),
            expected.config_digest.clone(),
            "--expected-instance-action-id".into(),
            expected.action_id.clone(),
            "--promotion-action-id".into(),
            action_id(operation_id, "promote"),
            "--stop-action-id".into(),
            promotion.stop_action_id.clone(),
            "--failed-start-action-id".into(),
            failed_start.action_id.clone(),
            "--failed-start-receipt-sha256".into(),
            failed_start_receipt_sha256,
            "--rollback-action-id".into(),
            action_id(operation_id, "rollback"),
            "--runtime-compatibility-plan".into(),
            compatibility_path.display().to_string(),
            "--runtime-compatibility-plan-sha256".into(),
            compatibility_sha256.into(),
        ],
    )?;
    let Some((returned_stage, Some(returned_promotion), Some(receipt))) = parse_stage_or_promotion(
        &output.stdout,
        operation_id,
        operation_request_digest,
        &request.profile,
    )?
    else {
        return Err("package rollback helper returned no durable rollback receipt".into());
    };
    if returned_stage != *stage || returned_promotion != *promotion {
        return Err("package rollback receipt differs from the exact stage/promotion".into());
    }
    validate_progress(
        &PackageUpgradeProgressV1 {
            api_version: "oasis7.local-package-upgrade-progress/v1".into(),
            profile: request.profile.clone(),
            operation_id: operation_id.into(),
            operation_request_digest: operation_request_digest.into(),
            stage_receipt: Some(stage.clone()),
            promotion_receipt: Some(promotion.clone()),
            rollback_receipt: Some(receipt.clone()),
        },
        request,
        operation_id,
        operation_request_digest,
    )?;
    Ok(receipt)
}
