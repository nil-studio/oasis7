//! The original complete envelope predicates, independent of upper side DTOs.
//! Inner codec refusal preserves legacy fallback; outer fatal never does.
use super::*;
use std::borrow::Borrow;
pub trait ConsensusPayloadDecodeOpsV1<B> {
    type Error;
    type Value: Borrow<serde_cbor::Value>;
    type Envelope: Borrow<ConsensusActionPayloadEnvelope<B>>;
    type RuntimeAction: Borrow<Action>;
    type Encoded: AsRef<[u8]>;
    fn decode_value(&mut self, bytes: &[u8]) -> Result<Result<Self::Value, String>, Self::Error>;
    fn decode_envelope(
        &mut self,
        bytes: &[u8],
    ) -> Result<Result<Self::Envelope, String>, Self::Error>;
    fn decode_runtime(
        &mut self,
        bytes: &[u8],
    ) -> Result<Result<Self::RuntimeAction, String>, Self::Error>;
    fn encode_envelope(
        &mut self,
        envelope: &ConsensusActionPayloadEnvelope<B>,
    ) -> Result<Result<Self::Encoded, String>, Self::Error>;
}
/// Preserve the real owner; a legacy payload is not cloned into a second DTO.
/// Neither alternative grants execution or acceptance permission.
pub enum DecodedConsensusPayloadV1<E, R> {
    Envelope(E),
    LegacyRuntime(R),
}

type ConsensusPayloadDecodeResultV1<Envelope, RuntimeAction, Error> =
    Result<Result<DecodedConsensusPayloadV1<Envelope, RuntimeAction>, String>, Error>;

pub fn decode_consensus_action_payload_envelope_with<
    B: ConsensusPayloadBodyViewV1,
    O: ConsensusPayloadDecodeOpsV1<B>,
