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
        .bind_cognition_runtime("read-world", "main", 0, None, "pending", 0)
        .unwrap();
    world
        .install_capability_agent_identity("7", "owner", 1)
        .unwrap();
    world.install_test_provider_capability_fixture("7").unwrap();
    let delegation = AgentSignerDelegationChangeV1 {
        world_id: "read-world".into(),
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
        let old = WorldServiceProjection::legacy_projection_from_world(&world, agent).unwrap();
        let current = WorldServiceProjection::from_world(&world, agent).unwrap();
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
            WorldServiceProjection::from_formal_committed_world(&world, agent, &reader).unwrap();
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
