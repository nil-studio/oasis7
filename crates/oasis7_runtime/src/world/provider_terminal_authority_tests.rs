use super::*;
use crate::{Action, LocalTestProviderAuthorityConfig, LocalTestProviderModuleArtifact};
use oasis7_agent_api::world_action::GeoPos;

fn prepared() -> (World, crate::ProviderBackedBootstrapAuthorityV1) {
    let mut world = World::new();
    world.submit_action(Action::RegisterAgent {
        agent_id: "starter-agent-0".into(),
        pos: GeoPos::new(0, 0, 0),
    });
    world.step().unwrap();
    let owner_key = hex::encode([51; 32]);
    let public_key = oasis7_client_api::world_service::sign_read_request("key", (), &owner_key)
        .unwrap()
        .subject_public_key;
    world.state.starter_oc_claims.insert(
        "starter-agent-0".into(),
        crate::gameplay_state::StarterOcClaimState {
            agent_id: "starter-agent-0".into(),
            player_id: "terminal-test-owner".into(),
            public_key: Some(public_key),
            amount: 1,
            claimed_at: world.state.time,
            source_treasury_bucket_id: None,
        },
    );
    let config = LocalTestProviderAuthorityConfig::hosted_public_join(
        "starter-agent-0",
        "terminal-test-owner",
        format!("blake3:{}", "1".repeat(64)),
    );
    let artifact = LocalTestProviderModuleArtifact {
        wasm_bytes: b"not-a-wasm-artifact".to_vec(),
        source_hash: crate::util::sha256_hex(b"terminal-test-source"),
        build_manifest_hash: crate::util::sha256_hex(b"terminal-test-build"),
    };
    world
        .install_local_test_provider_module(&config, &artifact)
        .unwrap();
    world
        .bind_cognition_runtime(
            "live-formal-release-default",
            "main",
            0,
            Some(config.finality_block_hash.clone()),
            "pending",
            0,
        )
        .unwrap();
    let authority = world
        .initialize_local_test_provider_authority(config, artifact)
        .unwrap()
        .bootstrap_authority;
    (world, authority)
}

fn expired() -> (World, crate::ProviderBackedBootstrapAuthorityV1) {
    let (mut world, authority) = prepared();
    while world.state().time <= authority.grant.expires_at_tick.unwrap() {
        world.step().unwrap();
    }
    (world, authority)
}

#[test]
fn expired_bootstrap_replay_retains_grant_allowance_and_exact_original_receipt() {
    let (mut world, original) = expired();
    let before = serde_json::to_value(world.snapshot()).unwrap();
    let original_receipt = world.cognition_economy().unwrap().provisions[&original.provision_id]
        .receipt_id
        .clone();
    let replay = world
        .bootstrap_provider_backed_authority(original.clone())
        .unwrap();
    assert_eq!(replay.receipt_id, original_receipt);
    assert_eq!(serde_json::to_value(world.snapshot()).unwrap(), before);
    assert_eq!(
        world
            .local_provider_capability_availability(&original.agent_id)
            .unwrap(),
        ProviderCapabilityAvailability::ExpiredOnly {
            grant_ids: vec![original.grant.grant_id.clone()]
        }
    );
    assert!(
        world
            .canonical_agent_service_context(&original.agent_id)
            .is_err()
    );
    assert!(
        world
            .capability_context_for_agent(
                &original.agent_id,
                original.invocation_context.presenter.clone(),
                "fresh-after-expiry"
            )
            .is_err()
    );
    assert!(
        world
            .validate_provider_grant_for_terminal(
                &original.agent_id,
                &original.grant.grant_id,
                original.grant.expires_at_tick.unwrap() + 1
            )
            .is_err()
    );
    world
        .validate_provider_grant_for_terminal(
            &original.agent_id,
            &original.grant.grant_id,
            original.grant.issued_at_tick,
        )
        .unwrap();
}

