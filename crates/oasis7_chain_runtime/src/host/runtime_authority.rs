use oasis7::network_tier_manifest::LoadedNetworkTierManifest;
use oasis7_node::NodeRole;
use oasis7_runtime::{GovernanceFinalitySignerRegistry, World as RuntimeWorld};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::fs;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

#[path = "runtime_authority_observer.rs"]
mod observer;

const MANAGED_TRIAD_NODE_IDS: [&str; 3] = [
    "triad-testnet-sequencer",
    "triad-testnet-storage",
    "triad-testnet-validator-47",
];

static AUTHORITY_PREFLIGHT_SEQUENCE: AtomicU64 = AtomicU64::new(0);

/// Immutable deployment authority facts captured from the inputs used by this
/// runtime instance.  This is deliberately separate from NodeSnapshot: the
/// deployment files are startup authority, not mutable consensus state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct RuntimeAuthorityBinding {
    pub(super) registry_ref: String,
    pub(super) registry_sha256: String,
    pub(super) registry_semantic_sha256: String,
    pub(super) inventory_ref: String,
    pub(super) inventory_sha256: String,
    pub(super) validator_signer_public_keys: BTreeMap<String, String>,
    pub(super) validator_stakes: BTreeMap<String, u64>,
    pub(super) validator_47_provider_peer_id: Option<String>,
}

/// Load and validate the deployment authority consumed by a public-testnet
/// runtime.  A partial binding is rejected, as is a deployment inventory that
/// is not bound by the loaded network-tier manifest.  No repository-local
/// path, digest, or signer is synthesized here.
#[cfg(test)]
pub(super) fn load_runtime_authority_binding(
    execution_world_dir: &Path,
    registry_path: Option<&Path>,
    inventory_path: Option<&Path>,
    loaded_network_tier_manifest: Option<&LoadedNetworkTierManifest>,
) -> Result<Option<RuntimeAuthorityBinding>, String> {
    load_runtime_authority_binding_for_node(
        execution_world_dir,
        "",
        NodeRole::Sequencer,
        registry_path,
        inventory_path,
        loaded_network_tier_manifest,
    )
}

/// Load authority with the node identity and effective runtime role available.
///
/// Managed triad identities are checked before role compatibility so an env or
/// CLI role override cannot downgrade a validator into the observer contract.
/// Non-managed observers use a manifest-bound registry authority because the
/// managed inventory is intentionally specific to the three-equal-validator
/// topology and is not a valid authority for the two-validator observer sync
/// adapter.
#[cfg(test)]
pub(super) fn load_runtime_authority_binding_for_node(
    execution_world_dir: &Path,
    node_id: &str,
    node_role: NodeRole,
    registry_path: Option<&Path>,
    inventory_path: Option<&Path>,
    loaded_network_tier_manifest: Option<&LoadedNetworkTierManifest>,
) -> Result<Option<RuntimeAuthorityBinding>, String> {
    let default_world_id = loaded_network_tier_manifest
        .map(|loaded| loaded.manifest.chain_id.as_str())
        .unwrap_or_default();
    load_runtime_authority_binding_for_node_with_world_id(
        execution_world_dir,
        node_id,
        node_role,
        default_world_id,
        registry_path,
        inventory_path,
        loaded_network_tier_manifest,
    )
}

