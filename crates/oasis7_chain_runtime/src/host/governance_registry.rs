use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use oasis7::network_tier_manifest::LoadedNetworkTierManifest;
use oasis7_node::{
    NodeConfig, NodeMainTokenControllerBindingConfig, NodeMainTokenControllerSignerPolicy,
    NodePosConfig, PosValidator,
};
use oasis7_runtime::{
    GovernanceFinalitySignerRegistry, GovernanceMainTokenControllerRegistry,
    GovernanceThresholdSignerPolicy, RollbackAuthorityRecord, RollbackAuthorityRegistry,
    RollbackAuthorityRole,
};
use serde::Deserialize;

const DEFAULT_FINALITY_SLOT_ID: &str = "governance.finality.v1";
const GOVERNANCE_REGISTRY_DEFAULT_VALIDATOR_STAKE: u64 = 100;
const DEFAULT_CONTROLLER_THRESHOLD: u16 = 2;
const ROLLBACK_ON_CALL_SLOT_ID: &str = "ops.rollback.on_call.v1";
const ROLLBACK_GOVERNANCE_SLOT_ID: &str = "governance.rollback.v1";

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum GenesisValidatorRegistryManifest {
    Document(GenesisValidatorRegistryDocument),
    PublicManifestEntries(Vec<GenesisValidatorRegistryEntry>),
}

#[derive(Debug, Deserialize)]
struct GenesisValidatorRegistryDocument {
    #[serde(default = "default_finality_slot_id")]
    slot_id: String,
    threshold: u16,
    #[serde(default)]
    threshold_bps: u16,
    validators: Vec<GenesisValidatorRegistryEntry>,
}

#[derive(Debug, Deserialize)]
struct GovernanceBootstrapGenesisDocument {
    #[serde(default)]
    governance_bootstrap_refs: Option<GovernanceBootstrapRefs>,
}

