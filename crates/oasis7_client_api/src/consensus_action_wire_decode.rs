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

pub fn decode_consensus_action_payload_envelope_with<
    B: ConsensusPayloadBodyViewV1,
    O: ConsensusPayloadDecodeOpsV1<B>,
>(
    payload_cbor: &[u8],
    ops: &mut O,
) -> Result<Result<DecodedConsensusPayloadV1<O::Envelope, O::RuntimeAction>, String>, O::Error> {
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
