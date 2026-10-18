use super::super::m1_builtin_wasm_artifact::m1_builtin_wasm_module_artifact_bytes;
use super::super::m4_builtin_wasm_artifact::m4_builtin_wasm_module_artifact_bytes;
use super::super::m5_builtin_wasm_artifact::m5_builtin_wasm_module_artifact_bytes;
use super::super::{
    ModuleArtifactIdentity, WorldError, builtin_wasm_materializer::builtin_wasm_distfs_root,
    load_builtin_wasm_with_fetch_fallback, m1_builtin_module_artifact_identity,
    m4_builtin_module_artifact_identity, m5_builtin_module_artifact_identity,
};
use super::{BuiltinReleaseManifestEntry, World};
const FAULT_SIG_BUILTIN_RELEASE_MANIFEST_UNREACHABLE: &str = "builtin_release_manifest_unreachable";
const FAULT_SIG_BUILTIN_RELEASE_MANIFEST_MISSING_OR_ROLLED_BACK: &str =
    "builtin_release_manifest_missing_or_rolled_back";
const FAULT_SIG_BUILTIN_RELEASE_MANIFEST_IDENTITY_DRIFT: &str =
    "builtin_release_manifest_identity_drift";

fn with_fault_signature(signature: &str, reason: impl Into<String>) -> String {
    format!("fault_signature={signature} {}", reason.into())
}

fn parse_hash_token_value(token: &str) -> &str {
    token
        .split_once('=')
        .map(|(_, hash)| hash)
        .unwrap_or(token)
        .trim()
}

fn normalize_module_set(module_set: &str) -> String {
    module_set.trim().to_ascii_lowercase()
}

fn normalize_module_id(module_id: &str) -> String {
    module_id.trim().to_string()
}

fn normalize_hash_tokens(hash_tokens: &[String]) -> Vec<String> {
    let mut hashes: Vec<String> = hash_tokens
        .iter()
        .map(|token| parse_hash_token_value(token.as_str()))
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .collect();
    hashes.sort();
    hashes.dedup();
    hashes
}

fn fallback_builtin_artifact_identity(
    module_set: &str,
    module_id: &str,
    wasm_hash: &str,
) -> Result<ModuleArtifactIdentity, WorldError> {
    match module_set {
        "m1" => m1_builtin_module_artifact_identity(module_id, wasm_hash),
        "m4" => m4_builtin_module_artifact_identity(module_id, wasm_hash),
        "m5" => m5_builtin_module_artifact_identity(module_id, wasm_hash),
        other => Err(WorldError::ModuleChangeInvalid {
            reason: format!("unsupported builtin module_set={other} for module_id={module_id}"),
        }),
    }
}

impl World {
    pub fn upsert_builtin_release_manifest_entry(
        &mut self,
        module_set: impl Into<String>,
        module_id: impl Into<String>,
        mut entry: BuiltinReleaseManifestEntry,
    ) -> Result<(), WorldError> {
        let module_set = normalize_module_set(module_set.into().as_str());
        let module_id = normalize_module_id(module_id.into().as_str());
        if module_set.is_empty() {
            return Err(WorldError::ModuleChangeInvalid {
                reason: "builtin release manifest module_set is empty".to_string(),
            });
        }
        if module_id.is_empty() {
            return Err(WorldError::ModuleChangeInvalid {
                reason: "builtin release manifest module_id is empty".to_string(),
            });
        }
        let normalized_hashes = normalize_hash_tokens(entry.hash_tokens.as_slice());
        if normalized_hashes.is_empty() {
            return Err(WorldError::ModuleChangeInvalid {
                reason: format!(
                    "builtin release manifest entry has no hash tokens module_set={} module_id={}",
                    module_set, module_id
                ),
            });
        }
        for (wasm_hash, identity) in &entry.artifact_identities {
            if wasm_hash.trim().is_empty() || !identity.is_complete() {
                return Err(WorldError::ModuleChangeInvalid {
                    reason: format!(
                        "builtin release manifest identity is invalid module_set={} module_id={} wasm_hash={}",
                        module_set, module_id, wasm_hash
                    ),
                });
            }
        }
        entry.hash_tokens = normalized_hashes;
        self.builtin_release_manifest
            .module_sets
            .entry(module_set)
            .or_default()
            .insert(module_id, entry);
        Ok(())
    }

    pub fn remove_builtin_release_manifest_entry(
        &mut self,
        module_set: &str,
        module_id: &str,
    ) -> bool {
        let module_set = normalize_module_set(module_set);
        let module_id = normalize_module_id(module_id);
        let Some(entries) = self
            .builtin_release_manifest
            .module_sets
            .get_mut(&module_set)
        else {
            return false;
        };
        let removed = entries.remove(&module_id).is_some();
        if entries.is_empty() {
            self.builtin_release_manifest
                .module_sets
                .remove(&module_set);
        }
        removed
    }