#[derive(Debug, Deserialize)]
struct GovernanceBootstrapRefs {
    governance_public_manifest_ref: String,
    #[serde(default)]
    genesis_validator_registry_ref: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
struct GenesisValidatorRegistryEntry {
    #[serde(default)]
    slot_id: Option<String>,
    #[serde(default)]
    node_id: Option<String>,
    #[serde(default)]
    signer_id: Option<String>,
    #[serde(default = "default_ed25519_scheme")]
    scheme: String,
    #[serde(default)]
    finality_signer_public_key: Option<String>,
    #[serde(default)]
    public_key_hex: Option<String>,
    #[serde(default = "default_genesis_validator_stake")]
    stake: u64,
    #[serde(default)]
    threshold: Option<u16>,
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum GovernancePublicManifest {
    Bundle(GovernancePublicManifestBundle),
    Entries(Vec<GenesisValidatorRegistryEntry>),
}

#[derive(Debug, Deserialize)]
struct GovernancePublicManifestBundle {
    #[serde(default)]
    entries: Vec<GenesisValidatorRegistryEntry>,
}

fn default_finality_slot_id() -> String {
    DEFAULT_FINALITY_SLOT_ID.to_string()
}

fn default_ed25519_scheme() -> String {
    "ed25519".to_string()
}

fn default_genesis_validator_stake() -> u64 {
    GOVERNANCE_REGISTRY_DEFAULT_VALIDATOR_STAKE
}

pub(super) fn ensure_world_governance_validator_registry(
    execution_world_dir: &Path,
    genesis_validator_registry_path: Option<&Path>,
    loaded_network_tier_manifest: Option<&LoadedNetworkTierManifest>,
) -> Result<(), String> {
    if import_network_tier_governance_bootstrap(
        execution_world_dir,
        genesis_validator_registry_path,
        loaded_network_tier_manifest,
    )? {
        return Ok(());
    }
    if world_has_effective_finality_registry(execution_world_dir)? {
        if network_tier_requires_governance_validator_registry(loaded_network_tier_manifest) {
            let world = super::execution_bridge::load_execution_world(execution_world_dir)?;
            if !world_is_bootstrap_only(&world)
                && world.snapshot().rollback_authority_registry.is_empty()
            {
                return Err(format!(
                    "public-tier existing world is missing the governed rollback authority registry; run the dedicated governance registry import migration before restart: execution_world_dir={}",
                    execution_world_dir.display()
                ));
            }
        }
        return Ok(());
    }
    if let Some(manifest_path) = genesis_validator_registry_path {
        import_genesis_validator_registry(execution_world_dir, manifest_path)?;
        return Ok(());
    }
    if network_tier_requires_governance_validator_registry(loaded_network_tier_manifest) {
        return Err(public_tier_missing_registry_error(
            execution_world_dir,
            loaded_network_tier_manifest,
        ));
    }
    Ok(())
}

fn import_network_tier_governance_bootstrap(
    execution_world_dir: &Path,
    explicit_genesis_validator_registry_path: Option<&Path>,
    loaded_network_tier_manifest: Option<&LoadedNetworkTierManifest>,
) -> Result<bool, String> {
    let Some(loaded) = loaded_network_tier_manifest else {
        return Ok(false);
    };
    let manifest_path = Path::new(loaded.source_path.as_str());
    let genesis_path = resolve_network_tier_runtime_ref_path(
        manifest_path,
        loaded.manifest.runtime_refs.genesis_ref.as_str(),
    );
    if !genesis_path.is_file() {
        return Ok(false);
    }
    let genesis_bytes = std::fs::read(genesis_path.as_path()).map_err(|err| {
        format!(
            "read network tier genesis_ref {} failed: {err}",
            genesis_path.display()
        )
    })?;
    let genesis: GovernanceBootstrapGenesisDocument =
        serde_json::from_slice(genesis_bytes.as_slice()).map_err(|err| {
            format!(
                "parse network tier genesis_ref {} failed: {err}",
                genesis_path.display()
            )
        })?;
    let Some(bootstrap_refs) = genesis.governance_bootstrap_refs else {
        return Ok(false);
    };

    let mut world = super::execution_bridge::load_execution_world(execution_world_dir)?;
    let needs_finality = world
        .resolve_governance_effective_finality_signer_registry()
        .map_err(|err| {
            format!("failed to resolve world governance effective finality registry: {err:?}")
        })?
        .is_none();
    let needs_controller = world.governance_main_token_controller_registry().is_none();
    let needs_rollback = world.snapshot().rollback_authority_registry.is_empty();
    if !needs_finality && !needs_controller && !needs_rollback {
        return Ok(true);
    }
    if !world_is_bootstrap_only(&world) {
        return Ok(false);
    }

    if needs_finality {
        let genesis_registry_path = explicit_genesis_validator_registry_path
            .map(Path::to_path_buf)
            .or_else(|| {
                bootstrap_refs
                    .genesis_validator_registry_ref
                    .as_deref()
                    .map(|raw| resolve_network_tier_runtime_ref_path(genesis_path.as_path(), raw))
            })
            .ok_or_else(|| {
                "governance bootstrap refs missing genesis_validator_registry_ref and no explicit --genesis-validator-registry was provided".to_string()
            })?;
        let finality_registry = load_genesis_finality_registry(genesis_registry_path.as_path())?;
        world
            .set_governance_finality_signer_registry(finality_registry)
            .map_err(|err| format!("write network-tier finality registry failed: {err:?}"))?;
    }

    if needs_controller || needs_rollback {
        let governance_manifest_path = resolve_network_tier_runtime_ref_path(
            genesis_path.as_path(),
            bootstrap_refs.governance_public_manifest_ref.as_str(),
        );
        let controller_entries =
            load_governance_public_manifest_entries(governance_manifest_path.as_path())?;
        if needs_controller {
            let controller_registry =
                build_main_token_controller_registry_from_entries(controller_entries.as_slice())?;
            world
                .set_governance_main_token_controller_registry(controller_registry)
                .map_err(|err| format!("write network-tier controller registry failed: {err:?}"))?;
        }
        if needs_rollback {
            let rollback_registry =
                build_rollback_authority_registry_from_entries(controller_entries.as_slice())?;
            world
                .set_rollback_authority_registry(rollback_registry)
                .map_err(|err| format!("write network-tier rollback registry failed: {err:?}"))?;
        }
    }

    world.save_to_dir(execution_world_dir).map_err(|err| {
        format!(
            "save execution world {} after network-tier governance bootstrap import failed: {err:?}",
            execution_world_dir.display()
        )
    })?;
    Ok(true)
}

pub(super) fn apply_world_governance_registry_overrides(
    mut config: NodeConfig,
    execution_world_dir: &Path,
) -> Result<NodeConfig, String> {
    let world = super::execution_bridge::load_execution_world(execution_world_dir)?;
    if let Some(registry) = world
        .resolve_governance_effective_finality_signer_registry()
        .map_err(|err| {
            format!("failed to resolve world governance effective finality registry: {err:?}")
        })?
    {
        let pos_config =
            node_pos_config_from_world_finality_registry(&registry, &config.pos_config);
        config = config.with_pos_config(pos_config).map_err(|err| {
            format!("failed to apply world governance finality registry: {err:?}")
        })?;
    }
    if let Some(registry) = world.governance_main_token_controller_registry() {
        let binding = node_main_token_controller_binding_from_registry(
            registry,
            config.main_token_controller_binding.clone(),
        );
        config = config
            .with_main_token_controller_binding(binding)
            .map_err(|err| {
                format!("failed to apply world governance controller registry: {err:?}")
            })?;
    }
    Ok(config)
}

fn world_has_effective_finality_registry(execution_world_dir: &Path) -> Result<bool, String> {
    let world = super::execution_bridge::load_execution_world(execution_world_dir)?;
    world
        .resolve_governance_effective_finality_signer_registry()
        .map(|registry| registry.is_some())
        .map_err(|err| {
            format!("failed to resolve world governance effective finality registry: {err:?}")
        })
}

fn import_genesis_validator_registry(
    execution_world_dir: &Path,
    manifest_path: &Path,
) -> Result<(), String> {
    let registry = load_genesis_finality_registry(manifest_path)?;
    let mut world = super::execution_bridge::load_execution_world(execution_world_dir)?;
    if world
        .resolve_governance_effective_finality_signer_registry()
        .map_err(|err| {
            format!("failed to resolve world governance effective finality registry: {err:?}")
        })?
        .is_some()
    {
        return Ok(());
    }
    if execution_world_has_persisted_state(execution_world_dir) {
        return Err(format!(
            "refusing to import genesis validator registry into existing execution world {} without reconciliation; use the dedicated governance registry import/migration path for non-empty worlds",
            execution_world_dir.display()
        ));
    }
    world
        .set_governance_finality_signer_registry(registry)
        .map_err(|err| format!("write genesis finality registry failed: {err:?}"))?;
    world.save_to_dir(execution_world_dir).map_err(|err| {
        format!(
            "save execution world {} after genesis validator registry import failed: {err:?}",
            execution_world_dir.display()
        )
    })
}

pub(super) fn load_genesis_finality_registry(
    manifest_path: &Path,
) -> Result<GovernanceFinalitySignerRegistry, String> {
    let manifest_bytes = std::fs::read(manifest_path).map_err(|err| {
        format!(
            "read genesis validator registry {} failed: {err}",
            manifest_path.display()
        )
    })?;
    let manifest: GenesisValidatorRegistryManifest =
        serde_json::from_slice(manifest_bytes.as_slice()).map_err(|err| {
            format!(
                "decode genesis validator registry {} failed: {err}",
                manifest_path.display()
            )
        })?;
    build_genesis_finality_registry(manifest)
}

fn load_governance_public_manifest_entries(
    manifest_path: &Path,
) -> Result<Vec<GenesisValidatorRegistryEntry>, String> {
    let manifest_bytes = std::fs::read(manifest_path).map_err(|err| {
        format!(
            "read governance public manifest {} failed: {err}",
            manifest_path.display()
        )
    })?;
    let manifest: GovernancePublicManifest = serde_json::from_slice(manifest_bytes.as_slice())
        .map_err(|err| {
            format!(
                "decode governance public manifest {} failed: {err}",
                manifest_path.display()
            )
        })?;
    Ok(match manifest {
        GovernancePublicManifest::Bundle(bundle) => bundle.entries,
        GovernancePublicManifest::Entries(entries) => entries,
    })
}

fn build_genesis_finality_registry(
    manifest: GenesisValidatorRegistryManifest,
) -> Result<GovernanceFinalitySignerRegistry, String> {
    match manifest {
        GenesisValidatorRegistryManifest::Document(document) => {
            build_genesis_finality_registry_from_entries(
                document.slot_id.trim(),
                document.threshold,
                document.threshold_bps,
                document.validators.as_slice(),
            )
        }
        GenesisValidatorRegistryManifest::PublicManifestEntries(entries) => {
            let finality_slot_id = DEFAULT_FINALITY_SLOT_ID;
            let finality_entries = entries
                .iter()
                .filter(|entry| entry.slot_id.as_deref() == Some(finality_slot_id))
                .cloned()
                .collect::<Vec<_>>();
            let threshold =
                explicit_public_manifest_threshold(finality_slot_id, finality_entries.as_slice())?;
            build_genesis_finality_registry_from_entries(
                finality_slot_id,
                threshold,
                0,
                finality_entries.as_slice(),
            )
        }
    }
}

fn explicit_public_manifest_threshold(
    finality_slot_id: &str,
    entries: &[GenesisValidatorRegistryEntry],
) -> Result<u16, String> {
    let mut thresholds = entries
        .iter()
        .filter_map(|entry| entry.threshold)
        .collect::<BTreeSet<_>>();
    match thresholds.len() {
        1 => Ok(thresholds.pop_first().expect("one threshold")),
        0 => Err(format!(
            "public-manifest genesis validator registry for slot {finality_slot_id} requires explicit threshold; use document form or set threshold on each finality entry"
        )),
        _ => Err(format!(
            "public-manifest genesis validator registry for slot {finality_slot_id} has inconsistent entry thresholds: {thresholds:?}"
        )),
    }
}

fn build_main_token_controller_registry_from_entries(
    entries: &[GenesisValidatorRegistryEntry],
) -> Result<GovernanceMainTokenControllerRegistry, String> {
    let mut controller_signer_policies = BTreeMap::new();
    for entry in entries.iter().filter(|entry| {
        !matches!(
            entry.slot_id.as_deref().map(str::trim),
            Some(DEFAULT_FINALITY_SLOT_ID | ROLLBACK_ON_CALL_SLOT_ID | ROLLBACK_GOVERNANCE_SLOT_ID)
        )
    }) {
        validate_governance_public_manifest_entry(entry)?;
        let slot_id = entry
            .slot_id
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| "governance public manifest entry requires slot_id".to_string())?
            .to_string();
        let threshold = entry.threshold.unwrap_or(DEFAULT_CONTROLLER_THRESHOLD);
        controller_signer_policies
            .entry(slot_id)
            .or_insert_with(|| GovernanceThresholdSignerPolicy {
                threshold,
                allowed_public_keys: BTreeSet::new(),
            })
            .allowed_public_keys
            .insert(genesis_validator_entry_public_key(entry)?.to_string());
    }
    if controller_signer_policies.is_empty() {
        return Err(
            "governance public manifests do not contain any controller signer slots".to_string(),
        );
    }
    Ok(GovernanceMainTokenControllerRegistry {
        genesis_controller_account_id: "msig.genesis.v1".to_string(),
        treasury_bucket_controller_slots: BTreeMap::from([
            (
                "staking_reward_pool".to_string(),
                "msig.staking_governance.v1".to_string(),
            ),
            (
                "ecosystem_pool".to_string(),
                "msig.ecosystem_governance.v1".to_string(),
            ),
            (
                "security_reserve".to_string(),
                "msig.security_council.v1".to_string(),
            ),
        ]),
        restricted_starter_claim_admin_account_ids: BTreeSet::new(),
        controller_signer_policies,
    })
}

fn build_rollback_authority_registry_from_entries(
    entries: &[GenesisValidatorRegistryEntry],
) -> Result<RollbackAuthorityRegistry, String> {
    let record = |slot_id: &str, role: RollbackAuthorityRole| {
        let matching = entries
            .iter()
            .filter(|entry| entry.slot_id.as_deref().map(str::trim) == Some(slot_id))
            .collect::<Vec<_>>();
        if matching.len() != 1 {
            return Err(format!(
                "governance public manifest requires exactly one rollback authority slot_id={slot_id} actual={}",
                matching.len()
            ));
        }
        let entry = matching[0];
        validate_governance_public_manifest_entry(entry)?;
        if entry.threshold != Some(1) {
            return Err(format!(
                "rollback authority slot requires explicit threshold=1 slot_id={slot_id}"
            ));
        }
        let authority_id = entry
            .signer_id
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| {
                format!("rollback authority slot requires signer_id slot_id={slot_id}")
            })?;
        Ok(RollbackAuthorityRecord {
            authority_id: authority_id.to_string(),
            role,
            public_key_hex: genesis_validator_entry_public_key(entry)?.to_ascii_lowercase(),
            active: true,
        })
    };
    RollbackAuthorityRegistry::new([
        record(ROLLBACK_ON_CALL_SLOT_ID, RollbackAuthorityRole::OnCall)?,
        record(
            ROLLBACK_GOVERNANCE_SLOT_ID,
            RollbackAuthorityRole::Governance,
        )?,
    ])
    .map_err(|err| format!("invalid governed rollback registry: {err:?}"))
}

