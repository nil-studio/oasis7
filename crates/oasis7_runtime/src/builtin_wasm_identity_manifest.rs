use std::collections::BTreeMap;

use serde::Deserialize;

use super::{ModuleArtifactIdentity, WorldError};

const IDENTITY_HASH_SIGNATURE_SCHEME: &str = "identity_hash_v1";
const IDENTITY_HASH_SIGNATURE_PREFIX: &str = "idhash:";

#[derive(Debug, Deserialize)]
struct BuiltinIdentityManifest {
    modules: Vec<BuiltinIdentityEntry>,
}

#[derive(Debug, Deserialize)]
struct BuiltinIdentityEntry {
    module_id: String,
    source_hash: String,
    build_manifest_hash: String,
    identity_hash: String,
    #[serde(default)]
    hash_tokens: Vec<String>,
    #[serde(default)]
    signer_node_id: Option<String>,
    #[serde(default)]
    signature_scheme: Option<String>,
    #[serde(default)]
    artifact_signatures: BTreeMap<String, String>,
}

fn parse_hash_token_value(token: &str) -> &str {
    token
        .split_once('=')
        .map(|(_, hash)| hash)
        .unwrap_or(token)
        .trim()
}

pub(crate) fn module_artifact_identity_from_manifest(
    manifest_json: &str,
    manifest_name: &str,
    module_id: &str,
    wasm_hash: &str,
) -> Result<ModuleArtifactIdentity, WorldError> {
    let manifest: BuiltinIdentityManifest =
        serde_json::from_str(manifest_json).map_err(|error| WorldError::ModuleChangeInvalid {
            reason: format!(
                "failed to parse builtin identity manifest {}: {}",
                manifest_name, error
            ),
        })?;

    let Some(entry) = manifest
        .modules
        .into_iter()
        .find(|candidate| candidate.module_id == module_id)
    else {
        return Err(WorldError::ModuleChangeInvalid {
            reason: format!(
                "builtin identity manifest {} missing module_id={}",
                manifest_name, module_id
            ),
        });
    };

    let expected_identity_hash = super::util::sha256_hex(
        format!(
            "{module_id}:{}:{}",
            entry.source_hash, entry.build_manifest_hash
        )
        .as_bytes(),
    );
    if expected_identity_hash != entry.identity_hash {
        return Err(WorldError::ModuleChangeInvalid {
            reason: format!(
                "builtin identity manifest {} has invalid identity_hash module_id={} expected={} actual={}",
                manifest_name, module_id, expected_identity_hash, entry.identity_hash
            ),
        });
    }

    if let Some(artifact_signature) = entry.artifact_signatures.get(wasm_hash).cloned() {
        let signer_node_id = entry
            .signer_node_id
            .clone()
            .filter(|value| !value.trim().is_empty())
            .ok_or_else(|| WorldError::ModuleChangeInvalid {
                reason: format!(
                    "builtin identity manifest {} missing signer_node_id module_id={}",
                    manifest_name, module_id
                ),
            })?;
        let signature_scheme = entry
            .signature_scheme
            .clone()
            .filter(|value| !value.trim().is_empty())
            .ok_or_else(|| WorldError::ModuleChangeInvalid {
                reason: format!(
                    "builtin identity manifest {} missing signature_scheme module_id={}",
                    manifest_name, module_id
                ),
            })?;

        return Ok(ModuleArtifactIdentity {
            source_hash: entry.source_hash,
            build_manifest_hash: entry.build_manifest_hash,
            signer_node_id,
            signature_scheme,
            artifact_signature,
        });
    }

    if !entry
        .hash_tokens
        .iter()
        .map(|token| parse_hash_token_value(token.as_str()))
        .any(|hash| hash == wasm_hash)
    {
        return Err(WorldError::ModuleChangeInvalid {
            reason: format!(
                "builtin identity manifest {} missing hash token module_id={} wasm_hash={}",
                manifest_name, module_id, wasm_hash
            ),
        });
    }

    let signer_node_id = entry
        .signer_node_id
        .unwrap_or_else(|| "builtin.module.release.signer".to_string());

    Ok(ModuleArtifactIdentity {
        source_hash: entry.source_hash,
        build_manifest_hash: entry.build_manifest_hash,
        signer_node_id,
        signature_scheme: IDENTITY_HASH_SIGNATURE_SCHEME.to_string(),
        artifact_signature: format!("{IDENTITY_HASH_SIGNATURE_PREFIX}{}", entry.identity_hash),
    })
}
