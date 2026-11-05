//! Serialization equivalence only; these DTOs grant no execution authority.
use super::*;

#[test]
fn pricing_and_final_override_views_preserve_the_original_envelope_wire_contract() {
    let original: crate::ActionEnvelope = serde_json::from_value(serde_json::json!({
        "id": 17,
        "action": {"type":"MoveAgent","data":{"agent_id":"alice","to":{"x_cm":1,"y_cm":2,"z_cm":3}}},
        "governance_authorization": {
            "schema_version":1,"world_id":"w","chain_id":"c","genesis_digest":"g",
            "parent_position":1,"parent_record_id":"p","parent_state_root":"s",
            "policy_digest":"d","controller_account_id":"a","nonce":1,
            "valid_from_position":2,"valid_through_position":3,"signatures":{}
        },
        "committed_recipe_origin": {
            "submission": {"verified_player_id":"p","public_key":"k","auth_nonce":1,
                "requester_agent_id":"alice","factory_id":"f","recipe_id":"r"},
            "consensus_action_id":1,"consensus_submitter_player_id":"p",
            "action_payload_hash":"h","committed_height":1,"action_root":"r"
        }
    }))
    .unwrap();
    let replacement = crate::Action::MoveAgent {
        agent_id: "alice".into(),
        to: oasis7_agent_api::world_action::GeoPos::new(4, 5, 6),
    };
    for retain_metadata in [true, false] {
        let expected = crate::ActionEnvelope {
            id: original.id,
            action: replacement.clone(),
            governance_authorization: retain_metadata
                .then(|| original.governance_authorization.clone())
                .flatten(),
            committed_recipe_origin: retain_metadata
                .then(|| original.committed_recipe_origin.clone())
                .flatten(),
        };
        let view = PreActionEnvelopeV1 {
            original: &original,
            action: &replacement,
            retain_metadata,
        };
        assert_eq!(
            serde_json::to_value(view).unwrap(),
            serde_json::to_value(&expected).unwrap()
        );
        assert_eq!(
            oasis7_wasm_abi::encode_canonical_cbor(&view).unwrap(),
            oasis7_wasm_abi::encode_canonical_cbor(&expected).unwrap()
        );
    }
}
