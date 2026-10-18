#[cfg(all(test, feature = "wasmtime", feature = "test_tier_full"))]
use super::world::World;
use super::{
    ModuleArtifactIdentity, WorldError, builtin_wasm_materializer::builtin_wasm_distfs_root,
    load_builtin_wasm_with_fetch_fallback,
};

const M1_BUILTIN_HASH_MANIFEST: &str = include_str!("world/artifacts/m1_builtin_modules.sha256");
const M1_BUILTIN_IDENTITY_MANIFEST: &str =
    include_str!("world/artifacts/m1_builtin_modules.identity.json");

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

fn hash_manifest_for_module(module_id: &str) -> Option<Vec<&'static str>> {
    for line in M1_BUILTIN_HASH_MANIFEST.lines() {
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
    super::builtin_wasm_identity_manifest::module_artifact_identity_from_manifest(
        M1_BUILTIN_IDENTITY_MANIFEST,
        "m1_builtin_modules.identity.json",
        module_id,
        wasm_hash,
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