pub(super) fn load_runtime_authority_binding_for_node_with_world_id(
    execution_world_dir: &Path,
    node_id: &str,
    node_role: NodeRole,
    effective_world_id: &str,
    registry_path: Option<&Path>,
    inventory_path: Option<&Path>,
    loaded_network_tier_manifest: Option<&LoadedNetworkTierManifest>,
) -> Result<Option<RuntimeAuthorityBinding>, String> {
    if let Some(loaded) = loaded_network_tier_manifest {
        validate_effective_world_id(effective_world_id, loaded)?;
    }
    if is_managed_triad_node_id(node_id)
        && node_role == NodeRole::Observer
        && registry_path.is_some()
        && inventory_path.is_none()
    {
        return Err(
            "managed triad startup requires --deployment-inventory for the effective node identity"
                .to_string(),
        );
    }
    validate_managed_node_role(node_id, node_role)?;
    if node_role == NodeRole::Observer
        && !is_managed_triad_node_id(node_id)
        && inventory_path.is_none()
        && let Some(registry_path) = registry_path
    {
        return observer::load_observer_registry_authority(
            execution_world_dir,
            effective_world_id,
            registry_path,
            loaded_network_tier_manifest,
        );
    }
    match (registry_path, inventory_path) {
        (None, None) => {
            if loaded_network_tier_manifest
                .is_some_and(|loaded| loaded.manifest.tier == "public_testnet")
            {
                return Err(
                    "public-testnet deployment authority requires --genesis-validator-registry and --deployment-inventory"
                        .to_string(),
                );
            }
            Ok(None)
        }
        (None, Some(_)) => {
            Err("deployment inventory authority requires --genesis-validator-registry".to_string())
        }
        (Some(_), None) => {
            Err("genesis validator registry authority requires --deployment-inventory".to_string())
        }
        (Some(registry_path), Some(inventory_path)) => {
            if !node_id.is_empty() && !is_managed_triad_node_id(node_id) {
                return Err(format!(
                    "full deployment authority requires a canonical managed triad identity, got {node_id}"
                ));
            }
            let loaded = loaded_network_tier_manifest.ok_or_else(|| {
                "deployment authority binding requires --network-tier-manifest".to_string()
            })?;
            let manifest_path = Path::new(loaded.source_path.as_str());
            let manifest_bytes = fs::read(manifest_path).map_err(|err| {
                format!(
                    "read network-tier manifest {} for deployment authority binding failed: {err}",
                    manifest_path.display()
                )
            })?;
            let manifest: serde_json::Value = serde_json::from_slice(manifest_bytes.as_slice())
                .map_err(|err| {
                    format!(
                        "parse network-tier manifest {} for deployment authority binding failed: {err}",
                        manifest_path.display()
                    )
                })?;
            let inventory_binding = manifest
                .get("deployment_inventory")
                .and_then(serde_json::Value::as_object)
                .ok_or_else(|| {
                    "network-tier manifest is missing deployment_inventory authority binding"
                        .to_string()
                })?;
            let inventory_ref = required_string(inventory_binding, "ref", "deployment_inventory")?;
            let expected_inventory_sha256 =
                required_sha256(inventory_binding, "sha256", "deployment_inventory")?;
            let inventory_sha256 = sha256_regular_file(inventory_path, "deployment inventory")?;
            if inventory_sha256 != expected_inventory_sha256 {
                return Err(format!(
                    "deployment inventory digest mismatch: manifest={} actual={} path={}",
                    expected_inventory_sha256,
                    inventory_sha256,
                    inventory_path.display()
                ));
            }
            let inventory_bytes = fs::read(inventory_path).map_err(|err| {
                format!(
                    "read deployment inventory {} failed: {err}",
                    inventory_path.display()
                )
            })?;
            let inventory: serde_json::Value = serde_json::from_slice(inventory_bytes.as_slice())
                .map_err(|err| {
                format!(
                    "parse deployment inventory {} failed: {err}",
                    inventory_path.display()
                )
            })?;
            let inventory_authority = validate_inventory_against_manifest(
                &inventory,
                loaded,
                inventory_ref.as_str(),
                registry_path,
                effective_world_id,
            )?;
            let registry_sha256 = sha256_regular_file(registry_path, "genesis validator registry")?;
            if registry_sha256 != inventory_authority.generated_registry_sha256 {
                return Err(format!(
                    "generated validator registry digest mismatch: inventory={} actual={} path={}",
                    inventory_authority.generated_registry_sha256,
                    registry_sha256,
                    registry_path.display()
                ));
            }
            let explicit_registry =
                super::governance_registry::load_genesis_finality_registry(registry_path)?;
            let canonical_explicit_registry =
                canonicalize_registry_for_authority(explicit_registry);
            validate_registry_against_inventory(
                &canonical_explicit_registry,
                &inventory_authority,
            )?;
            let world = load_authority_world(execution_world_dir, effective_world_id)?;
            let effective_registry = world
                .resolve_governance_effective_finality_signer_registry()
                .map_err(|err| {
                    format!(
                        "failed to resolve effective governance registry for authority preflight: {err:?}"
                    )
                })?;
            let canonical_effective_registry = effective_registry
                .as_ref()
                .map(|effective| canonicalize_registry_for_authority(effective.clone()));
            let registry_for_status = canonical_effective_registry
                .as_ref()
                .unwrap_or(&canonical_explicit_registry);
            if canonical_effective_registry
                .as_ref()
                .is_some_and(|effective| effective != &canonical_explicit_registry)
            {
                return Err(
                        "explicit genesis validator registry does not match persisted effective governance registry"
                        .to_string(),
                    );
            }
            let registry_ref = startup_ref(registry_path, "genesis validator registry")?;
            let registry_semantic_sha256 = semantic_registry_sha256(registry_for_status)?;
            if registry_semantic_sha256 != inventory_authority.generated_registry_semantic_sha256 {
                return Err(format!(
                    "generated validator registry semantic digest mismatch: inventory={} actual={}",
                    inventory_authority.generated_registry_semantic_sha256,
                    registry_semantic_sha256
                ));
            }
            Ok(Some(RuntimeAuthorityBinding {
                registry_ref,
                registry_sha256,
                registry_semantic_sha256,
                inventory_ref,
                inventory_sha256,
                validator_signer_public_keys: inventory_authority.validator_signer_public_keys,
                validator_stakes: inventory_authority.validator_stakes,
                validator_47_provider_peer_id: inventory_authority.validator_47_provider_peer_id,
            }))
        }
    }
}

