//! Actual backend observations exercise the independent client codec.
use super::*;
use crate::{authority, formal_projection::*};
use ed25519_dalek::SigningKey;
use oasis7_client_api::world_service::SignedServiceResponse;
use oasis7_client_api::world_service_wire::AgentSignerDelegationChangeV1;
use oasis7_game_client::read_projection::WorldReadProjectionV1;
fn secret(n: u8) -> String {
    hex::encode([n; 32])
}
fn public(n: u8) -> String {
    hex::encode(SigningKey::from_bytes(&[n; 32]).verifying_key().to_bytes())
}
fn populated_world() -> World {
    populated_world_with_binding("read-world")
}
fn populated_world_with_binding(world_id: &str) -> World {
    let mut seed = World::new();
    seed.submit_action(oasis7_runtime::Action::RegisterAgent {
        agent_id: "7".into(),
        pos: oasis7_agent_api::world_action::GeoPos {
            x_cm: 17,
            y_cm: 19,
            z_cm: 23,
        },
    });
    seed.step().unwrap();
    let mut state = serde_json::to_value(seed.state()).unwrap();
    state["starter_oc_claims"]["7"] = serde_json::json!({
        "agent_id":"7", "player_id":"owner", "public_key": public(7),
        "amount":1, "claimed_at":0, "source_treasury_bucket_id":null
    });
    state["pending_factory_builds"]["7"] = serde_json::json!({
        "job_id":7, "builder_agent_id":"7", "site_id":"007", "ready_at":31,
        "spec":{"factory_id":"factory", "display_name":"Factory", "tier":1,
        "tags":[], "build_cost":[], "build_time_ticks":3, "base_power_draw":1,
        "recipe_slots":1, "throughput_bps":10000, "maintenance_per_tick":0}
    });
    state["pending_material_transits"][u64::MAX.to_string()] = serde_json::json!({
        "job_id":u64::MAX, "requester_agent_id":"7", "from_ledger":"world", "to_ledger":"agent:7",
        "kind":"ore", "amount":3, "distance_km":2, "loss_bps":0, "ready_at":32
    });
    state["governance_votes"]["7"] = serde_json::json!({
        "proposal_key":"7", "votes_by_agent":{}, "tallies":{"007":1},
        "total_weight":1, "last_updated_at":1
    });
    let mut world = World::new_with_state(serde_json::from_value(state).unwrap());
    world
        .bind_cognition_runtime(world_id, "main", 0, None, "pending", 0)
        .unwrap();
    world
        .install_capability_agent_identity("7", "owner", 1)
        .unwrap();
    world.install_test_provider_capability_fixture("7").unwrap();
    let delegation = AgentSignerDelegationChangeV1 {
        world_id: world_id.into(),
        branch_id: "main".into(),
        agent_id: "7".into(),
        owner_binding: "owner".into(),
        agent_identity_generation: 1,
        generation: 1,
        delegate_public_key: public(8),
        revoked: false,
        nonce: 1,
    };
    world
        .apply_agent_signer_delegation(
            &authority::sign_read_request("delegation", delegation, &secret(7)).unwrap(),
        )
        .unwrap();
    world
}
fn assert_codec(projection: &WorldServiceProjection) -> WorldReadProjectionV1 {
    let raw = WorldReadProjectionV1::from_serializable(projection).unwrap();
    let json = serde_json::to_vec(projection).unwrap();
    let decoded: WorldReadProjectionV1 = serde_json::from_slice(&json).unwrap();
    assert_eq!(
        serde_json::to_value(projection).unwrap(),
        serde_json::to_value(&decoded).unwrap()
    );
    // Signatures use the canonical ABI codec, not struct declaration order in plain CBOR.
    let signing = authority::signing_bytes("view", projection).unwrap();
    assert_eq!(signing, authority::signing_bytes("view", &raw).unwrap());
    assert_eq!(signing, authority::signing_bytes("view", &decoded).unwrap());
    let signed =
        authority::sign_service_response("view", "request".into(), projection, &secret(9)).unwrap();
    let signed_json = serde_json::to_vec(&signed).unwrap();
    let signed_raw: SignedServiceResponse<WorldReadProjectionV1> =
        serde_json::from_slice(&signed_json).unwrap();
    authority::verify_service_response("view", "request", &signed_raw, &public(9)).unwrap();
    assert_eq!(decoded.observation().state.agents["7"].state.pos.x_cm, 17);
    assert_eq!(decoded.observation().state.time, projection.state.time);
    decoded
}
#[test]
fn typed_legacy_and_independent_read_codecs_preserve_public_owner_delegate_commitments() {
    let world = populated_world();
    for agent in [None, Some("7")] {
        let old = crate::projection::legacy_projection_from_world(&world, agent).unwrap();
        let current = crate::projection::from_world(&world, agent).unwrap();
        assert_eq!(
            serde_json::to_vec(&old).unwrap(),
            serde_json::to_vec(&current).unwrap()
        );
        assert_eq!(
            serde_cbor::to_vec(&old).unwrap(),
            serde_cbor::to_vec(&current).unwrap()
        );
        assert_codec(&current);
    }
    let commitments = commitments_for_world(&world).unwrap();
    let mut raw_leaves = Vec::new();
    for (agent, reader) in [
        (None, String::new()),
        (Some("7"), public(7)),
        (Some("7"), public(8)),
    ] {
        let audience = match agent {
            None => ProjectionAudienceV1::public(),
            Some(id) => ProjectionAudienceV1::agent(id, reader.clone()).unwrap(),
        };
        let projection =
            crate::projection::from_formal_committed_world(&world, agent, &reader).unwrap();
        let raw = assert_codec(&projection);
        let leaf = projection_leaf(&audience, &projection).unwrap();
        assert_eq!(leaf, projection_leaf(&audience, &raw).unwrap());
        commitments
            .inclusion(&audience)
            .unwrap()
            .verify(&audience, &raw, &commitments.root().unwrap())
            .unwrap();
        let leaf = projection_leaf(&audience, &raw).unwrap();
        raw_leaves.push((audience, leaf));
    }
    assert_eq!(
        ProjectionCommitmentsV1::from_leaves(raw_leaves)
            .unwrap()
            .root()
            .unwrap(),
        commitments.root().unwrap()
    );
}

