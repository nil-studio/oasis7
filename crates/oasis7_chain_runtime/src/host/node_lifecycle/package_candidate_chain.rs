//! Candidate-chain configuration validation for local-native package upgrades.
//!
//! The package operation, node receipts, and shell transaction carry the same
//! candidate plan. Its authorization input is an operator-pinned config path
//! and SHA256; it is not a chain signature or a claim that the candidate
//! issued trust.

use super::super::{
    local_io as io,
    model::{ChainAnchor, ChainConfig, CompatibilityChainAnchor, IdentityBinding, NodeConfig},
};
use super::{LOCAL_NATIVE_PROFILE, LocalNativePackageArtifactV1, PackageUpgradeV1};
use oasis7::network_tier_manifest::LoadedNetworkTierManifest;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    fs,
    os::unix::fs::{MetadataExt, OpenOptionsExt, PermissionsExt},
    path::{Path, PathBuf},
};

const INPUT_API: &str = "oasis7.operator-pinned-candidate-chain-config/v1";
const PLAN_API: &str = "oasis7.local-native-candidate-chain-plan/v1";

/// Explicit operator input. The path is pinned to the exact bytes before
/// staging and is copied into the immutable package transaction.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct CandidateChainConfigInputV1 {
    pub api_version: String,
    pub path: PathBuf,
    pub sha256: String,
}