fn execution_world_has_persisted_state(execution_world_dir: &Path) -> bool {
    [
        "snapshot.json",
        "journal.json",
        "snapshot.manifest.json",
        "journal.segments.json",
    ]
    .iter()
    .any(|name| execution_world_dir.join(name).exists())
        || execution_world_dir.join(".distfs-state").exists()
}

fn world_is_bootstrap_only(world: &oasis7_runtime::World) -> bool {
    world.snapshot().tick_consensus_records.is_empty() && world.journal().is_empty()
}

fn build_genesis_finality_registry_from_entries(
    slot_id: &str,
    threshold: u16,
    threshold_bps: u16,
    entries: &[GenesisValidatorRegistryEntry],
) -> Result<GovernanceFinalitySignerRegistry, String> {
    let slot_id = slot_id.trim();
    if slot_id.is_empty() {
        return Err("genesis validator registry slot_id cannot be empty".to_string());
    }
    if threshold == 0 {
        return Err("genesis validator registry threshold must be > 0".to_string());
    }
    let mut signer_bindings = BTreeMap::new();
    let mut validator_stakes = BTreeMap::new();
    for entry in entries {
        validate_genesis_validator_entry(entry)?;
        let node_id = genesis_validator_entry_node_id(entry)?;
        let binding_key = governance_finality_binding_key(slot_id, node_id.as_str());
        let public_key_hex = genesis_validator_entry_public_key(entry)?;
        if signer_bindings
            .insert(binding_key.clone(), public_key_hex.to_string())
            .is_some()
        {
            return Err(format!(
                "genesis validator registry duplicates validator binding {binding_key}"
            ));
        }
        validator_stakes.insert(binding_key, entry.stake);
    }
    if signer_bindings.is_empty() {
        return Err(format!(
            "genesis validator registry contains no validators for slot {slot_id}"
        ));
    }
    if usize::from(threshold) > signer_bindings.len() {
        return Err(format!(
            "genesis validator registry threshold {threshold} exceeds validator count {}",
            signer_bindings.len()
        ));
    }
    Ok(GovernanceFinalitySignerRegistry {
        slot_id: slot_id.to_string(),
        threshold,
        threshold_bps,
        signer_bindings,
        validator_stakes,
    })
}