#[test]
fn expired_bootstrap_does_not_recreate_missing_or_changed_originals() {
    let (world, original) = expired();
    let mut missing = world.clone();
    missing.cognition["cognition_economy"]["provisions"]
        .as_object_mut()
        .unwrap()
        .remove(&original.provision_id);
    assert!(
        missing
            .bootstrap_provider_backed_authority(original.clone())
            .is_err()
    );
    let mut changed = original.clone();
    changed.allowance += 1;
    assert!(
        world
            .clone()
            .bootstrap_provider_backed_authority(changed)
            .is_err()
    );
    let mut changed = world.clone();
    changed
        .capability_grants_v2
        .get_mut(&original.grant.grant_id)
        .unwrap()["expires_at_tick"] = serde_json::json!(u64::MAX);
    changed.refresh_capability_authorization_root().unwrap();
    assert!(
        changed
            .local_provider_capability_availability(&original.agent_id)
            .is_err()
    );
    let mut changed = world.clone();
    changed.capability_invocation_contexts.clear();
    changed.refresh_capability_authorization_root().unwrap();
    assert!(
        changed
            .local_provider_capability_availability(&original.agent_id)
            .is_err()
    );
    let mut revoked = world.clone();
    revoked
        .capability_revocation_state
        .revoked_grant_ids
        .insert(original.grant.grant_id.clone());
    revoked.refresh_capability_authorization_root().unwrap();
    assert!(
        revoked
            .local_provider_capability_availability(&original.agent_id)
            .is_err()
    );
    let mut moved_owner = world.clone();
    moved_owner
        .state
        .starter_oc_claims
        .get_mut(&original.agent_id)
        .unwrap()
        .player_id = "different-owner".into();
    assert!(
        moved_owner
            .local_provider_capability_availability(&original.agent_id)
            .is_err()
    );
    let mut changed = world;
    changed.state.agents.remove(&original.agent_id);
    assert!(
        changed
            .bootstrap_provider_backed_authority(original)
            .is_err()
    );
}

fn captured_request(world: &World) -> oasis7_agent_api::ContinuousAgentRequestContextV1 {
    let context = world
        .canonical_agent_service_context("starter-agent-0")
        .unwrap();
    let digest = format!("blake3:{}", "11".repeat(32));
    let mut request: oasis7_agent_api::ContinuousAgentRequestContextV1 = serde_json::from_value(serde_json::json!({
        "base_decision_request":{
            "observation":{"agent_id":"starter-agent-0","world_time":world.state.time,
                "observation":oasis7_agent_api::ProviderObservation::default(),
                "action_catalog":[{"action_ref":"wait","summary":"Wait"}],"timeout_budget_ms":60000},
            "timeout_budget_ms":60000,"capability_catalog":context.capability_catalog,
            "capability_invocation_context":context.capability_invocation_context},
        "context_discriminator":"oasis7.continuous-agent-context","context_version":1,
        "protocol_version":"continuous-agent-v1","agent_session_id":"session-terminal",
        "agent_turn_id":"turn-terminal","decision_request_id":"request-terminal","retry_seq":1,"transport_attempt":1,
        "agent_subject":"starter-agent-0","runtime_binding":world.current_cognition_runtime_binding().unwrap(),
        "observation_digest":digest,"capability_catalog_digest":digest,"capability_invocation_context_digest":digest,
        "memory_snapshot_digest":digest,"goal_snapshot_digest":digest,"continuation_digest":digest,
        "adapter_protocol_version":"loopback-http-v1","budget_contract":{"max_latency_ms":60000,"max_repair_attempts":2,"max_model_calls":1,"max_tool_calls":0},
        "request_digest":digest
    })).unwrap();
    request.capability_catalog_digest = oasis7_agent_api::h_v1(
        "oasis7.cognition.capability-catalog.v1",
        request
            .base_decision_request
            .capability_catalog
            .as_ref()
            .unwrap(),
    );
    request.capability_invocation_context_digest = oasis7_agent_api::h_v1(
        "oasis7.cognition.capability-invocation-context.v1",
        request
            .base_decision_request
            .capability_invocation_context
            .as_ref()
            .unwrap(),
    );
    request.request_digest = request.request_digest();
    request.validate_production_lane().unwrap();
    request
}
#[test]
fn signed_fresh_local_reserve_and_prefix_after_expiry_are_rejected_without_mutation() {
    use crate::service_admission::wire::{SchedulerIntentV1, SchedulerOperationV1};
    let (mut world, authority) = prepared();
    let request = captured_request(&world);
    while world.state.time <= authority.grant.expires_at_tick.unwrap() {
        world.step().unwrap();
    }
    let key = hex::encode([51; 32]);
    let quote = crate::CognitionLeaseQuoteV1::new("fresh-expired", "cognition_units", 1)
        .with_authority(
            "terminal-test-owner",
            "cognition_units.v1",
            "provider_cognition",
            "agent_turn",
            "fixed_unit_experimental",
            "local-test-provider-authority",
            world
                .current_cognition_runtime_binding()
                .unwrap()
                .base_world_hash
                .to_string(),
        );
    let lease = crate::CognitionLeaseRequestV1::new(
        request.provider_invocation_key().to_string(),
        "terminal-test-owner",
        "starter-agent-0",
        request.agent_session_id.clone(),
        request.agent_turn_id.clone(),
        request.decision_request_id.clone(),
        request.request_digest.to_string(),
        quote,
    );
    let before = serde_json::to_value(world.snapshot()).unwrap();
    let mut logical = request.clone();
    logical.transport_attempt = 1;
    let operations = [
        SchedulerOperationV1::ReserveLease(lease),
        SchedulerOperationV1::ProviderPrefix {
            request,
            context_digest: oasis7_agent_api::h_v1("oasis7.cognition.context.v1", &logical)
                .to_string(),
        },
    ];
    for (index, operation) in operations.into_iter().enumerate() {
        let signed = oasis7_client_api::world_service::sign_read_request(
            "scheduler",
            SchedulerIntentV1 {
                agent_id: "starter-agent-0".into(),
                request_id: format!("fresh-expired-{index}"),
                delegation_generation: 0,
                task_prefix_binding_revision: 0,
                captured_base_binding: world.current_cognition_base_binding().unwrap(),
                operation,
            },
            &key,
        )
        .unwrap();
        let error = world.apply_authenticated_scheduler(signed).unwrap_err();
        assert!(
            error.contains("expired local provider grant"),
            "must reach the grant-time guard: {error}"
        );
        assert_eq!(serde_json::to_value(world.snapshot()).unwrap(), before);
    }
}

