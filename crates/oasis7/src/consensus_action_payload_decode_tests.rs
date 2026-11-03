use super::*;

// Frozen pre-layout DTO: independently derived, with the original field order,
// defaults and adjacent-tag representation. No new decoder is used as oracle.
#[derive(Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename = "ConsensusActionPayloadEnvelope")]
struct LegacyEnvelope {
    version: u8,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    gameplay_submission_origin: Option<runtime::GameplaySubmissionOrigin>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    auth: Option<ConsensusActionAuthEnvelope>,
    body: LegacyBody,
}

#[derive(Debug, PartialEq, Serialize, Deserialize)]
#[serde(
    rename = "ConsensusActionPayloadBody",
    tag = "type",
    content = "data",
    rename_all = "snake_case"
)]
enum LegacyBody {
    WorldServiceIntent {
        request:
            crate::world_service::SubmitIntentRequest<crate::world_service::WorldServicePayloadV1>,
    },
    RuntimeAction {
        action: runtime::Action,
    },
    SimulatorAction {
        action: SimulatorAction,
        #[serde(default)]
        submitter: ActionSubmitter,
    },
}

fn assert_json_parity(input: &str) {
    let old = serde_json::from_str::<LegacyEnvelope>(input);
    let new = serde_json::from_str::<ConsensusActionPayloadEnvelope>(input);
    match (old, new) {
        (Ok(old), Ok(new)) => assert_eq!(
            serde_json::to_vec(&old).unwrap(),
            serde_json::to_vec(&new).unwrap()
        ),
        (Err(old), Err(new)) => assert_eq!(old.to_string(), new.to_string()),
        (old, new) => panic!("legacy/new decode differ: {old:?} / {new:?}"),
    }
}

#[test]
fn heap_payload_decode_preserves_legacy_json_order_defaults_and_errors() {
    let body = r#"{"type":"runtime_action","data":{"action":{"type":"QueryObservation","data":{"agent_id":"a"}}}}"#;
    for input in [
        format!(r#"{{"version":1,"body":{body}}}"#),
        format!(r#"{{"body":{body},"unknown":true,"version":77,"auth":null}}"#),
        format!(r#"{{"version":1,"version":2,"body":{body}}}"#),
        format!(r#"{{"version":1,"body":{body},"body":{body}}}"#),
        format!(r#"{{"body":{body}}}"#),
        r#"{"version":1,"body":{"data":{"action":{"data":{"agent_id":"a"},"type":"QueryObservation"}},"type":"runtime_action"}}"#.into(),
        r#"{"version":1,"body":{"type":"unknown","data":{}}}"#.into(),
    ] {
        assert_json_parity(&input);
    }
}

#[test]
fn heap_payload_decode_preserves_legacy_cbor_and_packed_field_indices() {
    let old = LegacyEnvelope {
        version: 1,
        gameplay_submission_origin: None,
        auth: None,
        body: LegacyBody::RuntimeAction {
            action: runtime::Action::QueryObservation {
                agent_id: "a".into(),
            },
        },
    };
    for bytes in [
        serde_cbor::to_vec(&old).unwrap(),
        serde_cbor::ser::to_vec_packed(&old).unwrap(),
    ] {
        let expected: LegacyEnvelope = serde_cbor::from_slice(&bytes).unwrap();
        let actual: ConsensusActionPayloadEnvelope = serde_cbor::from_slice(&bytes).unwrap();
        assert_eq!(
            serde_cbor::to_vec(&expected).unwrap(),
            serde_cbor::to_vec(&actual).unwrap()
        );
        assert_eq!(
            serde_cbor::ser::to_vec_packed(&expected).unwrap(),
            serde_cbor::ser::to_vec_packed(&actual).unwrap()
        );
    }
    // Sequence order follows the same four legacy fields, including optional
    // defaults only in maps, rather than introducing a new struct contract.
    let sequence = br#"[1,null,null,{"type":"runtime_action","data":{"action":{"type":"QueryObservation","data":{"agent_id":"a"}}}}]"#;
    assert_json_parity(std::str::from_utf8(sequence).unwrap());
}
