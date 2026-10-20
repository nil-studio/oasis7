use oasis7::network_tier_manifest::LoadedNetworkTierManifest;
use std::collections::BTreeMap;
use std::fs;
use std::path::{Component, Path};

use super::{
    RuntimeAuthorityBinding, canonicalize_registry_for_authority, required_sha256, required_string,
    semantic_registry_sha256, sha256_regular_file, startup_ref,
};

pub(super) fn load_observer_registry_authority(
    execution_world_dir: &Path,
    effective_world_id: &str,
    registry_path: &Path,
    loaded_network_tier_manifest: Option<&LoadedNetworkTierManifest>,
) -> Result<Option<RuntimeAuthorityBinding>, String> {
    let loaded = loaded_network_tier_manifest.ok_or_else(|| {
        "observer registry authority requires --network-tier-manifest".to_string()
    })?;
    if loaded.manifest.tier != "public_testnet" {
        return Err("observer registry authority requires public_testnet tier".to_string());
    }
    if !loaded.manifest.validator_policy.allow_observer_nodes {
        return Err(
            "observer registry authority requires validator_policy.allow_observer_nodes=true"
                .to_string(),
        );
    }

    let manifest_path = fs::canonicalize(Path::new(loaded.source_path.as_str())).map_err(|err| {
        format!(
            "canonicalize network-tier manifest {} for observer registry authority failed: {err}",
            loaded.source_path
        )
    })?;
    let registry_path = canonicalize_regular_file(registry_path, "genesis validator registry")?;
    let manifest_bytes = fs::read(&manifest_path).map_err(|err| {
        format!(
            "read network-tier manifest {} for observer registry authority failed: {err}",
            manifest_path.display()
        )
    })?;
    let manifest: serde_json::Value =
        serde_json::from_slice(manifest_bytes.as_slice()).map_err(|err| {
            format!(
                "parse network-tier manifest {} for observer registry authority failed: {err}",
                manifest_path.display()
            )
        })?;
    let registry_binding = manifest
        .get("deployment_validator_registry")
        .and_then(serde_json::Value::as_object)
        .ok_or_else(|| {
            "observer network-tier manifest is missing deployment_validator_registry authority binding"
                .to_string()
        })?;
    let registry_ref = required_string(registry_binding, "ref", "deployment_validator_registry")?;
    let expected_registry_sha256 =
        required_sha256(registry_binding, "sha256", "deployment_validator_registry")?;
    let expected_registry_semantic_sha256 = required_sha256(
        registry_binding,
        "semantic_sha256",
        "deployment_validator_registry",
    )?;
    let expected_registry_name = authority_file_name(registry_ref.as_str())?;
    if registry_path.file_name().and_then(|value| value.to_str())
        != Some(expected_registry_name.as_str())
    {
        return Err(format!(
            "observer registry path does not match manifest authority ref: expected={} actual={}",
            registry_ref,
            registry_path.display()
        ));
    }
    if registry_path.parent() != manifest_path.parent() {
        return Err(format!(
            "observer registry path must be installed beside the localized manifest: manifest={} registry={}",
            manifest_path.display(),
            registry_path.display()
        ));
    }

    let registry_sha256 = sha256_regular_file(&registry_path, "genesis validator registry")?;
    if registry_sha256 != expected_registry_sha256 {
        return Err(format!(
            "observer validator registry digest mismatch: manifest={} actual={} path={}",
            expected_registry_sha256,
            registry_sha256,
            registry_path.display()
        ));
    }
    let explicit_registry =
        super::super::governance_registry::load_genesis_finality_registry(&registry_path)?;
    if explicit_registry.slot_id != "governance.finality.v1" {
        return Err("observer validator registry slot_id is not canonical".to_string());
    }
    if explicit_registry.threshold == 0
        || usize::from(explicit_registry.threshold) > explicit_registry.signer_bindings.len()
    {
        return Err("observer validator registry threshold is invalid".to_string());
    }
    if explicit_registry.signer_bindings.len()
        != loaded.manifest.validator_policy.target_validator_count as usize
    {
        return Err(
            "observer validator registry count does not match manifest authority".to_string(),
        );
    }
    let canonical_explicit_registry = canonicalize_registry_for_authority(explicit_registry);
    let registry_semantic_sha256 = semantic_registry_sha256(&canonical_explicit_registry)?;
    if registry_semantic_sha256 != expected_registry_semantic_sha256 {
        return Err(format!(
            "observer validator registry semantic digest mismatch: manifest={} actual={}",
            expected_registry_semantic_sha256, registry_semantic_sha256
        ));
    }

    let world = super::load_authority_world(execution_world_dir, effective_world_id)?;
    let effective_registry = world
        .resolve_governance_effective_finality_signer_registry()
        .map_err(|err| {
            format!(
                "failed to resolve effective governance registry for observer authority preflight: {err:?}"
            )
        })?;
    if let Some(effective_registry) = effective_registry {
        let canonical_effective_registry = canonicalize_registry_for_authority(effective_registry);
        if canonical_effective_registry != canonical_explicit_registry {
            return Err(
                "observer registry does not match persisted effective governance registry"
                    .to_string(),
            );
        }
    }

    let mut validator_signer_public_keys = BTreeMap::new();
    let mut validator_stakes = BTreeMap::new();
    for (binding, public_key) in &canonical_explicit_registry.signer_bindings {
        let node_id = binding
            .strip_prefix("governance.finality.v1.")
            .unwrap_or(binding)
            .to_string();
        validator_signer_public_keys.insert(node_id.clone(), public_key.clone());
        if let Some(stake) = canonical_explicit_registry.validator_stakes.get(binding) {
            validator_stakes.insert(node_id, *stake);
        }
    }

    let registry_ref = startup_ref(&registry_path, "genesis validator registry")?;
    Ok(Some(RuntimeAuthorityBinding {
        registry_ref,
        registry_sha256,
        registry_semantic_sha256,
        // The observer contract deliberately has no deployment inventory.
        // Keep the existing status shape while making the absence explicit.
        inventory_ref: String::new(),
        inventory_sha256: String::new(),
        validator_signer_public_keys,
        validator_stakes,
        validator_47_provider_peer_id: None,
    }))
}

fn canonicalize_regular_file(path: &Path, label: &str) -> Result<std::path::PathBuf, String> {
    startup_ref(path, label)?;
    fs::canonicalize(path)
        .map_err(|err| format!("canonicalize {label} {} failed: {err}", path.display()))
}

fn authority_file_name(raw_ref: &str) -> Result<String, String> {
    let path = Path::new(raw_ref);
    if path.is_absolute()
        || path
            .components()
            .any(|component| matches!(component, Component::CurDir | Component::ParentDir))
    {
        return Err(format!(
            "observer registry authority ref must be a relative confined path: {raw_ref}"
        ));
    }
    path.file_name()
        .and_then(|value| value.to_str())
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
        .ok_or_else(|| format!("observer registry authority ref has no file name: {raw_ref}"))
}