>(
    payload_cbor: &[u8],
    ops: &mut O,
) -> ConsensusPayloadDecodeResultV1<O::Envelope, O::RuntimeAction, O::Error> {
    // Recognized null metadata cannot silently downgrade to a legacy action.
    if ops
        .decode_value(payload_cbor)?
        .ok()
        .is_some_and(|value| match value.borrow() {
            serde_cbor::Value::Map(map) => {
                map.get(&serde_cbor::Value::Text(
                    "gameplay_submission_origin".to_string(),
                )) == Some(&serde_cbor::Value::Null)
            }
            _ => false,
        })
    {
        return Ok(Err(
            "gameplay submission origin must not be null".to_string()
        ));
    }
    match ops.decode_envelope(payload_cbor)? {
        Ok(owner) => {
            let envelope = owner.borrow();
            match (
                envelope.version,
                envelope.gameplay_submission_origin.as_ref(),
            ) {
                (1, None) | (2, Some(_)) => {}
                _ => {
                    return Ok(Err(format!(
                        "unsupported consensus payload envelope version/origin combination {}",
                        envelope.version
                    )));
                }
            }
            if envelope.body.is_cleanup_v2() {
                let encoded = match ops.encode_envelope(envelope)? {
                    Ok(encoded) => encoded,
                    Err(error) => return Ok(Err(error)),
                };
                if encoded.as_ref() != payload_cbor {
                    return Ok(Err(
                        "cleanup v2 envelope is not exact typed canonical bytes".into(),
                    ));
                }
            }
            if let Some(origin) = &envelope.gameplay_submission_origin {
                match envelope.body.runtime_action() {
                    Some(Action::ScheduleRecipe {
                        requester_agent_id,
                        factory_id,
                        recipe_id,
                        ..
                    }) if origin.matches_recipe(requester_agent_id, factory_id, recipe_id) => {}
                    _ => {
                        return Ok(Err(
                            "gameplay submission origin does not match ScheduleRecipe".to_string(),
                        ));
                    }
                }
            }
            Ok(Ok(DecodedConsensusPayloadV1::Envelope(owner)))
        }
        Err(envelope_err) => {
            let recognized_origin =
                ops.decode_value(payload_cbor)?
                    .ok()
                    .is_some_and(|value| match value.borrow() {
                        serde_cbor::Value::Map(map) => {
                            map.contains_key(&serde_cbor::Value::Text(
                                "gameplay_submission_origin".to_string(),
                            )) || map.get(&serde_cbor::Value::Text("version".to_string()))
                                == Some(&serde_cbor::Value::Integer(2))
                        }
                        _ => false,
                    });
            if recognized_origin {
                return Ok(Err(format!(
                    "invalid gameplay submission origin envelope: {envelope_err}"
                )));
            }
            match ops.decode_runtime(payload_cbor)? {
                Ok(action) => Ok(Ok(DecodedConsensusPayloadV1::LegacyRuntime(action))),
                Err(runtime_err) => Ok(Err(format!(
                    "decode consensus payload envelope failed ({envelope_err}); runtime fallback failed ({runtime_err})"
                ))),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{Value, json};

    type Body = ConsensusActionPayloadBody<Value, (), Value, Value>;
    type Envelope = ConsensusActionPayloadEnvelope<Body>;

    #[derive(Default)]
    struct Codec {
        calls: Vec<&'static str>,
        fatal_at: Option<&'static str>,
    }
    impl Codec {
        fn step(&mut self, name: &'static str) -> Result<(), &'static str> {
            self.calls.push(name);
            if self.fatal_at == Some(name) {
                Err("caller budget exhausted")
            } else {
                Ok(())
            }
        }
    }
    impl ConsensusPayloadDecodeOpsV1<Body> for Codec {
        type Error = &'static str;
        type Value = serde_cbor::Value;
        type Envelope = Envelope;
        type RuntimeAction = Action;
        type Encoded = Vec<u8>;

        fn decode_value(
            &mut self,
            bytes: &[u8],
        ) -> Result<Result<Self::Value, String>, Self::Error> {
            self.step("value")?;
            Ok(serde_cbor::from_slice(bytes).map_err(|error| error.to_string()))
        }
        fn decode_envelope(
            &mut self,
            bytes: &[u8],
        ) -> Result<Result<Envelope, String>, Self::Error> {
            self.step("envelope")?;
            Ok(serde_cbor::from_slice(bytes).map_err(|error| error.to_string()))
        }
        fn decode_runtime(&mut self, bytes: &[u8]) -> Result<Result<Action, String>, Self::Error> {
            self.step("runtime")?;
            Ok(serde_cbor::from_slice(bytes).map_err(|error| error.to_string()))
        }
        fn encode_envelope(
            &mut self,
            value: &Envelope,
        ) -> Result<Result<Vec<u8>, String>, Self::Error> {
            self.step("encode")?;
            Ok(oasis7_wasm_abi::encode_canonical_cbor(value).map_err(|error| error.to_string()))
        }
    }
    fn action() -> Action {
        Action::QueryObservation {
            agent_id: "agent".into(),
        }
    }

    #[test]
    fn shared_envelope_preserves_runtime_wire_and_original_legacy_fallback() {
        let envelope = Envelope::from_runtime_action(action());
        let original = json!({
            "version": 1,
            "body": {"type": "runtime_action", "data": {
                "action": {"type": "QueryObservation", "data": {"agent_id": "agent"}}
            }}
        });
        assert_eq!(serde_json::to_value(&envelope).unwrap(), original);
        let bytes = oasis7_wasm_abi::encode_canonical_cbor(&envelope).unwrap();
        assert_eq!(
            bytes,
            oasis7_wasm_abi::encode_canonical_cbor(&original).unwrap()
        );
        let mut codec = Codec::default();
        match decode_consensus_action_payload_envelope_with(&bytes, &mut codec)
            .unwrap()
            .unwrap()
        {
            DecodedConsensusPayloadV1::Envelope(decoded) => assert_eq!(decoded, envelope),
            DecodedConsensusPayloadV1::LegacyRuntime(_) => panic!("envelope downgraded"),
        }
        assert_eq!(codec.calls, ["value", "envelope"]);
        let bytes = oasis7_wasm_abi::encode_canonical_cbor(&action()).unwrap();
        let mut codec = Codec::default();
        match decode_consensus_action_payload_envelope_with(&bytes, &mut codec)
            .unwrap()
            .unwrap()
        {
            DecodedConsensusPayloadV1::LegacyRuntime(decoded) => assert_eq!(decoded, action()),
            DecodedConsensusPayloadV1::Envelope(_) => panic!("legacy action changed"),
        }
        assert_eq!(codec.calls, ["value", "envelope", "value", "runtime"]);
    }

    #[test]
    fn recognized_origin_metadata_never_downgrades_to_a_legacy_action() {
        for (field, value) in [
            ("gameplay_submission_origin", Value::Null),
            ("version", json!(2)),
        ] {
            let mut raw = serde_json::to_value(action()).unwrap();
            raw.as_object_mut().unwrap().insert(field.into(), value);
            let mut codec = Codec::default();
            let result = decode_consensus_action_payload_envelope_with(
                &serde_cbor::to_vec(&raw).unwrap(),
                &mut codec,
            )
            .unwrap();
            assert!(result.is_err());
            assert!(!codec.calls.contains(&"runtime"));
        }
    }

    #[test]
    fn fatal_codec_failure_stops_before_any_fallback_or_retry() {
        let bytes = oasis7_wasm_abi::encode_canonical_cbor(&action()).unwrap();
        for stage in ["value", "envelope", "runtime"] {
            let mut codec = Codec {
                fatal_at: Some(stage),
                ..Codec::default()
            };
            assert!(matches!(
                decode_consensus_action_payload_envelope_with(&bytes, &mut codec),
                Err("caller budget exhausted")
            ));
            assert_eq!(codec.calls.last(), Some(&stage));
        }
    }

    #[test]
    fn cleanup_requires_exact_canonical_bytes_and_propagates_fatal_encoding() {
        let envelope = Envelope {
            version: 1,
            gameplay_submission_origin: None,
            auth: None,
            body: Body::WorldCleanupV2 {
                request: json!({"request": "cleanup"}),
            },
        };
        let canonical = oasis7_wasm_abi::encode_canonical_cbor(&envelope).unwrap();
        assert!(
            decode_consensus_action_payload_envelope_with(&canonical, &mut Codec::default())
                .unwrap()
                .is_ok()
        );
        let reordered = serde_cbor::to_vec(&envelope).unwrap();
        assert_ne!(canonical, reordered);
        let error =
            decode_consensus_action_payload_envelope_with(&reordered, &mut Codec::default())
                .unwrap()
                .err()
                .unwrap();
        assert_eq!(
            error,
            "cleanup v2 envelope is not exact typed canonical bytes"
        );
        let mut codec = Codec {
            fatal_at: Some("encode"),
            ..Codec::default()
        };
        assert!(matches!(
            decode_consensus_action_payload_envelope_with(&canonical, &mut codec),
            Err("caller budget exhausted")
        ));
        assert_eq!(codec.calls, ["value", "envelope", "encode"]);
    }

    #[test]
    fn recipe_origin_is_bound_to_the_actual_action() {
        let origin = GameplaySubmissionOrigin {
            verified_player_id: "player".into(),
            public_key: "ab".repeat(32),
            auth_nonce: 1,
            hosted_registration_nonce: None,
            requester_agent_id: "agent".into(),
            factory_id: "factory".into(),
            recipe_id: "recipe".into(),
        };
        let mut envelope = Envelope::from_recipe_submission(
            Action::ScheduleRecipe {
                requester_agent_id: "agent".into(),
                factory_id: "factory".into(),
                recipe_id: "recipe".into(),
                plan: oasis7_wasm_abi::RecipeExecutionPlan::accepted(
                    1,
                    vec![],
                    vec![],
                    vec![],
                    0,
                    1,
                ),
                logistics_route_ids: vec![],
                logistics_path_ids: vec![],
            },
            origin,
        );
        let bytes = oasis7_wasm_abi::encode_canonical_cbor(&envelope).unwrap();
        assert!(
            decode_consensus_action_payload_envelope_with(&bytes, &mut Codec::default())
                .unwrap()
                .is_ok()
        );
        envelope
            .gameplay_submission_origin
            .as_mut()
            .unwrap()
            .factory_id = "other".into();
        let bytes = oasis7_wasm_abi::encode_canonical_cbor(&envelope).unwrap();
        assert_eq!(
            decode_consensus_action_payload_envelope_with(&bytes, &mut Codec::default())
                .unwrap()
                .err()
                .unwrap(),
            "gameplay submission origin does not match ScheduleRecipe"
        );
    }
}
