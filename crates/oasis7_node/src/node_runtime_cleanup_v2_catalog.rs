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
                    | LocalConsensusActionPayloadBody::WorldGuardedStopV2 { .. }
            ) && encode_closed_task_catalog(&envelope)? != payload_cbor
            {
                return Err("noncanonical cleanup v2 catalog envelope".into());
            }
            Ok(envelope)
        }
        Err(_) if is_closed_task_catalog(payload_cbor) => {
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
pub(crate) fn is_closed_task_catalog(bytes: &[u8]) -> bool {
    use serde_cbor::Value;
    let Ok(Value::Map(envelope)) = serde_cbor::from_slice::<Value>(bytes) else {
        return false;
    };
    let Some(Value::Map(body)) = envelope.get(&Value::Text("body".into())) else {
        return false;
    };
    matches!(body.get(&Value::Text("type".into())), Some(Value::Text(tag)) if tag == "world_cleanup_v2" || tag == "world_guarded_stop_v2")
}

/// Classification only. Caller must still use the actual qualified hook.
pub(crate) fn closed_task_deadline(
    payload: &[u8],
) -> Result<
    Option<(
        oasis7_client_api::world_service::task_mutation_admission_v2::TaskMutationKindV2,
        u64,
    )>,
    String,
> {
    if !is_closed_task_catalog(payload) {
        return Ok(None);
    }
    let envelope = decode_local_consensus_action_payload_envelope(payload)?;
    match envelope.body {
        LocalConsensusActionPayloadBody::WorldGuardedStopV2 { request } => {
            if envelope.version != CONSENSUS_ACTION_PAYLOAD_ENVELOPE_VERSION
                || envelope.auth.is_some()
                || envelope.gameplay_submission_origin.is_some()
            {
                return Err("invalid closed Stop frame".into());
            }
            request.verify_owner_signature()?;
            Ok(Some((oasis7_client_api::world_service::task_mutation_admission_v2::TaskMutationKindV2::GuardedStop,request.original.request.execute_until_ms.0)))
        }
        LocalConsensusActionPayloadBody::WorldCleanupV2 { request } => {
            if envelope.version != CONSENSUS_ACTION_PAYLOAD_ENVELOPE_VERSION
                || envelope.auth.is_some()
                || envelope.gameplay_submission_origin.is_some()
            {
                return Err("invalid closed Close frame".into());
            }
            request.verify_owner_signature()?;
            Ok(Some((oasis7_client_api::world_service::task_mutation_admission_v2::TaskMutationKindV2::RevokeAndClose,request.original.request.cleanup_until_ms.0)))
        }
        _ => Ok(None),
    }
}

/// Match the official closed-frame field set without changing legacy local
/// RuntimeAction serialization (which historically emitted auth:null).
pub(super) fn encode_closed_task_catalog(
    envelope: &LocalConsensusActionPayloadEnvelope,
) -> Result<Vec<u8>, String> {
    if !matches!(
        envelope.body,
        LocalConsensusActionPayloadBody::WorldGuardedStopV2 { .. }
            | LocalConsensusActionPayloadBody::WorldCleanupV2 { .. }
    ) {
        return Err("not a closed task catalog".into());
    }
    #[derive(serde::Serialize)]
    struct ClosedFrame<'a> {
        version: u8,
        #[serde(skip_serializing_if = "Option::is_none")]
        gameplay_submission_origin:
            Option<&'a oasis7_proto::gameplay_submission_origin::GameplaySubmissionOrigin>,
        #[serde(skip_serializing_if = "Option::is_none")]
        auth: Option<&'a LocalConsensusActionAuthEnvelope>,
        body: &'a LocalConsensusActionPayloadBody,
    }
    serde_cbor::to_vec(&ClosedFrame {
        version: envelope.version,
        gameplay_submission_origin: envelope.gameplay_submission_origin.as_ref(),
        auth: envelope.auth.as_ref(),
        body: &envelope.body,
    })
    .map_err(|e| e.to_string())
}