/// Durable material needed to install or restore both lifecycle config files.
/// The helper persists this exact plan in the package transaction before the
/// old node is stopped. Snapshot bytes and metadata preserve the exact prior
/// config rather than reconstructing it during rollback.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct CandidateChainPlanV1 {
    pub api_version: String,
    pub input: CandidateChainConfigInputV1,
    pub input_bytes: String,
    pub candidate_material: Vec<CandidateChainMaterialV1>,
    pub prior_node_config: ConfigFileSnapshotV1,
    pub prior_node_binding: ConfigFileSnapshotV1,
    pub candidate_node_config_path: PathBuf,
    pub candidate_node_config_bytes: String,
    pub candidate_node_config_bytes_sha256: String,
    pub candidate_node_binding_path: PathBuf,
    pub candidate_node_binding_bytes: String,
    pub candidate_node_binding_bytes_sha256: String,
    pub prior_node_config_digest: String,
    pub prior_binding_digest: String,
    pub candidate_node_config_digest: String,
    pub candidate_binding_digest: String,
    pub prior_anchor_digest: String,
    pub candidate_anchor_digest: String,
    pub prior_manifest_sha256: String,
    pub candidate_manifest_sha256: String,
    pub prior_bundle_sha256: String,
    pub candidate_bundle_sha256: String,
    pub candidate_runtime_sha256: String,
    pub candidate_buildinfo_sha256: String,
    pub prior_anchor: ChainAnchor,
    pub candidate_anchor: ChainAnchor,
    pub candidate_node_config: NodeConfig,
    pub candidate_node_binding: IdentityBinding,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct CandidateChainMaterialV1 {
    pub role: String,
    pub path: PathBuf,
    pub sha256: String,
    pub bytes: String,
    pub uid: u32,
    pub gid: u32,
    pub mode: u32,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ConfigFileSnapshotV1 {
    pub path: PathBuf,
    pub bytes: String,
    pub sha256: String,
    pub uid: u32,
    pub gid: u32,
    pub mode: u32,
}

impl CandidateChainConfigInputV1 {
    pub(crate) fn validate_shape(&self) -> Result<(), String> {
        if self.api_version != INPUT_API || !self.path.is_absolute() || !is_sha256(&self.sha256) {
            return Err("candidate chain config requires its API version, absolute path, and canonical SHA256".into());
        }
        Ok(())
    }

    fn read_exact(&self) -> Result<String, String> {
        self.validate_shape()?;
        let path = io::regular(&self.path)?;
        if path != self.path {
            return Err(
                "candidate chain config path must be canonical and contain no symlink".into(),
            );
        }
        let bytes = fs::read(&self.path).map_err(|error| error.to_string())?;
        if sha256_bytes(&bytes) != self.sha256 {
            return Err("operator-pinned candidate chain config SHA256 mismatch".into());
        }
        String::from_utf8(bytes).map_err(|_| "candidate chain config must be UTF-8 JSON".into())
    }
}

/// Derive the exact candidate node config from the current identity, changing
/// only its chain anchor. This rejects changes to the node key, role, data
/// directory, RPC bind, and peer identity.
pub(crate) fn prepare_candidate_chain(
    current_config_path: &Path,
    input: &CandidateChainConfigInputV1,
    artifact: &LocalNativePackageArtifactV1,
    package: &PackageUpgradeV1,
) -> Result<CandidateChainPlanV1, String> {
    if package.profile != LOCAL_NATIVE_PROFILE {
        return Err(
            "candidate chain config is supported only by local-native package upgrades".into(),
        );
    }
    if package.native_artifact.as_ref() != Some(artifact) {
        return Err(
            "candidate chain validation artifact differs from the immutable package request".into(),
        );
    }

    let input_bytes = input.read_exact()?;
    let candidate_anchor = ChainConfig::load_bytes(&input.path, input_bytes.as_bytes())?;
    validate_captured_anchor(input, &input_bytes, &candidate_anchor)?;
    candidate_anchor.verify()?;
    if io::sha256(&input.path)? != input.sha256 {
        return Err("candidate chain config changed while it was being validated".into());
    }

    let requested_current_config_path = current_config_path.to_path_buf();
    let current_config_path = io::regular(&requested_current_config_path)?;
    if current_config_path != requested_current_config_path {
        return Err("current node config path must be canonical and contain no symlink".into());
    }
    let current_config = NodeConfig::load(&current_config_path)?;
    current_config.identity.chain.verify()?;
    let prior_node_config_digest = io::digest(&current_config)?;
    let prior_anchor = current_config.identity.chain.clone();
    let prior_binding_path = current_config.identity.data_dir.join("node-binding.json");
    let prior_binding: IdentityBinding = io::read(&prior_binding_path)?;
    if prior_binding != current_config.identity {
        return Err("current node binding differs from the exact running configuration".into());
    }
    let prior_binding_digest = io::digest(&prior_binding)?;
    let prior_node_config = capture_file(&current_config_path)?;
    let prior_node_binding = capture_file(&prior_binding_path)?;

    let (prior_compatibility, candidate_compatibility) = match (
        prior_anchor.compatibility(),
        candidate_anchor.compatibility(),
    ) {
        (Ok(prior), Ok(candidate)) => (prior, candidate),
        _ => {
            return Err(
                "candidate chain update cannot change the compatibility runtime profile".into(),
            );
        }
    };
    require_same_chain_inputs(prior_compatibility, candidate_compatibility)?;
    if prior_compatibility.manifest.path == candidate_compatibility.manifest.path
        || prior_compatibility.release_bundle.path == candidate_compatibility.release_bundle.path
    {
        return Err("candidate chain update requires distinct manifest and release-bundle files so rollback retains the original pins".into());
    }
    let candidate_material = vec![
        capture_material("chain_config", &input.path, &input.sha256)?,
        capture_material(
            "manifest",
            &candidate_compatibility.manifest.path,
            &candidate_compatibility.manifest.sha256,
        )?,
        capture_material(
            "genesis",
            &candidate_compatibility.genesis.path,
            &candidate_compatibility.genesis.sha256,
        )?,
        capture_material(
            "registry",
            &candidate_compatibility.registry.path,
            &candidate_compatibility.registry.sha256,
        )?,
        capture_material(
            "release_bundle",
            &candidate_compatibility.release_bundle.path,
            &candidate_compatibility.release_bundle.sha256,
        )?,
        capture_material(
            "bootstrap_peers",
            &candidate_compatibility.bootstrap_peers.path,
            &candidate_compatibility.bootstrap_peers.sha256,
        )?,
    ];
    let candidate_material_bytes =
        serde_json::to_vec(&candidate_material).map_err(|error| error.to_string())?;
    if candidate_material_bytes.len() > 2 * 1024 * 1024 {
        return Err(
            "candidate pinned chain material exceeds the bounded package request size".into(),
        );
    }

    let prior_manifest = LoadedNetworkTierManifest::load(&prior_compatibility.manifest.path)?;
    prior_manifest.validate_runtime_support()?;
    let candidate_manifest =
        LoadedNetworkTierManifest::load(&candidate_compatibility.manifest.path)?;
    candidate_manifest.validate_runtime_support()?;
    if !same_manifest_except_bundle_reference(
        &prior_compatibility.manifest.path,
        &candidate_compatibility.manifest.path,
    )? {
        return Err("candidate network-tier manifest changes policy or chain inputs outside its runtime bundle reference".into());
    }

    let (prior_bundle_path, prior_runtime_sha256) =
        approved_bundle_runtime_sha256(&prior_compatibility.manifest.path, &prior_manifest)?;
    let (candidate_bundle_path, candidate_runtime_sha256) = approved_bundle_runtime_sha256(
        &candidate_compatibility.manifest.path,
        &candidate_manifest,
    )?;
    if prior_runtime_sha256 == candidate_runtime_sha256 {
        return Err("candidate chain transaction requires a different runtime SHA256".into());
    }
    if candidate_runtime_sha256 != artifact.runtime_sha256
        || io::sha256(&artifact.runtime_binary)? != candidate_runtime_sha256
    {
        return Err(
            "candidate runtime binary does not match its manifest-bound release bundle".into(),
        );
    }
    if !same_bundle_outside_runtime_build(&prior_bundle_path, &candidate_bundle_path)? {
        return Err("candidate release bundle changes world, sidecar, evidence, or provenance fields outside runtime_build".into());
    }
    validate_buildinfo(package, artifact)?;
    validate_runtime_build_metadata(&candidate_bundle_path, artifact, package)?;

    let mut candidate_node_config = current_config.clone();
    candidate_node_config.identity.chain = candidate_anchor.clone();
    let candidate_node_binding = candidate_node_config.identity.clone();
    let candidate_node_config_bytes =
        serde_json::to_string_pretty(&candidate_node_config).map_err(|error| error.to_string())?;
    let candidate_node_binding_bytes =
        serde_json::to_string_pretty(&candidate_node_binding).map_err(|error| error.to_string())?;
    let candidate_node_config_bytes_sha256 = sha256_bytes(candidate_node_config_bytes.as_bytes());
    let candidate_node_binding_bytes_sha256 = sha256_bytes(candidate_node_binding_bytes.as_bytes());
    let candidate_node_config_digest = io::digest(&candidate_node_config)?;
    let candidate_binding_digest = io::digest(&candidate_node_binding)?;
    let candidate_node_config_path = current_config_path;
    let candidate_node_binding_path = prior_binding_path;
    let candidate_anchor_digest = io::digest(&candidate_anchor)?;
    let prior_anchor_digest = io::digest(&prior_anchor)?;
    let prior_manifest_sha256 = prior_compatibility.manifest.sha256.clone();
    let candidate_manifest_sha256 = candidate_compatibility.manifest.sha256.clone();
    let prior_bundle_sha256 = prior_compatibility.release_bundle.sha256.clone();
    let candidate_bundle_sha256 = candidate_compatibility.release_bundle.sha256.clone();

    Ok(CandidateChainPlanV1 {
        api_version: PLAN_API.into(),
        input: input.clone(),
        input_bytes,
        candidate_material,
        prior_node_config,
        prior_node_binding,
        candidate_node_config_path,
        candidate_node_config_bytes,
        candidate_node_config_bytes_sha256,
        candidate_node_binding_path,
        candidate_node_binding_bytes,
        candidate_node_binding_bytes_sha256,
        prior_node_config_digest,
        prior_binding_digest,
        candidate_node_config_digest,
        candidate_binding_digest,
        prior_anchor_digest,
        candidate_anchor_digest,
        prior_manifest_sha256,
        candidate_manifest_sha256,
        prior_bundle_sha256,
        candidate_bundle_sha256,
        candidate_runtime_sha256,
        candidate_buildinfo_sha256: artifact.buildinfo_sha256.clone(),
        prior_anchor,
        candidate_anchor,
        candidate_node_config,
        candidate_node_binding,
    })
}

fn capture_material(
    role: &str,
    path: &Path,
    expected_sha256: &str,
) -> Result<CandidateChainMaterialV1, String> {
    let path = io::regular(path)?;
    let metadata = fs::metadata(&path).map_err(|error| error.to_string())?;
    if metadata.len() > 2 * 1024 * 1024 {
        return Err(format!(
            "candidate pinned {role} material exceeds the bounded input size"
        ));
    }
    let bytes = fs::read(&path).map_err(|error| error.to_string())?;
    if sha256_bytes(&bytes) != expected_sha256 {
        return Err(format!(
            "candidate pinned {role} SHA256 changed during admission"
        ));
    }
    let bytes = String::from_utf8(bytes)
        .map_err(|_| format!("candidate pinned {role} material must be UTF-8"))?;
    Ok(CandidateChainMaterialV1 {
        role: role.into(),
        path,
        sha256: expected_sha256.into(),
        bytes,
        uid: metadata.uid(),
        gid: metadata.gid(),
        mode: metadata.permissions().mode() & 0o7777,
    })
}

pub(crate) fn persist_immutable_plan(
    data_dir: &Path,
    operation_id: &str,
    plan: &CandidateChainPlanV1,
) -> Result<(PathBuf, String), String> {
    if operation_id.is_empty()
        || operation_id.len() > 128
        || !operation_id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
        || operation_id.contains("..")
    {
        return Err("candidate-chain operation ID is not a safe path token".into());
    }
    let directory = data_dir.join("package-upgrade-candidates");
    io::private_dir(&directory)?;
    let path = directory.join(format!("{operation_id}.json"));
    let bytes = serde_json::to_vec(plan).map_err(|error| error.to_string())?;
    if bytes.len() > 4 * 1024 * 1024 {
        return Err("candidate-chain plan exceeds the node metadata size limit".into());
    }
    match fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(&path)
    {
        Ok(mut file) => {
            use std::io::Write;
            file.write_all(&bytes).map_err(|error| error.to_string())?;
            file.sync_all().map_err(|error| error.to_string())?;
            io::sync_dir(&directory)?;
        }
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
            let metadata = fs::symlink_metadata(&path).map_err(|error| error.to_string())?;
            if !metadata.is_file()
                || metadata.file_type().is_symlink()
                || metadata.permissions().mode() & 0o777 != 0o600
                || fs::read(&path).map_err(|error| error.to_string())? != bytes
            {
                return Err(
                    "candidate-chain plan path already contains different or unsafe bytes".into(),
                );
            }
        }
        Err(error) => return Err(error.to_string()),
    }
    let digest = io::sha256(&path)?;
    Ok((path, digest))
}

