use serde::Serialize;

#[derive(Debug, Serialize)]
pub(crate) struct ChainNetworkTierStatus {
    pub(crate) source_path: String,
    pub(crate) schema_version: String,
    pub(crate) tier: String,
    pub(crate) status: String,
    pub(crate) network_id: String,
    pub(crate) chain_id: String,
    pub(crate) bootstrap_peer_count: usize,
    pub(crate) governance_mode: String,
    pub(crate) validator_admission: String,
    pub(crate) target_validator_count: u64,
    pub(crate) allow_observer_nodes: bool,
    pub(crate) token_symbol: String,
    pub(crate) faucet_mode: String,
    pub(crate) reset_policy: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) release_policy: Option<oasis7::network_tier_manifest::NetworkTierReleasePolicy>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) world_policy: Option<oasis7::network_tier_manifest::NetworkTierWorldPolicy>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) authority_policy: Option<oasis7::network_tier_manifest::NetworkTierAuthorityPolicy>,
    pub(crate) runtime_supported: bool,
    pub(crate) value_semantics: String,
    pub(crate) rpc_ref: String,
    pub(crate) explorer_ref: String,
    pub(crate) faucet_ref: Option<String>,
    pub(crate) required_gates: Vec<String>,
    pub(crate) allowed_claims: Vec<String>,
    pub(crate) denied_claims: Vec<String>,
}

pub(super) fn build_network_tier_status(
    loaded: &oasis7::network_tier_manifest::LoadedNetworkTierManifest,
) -> ChainNetworkTierStatus {
    ChainNetworkTierStatus {
        source_path: loaded.source_path.clone(),
        schema_version: loaded.manifest.schema_version.clone(),
        tier: loaded.manifest.tier.clone(),
        status: loaded.manifest.status.clone(),
        network_id: loaded.manifest.network_id.clone(),
        chain_id: loaded.manifest.chain_id.clone(),
        bootstrap_peer_count: loaded.bootstrap_peers.len(),
        governance_mode: loaded.manifest.validator_policy.governance_mode.clone(),
        validator_admission: loaded.manifest.validator_policy.validator_admission.clone(),
        target_validator_count: loaded.manifest.validator_policy.target_validator_count,
        allow_observer_nodes: loaded.manifest.validator_policy.allow_observer_nodes,
        token_symbol: loaded.manifest.token_policy.symbol.clone(),
        faucet_mode: loaded.manifest.token_policy.faucet_mode.clone(),
        reset_policy: loaded.manifest.token_policy.reset_policy.clone(),
        release_policy: loaded.manifest.release_policy.clone(),
        world_policy: loaded.manifest.world_policy.clone(),
        authority_policy: loaded.manifest.authority_policy.clone(),
        runtime_supported: loaded.validate_runtime_support().is_ok(),
        value_semantics: loaded.manifest.token_policy.value_semantics.clone(),
        rpc_ref: loaded.manifest.endpoint_policy.rpc_ref.clone(),
        explorer_ref: loaded.manifest.endpoint_policy.explorer_ref.clone(),
        faucet_ref: loaded.manifest.endpoint_policy.faucet_ref.clone(),
        required_gates: loaded.manifest.promotion_policy.required_gates.clone(),
        allowed_claims: loaded.manifest.claims_policy.allowed_claims.clone(),
        denied_claims: loaded.manifest.claims_policy.denied_claims.clone(),
    }
}