fn governance_finality_binding_key(slot_id: &str, validator_id: &str) -> String {
    format!("{}.{}", slot_id.trim(), validator_id.trim())
}

fn validate_genesis_validator_entry(entry: &GenesisValidatorRegistryEntry) -> Result<(), String> {
    if !entry.scheme.trim().eq_ignore_ascii_case("ed25519") {
        return Err(format!(
            "genesis validator registry only supports ed25519 finality signers: node_id={:?} signer_id={:?} scheme={}",
            entry.node_id, entry.signer_id, entry.scheme
        ));
    }
    if entry.stake == 0 {
        return Err(format!(
            "genesis validator registry stake must be > 0: node_id={:?} signer_id={:?}",
            entry.node_id, entry.signer_id
        ));
    }
    let public_key_hex = genesis_validator_entry_public_key(entry)?;
    if public_key_hex.len() != 64 || !public_key_hex.chars().all(|ch| ch.is_ascii_hexdigit()) {
        return Err(format!(
            "genesis validator registry finality signer public key must be 32-byte hex: node_id={:?} signer_id={:?}",
            entry.node_id, entry.signer_id
        ));
    }
    Ok(())
}

fn validate_governance_public_manifest_entry(
    entry: &GenesisValidatorRegistryEntry,
) -> Result<(), String> {
    if !entry.scheme.trim().eq_ignore_ascii_case("ed25519") {
        return Err(format!(
            "governance public manifest only supports ed25519 signers: slot_id={:?} node_id={:?} signer_id={:?} scheme={}",
            entry.slot_id, entry.node_id, entry.signer_id, entry.scheme
        ));
    }
    let slot_id = entry
        .slot_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| "governance public manifest entry requires slot_id".to_string())?;
    if slot_id == DEFAULT_FINALITY_SLOT_ID {
        return Ok(());
    }
    let _ = genesis_validator_entry_node_id(entry)?;
    let public_key_hex = genesis_validator_entry_public_key(entry)?;
    if public_key_hex.len() != 64 || !public_key_hex.chars().all(|ch| ch.is_ascii_hexdigit()) {
        return Err(format!(
            "governance public manifest public key must be 32-byte hex: slot_id={slot_id} node_id={:?} signer_id={:?}",
            entry.node_id, entry.signer_id
        ));
    }
    if entry.threshold.is_some_and(|value| value == 0) {
        return Err(format!(
            "governance public manifest threshold must be > 0: slot_id={slot_id} node_id={:?} signer_id={:?}",
            entry.node_id, entry.signer_id
        ));
    }
    Ok(())
}