#[test]
fn revoked_delegate_loses_formal_read_and_private_leaf_while_owner_and_public_remain() {
    use crate::formal_read::{FormalReadHistoryV1, FormalReadVerifier};
    use crate::formal_read_fixtures::{
        actual_world_initial_fixture_from_world, actual_world_successor_fixture,
    };
    use crate::formal_read_service::{FormalCommittedReadService, FormalReadPublication};
    use oasis7_client_api::world_service::{
        ReadWorldViewRequest, ReadWorldViewResponse, VIEW_PATH, WORLD_SERVICE_CONTRACT_VERSION,
        WorldIdentity,
    };
    use oasis7_formal_protocol::controlled_authority::formal::InitialActivationTrustV1;

    fn read_current(
        service: &FormalCommittedReadService,
        trust: &InitialActivationTrustV1,
        scope: &str,
        reader_secret: &str,
    ) -> Result<ReadWorldViewResponse<WorldServiceProjection>, String> {
        let publication = service.publication();
        let mut verifier = FormalReadVerifier::new(trust, publication.anchor().clone())?;
        let request = ReadWorldViewRequest {
            observation_nonce: Some("07".repeat(32)),
            contract_version: WORLD_SERVICE_CONTRACT_VERSION,
            world: WorldIdentity {
                world_id: trust.world.world_id.clone(),
                genesis_digest: trust.world.genesis_digest.clone(),
            },
            scope_id: scope.into(),
            min_commit: None,
            fixed_commit: None,
            deadline_unix_ms: None,
            formal_anchor: Some(verifier.anchor_ref()?),
        };
        let signed = authority::sign_read_request(VIEW_PATH, request.clone(), reader_secret)?;
        let response = service.read(
            &signed,
            publication,
            &FormalReadHistoryV1 {
                schema_version: 1,
                steps: vec![],
            },
        )?;
        verifier.verify_view(&response, &request, &signed.subject_public_key)?;
        Ok(response)
    }

    let world = populated_world_with_binding("formal-fixture-world");
    let mut delegation = world.capability_revocation_state().agent_signer_delegations["7"].clone();
    let (world, trust, granted_anchor, _) =
        actual_world_initial_fixture_from_world(world, "32".repeat(32));
    let owner = ProjectionAudienceV1::agent("7", public(7)).unwrap();
    let delegate = ProjectionAudienceV1::agent("7", public(8)).unwrap();
    let public_audience = ProjectionAudienceV1::public();
    let before_tree = commitments_for_world(&world).unwrap();
    let before = FormalCommittedReadService::new(
        trust.clone(),
        FormalReadPublication::new(granted_anchor.clone(), world.clone()).unwrap(),
    )
    .unwrap();
    let delegated = read_current(&before, &trust, "agent:7", &secret(8)).unwrap();
    assert!(
        delegated
            .view
            .state
            .pending_factory_builds
            .contains_key(&7)
    );
    before_tree
        .inclusion(&delegate)
        .unwrap()
        .verify(&delegate, &delegated.view, &before_tree.root().unwrap())
        .unwrap();
    assert!(
        read_current(&before, &trust, "agent:7", &secret(7))
            .unwrap()
            .view
            .state
            .pending_factory_builds
            .contains_key(&7)
    );
    assert!(
        read_current(&before, &trust, "public", &secret(8))
            .unwrap()
            .view
            .state
            .pending_factory_builds
            .is_empty()
    );

    let mut world = world;
    delegation.generation += 1;
    delegation.nonce += 1;
    delegation.revoked = true;
    world
        .apply_agent_signer_delegation(
            &authority::sign_read_request("delegation", delegation, &secret(7)).unwrap(),
        )
        .unwrap();
    let (world, revoked_anchor, _) = actual_world_successor_fixture(&granted_anchor, world);
    let after_tree = commitments_for_world(&world).unwrap();
    assert!(after_tree.inclusion(&delegate).is_err());
    let after = FormalCommittedReadService::new(
        trust.clone(),
        FormalReadPublication::new(revoked_anchor, world).unwrap(),
    )
    .unwrap();
    assert_eq!(
        read_current(&after, &trust, "agent:7", &secret(8)).unwrap_err(),
        "provider delegation revoked or fenced"
    );
    for (scope, reader_secret, audience, private) in [
        ("agent:7", secret(7), owner, true),
        ("public", secret(8), public_audience, false),
    ] {
        let response = read_current(&after, &trust, scope, &reader_secret).unwrap();
        assert_eq!(
            response.view.state.pending_factory_builds.contains_key(&7),
            private
        );
        after_tree
            .inclusion(&audience)
            .unwrap()
            .verify(&audience, &response.view, &after_tree.root().unwrap())
            .unwrap();
    }
}

