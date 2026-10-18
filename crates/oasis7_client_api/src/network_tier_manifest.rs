use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

#[path = "network_tier_manifest/policies.rs"]
mod policies;
pub use policies::{NetworkTierAuthorityPolicy, NetworkTierReleasePolicy, NetworkTierWorldPolicy};

pub const NETWORK_TIER_MANIFEST_SCHEMA_V2: &str = "oasis7.network_tier_manifest.v2";

pub const NETWORK_TIER_MANIFEST_SCHEMA_V1: &str = "oasis7.network_tier_manifest.v1";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NetworkTierManifest {
    pub schema_version: String,
    pub tier: String,
    pub status: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub release_policy: Option<NetworkTierReleasePolicy>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub world_policy: Option<NetworkTierWorldPolicy>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub authority_policy: Option<NetworkTierAuthorityPolicy>,
    pub network_id: String,
    pub chain_id: String,
    pub runtime_refs: NetworkTierRuntimeRefs,
    pub endpoint_policy: NetworkTierEndpointPolicy,
    pub validator_policy: NetworkTierValidatorPolicy,
    pub token_policy: NetworkTierTokenPolicy,
    pub claims_policy: NetworkTierClaimsPolicy,
    pub promotion_policy: NetworkTierPromotionPolicy,
    #[serde(default)]
    pub evidence_refs: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NetworkTierRuntimeRefs {
    pub release_candidate_bundle_ref: String,
    pub genesis_ref: String,
    pub bootstrap_peer_ref: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NetworkTierEndpointPolicy {
    pub rpc_ref: String,
    pub explorer_ref: String,
    pub faucet_ref: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NetworkTierValidatorPolicy {
    pub governance_mode: String,
    pub validator_admission: String,
    pub target_validator_count: u64,
    pub allow_observer_nodes: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NetworkTierTokenPolicy {
    pub symbol: String,
    pub faucet_mode: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reset_policy: Option<String>,
    pub value_semantics: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NetworkTierClaimsPolicy {
    #[serde(default)]
    pub allowed_claims: Vec<String>,
    #[serde(default)]
    pub denied_claims: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NetworkTierPromotionPolicy {
    #[serde(default)]
    pub promote_from: Vec<String>,
    #[serde(default)]
    pub required_gates: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct LoadedNetworkTierManifest {
    pub source_path: String,
    pub manifest: NetworkTierManifest,
    pub bootstrap_peers: Vec<String>,
}

impl LoadedNetworkTierManifest {
    /// Schema validity does not activate a formal authority profile.
    pub fn validate_runtime_support(&self) -> Result<(), String> {
        if self.manifest.schema_version != NETWORK_TIER_MANIFEST_SCHEMA_V1 {
            return Err("network tier manifest authority activation is planned and unsupported by this runtime; v2 cannot start the compatibility prototype".to_string());
        }
        Ok(())
    }

    pub fn load(path: &Path) -> Result<Self, String> {
        let source_path = path
            .canonicalize()
            .unwrap_or_else(|_| path.to_path_buf())
            .to_string_lossy()
            .to_string();
        let raw = fs::read_to_string(path).map_err(|err| {
            format!(
                "read network tier manifest {} failed: {err}",
                path.display()
            )
        })?;
        let source: serde_json::Value = serde_json::from_str(raw.as_str()).map_err(|err| {
            format!(
                "parse network tier manifest {} failed: {err}",
                path.display()
            )
        })?;
        policies::validate_schema_fields(&source)?;
        let manifest: NetworkTierManifest = serde_json::from_str(raw.as_str()).map_err(|err| {
            format!(
                "parse network tier manifest {} failed: {err}",
                path.display()
            )
        })?;
        validate_manifest(&manifest, path)?;
        let bootstrap_path =
            resolve_manifest_relative_path(path, manifest.runtime_refs.bootstrap_peer_ref.as_str());
        let bootstrap_peers = load_bootstrap_peers(bootstrap_path.as_path())?;
        Ok(Self {
            source_path,
            manifest,
            bootstrap_peers,
        })
    }
}

fn validate_manifest(manifest: &NetworkTierManifest, path: &Path) -> Result<(), String> {
    if !matches!(
        manifest.schema_version.as_str(),
        NETWORK_TIER_MANIFEST_SCHEMA_V1 | NETWORK_TIER_MANIFEST_SCHEMA_V2
    ) {
        return Err(format!(
            "network tier manifest {} has unsupported schema_version `{}`",
            path.display(),
            manifest.schema_version
        ));
    }
    validate_choice(
        manifest.tier.as_str(),
        &["local_devnet", "public_testnet", "mainnet"],
        "tier",
        path,
    )?;
    validate_choice(
        manifest.status.as_str(),
        &["planned", "specified_skeleton_only", "rehearsal", "live"],
        "status",
        path,
    )?;
    validate_non_empty(manifest.network_id.as_str(), "network_id", path)?;
    validate_non_empty(manifest.chain_id.as_str(), "chain_id", path)?;
    validate_non_empty(
        manifest.runtime_refs.release_candidate_bundle_ref.as_str(),
        "runtime_refs.release_candidate_bundle_ref",
        path,
    )?;
    validate_non_empty(
        manifest.runtime_refs.genesis_ref.as_str(),
        "runtime_refs.genesis_ref",
        path,
    )?;
    validate_non_empty(
        manifest.runtime_refs.bootstrap_peer_ref.as_str(),
        "runtime_refs.bootstrap_peer_ref",
        path,
    )?;
    validate_non_empty(
        manifest.endpoint_policy.rpc_ref.as_str(),
        "endpoint_policy.rpc_ref",
        path,
    )?;
    validate_non_empty(
        manifest.endpoint_policy.explorer_ref.as_str(),
        "endpoint_policy.explorer_ref",
        path,
    )?;
    if let Some(faucet_ref) = manifest.endpoint_policy.faucet_ref.as_deref() {
        validate_non_empty(faucet_ref, "endpoint_policy.faucet_ref", path)?;
    }
    validate_choice(
        manifest.validator_policy.governance_mode.as_str(),
        &["bootstrap_local", "shared_ops", "governance_registry"],
        "validator_policy.governance_mode",
        path,
    )?;
    validate_choice(
        manifest.validator_policy.validator_admission.as_str(),
        &[
            "local_only",
            "shared_allowlist",
            "allowlist_or_governed_candidate",
            "governance_registry_only",
        ],
        "validator_policy.validator_admission",
        path,
    )?;
    if manifest.validator_policy.target_validator_count == 0 {
        return Err(format!(
            "network tier manifest {} requires validator_policy.target_validator_count > 0",
            path.display()
        ));
    }
    validate_non_empty(
        manifest.token_policy.symbol.as_str(),
        "token_policy.symbol",
        path,
    )?;
    validate_choice(
        manifest.token_policy.faucet_mode.as_str(),
        &["none", "operator_grant", "guarded_testnet_faucet"],
        "token_policy.faucet_mode",
        path,
    )?;
    if manifest.schema_version == NETWORK_TIER_MANIFEST_SCHEMA_V1 {
        validate_choice(
            manifest
                .token_policy
                .reset_policy
                .as_deref()
                .unwrap_or_default(),
            &["ephemeral", "resettable", "frozen"],
            "token_policy.reset_policy",
            path,
        )?;
    }
    validate_choice(
        manifest.token_policy.value_semantics.as_str(),
        &["preview", "testnet", "production"],
        "token_policy.value_semantics",
        path,
    )?;
    validate_string_list(
        manifest.claims_policy.allowed_claims.as_slice(),
        "claims_policy.allowed_claims",
        path,
    )?;
    validate_string_list(
        manifest.claims_policy.denied_claims.as_slice(),
        "claims_policy.denied_claims",
        path,
    )?;
    validate_string_list(
        manifest.promotion_policy.promote_from.as_slice(),
        "promotion_policy.promote_from",
        path,
    )?;
    validate_string_list(
        manifest.promotion_policy.required_gates.as_slice(),
        "promotion_policy.required_gates",
        path,
    )?;
    validate_string_list(manifest.evidence_refs.as_slice(), "evidence_refs", path)?;

    let genesis_path =
        resolve_manifest_relative_path(path, manifest.runtime_refs.genesis_ref.as_str());
    if !genesis_path.is_file() {
        return Err(format!(
            "network tier manifest {} references missing runtime_refs.genesis_ref file {}",
            path.display(),
            genesis_path.display()
        ));
    }
    let bootstrap_path =
        resolve_manifest_relative_path(path, manifest.runtime_refs.bootstrap_peer_ref.as_str());
    if !bootstrap_path.is_file() {
        return Err(format!(
            "network tier manifest {} references missing runtime_refs.bootstrap_peer_ref file {}",
            path.display(),
            bootstrap_path.display()
        ));
    }

    if manifest.schema_version == NETWORK_TIER_MANIFEST_SCHEMA_V1 {
        validate_tier_semantics(manifest, path)?;
    } else {
        policies::validate_planned_policy(manifest, path)?;
    }
    Ok(())
}

fn validate_non_empty(raw: &str, field: &str, path: &Path) -> Result<(), String> {
    if raw.trim().is_empty() {
        Err(format!(
            "network tier manifest {} requires non-empty {}",
            path.display(),
            field
        ))
    } else {
        Ok(())
    }
}

fn validate_choice(raw: &str, choices: &[&str], field: &str, path: &Path) -> Result<(), String> {
    if choices.contains(&raw) {
        Ok(())
    } else {
        Err(format!(
            "network tier manifest {} has invalid {} `{}`; expected one of: {}",
            path.display(),
            field,
            raw,
            choices.join(", ")
        ))
    }
}

fn validate_string_list(items: &[String], field: &str, path: &Path) -> Result<(), String> {
    for item in items {
        validate_non_empty(item.as_str(), field, path)?;
    }
    Ok(())
}

fn validate_tier_semantics(manifest: &NetworkTierManifest, path: &Path) -> Result<(), String> {
    let token_policy = &manifest.token_policy;
    let validator_policy = &manifest.validator_policy;
    let endpoint_policy = &manifest.endpoint_policy;
    let claims_policy = &manifest.claims_policy;
    let required_gates = &manifest.promotion_policy.required_gates;

    match manifest.tier.as_str() {
        "local_devnet" => {
            if token_policy.value_semantics != "preview" {
                return Err(format!(
                    "network tier manifest {} requires local_devnet value_semantics=preview",
                    path.display()
                ));
            }
            if token_policy.reset_policy.as_deref() != Some("ephemeral") {
                return Err(format!(
                    "network tier manifest {} requires local_devnet reset_policy=ephemeral",
                    path.display()
                ));
            }
            if validator_policy.validator_admission != "local_only" {
                return Err(format!(
                    "network tier manifest {} requires local_devnet validator_admission=local_only",
                    path.display()
                ));
            }
        }
        "public_testnet" => {
            if token_policy.value_semantics != "testnet" {
                return Err(format!(
                    "network tier manifest {} requires public_testnet value_semantics=testnet",
                    path.display()
                ));
            }
            if token_policy.reset_policy.as_deref() != Some("resettable") {
                return Err(format!(
                    "network tier manifest {} requires public_testnet reset_policy=resettable",
                    path.display()
                ));
            }
            if token_policy.faucet_mode != "guarded_testnet_faucet" {
                return Err(format!(
                    "network tier manifest {} requires public_testnet faucet_mode=guarded_testnet_faucet",
                    path.display()
                ));
            }
            if !matches!(
                validator_policy.validator_admission.as_str(),
                "allowlist_or_governed_candidate" | "shared_allowlist"
            ) {
                return Err(format!(
                    "network tier manifest {} requires public_testnet validator_admission to stay in shared_allowlist or allowlist_or_governed_candidate",
                    path.display()
                ));
            }
            if endpoint_policy
                .faucet_ref
                .as_deref()
                .unwrap_or_default()
                .trim()
                .is_empty()
            {
                return Err(format!(
                    "network tier manifest {} requires public_testnet endpoint_policy.faucet_ref",
                    path.display()
                ));
            }
            if !string_list_contains_ascii_case_insensitive(
                claims_policy.allowed_claims.as_slice(),
                "public_testnet",
            ) {
                return Err(format!(
                    "network tier manifest {} requires public_testnet claims to explicitly allow public_testnet",
                    path.display()
                ));
            }
            if !string_list_contains_ascii_case_insensitive(
                claims_policy.denied_claims.as_slice(),
                "production_oc_settlement",
            ) {
                return Err(format!(
                    "network tier manifest {} requires public_testnet claims to explicitly deny production_oc_settlement",
                    path.display()
                ));
            }
        }
        "mainnet" => {
            if token_policy.value_semantics != "production" {
                return Err(format!(
                    "network tier manifest {} requires mainnet value_semantics=production",
                    path.display()
                ));
            }
            if token_policy.reset_policy.as_deref() != Some("frozen") {
                return Err(format!(
                    "network tier manifest {} requires mainnet reset_policy=frozen",
                    path.display()
                ));
            }
            if token_policy.faucet_mode != "none" {
                return Err(format!(
                    "network tier manifest {} requires mainnet faucet_mode=none",
                    path.display()
                ));
            }
            if validator_policy.governance_mode != "governance_registry" {
                return Err(format!(
                    "network tier manifest {} requires mainnet governance_mode=governance_registry",
                    path.display()
                ));
            }
            if validator_policy.validator_admission != "governance_registry_only" {
                return Err(format!(
                    "network tier manifest {} requires mainnet validator_admission=governance_registry_only",
                    path.display()
                ));
            }
            if endpoint_policy.faucet_ref.is_some() {
                return Err(format!(
                    "network tier manifest {} must not define mainnet endpoint_policy.faucet_ref",
                    path.display()
                ));
            }
            for gate in ["MAINNET-1", "MAINNET-2", "MAINNET-3", "MAINNET-4"] {
                if !required_gates.iter().any(|value| value == gate) {
                    return Err(format!(
                        "network tier manifest {} requires mainnet gate {}",
                        path.display(),
                        gate
                    ));
                }
            }
            if string_list_contains_ascii_case_insensitive(
                claims_policy.allowed_claims.as_slice(),
                "faucet",
            ) {
                return Err(format!(
                    "network tier manifest {} must not allow faucet claims for mainnet",
                    path.display()
                ));
            }
        }
        _ => {}
    }

    if manifest.tier != "mainnet"
        && !string_list_contains_ascii_case_insensitive(
            claims_policy.denied_claims.as_slice(),
            "mainnet",
        )
    {
        return Err(format!(
            "network tier manifest {} requires non-mainnet tiers to explicitly deny mainnet claims",
            path.display()
        ));
    }

    Ok(())
}

fn resolve_manifest_relative_path(manifest_path: &Path, raw: &str) -> PathBuf {
    let candidate = PathBuf::from(raw);
    if candidate.is_absolute() {
        return candidate;
    }
    if let Some(parent) = manifest_path.parent() {
        return parent.join(&candidate);
    }
    candidate
}

fn string_list_contains_ascii_case_insensitive(items: &[String], needle: &str) -> bool {
    items
        .iter()
        .any(|item| contains_ascii_case_insensitive(item, needle))
}

fn contains_ascii_case_insensitive(haystack: &str, needle: &str) -> bool {
    if needle.is_empty() {
        return true;
    }
    haystack
        .as_bytes()
        .windows(needle.len())
        .any(|window| window.eq_ignore_ascii_case(needle.as_bytes()))
}

fn load_bootstrap_peers(path: &Path) -> Result<Vec<String>, String> {
    let raw = fs::read_to_string(path)
        .map_err(|err| format!("read bootstrap peer ref {} failed: {err}", path.display()))?;
    let mut peers = Vec::new();
    for line in raw.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        peers.push(trimmed.to_string());
    }
    if peers.is_empty() {
        return Err(format!(
            "bootstrap peer ref {} does not contain any peers",
            path.display()
        ));
    }
    Ok(peers)
}

#[cfg(test)]
#[path = "network_tier_manifest/tests.rs"]
mod tests;

#[cfg(test)]
#[path = "network_tier_manifest/planned_policy_tests.rs"]
mod planned_policy_tests;
