use super::{io, tree, verify};
use crate::{
    cli::{
        CliOptions,
        cli_contract::{self, OptionKind},
    },
    node_keypair_config,
};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Source {
    pub api_version: String,
    pub cwd: PathBuf,
    /// Exact original executable interpreter; omitted defaults cannot drift.
    pub runtime_binary_sha256: String,
    /// Actual argument tail passed to the old executable, after wrapper expansion.
    pub argv: Vec<String>,
    /// Literal, declared path/identity environment origins, never shell source.
    pub declared_env: BTreeMap<String, String>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Component {
    pub name: String,
    pub source: PathBuf,
    pub target: PathBuf,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Plan {
    pub api_version: String,
    pub operation_id: String,
    pub source_contract: PathBuf,
    pub source_digest: String,
    pub runtime_binary_sha256: String,
    pub destination: PathBuf,
    pub stage: PathBuf,
    pub node_id: String,
    pub world_id: String,
    pub public_key: String,
    pub peer_id: String,
    pub reward_signer_id: String,
    pub reward_signer_public_key: String,
    pub source_argv: Vec<String>,
    pub launch_argv: Vec<String>,
    pub paths: BTreeMap<String, PathBuf>,
    pub components: Vec<Component>,
    pub files: Vec<tree::Entry>,
    pub external_dependencies: Vec<tree::Entry>,
    pub closure: verify::Closure,
    pub proof_scope: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Launch {
    pub api_version: String,
    pub profile: String,
    pub plan_digest: String,
    pub effective_options_digest: String,
    pub runtime_binary_sha256: String,
    pub destination: PathBuf,
    pub argv: Vec<String>,
    pub backup_components: Vec<Component>,
    pub external_dependencies: Vec<tree::Entry>,
    pub node_id: String,
    pub public_key: String,
    pub peer_id: String,
    pub reward_signer_id: String,
    pub reward_signer_public_key: String,
    pub world_id: String,
}
pub(super) const PATH_FLAGS: &[&str] = &[
    "--config",
    "--runtime-root",
    "--execution-bridge-state",
    "--execution-world-dir",
    "--execution-records-dir",
    "--storage-root",
    "--replication-root",
];
const EXTERNAL_PATH_FLAGS: &[&str] = &[
    "--network-tier-manifest",
    "--genesis-validator-registry",
    "--deployment-inventory",
];
pub(super) fn runtime_digest() -> Result<String, String> {
    static DIGEST: std::sync::OnceLock<Result<String, String>> = std::sync::OnceLock::new();
    DIGEST
        .get_or_init(|| io::sha256(&std::env::current_exe().map_err(|e| e.to_string())?))
        .clone()
}
fn absolute(cwd: &Path, path: &Path) -> Result<PathBuf, String> {
    let path = if path.is_absolute() {
        path.to_path_buf()
    } else {
        cwd.join(path)
    };
    io::reject_symlinks(&path)?;
    std::fs::canonicalize(&path)
        .map_err(|e| format!("required legacy material {}: {e}", path.display()))
}
fn normalize(source: &Source) -> Result<Vec<String>, String> {
    if source.api_version != "oasis7.legacy-launch-source/v1" || !source.cwd.is_absolute() {
        return Err("unsupported legacy launch source schema/cwd".into());
    }
    io::reject_symlinks(&source.cwd)?;
    if !source.cwd.is_dir() {
        return Err("legacy cwd missing".into());
    }
    let mut argv = Vec::new();
    let mut args = source.argv.iter();
    while let Some(flag) = args.next() {
        let spec = cli_contract::option(flag)?;
        argv.push(spec.name.into());
        if spec.kind != OptionKind::Switch {
            let mut value = args
                .next()
                .ok_or_else(|| format!("{} requires a value", spec.name))?
                .as_str();
            let escaped = value == "--";
            if escaped {
                value = args.next().ok_or("escape missing value")?;
            }
            if value.starts_with("--") && !escaped {
                return Err("legacy literal flag-shaped value requires unsupported migration escape; use absolute material paths".into());
            }
            if PATH_FLAGS.contains(&spec.name) || EXTERNAL_PATH_FLAGS.contains(&spec.name) {
                argv.push(
                    absolute(&source.cwd, Path::new(value))?
                        .to_str()
                        .ok_or("non UTF8 path")?
                        .into(),
                );
            } else {
                if escaped {
                    argv.push("--".into());
                }
                argv.push(value.into());
            }
        }
    }
    Ok(argv)
}
fn signer_id(options: &CliOptions) -> &str {
    options
        .reward_runtime_signer_node_id
        .as_deref()
        .map(str::trim)
        .filter(|v| !v.is_empty())
        .unwrap_or(&options.node_id)
}
pub(super) fn options(argv: &[String]) -> Result<CliOptions, String> {
    let mut options = crate::parse_options(argv.iter().map(String::as_str))?;
    crate::apply_network_tier_manifest_defaults(&mut options);
    if options.node_role != oasis7_node::NodeRole::Observer
        || options.node_auto_attest_all_validators
        || options.guarded_initial_config.is_some()
        || options.formal_live_config.is_some()
        || !options.provider_backed_bootstrap_authority_paths.is_empty()
        || options.local_test_provider_authority_path.is_some()
        || options.local_test_provider_wasm_path.is_some()
        || options.local_test_provider_metadata_path.is_some()
    {
        return Err("UNSUPPORTED_PROFILE: only legacy observer without auto-attest/provider/guarded/formal authority may migrate".into());
    }
    Ok(options)
}
pub(super) fn replace_paths(
    argv: &[String],
    paths: &BTreeMap<String, PathBuf>,
) -> Result<Vec<String>, String> {
    let mut result = Vec::new();
    let mut args = argv.iter();
    while let Some(flag) = args.next() {
        let spec = cli_contract::option(flag)?;
        if spec.kind == OptionKind::Switch {
            result.push(flag.clone());
            continue;
        }
        let mut value = args.next().ok_or("argument missing value")?;
        let escaped = value == "--";
        if escaped {
            value = args.next().ok_or("argument escape missing value")?;
        }

        if !PATH_FLAGS.contains(&spec.name) {
            result.push(flag.clone());
            if escaped {
                result.push("--".into());
            }
            result.push(value.clone());
        }
    }
    for flag in PATH_FLAGS {
        result.extend([
            flag.to_string(),
            paths
                .get(*flag)
                .ok_or("resolved path missing")?
                .to_str()
                .ok_or("non UTF8 path")?
                .into(),
        ]);
    }
    Ok(result)
}
pub(super) fn plan(source_path: &Path, destination: &Path, id: &str) -> Result<Plan, String> {
    if id.is_empty()
        || id.len() > 128
        || !id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_'))
        || !destination.is_absolute()
    {
        return Err("stable migration ID and absolute destination required".into());
    }
    io::reject_symlinks(destination)?;
    let parent = destination.parent().ok_or("destination parent missing")?;
    crate::controlled_live_config::require_private_owned(parent, true)?;
    let source_path = io::regular(source_path)?;
    let source: Source = io::read(&source_path)?;
    if runtime_digest()? != source.runtime_binary_sha256 {
        return Err("UNSUPPORTED_RUNTIME_VERSION: source must pin the same runtime interpreter; omitted defaults may not be reinterpreted by another binary".into());
    }
    let mut argv = normalize(&source)?;
    let mut opts = options(&argv)?;
    let raw = crate::resolve_runtime_paths(&opts);
    let mut paths = BTreeMap::new();
    for (flag, path) in [
        ("--config", PathBuf::from(&opts.config_path)),
        ("--runtime-root", raw.runtime_root),
        ("--execution-bridge-state", raw.execution_bridge_state_path),
        ("--execution-world-dir", raw.execution_world_dir),
        ("--execution-records-dir", raw.execution_records_dir),
        ("--storage-root", raw.storage_root),
        ("--replication-root", raw.replication_root),
    ] {
        paths.insert(flag.to_string(), absolute(&source.cwd, &path)?);
    }
    argv = replace_paths(&argv, &paths)?;
    opts = options(&argv)?;
    for (name, value) in &source.declared_env {
        let flag = match name.as_str() {
            "CONFIG_PATH" => "--config",
            "RUNTIME_ROOT" => "--runtime-root",
            "EXECUTION_WORLD_DIR" => "--execution-world-dir",
            "EXECUTION_RECORDS_DIR" => "--execution-records-dir",
            "STORAGE_ROOT" => "--storage-root",
            "REPLICATION_ROOT" => "--replication-root",
            "EXECUTION_BRIDGE_STATE" => "--execution-bridge-state",
            "NODE_ID" => {
                if value != &opts.node_id {
                    return Err("declared NODE_ID differs from actual argv".into());
                }
                continue;
            }
            "REWARD_RUNTIME_SIGNER_NODE_ID" => {
                if value != signer_id(&opts) {
                    return Err("declared reward signer differs from argv".into());
                }
                continue;
            }
            _ => return Err(format!("undeclared env interpretation unsupported: {name}")),
        };
        if absolute(&source.cwd, Path::new(value))? != paths[flag] {
            return Err(format!("declared {name} differs from effective argv"));
        }
    }
    let key = node_keypair_config::read_existing_legacy_keypair(&paths["--config"])?;
    let peer = crate::derive_node_libp2p_identity_keypair_config(&opts.node_id, &key)?;
    let peer_id = oasis7_node::derive_libp2p_identity_keypair(&peer.private_key_hex)
        .map_err(|e| format!("{e:?}"))?
        .public()
        .to_peer_id()
        .to_string();
    let signer_id = signer_id(&opts).to_string();
    let signer = crate::derive_node_consensus_signer_keypair(&signer_id, &key)?;
    let mut roots = paths
        .iter()
        .map(|(name, path)| Component {
            name: name.trim_start_matches("--").into(),
            source: path.clone(),
            target: PathBuf::new(),
        })
        .collect::<Vec<_>>();
    let mirror = crate::execution_bridge::simulator_world_dir_from_execution_world_dir(
        &paths["--execution-world-dir"],
    );
    if !mirror.is_dir() {
        return Err("existing simulator mirror is required; migration never creates it".into());
    }
    roots.push(Component {
        name: "simulator-mirror".into(),
        source: mirror,
        target: PathBuf::new(),
    });
    roots.sort_by_key(|c| c.source.components().count());
    let mut components: Vec<Component> = Vec::new();
    for mut component in roots {
        if components
            .iter()
            .any(|parent| component.source.starts_with(&parent.source))
        {
            continue;
        }
        if destination.starts_with(&component.source) || component.source.starts_with(destination) {
            return Err("source/destination overlap".into());
        }
        component.target = if component.name == "simulator-mirror" {
            // The runtime derives this directory as a sibling of the execution
            // world. Keep that relationship after migration even when the two
            // source directories are independent roots and therefore separate
            // components in the staged tree.
            let world_path = &paths["--execution-world-dir"];
            let world_component = components
                .iter()
                .find(|parent| world_path.starts_with(&parent.source))
                .ok_or("execution world component missing before simulator mirror")?;
            let relocated_world = relocated_path(destination, world_component, world_path)?;
            let relocated_mirror =
                crate::execution_bridge::simulator_world_dir_from_execution_world_dir(
                    &relocated_world,
                );
            relocated_mirror
                .strip_prefix(destination)
                .map_err(|_| "relocated simulator mirror escaped migration destination")?
                .to_path_buf()
        } else {
            PathBuf::from("components").join(&component.name)
        };
        components.push(component);
    }
    let lock_path = paths["--execution-world-dir"].with_file_name(format!(
        "{}.lock",
        paths["--execution-world-dir"]
            .file_name()
            .ok_or("world name")?
            .to_string_lossy()
    ));
    let files = tree::inventory(&components, Some(&lock_path))?;
    let mut mapped = BTreeMap::new();
    for (flag, path) in &paths {
        let parent = components
            .iter()
            .find(|c| path.starts_with(&c.source))
            .ok_or("component mapping missing")?;
        mapped.insert(flag.clone(), relocated_path(destination, parent, path)?);
    }
    let launch_argv = replace_paths(&argv, &mapped)?;
    let mut external = Vec::new();
    for flag in EXTERNAL_PATH_FLAGS {
        let mut args = argv.iter();
        while let Some(f) = args.next() {
            let spec = cli_contract::option(f)?;
            if spec.kind != OptionKind::Switch {
                let mut value = args.next().ok_or("argv value missing")?;
                if value == "--" {
                    value = args.next().ok_or("argv escape missing value")?;
                }

                if spec.name == *flag {
                    external.push(tree::file_entry(Path::new(value), PathBuf::new())?);
                }
            }
        }
    }
    // Bundle/artifact references remain external immutable backup dependencies,
    // because rewriting a signed manifest or its absolute refs is not migration.
    if let Some(loaded) = opts.loaded_network_tier_manifest.as_ref() {
        let manifest = opts
            .network_tier_manifest_path
            .as_ref()
            .ok_or("manifest path missing")?;
        for raw in [
            &loaded.manifest.runtime_refs.release_candidate_bundle_ref,
            &loaded.manifest.runtime_refs.genesis_ref,
            &loaded.manifest.runtime_refs.bootstrap_peer_ref,
        ] {
            let path = absolute(manifest.parent().ok_or("manifest parent")?, Path::new(raw))?;
            let deps = tree::inventory(
                &[Component {
                    name: "external".into(),
                    source: path,
                    target: PathBuf::new(),
                }],
                None,
            )?;
            external.extend(deps);
        }
        // Attachments belong to the typed release bundle, not runtime_refs.
        // Reuse its existing integrity contract; unknown JSON fields are never
        // interpreted as dependencies or as authority material.
        let bundle_path = absolute(
            manifest.parent().ok_or("manifest parent")?,
            Path::new(&loaded.manifest.runtime_refs.release_candidate_bundle_ref),
        )?;
        for (_label, path) in crate::cli::inspect_network_tier_bundle_attachments(&bundle_path)? {
            external.extend(tree::inventory(
                &[Component {
                    name: "external".into(),
                    source: path,
                    target: PathBuf::new(),
                }],
                None,
            )?);
        }
    }
    let closure = verify::closure(&paths, &opts.world_id, &opts.node_id)?;
    if let Some(loaded) = opts.loaded_network_tier_manifest.as_ref() {
        let manifest = opts
            .network_tier_manifest_path
            .as_ref()
            .ok_or("manifest missing")?;
        let genesis = absolute(
            manifest.parent().ok_or("manifest parent")?,
            Path::new(&loaded.manifest.runtime_refs.genesis_ref),
        )?;
        let digest = io::sha256(&genesis)?;
        if closure.chain_id != loaded.manifest.chain_id
            || !(closure.genesis_ref == digest || closure.genesis_ref == format!("sha256:{digest}"))
        {
            return Err("legacy world chain/genesis differs from actual manifest material".into());
        }
    }

    let stage = parent.join(format!(".oasis7-legacy-migrate-{id}"));
    let plan = Plan {
        api_version: "oasis7.legacy-migration-plan/v1".into(),
        operation_id: id.into(),
        source_contract: source_path,
        source_digest: io::digest(&source)?,
        runtime_binary_sha256: source.runtime_binary_sha256.clone(),
        destination: destination.into(),
        stage,
        node_id: opts.node_id,
        world_id: opts.world_id,
        public_key: key.public_key_hex,
        peer_id,
        reward_signer_id: signer_id,
        reward_signer_public_key: signer.public_key_hex,
        source_argv: argv,
        launch_argv,
        paths,
        components,
        files,
        external_dependencies: external,
        closure,
        proof_scope: "legacy_compatibility_history_preserved_not_formal_authority".into(),
    };
    if serde_json::to_vec_pretty(&plan)
        .map_err(|e| e.to_string())?
        .len()
        > 4 * 1024 * 1024
    {
        return Err("legacy migration immutable plan metadata size limit".into());
    }
    Ok(plan)
}
pub(super) fn relocated_path(
    root: &Path,
    component: &Component,
    path: &Path,
) -> Result<PathBuf, String> {
    let suffix = path
        .strip_prefix(&component.source)
        .map_err(|e| e.to_string())?;
    let base = root.join(&component.target);
    Ok(if suffix.as_os_str().is_empty() {
        base
    } else {
        base.join(suffix)
    })
}

pub(crate) fn load_launch(path: &Path) -> Result<CliOptions, String> {
    crate::controlled_live_config::require_private_owned(
        path.parent().ok_or("aggregate launch parent missing")?,
        true,
    )?;
    crate::controlled_live_config::require_private_owned(path, false)?;
    let launch: Launch = io::read(path)?;
    if launch.api_version != "oasis7.legacy-runtime-launch/v1"
        || launch.profile != "legacy_observer_compatibility_v1"
        || path != launch.destination.join("legacy-launch.json")
    {
        return Err("unsupported aggregate legacy config/profile/location".into());
    }
    let receipt: Plan = io::read(&launch.destination.join("migration-plan.json"))?;
    if io::digest(&receipt)? != launch.plan_digest
        || receipt.runtime_binary_sha256 != launch.runtime_binary_sha256
        || receipt.launch_argv != launch.argv
        || receipt.destination != launch.destination
        || receipt.node_id != launch.node_id
        || receipt.world_id != launch.world_id
        || receipt.public_key != launch.public_key
        || receipt.peer_id != launch.peer_id
        || receipt.reward_signer_id != launch.reward_signer_id
        || receipt.reward_signer_public_key != launch.reward_signer_public_key
        || receipt.components != launch.backup_components
        || receipt.external_dependencies != launch.external_dependencies
    {
        return Err("aggregate launch differs from immutable migration plan".into());
    }
    if runtime_digest()? != launch.runtime_binary_sha256 {
        return Err(
            "migrated runtime executable changed; explicit compatibility upgrade required".into(),
        );
    }
    let opts = options(&launch.argv)?;
    if io::digest(&format!("{opts:?}"))? != launch.effective_options_digest {
        return Err(
            "aggregate effective runtime options changed; explicit compatibility upgrade required"
                .into(),
        );
    }
    let opts = options(&launch.argv)?;
    let key = node_keypair_config::read_existing_legacy_keypair(Path::new(&opts.config_path))?;
    let signer_id = signer_id(&opts);
    let signer = crate::derive_node_consensus_signer_keypair(signer_id, &key)?;
    let peer = crate::derive_node_libp2p_identity_keypair_config(&opts.node_id, &key)?;
    let peer_id = oasis7_node::derive_libp2p_identity_keypair(&peer.private_key_hex)
        .map_err(|e| format!("{e:?}"))?
        .public()
        .to_peer_id()
        .to_string();
    if key.public_key_hex != launch.public_key
        || peer_id != launch.peer_id
        || signer.public_key_hex != launch.reward_signer_public_key
        || signer_id != launch.reward_signer_id
        || opts.node_id != launch.node_id
        || opts.world_id != launch.world_id
    {
        return Err("migrated key/peer/world/reward signer identity changed".into());
    }
    tree::verify_entries(&launch.external_dependencies, None)?;
    let resolved = crate::resolve_runtime_paths(&opts);
    let paths = BTreeMap::from([
        ("--execution-world-dir".into(), resolved.execution_world_dir),
        (
            "--execution-records-dir".into(),
            resolved.execution_records_dir,
        ),
        (
            "--execution-bridge-state".into(),
            resolved.execution_bridge_state_path,
        ),
        ("--storage-root".into(), resolved.storage_root),
        ("--replication-root".into(), resolved.replication_root),
    ]);
    let current = verify::closure(&paths, &opts.world_id, &opts.node_id)?;
    if current.chain_id != receipt.closure.chain_id
        || current.genesis_ref != receipt.closure.genesis_ref
        || current.committed_height < receipt.closure.committed_height
    {
        return Err("migrated runtime world/genesis/head regressed".into());
    }
    let cut: serde_json::Value = io::read(
        &paths["--execution-records-dir"]
            .join(format!("{:020}.json", receipt.closure.committed_height)),
    )?;
    if cut["execution_block_hash"].as_str() != Some(&receipt.closure.execution_block_hash)
        || cut["execution_state_root"].as_str() != Some(&receipt.closure.committed_state_root)
        || io::digest(&cut)? != receipt.closure.committed_record_digest
    {
        return Err("migrated runtime history diverged from original migration cut".into());
    }
    Ok(opts)
}