fn genesis_validator_entry_node_id(
    entry: &GenesisValidatorRegistryEntry,
) -> Result<String, String> {
    entry
        .node_id
        .as_deref()
        .or(entry.signer_id.as_deref())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
        .ok_or_else(|| "genesis validator registry entry requires node_id or signer_id".to_string())
}

fn genesis_validator_entry_public_key(
    entry: &GenesisValidatorRegistryEntry,
) -> Result<&str, String> {
    entry
        .finality_signer_public_key
        .as_deref()
        .or(entry.public_key_hex.as_deref())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| {
            "genesis validator registry entry requires finality_signer_public_key or public_key_hex"
                .to_string()
        })
}

fn network_tier_requires_governance_validator_registry(
    loaded_network_tier_manifest: Option<&LoadedNetworkTierManifest>,
) -> bool {
    let Some(loaded) = loaded_network_tier_manifest else {
        return false;
    };
    let tier = loaded.manifest.tier.as_str();
    let admission = loaded
        .manifest
        .validator_policy
        .validator_admission
        .as_str();
    matches!(tier, "public_testnet" | "mainnet")
        || matches!(
            admission,
            "allowlist_or_governed_candidate" | "governance_registry_only"
        )
}

fn public_tier_missing_registry_error(
    execution_world_dir: &Path,
    loaded_network_tier_manifest: Option<&LoadedNetworkTierManifest>,
) -> String {
    let tier = loaded_network_tier_manifest
        .map(|loaded| loaded.manifest.tier.as_str())
        .unwrap_or("unknown");
    let admission = loaded_network_tier_manifest
        .map(|loaded| {
            loaded
                .manifest
                .validator_policy
                .validator_admission
                .as_str()
        })
        .unwrap_or("unknown");
    format!(
        "public validator registry required before startup: execution_world_dir={} tier={} validator_admission={} has no effective governance finality registry; provide --genesis-validator-registry <path> for first boot or migrate the world-state registry before restart",
        execution_world_dir.display(),
        tier,
        admission
    )
}

