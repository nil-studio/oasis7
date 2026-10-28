//! Authenticated no-task cognition reaches the ordinary legacy business path.
use crate::service_admission::{CognitionIntentV1, SignedReadRequest, sign_read_request};
use crate::*;
use oasis7_agent_api::world_action::GeoPos;

const PRIVATE: &str = "0707070707070707070707070707070707070707070707070707070707070707";

fn fixture() -> World {
    fixture_with_formal_history(false)
}

fn fixture_with_formal_history(formal: bool) -> World {
    let public = sign_read_request("fixture", (), PRIVATE)
        .unwrap()
        .subject_public_key;
    let other_public = sign_read_request("fixture", (), &hex::encode([8; 32]))
        .unwrap()
        .subject_public_key;
    let mut world = World::new();
    if formal {
        world.initialize_empty_formal_history().unwrap();
    }
    for (agent, owner, key) in [
        ("agent-a", "owner-a", public.clone()),
        ("agent-b", "owner-b", other_public),
    ] {
        world.submit_action(Action::RegisterAgent {
            agent_id: agent.into(),
            pos: GeoPos::new(0, 0, 0),
        });
        world.step().unwrap();
        if agent == "agent-b" {
            let funding = world.state().starter_oc_claims["agent-a"].amount;
            let mut supply = world.main_token_supply().clone();
            supply.total_supply += funding;
            supply.total_issued += funding;
            world.set_main_token_supply(supply);
            world
                .set_main_token_treasury_balance(MAIN_TOKEN_TREASURY_BUCKET_ECOSYSTEM_POOL, funding)
                .unwrap();
        }
        world.submit_action(Action::ClaimStarterOc {
            agent_id: agent.into(),
            player_id: owner.into(),
            public_key: Some(key),
        });
        world.step().unwrap();
        assert_eq!(world.current_agent_owner(agent).unwrap().player_id, owner);
    }
    let a = world.current_agent_owner("agent-a").unwrap();
    let b = world.current_agent_owner("agent-b").unwrap();
    assert_ne!(a.player_id, b.player_id);
    assert_ne!(a.public_key, b.public_key);
    assert_ne!(a.source_id, b.source_id);
    world
        .install_capability_agent_identity("agent-a", "owner-a", 1)
        .unwrap();
    world
        .set_location_anchor(LocationAnchorV1 {
            location_id: "location".into(),
            active: true,
            authority_revision: 1,
            effective_at: 0,
        })
        .unwrap();
    world
        .register_factory_site_authority(FactorySiteAuthorityV1 {
            site_id: "owned-site".into(),
            location_id: "location".into(),
            owner_agent_id: "agent-a".into(),
            authorized_agent_ids: vec![],
            chunk_ready: true,
            active: true,
            authority_revision: 1,
            registered_at: world.state().time,
        })
        .unwrap();
    for ledger in [
        MaterialLedgerId::agent("agent-a"),
        MaterialLedgerId::agent("agent-b"),
        MaterialLedgerId::site("owned-site"),
    ] {
        world
            .set_ledger_material_balance(ledger, "iron_ore", 10)
            .unwrap();
    }
    super::economy_factory_lifecycle::install_factory_authority(
        &mut world,
        "agent-a",
        "factory-site",
        "owned-factory",
        0,
    );
    let mut spec = super::economy_factory_lifecycle::factory_spec("owned-factory", 1, 1, 0);
    spec.build_cost.clear();
    world.submit_action(Action::BuildFactory {
        builder_agent_id: "agent-a".into(),
        site_id: "factory-site".into(),
        spec,
    });
    world.step().unwrap();
    world.step().unwrap();
    assert!(world.state().factories.contains_key("owned-factory"));
    world
        .set_ledger_material_balance(MaterialLedgerId::factory("owned-factory"), "iron_ore", 10)
        .unwrap();
    world
        .bind_cognition_runtime(
            "w1",
            "main",
            0,
            Some(format!("blake3:{}", "42".repeat(32))),
            "pending",
            0,
        )
        .unwrap();
    world
        .provision_cognition_for_agent("agent-a", "provision", "owner-a", 100)
        .unwrap();
    world
        .install_test_provider_capability_fixture_without_cognition_balance("agent-a")
        .unwrap();
    assert!(world.promote_cognition_runtime_finality().unwrap());
    world
}

fn transfer(source: MaterialLedgerId) -> Action {
    Action::TransferMaterial {
        requester_agent_id: "agent-a".into(),
        from_ledger: source,
        to_ledger: MaterialLedgerId::agent("agent-b"),
        kind: "iron_ore".into(),
        amount: 3,
        distance_km: 0,
        priority: None,
        route_id: None,
        route_ids: vec![],
        auto_reroute: false,
    }
}

