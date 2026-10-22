//! Shared authoritative ingress validation for execution hook adapters.
use oasis7_runtime::World;

pub(super) fn validate_payload(world: &World, payload_cbor: &[u8]) -> Result<(), String> {
    use oasis7::consensus_action_payload::{
        ConsensusActionAuthEnvelope, ConsensusActionPayloadBody,
        decode_consensus_action_payload_envelope,
    };
    let envelope = decode_consensus_action_payload_envelope(payload_cbor)
        .map_err(|error| format!("decode governance preflight payload: {error}"))?;
    if envelope.gameplay_submission_origin.is_some() {
        return Err("governance authorization cannot carry gameplay origin".into());
    }
    let ConsensusActionPayloadBody::RuntimeAction { action } = envelope.body else {
        return Err("governance authorization requires a runtime action".into());
    };
    let Some(ConsensusActionAuthEnvelope::GovernanceAction(proof)) = envelope.auth else {
        return Err("governance authorization proof missing".into());
    };
    world
        .verify_governance_action_for_next_commit(&action, &proof)
        .map_err(|error| format!("governance preflight denied: {error:?}"))
}