fn resolve_network_tier_runtime_ref_path(
    manifest_path: &Path,
    raw_ref: &str,
) -> std::path::PathBuf {
    let candidate = std::path::PathBuf::from(raw_ref);
    if candidate.is_absolute() {
        return candidate;
    }
    if candidate.exists() {
        return candidate;
    }
    for ancestor in manifest_path.ancestors() {
        let rooted_candidate = ancestor.join(candidate.clone());
        if rooted_candidate.exists() {
            return rooted_candidate;
        }
    }
    manifest_path
        .parent()
        .map(|parent| parent.join(candidate.clone()))
        .unwrap_or(candidate)
}

pub(super) fn validate_world_finality_signer_completeness(
    registry: &GovernanceFinalitySignerRegistry,
    requested: &[PosValidator],
) -> Result<(), String> {
    let keys = registry
        .signer_bindings
        .keys()
        .map(|key| validator_id_from_registry_binding(&registry.slot_id, key))
        .collect::<BTreeSet<_>>();
    if keys.is_empty()
        || registry
            .validator_stakes
            .keys()
            .map(|key| validator_id_from_registry_binding(&registry.slot_id, key))
            .chain(requested.iter().map(|v| v.validator_id.clone()))
            .any(|id| !keys.contains(&id))
    {
        return Err("verified World registry omits required validator signer keys".into());
    }
    Ok(())
}