fn is_managed_triad_node_id(node_id: &str) -> bool {
    MANAGED_TRIAD_NODE_IDS.contains(&node_id)
}

fn validate_managed_node_role(node_id: &str, node_role: NodeRole) -> Result<(), String> {
    let expected_role = match node_id {
        "triad-testnet-sequencer" => Some(NodeRole::Sequencer),
        "triad-testnet-storage" | "triad-testnet-validator-47" => Some(NodeRole::Storage),
        _ => None,
    };
    if let Some(expected_role) = expected_role
        && node_role != expected_role
    {
        return Err(format!(
            "managed node identity {node_id} requires runtime role {expected_role:?}, got {node_role:?}"
        ));
    }
    Ok(())
}

fn validate_effective_world_id(
    effective_world_id: &str,
    loaded: &LoadedNetworkTierManifest,
) -> Result<(), String> {
    if loaded.manifest.tier != "public_testnet" {
        return Ok(());
    }
    let effective_world_id = effective_world_id.trim();
    if effective_world_id.is_empty() {
        return Err("public-testnet authority requires a non-empty effective world id".to_string());
    }
    if effective_world_id != loaded.manifest.chain_id {
        return Err(format!(
            "effective runtime world id does not match network-tier manifest chain_id: expected={} actual={}",
            loaded.manifest.chain_id, effective_world_id
        ));
    }
    Ok(())
}

