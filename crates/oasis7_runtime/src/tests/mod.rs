//! Tests for the runtime module.
#![cfg_attr(not(feature = "test_tier_full"), allow(dead_code, unused_imports))]

use ed25519_dalek::{Signer, SigningKey};

use crate::{ModuleArtifactIdentity, util};

pub(super) fn pos(x: i64, y: i64) -> oasis7_agent_api::world_action::GeoPos {
    oasis7_agent_api::world_action::GeoPos {
        x_cm: x,
        y_cm: y,
        z_cm: 0,
    }
}

const TEST_MODULE_ARTIFACT_SIGNER_NODE_ID: &str = "test.module.release.signer";

pub(super) fn signed_test_artifact_identity(wasm_hash: &str) -> ModuleArtifactIdentity {
    let source_hash = util::sha256_hex(format!("test-src:{wasm_hash}").as_bytes());
    let build_manifest_hash = util::sha256_hex(b"test-build-manifest-v1");
    let payload = ModuleArtifactIdentity::signing_payload_v1(
        wasm_hash,
        source_hash.as_str(),
        build_manifest_hash.as_str(),
        TEST_MODULE_ARTIFACT_SIGNER_NODE_ID,
    );
    let signing_key = test_module_artifact_signing_key();
    let signature = signing_key.sign(payload.as_slice());
    ModuleArtifactIdentity {
        source_hash,
        build_manifest_hash,
        signer_node_id: TEST_MODULE_ARTIFACT_SIGNER_NODE_ID.to_string(),
        signature_scheme: ModuleArtifactIdentity::SIGNATURE_SCHEME_ED25519.to_string(),
        artifact_signature: format!(
            "{}{}",
            ModuleArtifactIdentity::SIGNATURE_PREFIX_ED25519_V1,
            hex::encode(signature.to_bytes())
        ),
    }
}

fn test_module_artifact_signing_key() -> SigningKey {
    // Deterministic testing key; production verification only trusts its public key.
    let seed = util::sha256_hex(b"oasis7-test-module-artifact-signer-v1");
    let seed_bytes = hex::decode(seed).expect("decode test module signing seed");
    let private_key_bytes: [u8; 32] = seed_bytes
        .as_slice()
        .try_into()
        .expect("test module signing seed is 32 bytes");
    SigningKey::from_bytes(&private_key_bytes)
}

#[cfg(feature = "wasmtime")]
pub(super) fn test_wasm_executor_config() -> oasis7_wasm_executor::WasmExecutorConfig {
    let mut config = oasis7_wasm_executor::WasmExecutorConfig::default();
    if cfg!(feature = "test_tier_full") {
        // Full-tier CI runs many builtin wasm tests in parallel on fresh runners.
        // Keep production defaults strict, but avoid conflating cold compile time with
        // module execution regressions in these integration-style tests.
        config.max_call_ms = 60_000;
    }
    config
}

