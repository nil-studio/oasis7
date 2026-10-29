//! Same issuer verifier for the authenticated private BFT data consumer.
use super::*;
use oasis7_net::AuthenticatedPeerRequestContext;
use oasis7_node::{
    NodeBftPrivateFetchKindV3, NodeBftPrivateReadAuthorizationV3, NodeBftPrivateReadGateV3,
    NodeBftPrivateReadScopeV3,
};

impl NodeBftPrivateReadGateV3 for FullSyncV3ReadGate {
    fn authorize(
        &self,
        transport: &AuthenticatedPeerRequestContext,
        authorization: &NodeBftPrivateReadAuthorizationV3,
        scope: &NodeBftPrivateReadScopeV3,
        make_response: &mut dyn FnMut(Instant) -> Result<Vec<u8>, String>,
    ) -> Result<Vec<u8>, String> {
        // The Node has verified the exact retained value and its proof before
        // supplying this scope. Recheck transport and live view at this bridge;
        // neither serialized scope claims nor possession of a signature grants
        // permission to read private material.
        if transport.peer_id().to_string() != scope.requester_peer_id {
            return Err("private BFT read requester differs from Noise peer".into());
        }
        let commitment = &scope.value.execution_commitment;
        let world = WorldIdentity {
            world_id: commitment.scope.world_id.clone(),
            genesis_digest: commitment.scope.genesis_hash.clone(),
        };
        let respond = |deadline| {
            transport.restrict_reply_until(deadline).map_err(|_| {
                FormalReadError::Internal("private reply release restriction rejected".into())
            })?;
            make_response(deadline).map_err(FormalReadError::Internal)
        };
        let result = match (scope.kind, authorization) {
            (
                NodeBftPrivateFetchKindV3::PendingCandidate,
                NodeBftPrivateReadAuthorizationV3::Pending(proof),
            ) => {
                if proof.body.validator_public_key_hex != scope.subject_public_key {
                    return Err("private candidate read subject mismatch".into());
                }
                self.authorize_candidate_with_response(
                    &BftCandidateReadSubjectV1 {
                        world,
                        chain_id: commitment.scope.chain_id.clone(),
                        authority_epoch: commitment.authority_epoch,
                        node_public_key: scope.subject_public_key.clone(),
                        policy_digest: proof.body.candidate_policy_digest.clone(),
                        grant_generation: proof.body.candidate_grant_generation,
                    },
                    respond,
                )
            }
            (
                NodeBftPrivateFetchKindV3::CommittedState,
                NodeBftPrivateReadAuthorizationV3::Committed(auth),
            ) => {
                if auth.subject_public_key != scope.subject_public_key {
                    return Err("private committed read subject mismatch".into());
                }
                self.authorize_bft_committed_with_response(
                    &auth.grant,
                    &BftCommittedReadScopeV1 {
                        world,
                        chain_id: commitment.scope.chain_id.clone(),
                        // The historical QC authenticates content. Current
                        // permission follows the runtime's verified live view.
                        authority_epoch: scope.view.schedule.authority_epoch(),
                    },
                    &scope.subject_public_key,
                    respond,
                )
            }
            _ => return Err("private BFT read purpose does not match authorization".into()),
        };
        result.map_err(|error| format!("private BFT read authorization rejected: {error:?}"))
    }
}