/// Re-derive the candidate anchor from the exact immutable input bytes carried
/// by the package plan. The path digest alone is insufficient if another
/// reader reparses a mutable source after the digest was captured.
pub(crate) fn validate_captured_anchor(
    input: &CandidateChainConfigInputV1,
    input_bytes: &str,
    candidate_anchor: &ChainAnchor,
) -> Result<(), String> {
    input.validate_shape()?;
    if sha256_bytes(input_bytes.as_bytes()) != input.sha256 {
        return Err("captured candidate chain bytes differ from the operator-pinned SHA256".into());
    }
    let decoded = ChainConfig::load_bytes(&input.path, input_bytes.as_bytes())?;
    decoded.verify()?;
    if &decoded != candidate_anchor {
        return Err(
            "candidate anchor was not derived from the exact operator-pinned config bytes".into(),
        );
    }
    Ok(())
}

fn capture_file(path: &Path) -> Result<ConfigFileSnapshotV1, String> {
    let path = io::regular(path)?;
    let metadata = fs::metadata(&path).map_err(|error| error.to_string())?;
    let bytes = fs::read(&path).map_err(|error| error.to_string())?;
    if bytes.len() > 4 * 1024 * 1024 {
        return Err("candidate lifecycle config exceeds the node metadata size limit".into());
    }
    let bytes = String::from_utf8(bytes)
        .map_err(|_| "candidate lifecycle config must be UTF-8 JSON".to_string())?;
    Ok(ConfigFileSnapshotV1 {
        path,
        sha256: sha256_bytes(bytes.as_bytes()),
        bytes,
        uid: metadata.uid(),
        gid: metadata.gid(),
        mode: metadata.permissions().mode() & 0o7777,
    })
}