#[cfg(feature = "test_tier_full")]
mod agent_activity;
#[cfg(feature = "test_tier_full")]
mod agent_claim_epoch_preparation;
#[cfg(feature = "test_tier_full")]
mod agent_claims;
#[cfg(feature = "test_tier_full")]
mod agent_claims_auto_funding;
#[cfg(feature = "test_tier_full")]
mod agent_cognition_commit_interleavings;
#[cfg(feature = "test_tier_full")]
mod agent_cognition_live_command;
#[cfg(feature = "test_tier_full")]
mod agent_cognition_live_mvcc;
#[cfg(feature = "test_tier_full")]
mod agent_cognition_live_persistence;
#[cfg(feature = "test_tier_full")]
mod agent_cognition_live_scheduler;
#[cfg(feature = "test_tier_full")]
mod agent_cognition_mvcc;
#[cfg(feature = "test_tier_full")]
mod agent_cognition_recovery;
#[cfg(feature = "test_tier_full")]
mod agent_cognition_retention_gc;
#[cfg(feature = "test_tier_full")]
mod agent_cognition_retention_s14;
#[cfg(feature = "test_tier_full")]
mod agent_cognition_runtime_feedback;
mod agent_cognition_runtime_hardening;
#[cfg(feature = "test_tier_full")]
mod agent_cognition_runtime_hardening_followup;
#[cfg(feature = "test_tier_full")]
mod agent_cognition_runtime_hardening_legacy_wake;
#[cfg(feature = "test_tier_full")]
mod agent_cognition_runtime_p1;
#[cfg(feature = "test_tier_full")]
mod agent_cognition_scheduler_backpressure;
#[cfg(feature = "test_tier_full")]
mod agent_cognition_scheduler_fairness;
#[cfg(feature = "test_tier_full")]
mod agent_cognition_wake_conditions;
#[cfg(feature = "test_tier_full")]
mod agent_default_modules;
#[cfg(feature = "test_tier_full")]
mod agent_intent_v2;
#[cfg(feature = "test_tier_full")]
mod alliance_war_publication_transaction_regressions;
mod apply_domain_event_guards;
mod audit;
mod basic;
mod body;
#[cfg(feature = "test_tier_full")]
mod builtin_wasm_identity;
#[cfg(feature = "test_tier_full")]
mod builtin_wasm_materializer;
mod capability_authorization_raw_publication_transaction_regressions;
#[cfg(feature = "test_tier_full")]
mod capability_authorization_transaction_regressions;
mod capability_command_raw_publication_transaction_regressions;
#[cfg(feature = "test_tier_full")]
mod capability_effect_receipt_raw_publication_transaction_regressions;
mod capability_grant_v2;
mod capability_grant_v2_catalog;
mod capability_grant_v2_fixture;
#[cfg(feature = "test_tier_full")]
mod capability_grant_v2_lifecycle;
#[cfg(feature = "test_tier_full")]
mod capability_grant_v2_recovery_regressions;
#[cfg(feature = "test_tier_full")]
mod capability_grant_v2_regressions;
#[cfg(feature = "test_tier_full")]
mod capability_grant_v2_selector_authz_regressions;
#[cfg(feature = "test_tier_full")]
mod cognition_economy;
#[cfg(feature = "test_tier_full")]
mod cognition_economy_binding_regressions;
#[cfg(feature = "test_tier_full")]
mod core_policy_publication_transaction_regressions;
#[cfg(feature = "test_tier_full")]
mod data_access_control;
#[cfg(feature = "test_tier_full")]
mod direct_module_output_transaction;
#[cfg(feature = "test_tier_full")]
mod domain_event_publication_classification_gate;
#[cfg(feature = "test_tier_full")]
mod economic_contract_publication_transaction_regressions;
mod economy;
#[cfg(feature = "test_tier_full")]
mod economy_bootstrap;
#[cfg(feature = "test_tier_full")]
mod economy_factory_authority;
mod economy_factory_lifecycle;
#[cfg(feature = "test_tier_full")]
mod economy_module_call_uses_outer_transaction;
#[cfg(feature = "test_tier_full")]
mod economy_module_compat;
#[cfg(feature = "test_tier_full")]
mod economy_module_requests;
#[cfg(feature = "test_tier_full")]
mod economy_priority_logistics;
mod effects;
mod execution_transaction_kernel;
#[cfg(feature = "test_tier_full")]
mod execution_transaction_regressions;
mod gameplay;
#[cfg(feature = "test_tier_full")]
mod gameplay_bootstrap;
mod gameplay_protocol;
#[cfg(feature = "test_tier_full")]
mod governance;
#[cfg(feature = "test_tier_full")]
mod governance_approval_queue_transaction_regressions;
#[cfg(feature = "test_tier_full")]
mod governance_emergency_brake_transaction_regressions;
#[cfg(feature = "test_tier_full")]
mod governance_emergency_veto_transaction_regressions;
#[cfg(feature = "test_tier_full")]
mod governance_finality_snapshot_transaction_regressions;
#[cfg(feature = "test_tier_full")]
mod governance_identity_penalty_application_transaction_regressions;
#[cfg(feature = "test_tier_full")]
mod governance_identity_penalty_resolution_transaction_regressions;
#[cfg(feature = "test_tier_full")]
mod governance_identity_penalty_transaction_regressions;
#[cfg(feature = "test_tier_full")]
mod governance_meta_publication_transaction_regressions;
#[cfg(feature = "test_tier_full")]
mod governance_proposal_transaction_regressions;
#[cfg(feature = "test_tier_full")]
mod governance_validator_admission;
#[cfg(feature = "test_tier_full")]
mod governance_vote_quote;
#[cfg(feature = "test_tier_full")]
mod industry_history_publication_transaction_regressions;
#[cfg(feature = "test_tier_full")]
mod industry_material_normalization_regressions;
#[cfg(feature = "test_tier_full")]
mod industry_publication_transaction_regressions;
#[cfg(feature = "test_tier_full")]
mod logistics_transfer_quote;
#[cfg(feature = "test_tier_full")]
mod main_token;
#[cfg(feature = "test_tier_full")]
mod main_token_economy_audit;
#[cfg(feature = "test_tier_full")]
mod major_world_event_projection;
#[cfg(feature = "test_tier_full")]
mod market_quote_decision_preview;
#[cfg(feature = "test_tier_full")]
mod module_action_loop;
#[cfg(feature = "test_tier_full")]
mod module_command_provenance;
#[cfg(feature = "test_tier_full")]
mod module_event_routing_transaction_regressions;
#[cfg(feature = "test_tier_full")]
mod module_lifecycle_transaction_regressions;
#[cfg(feature = "test_tier_full")]
mod module_routing_transaction_regressions;
#[cfg(feature = "test_tier_full")]
mod module_runtime_metering;
#[cfg(feature = "test_tier_full")]
mod module_tick_routing_transaction_regressions;
#[cfg(feature = "test_tier_full")]
mod module_visual_wasm_recovery;
mod modules;
#[cfg(feature = "test_tier_full")]
mod operability_release_gate;
#[cfg(feature = "test_tier_full")]
mod persistence;
#[cfg(feature = "test_tier_full")]
mod power_bootstrap;
#[cfg(feature = "test_tier_full")]
mod power_bootstrap_release_manifest_full;
mod provider_capability_fixture;
#[cfg(feature = "test_tier_full")]
mod reward_asset;
#[cfg(feature = "test_tier_full")]
mod reward_asset_settlement_action;
mod rules;
mod state_serialization;
#[cfg(feature = "test_tier_full")]
mod storage_cold_index;
mod storage_footprint_fixture;
#[cfg(feature = "test_tier_full")]
mod world_state_projection;
// Pre-submit DeclareWar projection and non-mutation contract.
#[cfg(feature = "test_tier_full")]
mod agent_delegation;
#[cfg(feature = "test_tier_full")]
mod war_declaration_quote;