pub(super) fn node_pos_config_from_world_finality_registry(
    registry: &GovernanceFinalitySignerRegistry,
    fallback: &NodePosConfig,
) -> NodePosConfig {
    let validator_signer_public_keys = registry
        .signer_bindings
        .iter()
        .map(|(binding_key, public_key_hex)| {
            (
                validator_id_from_registry_binding(registry.slot_id.as_str(), binding_key),
                public_key_hex.clone(),
            )
        })
        .collect::<BTreeMap<String, String>>();
    let validators = registry
        .signer_bindings
        .keys()
        .map(|binding_key| {
            let validator_id =
                validator_id_from_registry_binding(registry.slot_id.as_str(), binding_key);
            PosValidator {
                stake: registry
                    .validator_stakes
                    .get(validator_id.as_str())
                    .or_else(|| registry.validator_stakes.get(binding_key.as_str()))
                    .copied()
                    .unwrap_or(GOVERNANCE_REGISTRY_DEFAULT_VALIDATOR_STAKE),
                validator_id,
            }
        })
        .collect::<Vec<PosValidator>>();
    let validator_player_ids = validator_signer_public_keys
        .keys()
        .cloned()
        .map(|validator_id| (validator_id.clone(), validator_id))
        .collect::<BTreeMap<String, String>>();
    NodePosConfig {
        validators,
        validator_player_ids,
        validator_signer_public_keys,
        supermajority_numerator: fallback.supermajority_numerator,
        supermajority_denominator: fallback.supermajority_denominator,
        epoch_length_slots: fallback.epoch_length_slots,
        slot_duration_ms: fallback.slot_duration_ms,
        ticks_per_slot: fallback.ticks_per_slot,
        proposal_tick_phase: fallback.proposal_tick_phase,
        adaptive_tick_scheduler_enabled: fallback.adaptive_tick_scheduler_enabled,
        slot_clock_genesis_unix_ms: fallback.slot_clock_genesis_unix_ms,
        max_past_slot_lag: fallback.max_past_slot_lag,
    }
}

fn validator_id_from_registry_binding(slot_id: &str, binding_key: &str) -> String {
    let prefix = format!("{slot_id}.");
    binding_key
        .strip_prefix(prefix.as_str())
        .filter(|value| !value.is_empty())
        .unwrap_or(binding_key)
        .to_string()
}

fn node_main_token_controller_binding_from_registry(
    registry: &GovernanceMainTokenControllerRegistry,
    mut fallback: NodeMainTokenControllerBindingConfig,
) -> NodeMainTokenControllerBindingConfig {
    fallback.genesis_controller_account_id = registry.genesis_controller_account_id.clone();
    if !registry.treasury_bucket_controller_slots.is_empty() {
        fallback.treasury_bucket_controller_slots =
            registry.treasury_bucket_controller_slots.clone();
    }
    fallback.controller_signer_policies = registry
        .controller_signer_policies
        .iter()
        .map(|(account_id, policy)| {
            (
                account_id.clone(),
                node_main_token_controller_signer_policy_from_registry(policy),
            )
        })
        .collect::<BTreeMap<String, NodeMainTokenControllerSignerPolicy>>();
    fallback
}

fn node_main_token_controller_signer_policy_from_registry(
    policy: &GovernanceThresholdSignerPolicy,
) -> NodeMainTokenControllerSignerPolicy {
    NodeMainTokenControllerSignerPolicy {
        threshold: policy.threshold,
        allowed_public_keys: policy
            .allowed_public_keys
            .iter()
            .cloned()
            .collect::<BTreeSet<String>>(),
    }
}

#[cfg(test)]
#[path = "governance_registry_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "governance_signer_completeness_tests.rs"]
mod signer_completeness_tests;
