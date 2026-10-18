use crate::service_admission::*;
use super::*;
use crate::{Action, WorldState};
use oasis7_proto::player_auth::sign_agent_chat_auth_proof;
use oasis7_client_api::world_service::*;
use oasis7_client_api::world_service::sign_read_request;

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
fn canonical_goal_fences_signature_owner_branch_generation_nonce_and_replacement() {
    let (mut world, private, public) = fixture();
    let initial = chat(&private, &public, 1, None);
    let accepted = apply(&mut world, initial.clone());
    assert!(validate_owner(&world, &initial).is_ok()); // original correlation replay auth
    assert!(
        validate_fresh(&world, &initial)
            .unwrap_err()
            .contains("nonce")
    );
    let before = serde_json::to_value(world.snapshot()).unwrap();
    let stale = chat(&private, &public, 2, None);
    assert!(world.apply_authenticated_agent_chat(&stale).is_err());
    assert_eq!(serde_json::to_value(world.snapshot()).unwrap(), before);
    for variant in 0..5 {
        let mut invalid = chat(
            &private,
            &public,
            2,
            Some(accepted["intent_id"].as_str().unwrap().into()),
        );
        match variant {
            0 => invalid.world_id = Some("foreign".into()),
            1 => invalid.canonical_authority.as_mut().unwrap().branch_id = "fork".into(),
            2 => {
                invalid
                    .canonical_authority
                    .as_mut()
                    .unwrap()
                    .agent_identity_generation = 2
            }
            3 => invalid.reorg_epoch = Some(1),
            _ => invalid.message.push_str(" tampered"),
        }
        if variant != 4 {
            invalid.auth =
                Some(sign_agent_chat_auth_proof(&invalid, 2, &public, &private).unwrap());
        }
        assert!(world.apply_authenticated_agent_chat(&invalid).is_err());
        assert_eq!(serde_json::to_value(world.snapshot()).unwrap(), before);
    }
    let stranger_private = hex::encode([72u8; 32]);
    let stranger = sign_read_request("fixture", (), &stranger_private)
        .unwrap()
        .subject_public_key;
    assert!(
        validate_owner(
            &world,
            &chat(
                &stranger_private,
                &stranger,
                2,
                Some(accepted["intent_id"].as_str().unwrap().into())
            )
        )
        .is_err()
    );
    let next = chat(
        &private,
        &public,
        2,
        Some(accepted["intent_id"].as_str().unwrap().into()),
    );
    let next_receipt = apply(&mut world, next);
    assert_ne!(accepted["intent_id"], next_receipt["intent_id"]);
    assert_eq!(
        world.state().agent_intent_ledger[accepted["intent_id"].as_str().unwrap()].status,
        "superseded"
    );
    world
        .install_capability_agent_identity("agent", "different-owner", 2)
        .unwrap();
    assert!(owner_view(&world, "agent").unwrap().is_none());
}
#[test]
fn canonical_authority_is_signed_and_legacy_absence_has_exact_codec() {
    let (_, private, public) = fixture();
    let mut request = chat(&private, &public, 1, None);
    request.canonical_authority = None;
    request.auth = Some(sign_agent_chat_auth_proof(&request, 1, &public, &private).unwrap());
    assert!(
        serde_json::to_value(&request)
            .unwrap()
            .get("canonical_authority")
            .is_none()
    );
    let original_signature = request.auth.as_ref().unwrap().signature.clone();
    let roundtrip: AgentChatRequest =
        serde_json::from_value(serde_json::to_value(&request).unwrap()).unwrap();
    assert_eq!(
        sign_agent_chat_auth_proof(&roundtrip, 1, &public, &private)
            .unwrap()
            .signature,
        original_signature
    );
    request.canonical_authority = Some(oasis7_proto::viewer::CanonicalAgentChatAuthorityV1 {
        branch_id: "main".into(),
        agent_identity_generation: 1,
    });
    assert!(verify_agent_chat_auth_proof(&request, request.auth.as_ref().unwrap()).is_err());
}

#[test]
fn canonical_owner_read_codec_golden() {
    let request = oasis7_client_api::world_service::ReadWorldViewRequest {
        observation_nonce: None,
        formal_anchor: None,
        contract_version: 1,
        world: oasis7_client_api::world_service::WorldIdentity {
            world_id: "world".into(),
            genesis_digest: "genesis".into(),
        },
        scope_id: "agent:owner-agent".into(),
        min_commit: None,
        fixed_commit: None,
        deadline_unix_ms: None,
    };
    assert_eq!(
        hex::encode(
            oasis7_client_api::world_service::signing_bytes(
                oasis7_client_api::world_service::VIEW_PATH,
                &request
            )
            .unwrap()
        ),
        "83776f61736973372e776f726c642d736572766963652e76316e2f76312f776f726c642f76696577a665776f726c64a268776f726c645f696465776f726c646e67656e657369735f6469676573746767656e657369736873636f70655f6964716167656e743a6f776e65722d6167656e746a6d696e5f636f6d6d6974f66c66697865645f636f6d6d6974f670636f6e74726163745f76657273696f6e0170646561646c696e655f756e69785f6d73f6"
    );
}