    fn online_builtin_hashes_for_module(
        &self,
        module_set: &str,
        module_id: &str,
    ) -> Option<Vec<String>> {
        let module_set = normalize_module_set(module_set);
        let module_id = normalize_module_id(module_id);
        self.builtin_release_manifest
            .module_sets
            .get(module_set.as_str())
            .and_then(|entries| entries.get(module_id.as_str()))
            .map(|entry| normalize_hash_tokens(entry.hash_tokens.as_slice()))
            .filter(|hashes| !hashes.is_empty())
    }

    pub(super) fn load_builtin_wasm_artifact_for_module(
        &self,
        module_set: &str,
        module_id: &str,
    ) -> Result<Vec<u8>, WorldError> {
        let module_set = normalize_module_set(module_set);
        let module_id = normalize_module_id(module_id);
        if let Some(hashes) =
            self.online_builtin_hashes_for_module(module_set.as_str(), module_id.as_str())
        {
            let hash_refs: Vec<&str> = hashes.iter().map(String::as_str).collect();
            let distfs_root = builtin_wasm_distfs_root();
            return load_builtin_wasm_with_fetch_fallback(module_id.as_str(), &hash_refs, &distfs_root)
                .map_err(|error| WorldError::ModuleChangeInvalid {
                    reason: with_fault_signature(
                        FAULT_SIG_BUILTIN_RELEASE_MANIFEST_UNREACHABLE,
                        format!(
                        "failed to materialize builtin wasm artifact module_set={} module_id={} hashes=[{}] distfs_root={} err={:?}",
                        module_set,
                        module_id,
                        hashes.join(","),
                        distfs_root.display(),
                        error
                        ),
                    ),
                });
        }
        if !self.release_security_policy.allow_builtin_manifest_fallback {
            return Err(WorldError::ModuleChangeInvalid {
                reason: with_fault_signature(
                    FAULT_SIG_BUILTIN_RELEASE_MANIFEST_MISSING_OR_ROLLED_BACK,
                    format!(
                        "builtin release manifest entry missing module_set={} module_id={} fallback_allowed={}",
                        module_set,
                        module_id,
                        self.release_security_policy.allow_builtin_manifest_fallback
                    ),
                ),
            });
        }
        match module_set.as_str() {
            "m1" => m1_builtin_wasm_module_artifact_bytes(module_id.as_str()),
            "m4" => m4_builtin_wasm_module_artifact_bytes(module_id.as_str()),
            "m5" => m5_builtin_wasm_module_artifact_bytes(module_id.as_str()),
            other => Err(WorldError::ModuleChangeInvalid {
                reason: format!("unsupported builtin module_set={other} for module_id={module_id}"),
            }),
        }
    }

    pub(super) fn resolve_builtin_module_artifact_identity(
        &self,
        module_set: &str,
        module_id: &str,
        wasm_hash: &str,
    ) -> Result<ModuleArtifactIdentity, WorldError> {
        let module_set = normalize_module_set(module_set);
        let module_id = normalize_module_id(module_id);
        let manifest_entry = self
            .builtin_release_manifest
            .module_sets
            .get(module_set.as_str())
            .and_then(|entries| entries.get(module_id.as_str()))
            .cloned();
        if let Some(entry) = manifest_entry {
            if let Some(identity) = entry.artifact_identities.get(wasm_hash) {
                return Ok(identity.clone());
            }
            if self.release_security_policy.allow_builtin_manifest_fallback {
                return fallback_builtin_artifact_identity(
                    module_set.as_str(),
                    module_id.as_str(),
                    wasm_hash,
                );
            }
            let known_wasm_hashes = entry
                .artifact_identities
                .keys()
                .cloned()
                .collect::<Vec<_>>()
                .join(",");
            return Err(WorldError::ModuleChangeInvalid {
                reason: with_fault_signature(
                    FAULT_SIG_BUILTIN_RELEASE_MANIFEST_IDENTITY_DRIFT,
                    format!(
                        "builtin release artifact identity missing module_set={} module_id={} wasm_hash={} known_wasm_hashes=[{}] fallback_allowed={}",
                        module_set,
                        module_id,
                        wasm_hash,
                        known_wasm_hashes,
                        self.release_security_policy.allow_builtin_manifest_fallback
                    ),
                ),
            });
        }
        if self.release_security_policy.allow_builtin_manifest_fallback {
            return fallback_builtin_artifact_identity(
                module_set.as_str(),
                module_id.as_str(),
                wasm_hash,
            );
        }
        Err(WorldError::ModuleChangeInvalid {
            reason: with_fault_signature(
                FAULT_SIG_BUILTIN_RELEASE_MANIFEST_MISSING_OR_ROLLED_BACK,
                format!(
                    "builtin release artifact identity missing module_set={} module_id={} wasm_hash={} fallback_allowed={}",
                    module_set,
                    module_id,
                    wasm_hash,
                    self.release_security_policy.allow_builtin_manifest_fallback
                ),
            ),
        })
    }
}