pub(super) fn load_authority_world(
    execution_world_dir: &Path,
    effective_world_id: &str,
) -> Result<RuntimeWorld, String> {
    let has_persisted_state = authority_world_has_persisted_state(execution_world_dir);
    let world = if has_persisted_state {
        load_authority_world_from_isolated_copy(execution_world_dir)?
    } else {
        super::execution_bridge::load_execution_world(execution_world_dir)?
    };
    let persisted_world_id = world.chain_resource_manifest().world_id.as_str();
    if has_persisted_state && persisted_world_id == "unbound" {
        return Err(
            "persisted execution world has an unbound world id; public-testnet authority requires a fresh world or an explicitly bound persisted world".to_string(),
        );
    }
    if persisted_world_id != "unbound" && persisted_world_id != effective_world_id {
        return Err(format!(
            "persisted execution world id does not match effective runtime world id: expected={} actual={}",
            effective_world_id, persisted_world_id
        ));
    }
    Ok(world)
}

fn authority_world_has_persisted_state(execution_world_dir: &Path) -> bool {
    [
        "snapshot.json",
        "journal.json",
        "snapshot.manifest.json",
        "journal.segments.json",
        ".distfs-state",
    ]
    .iter()
    .any(|name| fs::symlink_metadata(execution_world_dir.join(name)).is_ok())
}

fn load_authority_world_from_isolated_copy(
    execution_world_dir: &Path,
) -> Result<RuntimeWorld, String> {
    load_world_from_isolated_copy(execution_world_dir, false)
}

/// Migration compares the persisted snapshot before applying any new release
/// policy. Loading occurs in a disposable copy so recovery cannot change source.
pub(super) fn load_persisted_world_exact(
    execution_world_dir: &Path,
) -> Result<RuntimeWorld, String> {
    load_world_from_isolated_copy(execution_world_dir, true)
}

fn load_world_from_isolated_copy(
    execution_world_dir: &Path,
    exact: bool,
) -> Result<RuntimeWorld, String> {
    let isolated_dir = create_authority_preflight_dir()?;
    let load_result = copy_authority_world_tree(execution_world_dir, isolated_dir.as_path())
        .and_then(|()| {
            if exact {
                oasis7::runtime_host::world::load_from_dir(isolated_dir.as_path())
                    .map_err(|error| format!("load original persisted world: {error:?}"))
            } else {
                super::execution_bridge::load_execution_world(isolated_dir.as_path())
            }
        });
    let cleanup_result = fs::remove_dir_all(isolated_dir.as_path()).map_err(|err| {
        format!(
            "remove isolated authority world copy {} failed: {err}",
            isolated_dir.display()
        )
    });
    match (load_result, cleanup_result) {
        (Ok(world), Ok(())) => Ok(world),
        (Err(load_error), Ok(())) => Err(load_error),
        (Ok(_), Err(cleanup_error)) => Err(cleanup_error),
        (Err(load_error), Err(cleanup_error)) => Err(format!(
            "{load_error}; cleanup also failed: {cleanup_error}"
        )),
    }
}

fn create_authority_preflight_dir() -> Result<PathBuf, String> {
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|err| format!("read authority preflight clock failed: {err}"))?
        .as_nanos();
    let process_id = std::process::id();
    for _ in 0..64 {
        let sequence = AUTHORITY_PREFLIGHT_SEQUENCE.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "oasis7-runtime-authority-preflight-{process_id}-{timestamp}-{sequence}"
        ));
        let mut builder = fs::DirBuilder::new();
        #[cfg(unix)]
        {
            use std::os::unix::fs::DirBuilderExt;
            builder.mode(0o700);
        }
        match builder.create(path.as_path()) {
            Ok(()) => return Ok(path),
            Err(error) if error.kind() == ErrorKind::AlreadyExists => continue,
            Err(error) => {
                return Err(format!(
                    "create isolated authority world copy {} failed: {error}",
                    path.display()
                ));
            }
        }
    }
    Err("create isolated authority world copy exhausted collision retries".to_string())
}