fn require_same_chain_inputs(
    prior: &CompatibilityChainAnchor,
    candidate: &CompatibilityChainAnchor,
) -> Result<(), String> {
    if prior.runtime_profile != candidate.runtime_profile
        || prior.world_id != candidate.world_id
        || prior.chain_id != candidate.chain_id
        || prior.genesis != candidate.genesis
        || prior.registry != candidate.registry
        || prior.bootstrap_peers != candidate.bootstrap_peers
    {
        return Err("candidate chain update changes the pinned world, genesis, registry, or bootstrap peers".into());
    }
    Ok(())
}

fn approved_bundle_runtime_sha256(
    manifest_path: &Path,
    manifest: &LoadedNetworkTierManifest,
) -> Result<(PathBuf, String), String> {
    super::super::super::cli::approved_runtime_sha256_for_network_tier_bundle(
        manifest_path,
        manifest,
    )
}

fn same_manifest_except_bundle_reference(prior: &Path, candidate: &Path) -> Result<bool, String> {
    let mut prior = read_json(prior)?;
    let mut candidate = read_json(candidate)?;
    normalize_manifest_bundle_reference(&mut prior)?;
    normalize_manifest_bundle_reference(&mut candidate)?;
    Ok(prior == candidate)
}

fn normalize_manifest_bundle_reference(manifest: &mut Value) -> Result<(), String> {
    let reference = manifest
        .get_mut("runtime_refs")
        .and_then(Value::as_object_mut)
        .and_then(|runtime_refs| runtime_refs.get_mut("release_candidate_bundle_ref"))
        .ok_or("network-tier manifest omits its release candidate bundle reference")?;
    *reference = Value::String("<operator-approved-candidate-bundle>".into());
    Ok(())
}

