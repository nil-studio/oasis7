use super::{FormalBftReadPublicationInputV2, FormalBftReadPublicationV2};
use crate::{
    authority,
    formal_read_error::FormalReadError,
    formal_read_fixtures as fixtures,
    formal_result::{FormalBftResultSuccessorInputV1, FormalBusinessResultTreeV1},
};
use ed25519_dalek::SigningKey;
use oasis7_client_api::world_service::{
    FORMAL_BFT_READ_PATH_V2, FormalBftReadRequestV2, FormalBftReadTargetV2,
};
use oasis7_consensus::tendermint_v2::{
    BFT_FORMAL_EXECUTION_DOMAIN_V1, BFT_FORMAL_PROFILE_V1, BFT_FORMAL_PROFILE_VERSION_V1,
    BftProfileTargetV1, BftSchedulePolicyV2, FormalBftExecutionCommitmentV1,
    verify_formal_bft_handoff_v1,
};
use oasis7_formal_protocol::controlled_authority::formal::{
    FormalQualifiedCommitProofV1, ProfileHandoffV1, VerifiedFormalReadAnchorV1,
    business_index::{BusinessIndexNodeReader, BusinessIndexNodeWriteV1},
};
use oasis7_proto::distributed_bft::v2::{
    BFT_PROTOCOL_VERSION_V2, BftBlockV2, BftCommitCertificateV2, BftProposalHeaderV2,
    BftProposalV2, BftScopeV2, BftValidatorV2, BftVotePhaseV2, BftVoteV2, BftVoteValueV2,
};
use oasis7_runtime::{World, blake3_hex};
use std::collections::BTreeMap;

#[derive(Default)]
struct MemoryIndex(BTreeMap<String, Vec<u8>>);

impl BusinessIndexNodeReader for MemoryIndex {
    fn read_node(&self, digest: &str) -> Result<Option<Vec<u8>>, String> {
        Ok(self.0.get(digest).cloned())
    }
}

impl MemoryIndex {
    fn persist(&mut self, writes: &[BusinessIndexNodeWriteV1]) {
        for write in writes {
            self.0.insert(write.digest.clone(), write.bytes.clone());
        }
    }
}

struct Fixture {
    publication: FormalBftReadPublicationV2,
    anchor: VerifiedFormalReadAnchorV1,
    target: BftProfileTargetV1,
    exact_target: FormalBftReadTargetV2,
}