fn copy_authority_world_tree(source: &Path, destination: &Path) -> Result<(), String> {
    let source_type = fs::symlink_metadata(source).map_err(|err| {
        format!(
            "inspect persisted execution world {} failed: {err}",
            source.display()
        )
    })?;
    if source_type.file_type().is_symlink() || !source_type.is_dir() {
        return Err(format!(
            "persisted execution world {} must be a non-symlink directory",
            source.display()
        ));
    }
    for entry in fs::read_dir(source).map_err(|err| {
        format!(
            "read persisted execution world {} failed: {err}",
            source.display()
        )
    })? {
        let entry = entry.map_err(|err| {
            format!(
                "read persisted execution world entry {} failed: {err}",
                source.display()
            )
        })?;
        let source_path = entry.path();
        let destination_path = destination.join(entry.file_name());
        let file_type = entry.file_type().map_err(|err| {
            format!(
                "inspect persisted execution world entry {} failed: {err}",
                source_path.display()
            )
        })?;
        if file_type.is_symlink() {
            return Err(format!(
                "persisted execution world contains unsupported symlink {}",
                source_path.display()
            ));
        }
        if file_type.is_dir() {
            fs::create_dir(destination_path.as_path()).map_err(|err| {
                format!(
                    "create isolated authority world directory {} failed: {err}",
                    destination_path.display()
                )
            })?;
            copy_authority_world_tree(source_path.as_path(), destination_path.as_path())?;
        } else if file_type.is_file() {
            fs::copy(source_path.as_path(), destination_path.as_path()).map_err(|err| {
                format!(
                    "copy persisted execution world file {} failed: {err}",
                    source_path.display()
                )
            })?;
        } else {
            return Err(format!(
                "persisted execution world contains unsupported entry {}",
                source_path.display()
            ));
        }
    }
    Ok(())
}

#[derive(Debug)]
struct InventoryAuthority {
    validator_signer_public_keys: BTreeMap<String, String>,
    validator_stakes: BTreeMap<String, u64>,
    validator_47_provider_peer_id: Option<String>,
    generated_registry_sha256: String,
    generated_registry_semantic_sha256: String,
}