#[test]
fn historical_expired_grant_does_not_block_a_separately_admitted_live_renewal() {
    let (mut world, original) = expired();
    let config = LocalTestProviderAuthorityConfig::hosted_public_join(
        "starter-agent-0",
        "terminal-test-owner",
        format!("blake3:{}", "1".repeat(64)),
    );
    let artifact = LocalTestProviderModuleArtifact {
        wasm_bytes: b"not-a-wasm-artifact".to_vec(),
        source_hash: crate::util::sha256_hex(b"terminal-test-source"),
        build_manifest_hash: crate::util::sha256_hex(b"terminal-test-build"),
    };
    let renewed = world
        .initialize_local_test_provider_authority(config, artifact)
        .unwrap();
    assert_ne!(
        renewed.bootstrap_authority.grant.grant_id,
        original.grant.grant_id
    );
    assert_eq!(
        world
            .local_provider_capability_availability(&original.agent_id)
            .unwrap(),
        ProviderCapabilityAvailability::Live
    );
    assert_eq!(world.cognition_economy().unwrap().provisions.len(), 1);
    while world.state.time <= renewed.bootstrap_authority.grant.expires_at_tick.unwrap() {
        world.step().unwrap();
    }
    let ProviderCapabilityAvailability::ExpiredOnly { grant_ids } = world
        .local_provider_capability_availability(&original.agent_id)
        .unwrap()
    else {
        panic!("both grants must be expired");
    };
    assert_eq!(grant_ids.len(), 2);
    assert!(grant_ids.contains(&original.grant.grant_id));
    assert!(grant_ids.contains(&renewed.bootstrap_authority.grant.grant_id));
    world
        .validate_provider_grant_for_terminal(
            &original.agent_id,
            &original.grant.grant_id,
            original.grant.issued_at_tick,
        )
        .unwrap();
    world
        .validate_provider_grant_for_terminal(
            &original.agent_id,
            &renewed.bootstrap_authority.grant.grant_id,
            renewed.bootstrap_authority.grant.issued_at_tick,
        )
        .unwrap();
    let mut revoked = world.clone();
    revoked
        .capability_revocation_state
        .revoked_grant_ids
        .insert(original.grant.grant_id.clone());
    revoked.refresh_capability_authorization_root().unwrap();
    assert!(
        revoked
            .local_provider_capability_availability(&original.agent_id)
            .is_err()
    );
    let mut wrong_context = world.clone();
    let context = wrong_context
        .capability_invocation_contexts
        .values_mut()
        .find(|context| context.grant_id == original.grant.grant_id)
        .unwrap();
    context.grant_id = renewed.bootstrap_authority.grant.grant_id.clone();
    wrong_context
        .refresh_capability_authorization_root()
        .unwrap();
    assert!(
        wrong_context
            .local_provider_capability_availability(&original.agent_id)
            .is_err()
    );
    assert!(
        world
            .validate_provider_grant_for_terminal(
                &original.agent_id,
                "missing-original",
                original.grant.issued_at_tick
            )
            .is_err()
    );
}