fn fixture() -> Fixture {
    let mut seed = World::new_production_hardened();
    seed.bind_cognition_runtime("formal-fixture-world", "main", 0, None, "pending", 0)
        .unwrap();
    seed.step().unwrap();
    let (initial_world, trust, unindexed_anchor, initial_proof) =
        fixtures::actual_world_initial_fixture_from_world(seed, fixtures::h(50));
    let identity = unindexed_anchor.head().record().world.clone();
    let mut index = MemoryIndex::default();
    let initial_tree =
        FormalBusinessResultTreeV1::for_initial_world(&initial_world, &identity, &index).unwrap();
    index.persist(&initial_tree.delta.node_writes);

    let mut initial_record = initial_proof.decision.record.clone();
    initial_record.result_root = initial_tree.tree.root().unwrap();
    let initial_proof = fixtures::qualified_proof(
        initial_record,
        None,
        unindexed_anchor.authority(),
        None,
        None,
        4,
        5,
        6,
    );
    let initial_anchor = VerifiedFormalReadAnchorV1::from_initial(
        unindexed_anchor.initial_certificate(),
        &trust,
        &initial_proof,
    )
    .unwrap();

    let signing_key = SigningKey::from_bytes(&[31; 32]);
    let target = BftProfileTargetV1::new(
        BftScopeV2 {
            world_id: identity.world_id.clone(),
            chain_id: identity.chain_id.clone(),
            genesis_hash: identity.genesis_digest.clone(),
        },
        2,
        vec![BftValidatorV2 {
            validator_id: "validator-a".into(),
            public_key_hex: hex::encode(signing_key.verifying_key().to_bytes()),
            voting_power: 7,
        }],
        BftSchedulePolicyV2::new(4, BFT_FORMAL_PROFILE_V1).unwrap(),
    )
    .unwrap();

    let (handoff_world, _, provisional_h_proof) =
        fixtures::actual_world_successor_fixture(&initial_anchor, initial_world);
    let h_snapshot = handoff_world.snapshot();
    let h_execution_root =
        blake3_hex(&oasis7_world_state::codec::snapshot_cbor(&h_snapshot).unwrap());
    let h_tree =
        FormalBusinessResultTreeV1::extend_bft_successor(FormalBftResultSuccessorInputV1 {
            world: &handoff_world,
            identity: &identity,
            height: 2,
            parent_result_root: &initial_tree.tree.root().unwrap(),
            execution_state_root: &h_execution_root,
            previous: &initial_tree.tree,
            new_results: &[],
            reader: &index,
        })
        .unwrap();
    index.persist(&h_tree.delta.node_writes);

    let handoff = ProfileHandoffV1 {
        next_profile: BFT_FORMAL_PROFILE_V1.into(),
        next_profile_version: BFT_FORMAL_PROFILE_VERSION_V1,
        next_authority_digest: target.target_digest().unwrap(),
        valid_from_position: 3,
    };
    let mut frozen = provisional_h_proof
        .decision
        .frozen
        .clone()
        .expect("successor fixture has a frozen certificate");
    frozen.reservation.body.handoff = Some(handoff.clone());
    frozen.reservation.signature_hex = fixtures::sign(
        oasis7_formal_protocol::controlled_authority::formal::frozen_signing_bytes(
            &frozen.reservation.body,
        )
        .unwrap(),
        4,
    );
    let reservation_id = frozen.reservation.body.reservation_id().unwrap();
    for (receipt, key) in [(&mut frozen.primary, 5), (&mut frozen.replica, 6)] {
        receipt.body.reservation_id = reservation_id.clone();
        receipt.signature_hex = fixtures::sign(
            oasis7_formal_protocol::controlled_authority::formal::frozen_receipt_signing_bytes(
                &receipt.body,
            )
            .unwrap(),
            key,
        );
    }
    let mut h_record = provisional_h_proof.decision.record.clone();
    h_record.result_root = h_tree.tree.root().unwrap();
    h_record.handoff = Some(handoff);
    h_record.frozen_reservation_id = Some(reservation_id);
    let h_proof: FormalQualifiedCommitProofV1 = fixtures::qualified_proof(
        h_record,
        Some(frozen),
        initial_anchor.authority(),
        Some(initial_anchor.head()),
        None,
        4,
        5,
        6,
    );
    let h_commit = oasis7_formal_protocol::controlled_authority::formal::verify_formal_qualified(
        &h_proof,
        initial_anchor.authority(),
        Some(initial_anchor.head()),
        None,
    )
    .unwrap();
    let h_anchor = initial_anchor
        .after_verified_commit(initial_anchor.authority().clone(), h_commit)
        .unwrap();
    let handoff_cap = verify_formal_bft_handoff_v1(&h_anchor, target.clone()).unwrap();
    let h_semantic_state_root = handoff_world
        .execution_state_identity()
        .unwrap()
        .state_root_hash;
    let h_world_manifest_hash = handoff_world.current_manifest_hash().unwrap();
    assert_ne!(
        h_world_manifest_hash,
        handoff_cap.governing_manifest_digest(),
        "the runtime-governing digest and replayed World manifest are distinct"
    );

    let mut committed_world = handoff_world;
    committed_world.step().unwrap();
    let committed_world = World::from_snapshot(
        committed_world.snapshot_with_chain_resource_context(
            oasis7_runtime::ChainResourceDerivationContext {
                world_id: &identity.world_id,
                chain_id: &identity.chain_id,
                genesis_ref: Some(&identity.genesis_digest),
                created_at_height: 0,
                manifest_height: 0,
                commit_block_hash: None,
                tick: committed_world.state().time,
            },
            fixtures::h(10),
            fixtures::h(11),
        ),
        committed_world.journal().clone(),
    )
    .unwrap();
    let execution_state_root =
        blake3_hex(&oasis7_world_state::codec::snapshot_cbor(&committed_world.snapshot()).unwrap());
    let state_root = committed_world
        .execution_state_identity()
        .unwrap()
        .state_root_hash;
    let post_manifest_hash = committed_world.current_manifest_hash().unwrap();
    let projection_root = crate::formal_projection::commitments_for_world(&committed_world)
        .unwrap()
        .root()
        .unwrap();
    let h_result_root = h_tree.tree.root().unwrap();
    let result_update =
        FormalBusinessResultTreeV1::extend_bft_successor(FormalBftResultSuccessorInputV1 {
            world: &committed_world,
            identity: &identity,
            height: 3,
            parent_result_root: &h_result_root,
            execution_state_root: &execution_state_root,
            previous: &h_tree.tree,
            new_results: &[],
            reader: &index,
        })
        .unwrap();

    let mut block = BftBlockV2 {
        protocol_version: BFT_PROTOCOL_VERSION_V2,
        scope: target.scope().clone(),
        height: 3,
        parent_commit_ref: h_anchor.head().head().record_id.clone(),
        authority_epoch: target.authority_epoch(),
        active_validator_set_hash: target.initial_validator_set_hash().unwrap(),
        schedule_policy_hash: target.schedule_policy_hash().unwrap(),
        manifest_hash: h_world_manifest_hash.clone(),
        post_manifest_hash: post_manifest_hash.clone(),
        actions_cbor: vec![0x80],
        action_root: fixtures::h(71),
        event_root: fixtures::h(72),
        execution_block_hash: fixtures::h(73),
        execution_state_root: execution_state_root.clone(),
        state_root: state_root.clone(),
        transition: None,
        transition_ref: None,
    };
    let commitment = FormalBftExecutionCommitmentV1 {
        domain: BFT_FORMAL_EXECUTION_DOMAIN_V1.into(),
        schema_version: 1,
        profile: BFT_FORMAL_PROFILE_V1.into(),
        profile_version: BFT_FORMAL_PROFILE_VERSION_V1,
        activation_digest: handoff_cap.activation_digest().to_owned(),
        scope: target.scope().clone(),
        height: 3,
        parent_commit_ref: h_anchor.head().head().record_id.clone(),
        parent_tick: handoff_cap.handoff_position(),
        parent_semantic_state_root: h_semantic_state_root,
        authority_epoch: target.authority_epoch(),
        active_validator_set_hash: block.active_validator_set_hash.clone(),
        schedule_policy_hash: block.schedule_policy_hash.clone(),
        manifest_hash: block.manifest_hash.clone(),
        post_manifest_hash: post_manifest_hash.clone(),
        action_root: block.action_root.clone(),
        event_root: block.event_root.clone(),
        execution_state_root,
        state_root,
        projection_root: projection_root.clone(),
        result_root: result_update.tree.root().unwrap(),
        transition: None,
        transition_ref: None,
    };
    block.execution_block_hash = commitment.digest().unwrap();
    let schedule = handoff_cap
        .initial_schedule_for_commitment(&commitment, &block)
        .unwrap();
    let proposer = schedule.expected_proposer_for_round(0).unwrap();
    let block_id = block.block_id().unwrap();
    let proposal = BftProposalV2 {
        header: BftProposalHeaderV2::sign(
            target.scope().clone(),
            3,
            0,
            schedule.active_set().set_hash().to_owned(),
            schedule.policy_hash().to_owned(),
            proposer,
            block_id.clone(),
            None,
            &signing_key,
        )
        .unwrap(),
        block,
        valid_round_evidence: None,
    };
    let precommit = BftVoteV2::sign(
        target.scope().clone(),
        3,
        0,
        BftVotePhaseV2::Precommit,
        schedule.active_set().set_hash().to_owned(),
        schedule.policy_hash().to_owned(),
        BftVoteValueV2::Block {
            block_id: block_id.clone(),
        },
        "validator-a".into(),
        &signing_key,
    )
    .unwrap();
    let certificate = BftCommitCertificateV2 {
        proposal: Box::new(proposal),
        precommits: vec![precommit],
    };
    let exact_target = FormalBftReadTargetV2 {
        height: 3,
        block_id,
    };
    let publication = FormalBftReadPublicationV2::new(FormalBftReadPublicationInputV2 {
        handoff_anchor: h_anchor.clone(),
        target_config: target.clone(),
        exact_target: exact_target.clone(),
        certificate: &certificate,
        commitment: &commitment,
        committed_world: &committed_world,
        new_results: &[],
        h_result_tree: &h_tree.tree,
        h_result_index_reader: &index,
    })
    .unwrap();

    Fixture {
        publication,
        anchor: h_anchor,
        target,
        exact_target,
    }
}