fn validate_inventory_against_manifest(
    inventory: &serde_json::Value,
    loaded: &LoadedNetworkTierManifest,
    expected_inventory_ref: &str,
    registry_path: &Path,
    effective_world_id: &str,
) -> Result<InventoryAuthority, String> {
    let object = inventory
        .as_object()
        .ok_or_else(|| "deployment inventory must be a JSON object".to_string())?;
    if object
        .get("schema_version")
        .and_then(serde_json::Value::as_str)
        != Some("oasis7.public_testnet_validator_triad_inventory.v1")
    {
        return Err("deployment inventory schema is unsupported".to_string());
    }
    if object.get("repository").and_then(serde_json::Value::as_str) != Some("eng-cc/oasis7") {
        return Err("deployment inventory repository is not eng-cc/oasis7".to_string());
    }
    if object
        .get("network_tier")
        .and_then(serde_json::Value::as_str)
        != Some("public_testnet")
        || object.get("topology").and_then(serde_json::Value::as_str)
            != Some("three_equal_validator")
    {
        return Err("deployment inventory network tier/topology is unsupported".to_string());
    }
    let authority = object
        .get("authority")
        .and_then(serde_json::Value::as_object)
        .ok_or_else(|| "deployment inventory authority is missing".to_string())?;
    let authority_string = |key: &str| {
        authority
            .get(key)
            .and_then(serde_json::Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| format!("deployment inventory authority.{key} is missing"))
    };
    let expected_world_id = authority_string("world_id")?;
    let expected_chain_id = authority_string("chain_id")?;
    let expected_network_tier = authority_string("network_tier")?;
    let expected_registry_ref = authority_string("registry_ref")?;
    let expected_generated_registry_sha256 = required_sha256(
        authority,
        "generated_registry_sha256",
        "deployment inventory authority",
    )?;
    let expected_generated_registry_semantic_sha256 = required_sha256(
        authority,
        "generated_registry_semantic_sha256",
        "deployment inventory authority",
    )?;
    if expected_inventory_ref != "scripts/public-testnet-validator-triad-inventory.v1.json" {
        return Err(format!(
            "network-tier deployment_inventory.ref is not the governed triad inventory: {expected_inventory_ref}"
        ));
    }
    let expected_registry_name = Path::new(expected_registry_ref)
        .file_name()
        .and_then(|value| value.to_str())
        .ok_or_else(|| "deployment inventory authority.registry_ref is invalid".to_string())?;
    if registry_path.file_name().and_then(|value| value.to_str()) != Some(expected_registry_name) {
        return Err(format!(
            "genesis validator registry path does not match inventory authority.registry_ref: expected={expected_registry_name} actual={}",
            registry_path.display()
        ));
    }
    if expected_network_tier != loaded.manifest.tier
        || expected_world_id != loaded.manifest.network_id
        || expected_chain_id != loaded.manifest.chain_id
    {
        return Err(
            "deployment inventory identity does not match the loaded network-tier manifest"
                .to_string(),
        );
    }
    if expected_world_id != effective_world_id {
        return Err(format!(
            "deployment inventory world_id does not match effective runtime world id: expected={} actual={}",
            expected_world_id, effective_world_id
        ));
    }
    let manifest_bytes = fs::read(Path::new(loaded.source_path.as_str())).map_err(|err| {
        format!(
            "read network-tier manifest {} for registry authority binding failed: {err}",
            loaded.source_path
        )
    })?;
    let manifest_value: serde_json::Value = serde_json::from_slice(manifest_bytes.as_slice())
        .map_err(|err| {
            format!(
                "parse network-tier manifest {} for registry authority binding failed: {err}",
                loaded.source_path
            )
        })?;
    let manifest_registry_binding = manifest_value
        .get("deployment_validator_registry")
        .and_then(serde_json::Value::as_object)
        .ok_or_else(|| {
            "network-tier manifest is missing deployment_validator_registry authority binding"
                .to_string()
        })?;
    let manifest_registry_ref = required_string(
        manifest_registry_binding,
        "ref",
        "deployment_validator_registry",
    )?;
    if manifest_registry_ref != expected_registry_ref {
        return Err(format!(
            "network-tier manifest deployment validator registry ref does not match inventory authority: expected={expected_registry_ref} actual={manifest_registry_ref}"
        ));
    }
    let manifest_registry_sha256 = required_sha256(
        manifest_registry_binding,
        "sha256",
        "deployment_validator_registry",
    )?;
    if manifest_registry_sha256 != expected_generated_registry_sha256 {
        return Err(format!(
            "network-tier manifest deployment validator registry digest does not match inventory authority: expected={expected_generated_registry_sha256} actual={manifest_registry_sha256}"
        ));
    }
    let manifest_registry_semantic_sha256 = required_sha256(
        manifest_registry_binding,
        "semantic_sha256",
        "deployment_validator_registry",
    )?;
    if manifest_registry_semantic_sha256 != expected_generated_registry_semantic_sha256 {
        return Err(format!(
            "network-tier manifest deployment validator registry semantic digest does not match inventory authority: expected={expected_generated_registry_semantic_sha256} actual={manifest_registry_semantic_sha256}"
        ));
    }
    let validator_set = object
        .get("validator_set")
        .and_then(serde_json::Value::as_object)
        .ok_or_else(|| "deployment inventory validator_set is missing".to_string())?;
    let count = validator_set
        .get("count")
        .and_then(serde_json::Value::as_u64)
        .ok_or_else(|| "deployment inventory validator_set.count is missing".to_string())?;
    if count != 3 || loaded.manifest.validator_policy.target_validator_count != count {
        return Err("deployment inventory validator count does not match manifest".to_string());
    }
    if validator_set.get("stakes") != Some(&serde_json::json!([100, 100, 100]))
        || validator_set.get("total_stake") != Some(&serde_json::json!(300))
        || validator_set.get("required_stake") != Some(&serde_json::json!(200))
        || validator_set.get("quorum")
            != Some(&serde_json::json!({"numerator": 2, "denominator": 3}))
    {
        return Err(
            "deployment inventory validator quorum is not the governed 3x100 policy".to_string(),
        );
    }
    if object.get("governance")
        != Some(&serde_json::json!({"signer_count": 3, "threshold": 2, "threshold_bps": 6667}))
    {
        return Err(
            "deployment inventory governance policy is not the governed 2-of-3 policy".to_string(),
        );
    }
    let nodes = object
        .get("nodes")
        .and_then(serde_json::Value::as_object)
        .ok_or_else(|| "deployment inventory nodes are missing".to_string())?;
    if nodes.len() != 3 {
        return Err("deployment inventory must contain exactly three validator nodes".to_string());
    }
    let mut validator_signer_public_keys = BTreeMap::new();
    let mut validator_stakes = BTreeMap::new();
    let mut validator_47_provider_peer_id = None;
    for (name, node) in nodes {
        let node = node
            .as_object()
            .ok_or_else(|| format!("deployment inventory node {name} is not an object"))?;
        let node_id = required_value_string(node, "node_id", name)?;
        let signer = required_value_string(node, "finality_signer_public_key", name)?;
        let stake = node
            .get("stake")
            .and_then(serde_json::Value::as_u64)
            .ok_or_else(|| format!("deployment inventory node {name}.stake is missing"))?;
        if stake != 100
            || signer.len() != 64
            || !signer.bytes().all(|byte| byte.is_ascii_hexdigit())
        {
            return Err(format!(
                "deployment inventory node {name} signer/stake is invalid"
            ));
        }
        validator_signer_public_keys.insert(node_id.clone(), signer.to_ascii_lowercase());
        validator_stakes.insert(node_id.clone(), stake);
        if name == "validator-47" {
            validator_47_provider_peer_id =
                Some(required_value_string(node, "libp2p_peer_id", name)?);
        }
    }
    if !nodes.contains_key("sequencer-204")
        || !nodes.contains_key("storage-205")
        || !nodes.contains_key("validator-47")
        || validator_signer_public_keys.len() != 3
    {
        return Err("deployment inventory validator node set is not canonical".to_string());
    }
    Ok(InventoryAuthority {
        validator_signer_public_keys,
        validator_stakes,
        validator_47_provider_peer_id,
        generated_registry_sha256: expected_generated_registry_sha256,
        generated_registry_semantic_sha256: expected_generated_registry_semantic_sha256,
    })
}