fn signed(world: &World, action: Action) -> SignedReadRequest<CognitionIntentV1> {
    let digest = |value| format!("blake3:{}", hex::encode([value; 32]));
    let root = world.capability_authorization_root().to_string();
    let request = RuntimeCognitionCommitRequestV1 {
        agent_id: "agent-a".into(),
        agent_session_id: "source-session".into(),
        agent_turn_id: "source-turn".into(),
        decision_request_id: "source-decision".into(),
        retry_seq: 1,
        transport_attempt: 1,
        request_digest: digest(1),
        observation_digest: digest(2),
        context_digest: digest(3),
        capability_snapshot_hash: oasis7_agent_api::h_v1("oasis7.runtime.manifest.v1", &root)
            .to_string(),
        authority_context_hash: oasis7_agent_api::h_v1(
            "oasis7.runtime.authority-context.v1",
            &root,
        )
        .to_string(),
        captured_base_binding: world.current_cognition_base_binding().unwrap(),
    };
    let mut artifact = RuntimeCognitionResponseArtifactV1 {
        schema_version: 1,
        context_discriminator: RuntimeCognitionResponseArtifactV1::CONTEXT_DISCRIMINATOR.into(),
        context_version: 1,
        agent_session_id: request.agent_session_id.clone(),
        agent_turn_id: request.agent_turn_id.clone(),
        decision_request_id: request.decision_request_id.clone(),
        retry_seq: 1,
        transport_attempt: 1,
        request_digest: request.request_digest.clone(),
        response_digest: digest(4),
        artifact_digest: String::new(),
    };
    artifact.refresh_artifact_digest();
    sign_read_request(
        "cognition",
        CognitionIntentV1 {
            request,
            action,
            response_artifact: artifact,
            delegation_generation: 0,
            causal_proposal: None,
            task_fence: None,
        },
        PRIVATE,
    )
    .unwrap()
}

#[test]
fn authenticated_no_task_cognition_rejects_another_agents_material_source() {
    let mut world = fixture();
    let mut action = transfer(MaterialLedgerId::agent("agent-b"));
    if let Action::TransferMaterial { to_ledger, .. } = &mut action {
        *to_ledger = MaterialLedgerId::agent("agent-a");
    }
    let intent = signed(&world, action);
    crate::service_admission::agent_authority::validate_cognition(&world, &intent).unwrap();
    let before = serde_json::to_value(world.state()).unwrap();
    let receipts = world.cognition()["receipt_registry"].clone();
    let budget = world.cognition()["agent_delegation"]["spent"].clone();
    let outcome = world.commit_authenticated_cognition(intent);
    if outcome.is_ok() {
        // RED must prove the original business debit/credit, not merely an
        // accepted cognition whose action may have been rejected later.
        assert_eq!(
            world.ledger_material_balance(&MaterialLedgerId::agent("agent-b"), "iron_ore"),
            7
        );
        assert_eq!(
            world.ledger_material_balance(&MaterialLedgerId::agent("agent-a"), "iron_ore"),
            13
        );
        assert!(
            world
                .agent_causal_receipts("agent-a")
                .unwrap()
                .iter()
                .any(|r| r
                    .actual_consequence
                    .to_string()
                    .contains("MaterialTransferred"))
        );
    }
    let error = outcome.expect_err("another Agent source must be rejected");
    assert!(error.contains("asset_source_unsupported"), "{error}");
    assert_eq!(serde_json::to_value(world.state()).unwrap(), before);
    assert_eq!(world.cognition()["receipt_registry"], receipts);
    assert_eq!(world.cognition()["agent_delegation"]["spent"], budget);
    assert!(world.agent_causal_receipts("agent-a").unwrap().is_empty());
}