fn signed_request(
    verifier: &oasis7_game_client::FormalBftReadVerifierV2,
    scope_id: &str,
) -> oasis7_client_api::world_service::SignedReadRequest<FormalBftReadRequestV2> {
    let request = verifier.request(scope_id, [91; 32]).unwrap();
    authority::sign_read_request(FORMAL_BFT_READ_PATH_V2, request, &fixtures::h(77)).unwrap()
}

#[test]
fn exact_bft_publication_roundtrips_through_signed_request_and_client_verifier() {
    let fixture = fixture();
    let verifier = oasis7_game_client::FormalBftReadVerifierV2::new(
        fixture.anchor.clone(),
        fixture.target.clone(),
        fixture.exact_target.clone(),
    )
    .unwrap();
    let signed = signed_request(&verifier, "public");
    let response = fixture.publication.read(&signed, None).unwrap();
    let verified = verifier.verify_response(&signed, response).unwrap();
    assert_eq!(
        verified.certificate().block_id(),
        verified.request().exact_target.block_id
    );
    assert_eq!(
        verified.commitment().projection_root(),
        verified.response().head.projection_root
    );
    assert_eq!(verified.view().state.time, 3);

    let private = signed_request(&verifier, "agent:missing-agent");
    assert!(fixture.publication.read(&private, None).is_err());
}

