use super::*;

#[test]
fn committed_recipe_origin_envelope_legacy_bytes_and_malformed_fail_closed() {
    let action = runtime::Action::RegisterAgent {
        agent_id: "a".into(),
        pos: oasis7_agent_api::world_action::GeoPos::new(0, 0, 0),
    };
    let envelope = ConsensusActionPayloadEnvelope::from_runtime_action(action.clone());
    let encoded = encode_consensus_action_payload(&envelope).unwrap();
    let value: serde_cbor::Value = serde_cbor::from_slice(&encoded).unwrap();
    let serde_cbor::Value::Map(mut map) = value else {
        panic!("envelope map")
    };
    assert!(!map.contains_key(&serde_cbor::Value::Text(
        "gameplay_submission_origin".into()
    )));
    assert_eq!(
        decode_consensus_action_payload_envelope(&encoded).unwrap(),
        envelope
    );
    let old = serde_cbor::to_vec(&action).unwrap();
    assert_eq!(
        decode_consensus_action_payload(&old).unwrap(),
        envelope.body
    );
    map.insert(
        serde_cbor::Value::Text("gameplay_submission_origin".into()),
        serde_cbor::Value::Text("malformed".into()),
    );
    let mut null_map = map.clone();
    null_map.insert(
        serde_cbor::Value::Text("gameplay_submission_origin".into()),
        serde_cbor::Value::Null,
    );
    assert!(
        decode_consensus_action_payload_envelope(&serde_cbor::to_vec(&null_map).unwrap()).is_err()
    );
    let malformed = serde_cbor::to_vec(&map).unwrap();
    assert!(decode_consensus_action_payload_envelope(&malformed).is_err());
    // A hybrid valid legacy action cannot hide a recognized malformed origin.
    let serde_cbor::Value::Map(mut legacy_map) = serde_cbor::from_slice(&old).unwrap() else {
        panic!("legacy map")
    };
    legacy_map.extend(map);
    assert!(
        decode_consensus_action_payload_envelope(&serde_cbor::to_vec(&legacy_map).unwrap())
            .is_err()
    );
}

#[test]
fn committed_recipe_origin_rejects_non_recipe_and_unknown_metadata() {
    let mut envelope =
        ConsensusActionPayloadEnvelope::from_runtime_action(runtime::Action::RegisterAgent {
            agent_id: "a".into(),
            pos: oasis7_agent_api::world_action::GeoPos::new(0, 0, 0),
        });
    envelope.version = 2;
    envelope.gameplay_submission_origin = Some(runtime::GameplaySubmissionOrigin {
        verified_player_id: "browser-player".into(),
        public_key: "a".repeat(64),
        auth_nonce: 7,
        hosted_registration_nonce: None,
        requester_agent_id: "builder-a".into(),
        factory_id: "factory.test".into(),
        recipe_id: "recipe.test".into(),
    });
    assert!(
        decode_consensus_action_payload_envelope(
            &encode_consensus_action_payload(&envelope).unwrap()
        )
        .is_err()
    );
    let mut raw = serde_json::to_value(&envelope).unwrap();
    raw["gameplay_submission_origin"]["unknown_field"] = serde_json::json!(1);
    assert!(serde_json::from_value::<ConsensusActionPayloadEnvelope>(raw).is_err());
}

#[test]
fn committed_recipe_origin_version_two_is_explicit_and_legacy_one_bytes_unchanged() {
    let action = runtime::Action::ScheduleRecipe {
        requester_agent_id: "builder-a".into(),
        factory_id: "factory.test".into(),
        recipe_id: "recipe.test".into(),
        plan: oasis7_wasm_abi::RecipeExecutionPlan::accepted(1, vec![], vec![], vec![], 0, 1),
        logistics_route_ids: vec![],
        logistics_path_ids: vec![],
    };
    let origin = runtime::GameplaySubmissionOrigin {
        verified_player_id: "browser-player".into(),
        public_key: "a".repeat(64),
        auth_nonce: 7,
        hosted_registration_nonce: None,
        requester_agent_id: "builder-a".into(),
        factory_id: "factory.test".into(),
        recipe_id: "recipe.test".into(),
    };
    let mut envelope =
        ConsensusActionPayloadEnvelope::from_recipe_submission(action.clone(), origin);
    let bytes = encode_consensus_action_payload(&envelope).unwrap();
    assert_eq!(
        decode_consensus_action_payload_envelope(&bytes)
            .unwrap()
            .version,
        2
    );
    // The legacy decoder explicitly rejects every non-v1 envelope before execution.
    fn legacy_v1_decode(bytes: &[u8]) -> Result<ConsensusActionPayloadEnvelope, String> {
        let envelope: ConsensusActionPayloadEnvelope =
            serde_cbor::from_slice(bytes).map_err(|e| e.to_string())?;
        if envelope.version != 1 {
            return Err("unsupported consensus payload envelope version".into());
        }
        Ok(envelope)
    }
    assert!(
        legacy_v1_decode(&bytes)
            .unwrap_err()
            .contains("unsupported")
    );
    envelope.version = 1;
    assert!(
        decode_consensus_action_payload_envelope(
            &encode_consensus_action_payload(&envelope).unwrap()
        )
        .is_err()
    );
    envelope.version = 2;
    envelope.gameplay_submission_origin = None;
    assert!(
        decode_consensus_action_payload_envelope(
            &encode_consensus_action_payload(&envelope).unwrap()
        )
        .is_err()
    );
    #[derive(Serialize)]
    struct LegacyEnvelope<'a> {
        version: u8,
        #[serde(skip_serializing_if = "Option::is_none")]
        auth: Option<&'a ConsensusActionAuthEnvelope>,
        body: &'a ConsensusActionPayloadBody,
    }
    let legacy = ConsensusActionPayloadEnvelope::from_runtime_action(action);
    assert_eq!(
        encode_consensus_action_payload(&legacy).unwrap(),
        serde_cbor::to_vec(&LegacyEnvelope {
            version: 1,
            auth: None,
            body: &legacy.body
        })
        .unwrap()
    );
}