#[test]
fn reader_keeps_current_claim_sequence_without_disclosing_terminal_tombstones() {
    let mut state = populated_world().state().clone();
    state.agent_claims.insert(
        "7".into(),
        serde_json::from_value(serde_json::json!({
            "target_agent_id":"7", "claim_owner_id":"7"
        }))
        .unwrap(),
    );
    state
        .agent_claim_acquisition_sequences
        .insert("7".into(), 3);
    state
        .agent_claim_acquisition_sequences
        .insert("terminated-target".into(), 9);
    let world = World::new_with_state(state);
    let projection = crate::projection::from_world(&world, Some("7")).unwrap();
    assert_eq!(
        projection.state.agent_claim_acquisition_sequences.get("7"),
        Some(&3)
    );
    assert!(
        !projection
            .state
            .agent_claim_acquisition_sequences
            .contains_key("terminated-target")
    );
    assert_eq!(
        serde_json::to_value(&projection).unwrap(),
        serde_json::to_value(
            crate::projection::legacy_projection_from_world(&world, Some("7")).unwrap()
        )
        .unwrap()
    );
}

#[test]
fn extracted_projection_matches_pre_split_public_owner_delegate_golden_bytes() {
    let world = populated_world();
    for (name, scope, reader) in [
        ("public", None, "".to_string()),
        ("owner", Some("7"), public(7)),
        ("delegate", Some("7"), public(8)),
    ] {
        let projection =
            crate::projection::from_formal_committed_world(&world, scope, &reader).unwrap();
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../oasis7_world_projection/fixtures");
        assert_eq!(
            oasis7_world_projection::codec::projection_to_json(&projection).unwrap(),
            std::fs::read(root.join(format!("{name}.json"))).unwrap(),
            "{name} JSON"
        );
        assert_eq!(
            oasis7_world_projection::codec::projection_to_cbor(&projection).unwrap(),
            std::fs::read(root.join(format!("{name}.cbor"))).unwrap(),
            "{name} CBOR"
        );
        assert_eq!(
            authority::signing_bytes("view", &projection).unwrap(),
            std::fs::read(root.join(format!("{name}.signing"))).unwrap(),
            "{name} signing"
        );
    }
}