/// Keep the deployment authority digest stable across the runtime's two
/// equivalent registry encodings.  Genesis documents use threshold_bps=0 as
/// a derived-value sentinel, while World validation persists the derived
/// percentage.  The authority contract canonicalizes only that exact derived
/// value; a non-default explicit percentage remains distinct and is rejected
/// by the inventory binding.
pub(super) fn canonicalize_registry_for_authority(
    mut registry: GovernanceFinalitySignerRegistry,
) -> GovernanceFinalitySignerRegistry {
    let derived_threshold_bps =
        default_threshold_bps(registry.threshold, registry.signer_bindings.len());
    if registry.threshold_bps == derived_threshold_bps {
        registry.threshold_bps = 0;
    }
    registry
}

fn default_threshold_bps(required_signers: u16, total_signers: usize) -> u16 {
    if required_signers == 0 || total_signers == 0 {
        return 0;
    }
    let total_signers = total_signers as u128;
    let required_signers = u128::from(required_signers);
    required_signers
        .saturating_mul(10_000)
        .saturating_add(total_signers.saturating_sub(1))
        .saturating_div(total_signers)
        .min(10_000) as u16
}

fn validate_registry_against_inventory(
    registry: &GovernanceFinalitySignerRegistry,
    inventory: &InventoryAuthority,
) -> Result<(), String> {
    let expected_signer_bindings = inventory
        .validator_signer_public_keys
        .iter()
        .map(|(node_id, public_key)| {
            (
                format!("governance.finality.v1.{node_id}"),
                public_key.clone(),
            )
        })
        .collect::<BTreeMap<_, _>>();
    let expected_validator_stakes = inventory
        .validator_stakes
        .iter()
        .map(|(node_id, stake)| (format!("governance.finality.v1.{node_id}"), *stake))
        .collect::<BTreeMap<_, _>>();
    if registry.slot_id != "governance.finality.v1"
        || registry.threshold != 2
        || registry.threshold_bps != 0
        || registry.signer_bindings != expected_signer_bindings
        || registry.validator_stakes != expected_validator_stakes
    {
        return Err(
            "genesis validator registry does not match the governed inventory validator authority"
                .to_string(),
        );
    }
    Ok(())
}

