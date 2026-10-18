use crate::runtime::{Action, WorldState};
use crate::runtime::{
    World,
    service_admission::{agent_chat::*, *},
};
use crate::world_service::projection;
use oasis7_client_api::world_service::sign_read_request;
use oasis7_proto::player_auth::sign_agent_chat_auth_proof;
use oasis7_proto::viewer::AgentChatRequest;

fn fixture() -> (World, String, String) {
    let private = hex::encode([71u8; 32]);
    let public = sign_read_request("fixture", (), &private)
        .unwrap()
        .subject_public_key;
    let mut world = World::new_with_state(WorldState::default());
    world.submit_action(Action::RegisterAgent {
        agent_id: "agent".into(),
        pos: oasis7_agent_api::world_action::GeoPos::new(0, 0, 0),
    });
    world.step().unwrap();
    world.submit_action(Action::ClaimStarterOc {
        agent_id: "agent".into(),
        player_id: "owner".into(),
        public_key: Some(public.clone()),
    });
    world.step().unwrap();
    world
        .install_capability_agent_identity("agent", "owner", 1)
        .unwrap();
    world
        .bind_cognition_runtime("world", "main", 0, None, "pending", 0)
        .unwrap();
    (world, private, public)
}
fn chat(private: &str, public: &str, nonce: u64, replacement: Option<String>) -> AgentChatRequest {
    let mut request = AgentChatRequest {
        agent_id: "agent".into(),
        message: "private engineering goal".into(),
        player_id: Some("owner".into()),
        public_key: Some(public.into()),
        auth: None,
        intent_tick: Some(2),
        intent_seq: Some(nonce),
        world_id: Some("world".into()),
        reorg_epoch: Some(0),
        authority_scope: Some("player_agent_chat".into()),
        replaces_intent_id: replacement,
        canonical_authority: Some(oasis7_proto::viewer::CanonicalAgentChatAuthorityV1 {
            branch_id: "main".into(),
            agent_identity_generation: 1,
        }),
    };
    request.auth = Some(sign_agent_chat_auth_proof(&request, nonce, public, private).unwrap());
    request
}
fn apply(world: &mut World, request: AgentChatRequest) -> serde_json::Value {
    let payload = WorldServicePayloadV1::AgentChat(request.clone());
    let correlation = derive_correlation(
        WorldIdentity {
            world_id: "world".into(),
            genesis_digest: "fixture".into(),
        },
        &payload,
    )
    .unwrap();
    let receipt = world.apply_authenticated_agent_chat(&request).unwrap();
    world
        .record_world_service_result(CanonicalIntentResultV1 {
            request: SubmitIntentRequest {
                contract_version: 1,
                correlation,
                deadline_unix_ms: None,
                signed_payload: payload,
            },
            action_id: 1,
            committed_height: 1,
            receipt: receipt.clone(),
            rejected: None,
        })
        .unwrap();
    receipt
}
#[test]
fn canonical_owner_goal_survives_journal_restore_and_is_private_to_authorized_view() {
    let (mut world, private, public) = fixture();
    let request = chat(&private, &public, 1, None);
    let before = serde_json::to_value(world.snapshot()).unwrap();
    validate_fresh(&world, &request).unwrap();
    assert_eq!(serde_json::to_value(world.snapshot()).unwrap(), before);
    let receipt = apply(&mut world, request);
    let snapshot = world.snapshot_with_chain_resource_context(
        crate::runtime::ChainResourceDerivationContext {
            world_id: "world",
            chain_id: "fixture-chain",
            genesis_ref: None,
            created_at_height: world.state().time,
            manifest_height: world.state().time,
            commit_block_hash: None,
            tick: world.state().time,
        },
        "fixture",
        "fixture",
    );
    let restored = World::from_snapshot(snapshot, world.journal().clone()).unwrap();
    let owner = projection::WorldServiceProjection::from_authenticated_world(
        &restored,
        Some("agent"),
        &public,
    )
    .unwrap();
    let goal = owner.canonical_agent_chat.unwrap().goal.unwrap();
    assert_eq!(goal.message, "private engineering goal");
    assert_eq!(goal.intent_id, receipt["intent_id"]);
    let public_view = serde_json::to_value(
        projection::WorldServiceProjection::from_world(&restored, None).unwrap(),
    )
    .unwrap();
    assert!(public_view.get("canonical_agent_chat").is_none());
    assert!(!public_view.to_string().contains("private engineering goal"));
    let delegated = projection::WorldServiceProjection::from_authenticated_world(
        &restored,
        Some("agent"),
        "different-key",
    )
    .unwrap();
    assert!(delegated.canonical_agent_chat.is_none());
    assert!(
        !serde_json::to_string(&delegated)
            .unwrap()
            .contains("private engineering goal")
    );
    assert!(
        !serde_json::to_string(
            &world
                .journal()
                .events
                .iter()
                .filter(|event| matches!(event.body, crate::runtime::WorldEventBody::Domain(_)))
                .collect::<Vec<_>>()
        )
        .unwrap()
        .contains("private engineering goal")
    );
}