#[cfg(feature = "test_tier_full")]
mod participant_policy;
#[cfg(feature = "test_tier_full")]
mod world_state_extraction;

#[cfg(feature = "test_tier_full")]
mod cognition_material_source;

const _: () = {
    let _: Option<crate::AgentDelegationDecisionV1> = None;
    let _ = (
        crate::world::derive_agent_chat_request_digest,
        crate::m1_builtin_manifest_hash_tokens,
        crate::m4_builtin_manifest_hash_tokens,
        crate::m5_builtin_manifest_hash_tokens,
        crate::project_major_world_events,
        crate::major_world_event::project_major_world_events_with_canonical_state_indexed_for_test,
        crate::World::record_agent_chat_intent,
        crate::World::prepared_due_material_transits_for_test,
        crate::World::fail_next_append_after_reducer_for_test,
        crate::World::remove_module_release_mapping_for_test,
        crate::World::prepared_next_agent_claim_event_for_test,
        crate::World::append_legacy_claim_fixture,
        crate::World::append_capability_identity_batch_for_test,
        crate::World::prepare_factory_depreciation_event_bodies_for_test,
        crate::World::prepared_governance_finalization_events_for_test,
        crate::World::prepared_crisis_auto_spawn_event_for_test,
        crate::World::prepared_crisis_timeout_events_for_test,
        crate::World::prepared_next_due_war_event_for_test,
        crate::World::prepared_restricted_starter_claim_grant_expiry_accounts_for_test,
        crate::World::step_governance_fixture,
    );
};