fn required_value_string(
    object: &serde_json::Map<String, serde_json::Value>,
    key: &str,
    scope: &str,
) -> Result<String, String> {
    object
        .get(key)
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
        .ok_or_else(|| format!("deployment inventory node {scope}.{key} is missing"))
}

pub(super) fn semantic_registry_sha256(
    registry: &GovernanceFinalitySignerRegistry,
) -> Result<String, String> {
    let mut canonical = BTreeMap::new();
    canonical.insert(
        "signer_bindings",
        serde_json::to_value(&registry.signer_bindings).map_err(|err| err.to_string())?,
    );
    canonical.insert(
        "slot_id",
        serde_json::Value::String(registry.slot_id.clone()),
    );
    canonical.insert("threshold", serde_json::json!(registry.threshold));
    canonical.insert("threshold_bps", serde_json::json!(registry.threshold_bps));
    canonical.insert(
        "validator_stakes",
        serde_json::to_value(&registry.validator_stakes).map_err(|err| err.to_string())?,
    );
    let bytes = serde_json::to_vec(&canonical)
        .map_err(|err| format!("encode registry semantic identity failed: {err}"))?;
    Ok(hex::encode(Sha256::digest(bytes.as_slice())))
}

fn startup_ref(path: &Path, label: &str) -> Result<String, String> {
    let metadata = fs::symlink_metadata(path)
        .map_err(|err| format!("read {label} metadata {} failed: {err}", path.display()))?;
    if metadata.file_type().is_symlink() || !metadata.file_type().is_file() {
        return Err(format!(
            "{label} must be a regular non-symlink file: {}",
            path.display()
        ));
    }
    let value = path.to_string_lossy().trim().to_string();
    if value.is_empty() {
        return Err(format!("{label} reference must be non-empty"));
    }
    Ok(value)
}

fn sha256_regular_file(path: &Path, label: &str) -> Result<String, String> {
    let _ = startup_ref(path, label)?;
    let bytes = fs::read(path)
        .map_err(|err| format!("read {label} {} for sha256 failed: {err}", path.display()))?;
    Ok(hex::encode(Sha256::digest(bytes.as_slice())))
}

fn required_string(
    object: &serde_json::Map<String, serde_json::Value>,
    key: &str,
    scope: &str,
) -> Result<String, String> {
    let value = object
        .get(key)
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| format!("{scope}.{key} must be a non-empty string"))?;
    Ok(value.to_string())
}

fn required_sha256(
    object: &serde_json::Map<String, serde_json::Value>,
    key: &str,
    scope: &str,
) -> Result<String, String> {
    let value = required_string(object, key, scope)?.to_ascii_lowercase();
    if value.len() != 64 || !value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(format!(
            "{scope}.{key} must be a 64-character sha256 hex digest"
        ));
    }
    Ok(value)
}

#[cfg(test)]
#[path = "runtime_authority_tests.rs"]
mod tests;