#[test]
fn authenticated_no_task_cognition_own_agent_and_site_transfer_once() {
    for (source, inactive_site) in [
        (MaterialLedgerId::agent("agent-a"), false),
        (MaterialLedgerId::site("owned-site"), false),
        (MaterialLedgerId::factory("owned-factory"), false),
        (MaterialLedgerId::site("owned-site"), true),
    ] {
        let mut world = fixture();
        if inactive_site {
            let mut authority = world.state().factory_site_authorities["owned-site"].clone();
            authority.active = false;
            authority.chunk_ready = false;
            authority.authority_revision += 1;
            world.register_factory_site_authority(authority).unwrap();
        }
        let intent = signed(&world, transfer(source.clone()));
        let receipt = world
            .commit_authenticated_cognition(intent.clone())
            .unwrap();
        assert_eq!(world.ledger_material_balance(&source, "iron_ore"), 7);
        assert_eq!(
            world.ledger_material_balance(&MaterialLedgerId::agent("agent-b"), "iron_ore"),
            13
        );
        let after = serde_json::to_value(world.snapshot()).unwrap();
        assert_eq!(
            world
                .commit_authenticated_cognition(intent.clone())
                .unwrap(),
            receipt
        );
        assert_eq!(serde_json::to_value(world.snapshot()).unwrap(), after);
        let mut restored = World::from_snapshot(world.snapshot(), world.journal().clone()).unwrap();
        assert_eq!(
            restored.commit_authenticated_cognition(intent).unwrap(),
            receipt
        );
        assert_eq!(restored.ledger_material_balance(&source, "iron_ore"), 7);
        assert_eq!(
            restored.ledger_material_balance(&MaterialLedgerId::agent("agent-b"), "iron_ore"),
            13
        );
    }
}

#[test]
fn authenticated_cognition_material_source_keeps_native_grant_restrictions() {
    for source in [
        MaterialLedgerId::agent("agent-a"),
        MaterialLedgerId::agent("agent-b"),
        MaterialLedgerId::site("owned-site"),
    ] {
        let mut world = fixture();
        let mut action = transfer(source.clone());
        let destination = if source == MaterialLedgerId::agent("agent-b") {
            "agent-a"
        } else {
            "agent-b"
        };
        if let Action::TransferMaterial { to_ledger, .. } = &mut action {
            *to_ledger = MaterialLedgerId::agent(destination);
        }
        let owner = world.current_agent_owner("agent-a").unwrap();
        world
            .install_test_native_delegation_grant(AgentDelegationGrantV1 {
                grant_id: "material-grant".into(),
                source_id: owner.source_id,
                issuer_id: "owner-a".into(),
                owner_id: "owner-a".into(),
                organization_id: None,
                agent_id: "agent-a".into(),
                object_id: format!("{source}->agent:{destination}:iron_ore"),
                action_kinds: vec!["TransferMaterial".into()],
                revision: 1,
                period_id: "period".into(),
                valid_from_tick: 0,
                valid_until_tick: 1000,
                limit_units: 10,
                resource_kind: "electricity".into(),
                revoked: false,
            })
            .unwrap();
        let intent = signed(&world, action);
        let before = serde_json::to_value(world.state()).unwrap();
        let budget = world.agent_delegation_authorizations("agent-a").unwrap();
        let outcome = world.commit_authenticated_cognition(intent);
        if source == MaterialLedgerId::agent("agent-a") {
            outcome.unwrap();
            assert_eq!(world.ledger_material_balance(&source, "iron_ore"), 7);
        } else {
            let error = outcome.unwrap_err();
            assert!(error.contains("asset_source_"), "{error}");
            assert_eq!(serde_json::to_value(world.state()).unwrap(), before);
            assert_eq!(
                world.agent_delegation_authorizations("agent-a").unwrap(),
                budget
            );
            assert!(world.agent_causal_receipts("agent-a").unwrap().is_empty());
        }
    }
}

#[test]
fn authenticated_cognition_rejects_unowned_site_access_only_and_unknown_sources() {
    for source in [
        MaterialLedgerId::site("access-only"),
        MaterialLedgerId::site("missing"),
        MaterialLedgerId::factory("missing"),
        MaterialLedgerId::world(),
    ] {
        let mut world = fixture();
        world
            .register_factory_site_authority(FactorySiteAuthorityV1 {
                site_id: "access-only".into(),
                location_id: "location".into(),
                owner_agent_id: "agent-b".into(),
                authorized_agent_ids: vec!["agent-a".into()],
                chunk_ready: true,
                active: true,
                authority_revision: 1,
                registered_at: world.state().time,
            })
            .unwrap();
        world
            .set_ledger_material_balance(source.clone(), "iron_ore", 10)
            .unwrap();
        let intent = signed(&world, transfer(source));
        let before = serde_json::to_value(world.state()).unwrap();
        let error = world.commit_authenticated_cognition(intent).unwrap_err();
        assert!(error.contains("asset_source_unsupported"), "{error}");
        assert_eq!(serde_json::to_value(world.state()).unwrap(), before);
        assert!(world.agent_causal_receipts("agent-a").unwrap().is_empty());
    }
}

