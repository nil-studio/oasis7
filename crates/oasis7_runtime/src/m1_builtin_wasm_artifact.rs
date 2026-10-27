#[cfg(all(test, feature = "wasmtime", feature = "test_tier_full"))]
use super::world::World;
use super::{
    ModuleArtifactIdentity, WorldError, builtin_wasm_materializer::builtin_wasm_distfs_root,
    load_builtin_wasm_with_fetch_fallback,
};

const M1_BUILTIN_HASH_MANIFEST: &str = include_str!("world/artifacts/m1_builtin_modules.sha256");
const M1_BUILTIN_IDENTITY_MANIFEST: &str =
    include_str!("world/artifacts/m1_builtin_modules.identity.json");
// Published legacy release manifests are frozen. New storage semantics have a
// distinct versioned binary identity; historical snapshots retain their own hashes.
const M1_STORAGE_V2_HASH_MANIFEST: &str =
    include_str!("world/artifacts/m1_storage_power_v0_2_0.sha256");
const M1_STORAGE_V2_IDENTITY_MANIFEST: &str =
    include_str!("world/artifacts/m1_storage_power_v0_2_0.identity.json");

#[cfg(all(test, feature = "wasmtime", feature = "test_tier_full"))]
pub(crate) fn m1_builtin_module_ids_manifest() -> Vec<&'static str> {
    include_str!("world/artifacts/m1_builtin_module_ids.txt")
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .collect()
}

fn hash_value_from_manifest_token(token: &'static str) -> Option<&'static str> {
    let value = token
        .split_once('=')
        .map(|(_, hash)| hash)
        .unwrap_or(token)
        .trim();
    if value.is_empty() { None } else { Some(value) }
}

pub(crate) fn storage_power_v2_hash_matches(wasm_hash: &str) -> bool {
    M1_STORAGE_V2_HASH_MANIFEST
        .split_whitespace()
        .skip(1)
        .filter_map(hash_value_from_manifest_token)
        .any(|hash| hash == wasm_hash)
}

fn hash_manifest_for_module(module_id: &str) -> Option<Vec<&'static str>> {
    let manifest = if module_id == super::M1_STORAGE_POWER_MODULE_ID {
        M1_STORAGE_V2_HASH_MANIFEST
    } else {
        M1_BUILTIN_HASH_MANIFEST
    };
    for line in manifest.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let mut parts = line.split_whitespace();
        let Some(id) = parts.next() else {
            continue;
        };
        if id == module_id {
            let hashes: Vec<&'static str> =
                parts.filter_map(hash_value_from_manifest_token).collect();
            if !hashes.is_empty() {
                return Some(hashes);
            }
        }
    }
    None
}

#[cfg(test)]
pub(crate) fn m1_builtin_manifest_hash_tokens(module_id: &str) -> Option<Vec<String>> {
    hash_manifest_for_module(module_id)
        .map(|tokens| tokens.into_iter().map(str::to_string).collect())
}

pub(crate) fn m1_builtin_wasm_module_artifact_bytes(
    module_id: &str,
) -> Result<Vec<u8>, WorldError> {
    let expected_hashes =
        hash_manifest_for_module(module_id).ok_or_else(|| WorldError::ModuleChangeInvalid {
            reason: format!("missing builtin wasm hash manifest entry for module_id={module_id}"),
        })?;
    let distfs_root = builtin_wasm_distfs_root();
    let wasm_bytes = load_builtin_wasm_with_fetch_fallback(module_id, &expected_hashes, &distfs_root)
        .map_err(|error| WorldError::ModuleChangeInvalid {
            reason: format!(
                "failed to materialize builtin wasm artifact module_id={module_id}, hashes=[{}], distfs_root={}, err={error:?}",
                expected_hashes.join(","),
                distfs_root.display()
            ),
        })?;

    Ok(wasm_bytes)
}

pub(crate) fn m1_builtin_module_artifact_identity(
    module_id: &str,
    wasm_hash: &str,
) -> Result<ModuleArtifactIdentity, WorldError> {
    let current_storage_hash = storage_power_v2_hash_matches(wasm_hash);
    let (manifest, filename) =
        if module_id == super::M1_STORAGE_POWER_MODULE_ID && current_storage_hash {
            (
                M1_STORAGE_V2_IDENTITY_MANIFEST,
                "m1_storage_power_v0_2_0.identity.json",
            )
        } else {
            (
                M1_BUILTIN_IDENTITY_MANIFEST,
                "m1_builtin_modules.identity.json",
            )
        };
    super::builtin_wasm_identity_manifest::module_artifact_identity_from_manifest(
        manifest, filename, module_id, wasm_hash,
    )
}

#[cfg(all(test, feature = "wasmtime", feature = "test_tier_full"))]
pub(crate) fn register_m1_builtin_wasm_module_artifact(
    world: &mut World,
    module_id: &str,
) -> Result<String, WorldError> {
    let wasm_bytes = m1_builtin_wasm_module_artifact_bytes(module_id)?;
    let wasm_hash = super::util::sha256_hex(&wasm_bytes);
    world.register_module_artifact(wasm_hash.clone(), &wasm_bytes)?;
    Ok(wasm_hash)
}

#[cfg(test)]
mod storage_version_identity_tests {
    use super::*;

    #[test]
    fn historical_storage_hash_keeps_its_frozen_identity() {
        let hash = "35ee85efeeb41317ff63432131a67541856e36f7d5845ce9237bc8c63497ef06";
        let expected =
            super::super::builtin_wasm_identity_manifest::module_artifact_identity_from_manifest(
                M1_BUILTIN_IDENTITY_MANIFEST,
                "m1_builtin_modules.identity.json",
                "m1.power.storage",
                hash,
            )
            .unwrap();
        assert_eq!(
            m1_builtin_module_artifact_identity("m1.power.storage", hash).unwrap(),
            expected
        );
    }

    #[test]
    fn current_storage_hash_selects_only_the_new_version_identity() {
        let hash = hash_manifest_for_module("m1.power.storage").unwrap()[0];
        let expected =
            super::super::builtin_wasm_identity_manifest::module_artifact_identity_from_manifest(
                M1_STORAGE_V2_IDENTITY_MANIFEST,
                "m1_storage_power_v0_2_0.identity.json",
                "m1.power.storage",
                hash,
            )
            .unwrap();
        assert_eq!(
            m1_builtin_module_artifact_identity("m1.power.storage", hash).unwrap(),
            expected
        );
        assert!(m1_builtin_module_artifact_identity("m1.power.storage", &"0".repeat(64)).is_err());
    }
}
