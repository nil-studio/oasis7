//! Shared original metadata predicates, with the concrete legacy side DTOs.
use oasis7::consensus_action_payload::ConsensusActionPayloadEnvelope;
pub(crate) fn validate_committed_payload_metadata_v1(envelope:&ConsensusActionPayloadEnvelope)->Result<(),String>{
    oasis7_client_api::consensus_action_wire::validate_committed_payload_metadata_v1(envelope)
}