#[test]
fn authenticated_cognition_material_source_formal_sequence_guard_is_separate() {
    struct EmptyNodes;
    impl oasis7_formal_protocol::controlled_authority::formal::business_index::BusinessIndexNodeReader for EmptyNodes {
        fn read_node(&self, _: &str) -> Result<Option<Vec<u8>>, String> { Ok(None) }
    }
    let mut world = fixture_with_formal_history(true);
    let bytes = serde_cbor::to_vec(&world.snapshot()).unwrap();
    // Test-only qualified parent exercises the sequencing guard, not production
    // finality or remote profile reachability.
    let parent = VerifiedFormalRetainedHistoryParentV1::for_test_snapshot_at_tick(
        world.state().retained_history_root.clone().unwrap(),
        &bytes,
        world.state().time,
    );
    world
        .attach_verified_formal_history(parent, std::sync::Arc::new(EmptyNodes))
        .unwrap();
    let context = RuntimeCommittedTickContext {
        height: world.state().time + 1,
        slot: world.state().time,
        epoch: 0,
        node_block_hash: "a".repeat(64),
        action_root: "fixture".into(),
        authority_node_id: world.tick_consensus_authority_source().into(),
        committed_at_unix_ms: 0,
    };
    world.begin_formal_execution_tick(&context).unwrap();
    // Use an owned source: an ownership rejection must not mask this guard.
    let intent = signed(&world, transfer(MaterialLedgerId::agent("agent-a")));
    let before = serde_json::to_value(world.state()).unwrap();
    let error = world.commit_authenticated_cognition(intent).unwrap_err();
    assert!(
        error.contains("formal successor requires its exact committed context step"),
        "{error}"
    );
    assert_eq!(serde_json::to_value(world.state()).unwrap(), before);
    assert!(world.agent_causal_receipts("agent-a").unwrap().is_empty());
}

#[test]
fn cognition_material_source_revalidates_current_owner_for_bound_decision() {
    let mut world = fixture();
    let action = transfer(MaterialLedgerId::site("owned-site"));
    let decision = AgentDelegationDecisionV1 {
        decision_request_id: "bound-site".into(),
        agent_id: "agent-a".into(),
        action_digest: crate::cognition_recovery::cognition_digest_v1(
            "oasis7.cognition.action.v1",
            &serde_json::to_value(&action).unwrap(),
        ),
        grant_id: None,
        grant_revision: None,
        quote_id: None,
        quoted_units: 0,
        quote_valid_until_tick: world.state().time + 1,
        context: Default::default(),
    };
    world
        .bind_agent_delegation_decision(decision.clone(), &action)
        .unwrap();
    let mut authority = world.state().factory_site_authorities["owned-site"].clone();
    authority.owner_agent_id = "agent-b".into();
    authority.authorized_agent_ids = vec!["agent-a".into()];
    authority.authority_revision += 1;
    world.register_factory_site_authority(authority).unwrap();
    let before = serde_json::to_value(world.state()).unwrap();
    let error = world
        .bind_agent_delegation_decision(decision, &action)
        .unwrap_err();
    assert!(format!("{error:?}").contains("asset_source_unsupported"));
    assert_eq!(serde_json::to_value(world.state()).unwrap(), before);
}

#[test]
fn cognition_material_source_denial_keeps_existing_intent_audit_without_business_effect() {
    let mut world = fixture();
    world
        .record_agent_chat_intent("owner-a", "agent-a", 1, "deliver owned materials")
        .unwrap();
    let mut action = transfer(MaterialLedgerId::agent("agent-b"));
    if let Action::TransferMaterial { to_ledger, .. } = &mut action {
        *to_ledger = MaterialLedgerId::agent("agent-a");
    }
    let intent = signed(&world, action);
    let before = world.state().material_ledgers.clone();
    let transits = world.state().pending_material_transits.clone();
    let receipts = world.cognition()["receipt_registry"].clone();
    let error = world.commit_authenticated_cognition(intent).unwrap_err();
    assert!(error.contains("asset_source_unsupported"), "{error}");
    assert_eq!(world.state().material_ledgers, before);
    assert_eq!(world.state().pending_material_transits, transits);
    assert_eq!(world.cognition()["receipt_registry"], receipts);
    assert!(world.agent_causal_receipts("agent-a").unwrap().is_empty());
    let intent = world
        .state()
        .agent_intent_ledger
        .values()
        .find(|i| i.agent_id == "agent-a")
        .unwrap();
    assert_eq!(intent.status, "rejected");
    assert_eq!(
        intent.reason_code.as_deref(),
        Some("agency_asset_source_unsupported_replan_required")
    );
}