fn same_bundle_outside_runtime_build(prior: &Path, candidate: &Path) -> Result<bool, String> {
    let mut prior = read_json(prior)?;
    let mut candidate = read_json(candidate)?;
    let prior_build = take_runtime_build(&mut prior)?;
    let candidate_build = take_runtime_build(&mut candidate)?;
    validate_runtime_build_shape(&prior_build, false)?;
    validate_runtime_build_shape(&candidate_build, true)?;
    Ok(prior == candidate)
}

fn take_runtime_build(bundle: &mut Value) -> Result<Value, String> {
    bundle
        .as_object_mut()
        .and_then(|object| object.remove("runtime_build"))
        .ok_or_else(|| "release candidate bundle omits runtime_build".to_string())
}

fn validate_runtime_build_shape(runtime_build: &Value, candidate: bool) -> Result<(), String> {
    const ALLOWED: &[&str] = &[
        "commit",
        "file_count",
        "git_commit",
        "kind",
        "package_version",
        "path",
        "ref",
        "resolved_path",
        "run_id",
        "sha256",
        "sha256_tree",
        "size_bytes",
        "total_bytes",
        "updated_by",
    ];
    let object = runtime_build
        .as_object()
        .ok_or("runtime_build must be a JSON object")?;
    if object.keys().any(|key| !ALLOWED.contains(&key.as_str())) {
        return Err(
            "runtime_build contains an unrecognized field; candidate provenance cannot be inferred"
                .into(),
        );
    }
    let digest = object
        .get("sha256")
        .and_then(Value::as_str)
        .ok_or("runtime_build.sha256 is required")?;
    if !is_sha256(digest) {
        return Err("runtime_build.sha256 must be canonical lowercase SHA256".into());
    }
    if candidate {
        if object.get("kind").and_then(Value::as_str) != Some("file") {
            return Err("candidate runtime_build.kind must identify a file".into());
        }
        for name in ["ref", "resolved_path", "path", "updated_by"] {
            if object.get(name).is_some_and(|value| {
                value
                    .as_str()
                    .is_none_or(|text| text.trim().is_empty() || text.len() > 2048)
            }) {
                return Err(format!(
                    "candidate runtime_build.{name} must be a bounded non-empty string"
                ));
            }
        }
        for name in ["size_bytes", "file_count", "total_bytes"] {
            if object
                .get(name)
                .is_some_and(|value| value.as_u64().is_none())
            {
                return Err(format!(
                    "candidate runtime_build.{name} must be a non-negative integer"
                ));
            }
        }
        if object.get("kind").and_then(Value::as_str) == Some("file")
            && (object.contains_key("sha256_tree")
                || object.contains_key("file_count")
                || object.contains_key("total_bytes"))
        {
            return Err(
                "candidate file runtime_build cannot contain directory-only digest or size fields"
                    .into(),
            );
        }
    }
    Ok(())
}

