//! Closed new catalog with unchanged legacy raw-runtime fallback.
use super::*;
pub(super) fn decode_local_consensus_action_payload_envelope(
    payload_cbor: &[u8],
) -> Result<LocalConsensusActionPayloadEnvelope, String> {
    gameplay_origin::validate_metadata_presence(payload_cbor)?;
    match serde_cbor::from_slice::<LocalConsensusActionPayloadEnvelope>(payload_cbor) {
        Ok(envelope) => {
            gameplay_origin::validate_envelope(&envelope)?;
            if matches!(
                &envelope.body,
                LocalConsensusActionPayloadBody::WorldCleanupV2 { .. }
            ) && serde_cbor::to_vec(&envelope).map_err(|e| e.to_string())? != payload_cbor
            {
                return Err("noncanonical cleanup v2 catalog envelope".into());
            }
            Ok(envelope)
        }
        Err(_) if recognized_cleanup_v2(payload_cbor) => {
            Err("invalid cleanup v2 catalog envelope".into())
        }
        Err(error) if gameplay_origin::recognized_metadata(payload_cbor) => {
            Err(format!("invalid gameplay origin envelope: {error}"))
        }
        Err(_) => Ok(LocalConsensusActionPayloadEnvelope {
            version: CONSENSUS_ACTION_PAYLOAD_ENVELOPE_VERSION,
            gameplay_submission_origin: None,
            auth: None,
            body: LocalConsensusActionPayloadBody::RuntimeAction {
                action: serde_cbor::from_slice::<JsonValue>(payload_cbor)
                    .map_err(|err| format!("legacy runtime action decode failed: {err}"))?,
            },
        }),
    }
}

// Only the new catalog is closed here; legacy raw-runtime fallback stays intact.
fn recognized_cleanup_v2(bytes: &[u8]) -> bool {
    use serde_cbor::Value;
    let Ok(Value::Map(envelope)) = serde_cbor::from_slice::<Value>(bytes) else {
        return false;
    };
    let Some(Value::Map(body)) = envelope.get(&Value::Text("body".into())) else {
        return false;
    };
    body.get(&Value::Text("type".into())) == Some(&Value::Text("world_cleanup_v2".into()))
}
