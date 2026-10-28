//! Compact protocol framing only: these vectors do not prove World execution,
//! current issuer permission, provider custody, or active membership.
use super::*;
use ed25519_dalek::{Signer, SigningKey};
use oasis7_client_api::{
    world_service::WorldIdentity,
    world_service_full_sync::{FormalNodeReadGrantBodyV1, FormalNodeReadRoleV1},
};
use oasis7_consensus::tendermint_v2::BftValidatorSetV2;
use oasis7_consensus::tendermint_v3::FormalBftExecutionCommitmentV1;
use oasis7_proto::distributed_bft::{
    v2::{BftScopeV2, BftValidatorTransitionV2, BftValidatorV2},
    v3::BftBlockHeaderV3,
};

fn hash(n: u8) -> String {
    format!("{n:02x}").repeat(32)
}
fn large_transition_value() -> BftHeaderValueV3 {
    let scope = BftScopeV2 {
        world_id: "wire-boundary".into(),
        chain_id: "chain".into(),
        genesis_hash: hash(1),
    };
    let validators: Vec<_> = (0u16..256)
        .map(|i| {
            let mut seed = [19; 32];
            seed[..2].copy_from_slice(&i.to_be_bytes());
            BftValidatorV2 {
                validator_id: format!("{i:03}-{}", "v".repeat(252)),
                public_key_hex: hex::encode(
                    SigningKey::from_bytes(&seed).verifying_key().to_bytes(),
                ),
                voting_power: 1,
            }
        })
        .collect();
    assert!(validators.iter().all(|v| v.validator_id.len() == 256));
    let set = BftValidatorSetV2::new(scope.clone(), validators.clone()).unwrap();
    let transition = BftValidatorTransitionV2 {
        source_commit_ref: hash(2),
        source_state_root: hash(3),
        source_profile: "bft".into(),
        source_epoch: 0,
        target_epoch: 2,
        epoch_length_ticks: 2,
        mapping_ruleset_hash: hash(4),
        scheduled_height: 2,
        activation_height: 4,
        old_validator_set_hash: hash(5),
        new_validators: validators,
        new_validator_set_hash: set.set_hash().into(),
        governance_action_digest: hash(6),
        projected_registry_digest: hash(7),
    };
    transition.validate().unwrap();
    let commitment = FormalBftExecutionCommitmentV1 {
        domain: "oasis7.formal.bft.execution.v1".into(),
        schema_version: 1,
        profile: "bft".into(),
        profile_version: 1,
        activation_digest: hash(8),
        scope,
        height: 2,
        parent_tick: 1,
        parent_commit_ref: hash(2),
        parent_semantic_state_root: hash(3),
        authority_epoch: 1,
        active_validator_set_hash: hash(5),
        schedule_policy_hash: hash(4),
        manifest_hash: hash(9),
        post_manifest_hash: hash(10),
        action_root: hash(11),
        event_root: hash(12),
        execution_state_root: hash(13),
        state_root: hash(14),
        projection_root: hash(15),
        result_root: hash(16),
        transition: Some(transition),
        transition_ref: None,
    };
    let value = BftHeaderValueV3 {
        header: BftBlockHeaderV3 {
            protocol_version: 3,
            execution_commitment_hash: commitment.digest().unwrap(),
            actions_cbor_digest: hash(17),
        },
        execution_commitment: commitment,
    };
    value.validate_binding().unwrap();
    value
}
fn target(value: &BftHeaderValueV3, kind: NodeBftPrivateFetchKindV3) -> Target {
    Target {
        context: BftPeerContextV3 {
            scope: value.execution_commitment.scope.clone(),
            activation_digest: value.execution_commitment.activation_digest.clone(),
            target_digest: hash(18),
        },
        height: value.execution_commitment.height,
        header_id: value.block_id().unwrap(),
        execution_commitment_hash: value.header.execution_commitment_hash.clone(),
        kind,
        requester_peer_id: libp2p::PeerId::random().to_string(),
        serving_peer_id: libp2p::PeerId::random().to_string(),
        nonce: hash(19),
        operation: BftCandidateReadOperationV3::Manifest,
    }
}
fn pending(value: &BftHeaderValueV3) -> Request {
    let target = target(value, NodeBftPrivateFetchKindV3::PendingCandidate);
    let key = SigningKey::from_bytes(&[21; 32]);
    let public = hex::encode(key.verifying_key().to_bytes());
    let body = target
        .pending_body(value, "validator".into(), public.clone(), hash(20), 1)
        .unwrap();
    let proof = BftCandidateReadProofV3::sign(body.clone(), &key).unwrap();
    proof.verify_for(&body, &public).unwrap();
    Request {
        domain: DOMAIN.into(),
        schema_version: 2,
        target,
        authorization: NodeBftPrivateReadAuthorizationV3::Pending(Box::new(proof)),
        chunk_index: None,
    }
}
fn committed(value: &BftHeaderValueV3) -> Request {
    let target = target(value, NodeBftPrivateFetchKindV3::CommittedState);
    let reader = SigningKey::from_bytes(&[22; 32]);
    let subject = hex::encode(reader.verifying_key().to_bytes());
    let grant = FormalNodeReadGrantV1::sign(
        FormalNodeReadGrantBodyV1 {
            schema_version: 1,
            world: WorldIdentity {
                world_id: target.context.scope.world_id.clone(),
                genesis_digest: target.context.scope.genesis_hash.clone(),
            },
            chain_id: target.context.scope.chain_id.clone(),
            node_public_key: subject.clone(),
            role: FormalNodeReadRoleV1::FullReadOnly,
            grant_epoch: 1,
            authority_epoch: 1,
            not_before_unix_ms: 1,
            expires_unix_ms: 1000,
        },
        &hex::encode([23; 32]),
    )
    .unwrap();
    let payload = committed_payload(&target, &grant, &subject).unwrap();
    let signature = hex::encode(reader.sign(&payload).to_bytes());
    verify_signature(&subject, &payload, &signature).unwrap();
    Request {
        domain: DOMAIN.into(),
        schema_version: 2,
        target,
        authorization: NodeBftPrivateReadAuthorizationV3::Committed(Box::new(
            NodeBftCommittedReadAuthorizationV3 {
                grant,
                subject_public_key: subject,
                signature_hex: signature,
            },
        )),
        chunk_index: None,
    }
}
#[test]
fn compact_wire2_real_signed_requests_fit_with_maximum_transition() {
    let value = large_transition_value();
    assert!(encode(&value).unwrap().len() > MAX_FRAME);
    assert!(encode_frame(&value).is_err());
    for request in [pending(&value), committed(&value)] {
        request.validate().unwrap();
        request.target.validate_value(&value).unwrap();
        let bytes = encode_frame(&request).unwrap();
        assert!(bytes.len() < MAX_FRAME);
        let decoded: Request = decode_frame(&bytes).unwrap();
        decoded.validate().unwrap();
        decoded.target.validate_value(&value).unwrap();
    }
}
#[test]
fn compact_wire2_exact_value_binding_rejects_changed_identity() {
    let value = large_transition_value();
    let original = target(&value, NodeBftPrivateFetchKindV3::PendingCandidate);
    for field in 0..3 {
        let mut changed = original.clone();
        match field {
            0 => changed.height += 1,
            1 => changed.header_id = hash(90),
            _ => changed.execution_commitment_hash = hash(91),
        }
        changed.validate().unwrap();
        assert!(changed.validate_value(&value).is_err());
    }
}
#[test]
fn compact_wire2_rejects_old_domain_schema_noncanonical_and_trailing() {
    let value = large_transition_value();
    let request = pending(&value);
    for (domain, schema) in [("oasis7.bft.private-candidate.exchange.v1", 2), (DOMAIN, 1)] {
        let mut old = request.clone();
        old.domain = domain.into();
        old.schema_version = schema;
        let decoded: Request = decode_frame(&encode_frame(&old).unwrap()).unwrap();
        assert!(decoded.validate().is_err());
    }
    let mut trailing = encode_frame(&request).unwrap();
    trailing.push(0);
    assert!(decode_frame::<Request>(&trailing).is_err());
    assert!(decode_frame::<u64>(&[0x18, 0x01]).is_err());
    assert_eq!(decode_frame::<u64>(&[0x01]).unwrap(), 1);
}