fn validate_runtime_build_metadata(
    candidate_bundle_path: &Path,
    artifact: &LocalNativePackageArtifactV1,
    package: &PackageUpgradeV1,
) -> Result<(), String> {
    let bundle = read_json(candidate_bundle_path)?;
    let runtime_build = bundle
        .get("runtime_build")
        .and_then(Value::as_object)
        .ok_or("candidate release bundle omits runtime_build")?;
    validate_runtime_build_shape(&Value::Object(runtime_build.clone()), true)?;
    let fields = read_buildinfo(&artifact.buildinfo)?;
    for (bundle_name, buildinfo_name) in [
        ("package_version", "package_version"),
        ("git_commit", "commit"),
        ("commit", "commit"),
        ("run_id", "run_id"),
    ] {
        if let Some(value) = runtime_build.get(bundle_name) {
            let value = value
                .as_str()
                .ok_or_else(|| format!("candidate runtime_build.{bundle_name} must be a string"))?;
            if fields.get(buildinfo_name).map(String::as_str) != Some(value) {
                return Err(format!(
                    "candidate runtime_build.{bundle_name} differs from BUILDINFO.{buildinfo_name}"
                ));
            }
        }
    }
    if runtime_build.get("sha256").and_then(Value::as_str) != Some(artifact.runtime_sha256.as_str())
        || runtime_build
            .get("size_bytes")
            .and_then(Value::as_u64)
            .is_some_and(|size| {
                fs::metadata(&artifact.runtime_binary).map(|m| m.len()).ok() != Some(size)
            })
        || fields.get("package_version").map(String::as_str)
            != Some(package.package_version.as_str())
        || fields.get("commit").map(String::as_str) != Some(package.commit.as_str())
        || fields.get("run_id").map(String::as_str) != Some(package.run_id.as_str())
    {
        return Err("candidate runtime_build and BUILDINFO do not bind the exact package version, commit, run, and binary".into());
    }
    let actual_runtime = io::regular(&artifact.runtime_binary)?;
    for name in ["resolved_path", "path"] {
        if let Some(value) = runtime_build.get(name).and_then(Value::as_str)
            && Path::new(value).is_absolute()
        {
            let candidate_runtime = package.node_root.join("current/bin/oasis7_chain_runtime");
            if Path::new(value) != actual_runtime.as_path()
                && Path::new(value) != candidate_runtime.as_path()
            {
                return Err(format!(
                    "candidate runtime_build.{name} is not the candidate artifact or future current runtime path"
                ));
            }
        }
    }
    Ok(())
}

fn read_buildinfo(path: &Path) -> Result<std::collections::BTreeMap<String, String>, String> {
    io::regular(path)?;
    let raw = fs::read_to_string(path).map_err(|error| error.to_string())?;
    let mut fields = std::collections::BTreeMap::new();
    for line in raw.lines() {
        let (key, value) = line
            .split_once('=')
            .ok_or("candidate BUILDINFO contains a malformed line")?;
        if key.trim().is_empty() || fields.insert(key.to_owned(), value.to_owned()).is_some() {
            return Err("candidate BUILDINFO contains an empty or duplicate field".into());
        }
    }
    Ok(fields)
}

fn read_json(path: &Path) -> Result<Value, String> {
    let bytes = fs::read(io::regular(path)?).map_err(|error| error.to_string())?;
    serde_json::from_slice(&bytes).map_err(|error| error.to_string())
}

fn validate_buildinfo(
    package: &PackageUpgradeV1,
    artifact: &LocalNativePackageArtifactV1,
) -> Result<(), String> {
    if io::sha256(&artifact.buildinfo)? != artifact.buildinfo_sha256
        || io::sha256(&artifact.runtime_binary)? != artifact.runtime_sha256
    {
        return Err("candidate runtime or BUILDINFO changed after request admission".into());
    }
    let fields = read_buildinfo(&artifact.buildinfo)?;
    if fields.get("platform").map(String::as_str) != Some("local-native")
        || fields.get("package_version").map(String::as_str)
            != Some(package.package_version.as_str())
        || fields.get("commit").map(String::as_str) != Some(package.commit.as_str())
        || fields.get("run_id").map(String::as_str) != Some(package.run_id.as_str())
        || fields.get("runtime_sha256").map(String::as_str)
            != Some(artifact.runtime_sha256.as_str())
    {
        return Err(
            "candidate BUILDINFO does not bind the approved package provenance and runtime".into(),
        );
    }
    Ok(())
}

fn is_sha256(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

pub(super) fn sha256_bytes(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    hex::encode(Sha256::digest(bytes))
}
