use super::{
    NETWORK_TIER_MANIFEST_SCHEMA_V1, NETWORK_TIER_MANIFEST_SCHEMA_V2, NetworkTierManifest,
    string_list_contains_ascii_case_insensitive, validate_choice, validate_non_empty,
};
use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NetworkTierReleasePolicy {
    pub stage: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NetworkTierWorldPolicy {
    pub world_id: String,
    pub retention: String,
    pub reset_policy: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NetworkTierAuthorityPolicy {
    pub profile: String,
    pub profile_version: u64,
    pub activation: String,
}

// Inspect presence before serde turns explicit null into None. v1 must never
// silently discard a declaration that its prototype does not understand.
pub(super) fn validate_schema_fields(source: &serde_json::Value) -> Result<(), String> {
    let policies = ["release_policy", "world_policy", "authority_policy"];
    match source
        .get("schema_version")
        .and_then(serde_json::Value::as_str)
    {
        Some(NETWORK_TIER_MANIFEST_SCHEMA_V1) => {
            for field in policies {
                if source.get(field).is_some() {
                    return Err(format!("v1 network tier manifest forbids {field}"));
                }
            }
        }
        Some(NETWORK_TIER_MANIFEST_SCHEMA_V2) => {
            for field in policies {
                if !source.get(field).is_some_and(serde_json::Value::is_object) {
                    return Err(format!("v2 network tier manifest requires {field} object"));
                }
            }
            if source
                .get("token_policy")
                .and_then(|policy| policy.get("reset_policy"))
                .is_some()
            {
                return Err(
                    "v2 forbids token_policy.reset_policy; use world_policy.reset_policy".into(),
                );
            }
        }
        _ => return Err("unsupported network tier manifest schema_version".into()),
    }
    Ok(())
}

pub(super) fn validate_planned_policy(
    manifest: &NetworkTierManifest,
    path: &Path,
) -> Result<(), String> {
    let release = manifest
        .release_policy
        .as_ref()
        .ok_or("v2 requires release_policy")?;
    let world = manifest
        .world_policy
        .as_ref()
        .ok_or("v2 requires world_policy")?;
    let authority = manifest
        .authority_policy
        .as_ref()
        .ok_or("v2 requires authority_policy")?;
    validate_choice(
        manifest.tier.as_str(),
        &["local_devnet", "public_testnet"],
        "v2 tier",
        path,
    )?;
    validate_choice(
        manifest.status.as_str(),
        &["planned", "specified_skeleton_only", "rehearsal"],
        "v2 status",
        path,
    )?;
    validate_choice(
        release.stage.as_str(),
        &["limited_preview"],
        "release_policy.stage",
        path,
    )?;
    validate_non_empty(&world.world_id, "world_policy.world_id", path)?;
    validate_choice(
        world.retention.as_str(),
        &["persistent"],
        "world_policy.retention",
        path,
    )?;
    validate_choice(
        world.reset_policy.as_str(),
        &["frozen"],
        "world_policy.reset_policy",
        path,
    )?;
    validate_choice(
        authority.profile.as_str(),
        &["controlled_single_authority"],
        "authority_policy.profile",
        path,
    )?;
    validate_choice(
        authority.activation.as_str(),
        &["planned"],
        "authority_policy.activation",
        path,
    )?;
    if authority.profile_version != 1 {
        return Err(
            "unsupported authority_policy.profile_version; only version 1 is declared".into(),
        );
    }
    validate_choice(
        manifest.token_policy.value_semantics.as_str(),
        &["preview"],
        "v2 token_policy.value_semantics",
        path,
    )?;
    validate_choice(
        manifest.token_policy.faucet_mode.as_str(),
        &["none", "operator_grant"],
        "v2 token_policy.faucet_mode",
        path,
    )?;
    if manifest.token_policy.faucet_mode == "none" && manifest.endpoint_policy.faucet_ref.is_some()
    {
        return Err("faucet_mode=none forbids endpoint_policy.faucet_ref".into());
    }
    let admission = if manifest.tier == "local_devnet" {
        &["local_only"][..]
    } else {
        &["shared_allowlist", "allowlist_or_governed_candidate"][..]
    };
    validate_choice(
        manifest.validator_policy.validator_admission.as_str(),
        admission,
        "v2 validator_admission",
        path,
    )?;
    for claim in [
        "mainnet",
        "production_oc_settlement",
        "controlled_single_authority_live",
        "persistent_world_live",
        "distributed_finality",
    ] {
        if string_list_contains_ascii_case_insensitive(
            &manifest.claims_policy.allowed_claims,
            claim,
        ) {
            return Err(format!("v2 planned preview forbids allowed claim: {claim}"));
        }
    }
    let denied = &manifest.claims_policy.denied_claims;
    if !string_list_contains_ascii_case_insensitive(denied, "mainnet")
        || !string_list_contains_ascii_case_insensitive(denied, "production_oc_settlement")
    {
        return Err("v2 preview must deny mainnet and production_oc_settlement claims".into());
    }
    if manifest.tier == "public_testnet"
        && !string_list_contains_ascii_case_insensitive(
            &manifest.claims_policy.allowed_claims,
            "public_testnet",
        )
    {
        return Err("public_testnet must explicitly allow public_testnet claims".into());
    }
    // These are declaration checks, not evidence that the world is activated,
    // durable, or that this referenced artifact proves a fixed genesis identity.
    let genesis = super::resolve_manifest_relative_path(path, &manifest.runtime_refs.genesis_ref);
    let bytes = std::fs::read(&genesis).map_err(|err| format!("read genesis_ref failed: {err}"))?;
    let identity: serde_json::Value = serde_json::from_slice(&bytes)
        .map_err(|err| format!("parse v2 genesis_ref identity failed: {err}"))?;
    if identity.get("world_id").and_then(serde_json::Value::as_str) != Some(world.world_id.as_str())
        || identity.get("chain_id").and_then(serde_json::Value::as_str)
            != Some(manifest.chain_id.as_str())
    {
        return Err("v2 world_policy.world_id/chain_id must match genesis_ref identity".into());
    }
    Ok(())
}
