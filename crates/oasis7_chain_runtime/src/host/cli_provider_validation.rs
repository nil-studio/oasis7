//! Shared local-provider admission; both flat compatibility and typed dev configuration use it.
use super::*;
pub(super) fn validate_local_test_provider_options(options: &CliOptions) -> Result<(), String> {
    let any_setup_option = options.local_test_provider_authority_path.is_some()
        || options.local_test_provider_wasm_path.is_some()
        || options.local_test_provider_metadata_path.is_some()
        || options.local_test_provider_finality_block_hash.is_some()
        || options.local_test_provider_agent_id != DEFAULT_LOCAL_TEST_PROVIDER_AGENT_ID
        || options.local_test_provider_owner_binding != DEFAULT_LOCAL_TEST_PROVIDER_OWNER_BINDING
        || options.local_test_provider_session_mode != DEFAULT_LOCAL_TEST_PROVIDER_SESSION_MODE;
    if !any_setup_option {
        if provider_tuple_requested(options) {
            return Err(
                "provider tuple admission requires local test provider authority setup".to_string(),
            );
        }
        return Ok(());
    }
    if options.storage_profile != StorageProfile::DevLocal {
        return Err("local test authority setup requires --storage-profile dev_local".to_string());
    }
    if options.network_tier_manifest_path.is_some() {
        return Err("local test authority setup cannot use --network-tier-manifest".to_string());
    }
    let required = [
        (
            "--local-test-provider-authority",
            options.local_test_provider_authority_path.is_some(),
        ),
        (
            "--local-test-provider-wasm",
            options.local_test_provider_wasm_path.is_some(),
        ),
        (
            "--local-test-provider-metadata",
            options.local_test_provider_metadata_path.is_some(),
        ),
        (
            "--local-test-provider-finality-block-hash",
            options.local_test_provider_finality_block_hash.is_some(),
        ),
    ];
    for (flag, present) in required {
        if !present {
            return Err(format!(
                "{flag} is required when local test authority setup is enabled"
            ));
        }
    }
    if options.local_test_provider_agent_id.trim().is_empty() {
        return Err("--local-test-provider-agent-id requires a non-empty value".to_string());
    }
    if options.local_test_provider_owner_binding.trim().is_empty() {
        return Err("--local-test-provider-owner-binding requires a non-empty value".to_string());
    }
    if !matches!(
        options.local_test_provider_session_mode.as_str(),
        "hosted_public_join" | "loopback"
    ) {
        return Err(
            "--local-test-provider-session-mode must be hosted_public_join or loopback".to_string(),
        );
    }
    let finality = options
        .local_test_provider_finality_block_hash
        .as_deref()
        .unwrap_or_default();
    if !valid_blake3_digest(finality) {
        return Err(
            "--local-test-provider-finality-block-hash must be blake3:<64 lowercase hex characters>"
                .to_string(),
        );
    }
    if provider_tuple_requested(options) {
        if !hosted_local_mock_provider_tuple(options) {
            return Err(
                "Hosted local-mock funding requires the exact provider_backed/provider_local_mock/worldsim_provider_v1/loopback_http/player_parity tuple"
                    .to_string(),
            );
        }
        if !cfg!(feature = "test_tier_required") {
            return Err(
                "Hosted local-mock funding requires a binary built with feature test_tier_required"
                    .to_string(),
            );
        }
        if !options.chain_local_standalone_test {
            return Err(
                "Hosted local-mock funding requires --chain-local-standalone-test".to_string(),
            );
        }
    }
    Ok(())
}

fn provider_tuple_requested(options: &CliOptions) -> bool {
    options.agent_decision_source.is_some()
        || options.agent_provider_backend.is_some()
        || options.agent_provider_contract.is_some()
        || options.agent_provider_transport.is_some()
        || options.agent_execution_lane.is_some()
}

fn hosted_local_mock_provider_tuple(options: &CliOptions) -> bool {
    options.agent_decision_source.as_deref() == Some("provider_backed")
        && options.agent_provider_backend.as_deref() == Some("provider_local_mock")
        && options.agent_provider_contract.as_deref() == Some("worldsim_provider_v1")
        && options.agent_provider_transport.as_deref() == Some("loopback_http")
        && options.agent_execution_lane.as_deref() == Some("player_parity")
        && options.local_test_provider_session_mode == "hosted_public_join"
}