#[test]
fn bft_client_rejects_projection_audience_detached_from_signed_scope() {
    let fixture = fixture();
    let verifier = oasis7_game_client::FormalBftReadVerifierV2::new(
        fixture.anchor.clone(),
        fixture.target.clone(),
        fixture.exact_target.clone(),
    )
    .unwrap();
    let signed = signed_request(&verifier, "public");
    let mut response = fixture.publication.read(&signed, None).unwrap();
    response.projection.audience = oasis7_client_api::world_service::FormalProjectionAudienceV1 {
        scope_id: "agent:other".into(),
        reader_public_key: Some(fixtures::public(77)),
    };
    assert!(verifier.verify_response(&signed, response).is_err());
}

#[test]
fn private_bft_read_rejects_current_authorization_world_from_another_chain() {
    let fixture = fixture();
    let verifier = oasis7_game_client::FormalBftReadVerifierV2::new(
        fixture.anchor.clone(),
        fixture.target.clone(),
        fixture.exact_target.clone(),
    )
    .unwrap();
    let signed = signed_request(&verifier, "agent:missing-agent");
    let committed_world = &fixture.publication.committed_world;
    let identity = fixture.anchor.head().record().world.clone();
    let other_chain_snapshot = committed_world.snapshot_with_chain_resource_context(
        oasis7_runtime::ChainResourceDerivationContext {
            world_id: &identity.world_id,
            chain_id: "another-chain-with-the-same-world-and-genesis",
            genesis_ref: Some(&identity.genesis_digest),
            created_at_height: 0,
            manifest_height: 0,
            commit_block_hash: None,
            tick: committed_world.state().time,
        },
        fixtures::h(10),
        fixtures::h(11),
    );
    let other_chain_world =
        World::from_snapshot(other_chain_snapshot, committed_world.journal().clone()).unwrap();

    let error = fixture
        .publication
        .read(&signed, Some(&other_chain_world))
        .unwrap_err();
    assert!(matches!(
        error,
        FormalReadError::GrantRejected(message) if message.contains("identity differs")
    ));
}
