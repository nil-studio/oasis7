//! Actual private ledger acceptance is separate from permission to disclose
//! original action bytes in a public or single-Agent light-read proof.
use super::prefix_ledger_tests::open_prefix;
use super::*;
use oasis7::consensus_action_payload::{
    ConsensusActionPayloadEnvelope, encode_consensus_action_payload,
};
use oasis7::world_service::formal_bft_prefix_read_service::{
    FormalBftReadPublicationInputV3, FormalBftReadPublicationV3,
};
use oasis7::world_service::formal_bft_read_service::{
    FormalBftReadPublicationInputV2, FormalBftReadPublicationV2,
};
use oasis7::world_service::formal_read_error::FormalReadError;
use oasis7_client_api::world_service::*;
use oasis7_node::NodeConsensusAction;

#[test]
fn formal_prefix_light_read_refuses_original_actions_even_with_valid_qc_and_world() {
    std::thread::Builder::new()
        .stack_size(32 * 1024 * 1024)
        .spawn(action_case)
        .unwrap()
        .join()
        .unwrap();
}
fn action_case() {
    let h = qualified_handoff();
    let builder = Scratch::new();
    let mut execution = open_formal_read(&h, &builder, h.reader.clone(), true).unwrap();
    let action = NodeConsensusAction::from_payload(
        1,
        "v0",
        encode_consensus_action_payload(&ConsensusActionPayloadEnvelope::from_runtime_action(
            Action::RegisterAgent {
                agent_id: "original-input-cannot-be-declassified-by-projection".into(),
                pos: oasis7::geometry::GeoPos::new(1, 2, 3),
            },
        ))
        .unwrap(),
    )
    .unwrap();
    let block = execution.build_candidate(vec![action]).unwrap();
    let executed = execution.execute_candidate(&block).unwrap();
    assert!(
        executed
            .world
            .state()
            .agents
            .contains_key("original-input-cannot-be-declassified-by-projection")
    );
    let certificate = certificate(block, execution.schedule());
    let prefix = oasis7_formal_protocol::bft::FormalBftCertifiedPrefixV1 {
        schema_version: 1,
        entries: vec![oasis7_formal_protocol::bft::FormalBftCertifiedEntryV1 {
            certificate: certificate.clone(),
            commitment: executed.commitment.clone(),
        }],
    };
    prefix.verify(&execution.handoff).unwrap();
    let old_cut = execution.commit_certificate(certificate.clone()).unwrap();
    let height = certificate.proposal.block.height;
    let block_id = certificate.proposal.block.block_id().unwrap();
    let old_result = FormalBftReadPublicationV2::new(FormalBftReadPublicationInputV2 {
        handoff_anchor: h.anchor.clone(),
        target_config: h.target.clone(),
        exact_target: FormalBftReadTargetV2 {
            height,
            block_id: block_id.clone(),
        },
        certificate: old_cut.certificate(),
        commitment: old_cut.commitment(),
        committed_world: old_cut.world(),
        new_results: old_cut.new_results(),
        h_result_tree: old_cut.h_result_tree(),
        h_result_index_reader: old_cut.h_index_reader(),
    });
    assert!(
        matches!(old_result,Err(FormalReadError::NotReady(ref reason)) if reason.contains("private-input-safe"))
    );
    let consumer = Scratch::new();
    let cut = open_prefix(&h, &consumer, h.reader.clone(), true)
        .unwrap()
        .install_prefix(&prefix, height, &block_id)
        .unwrap();
    let exact = FormalBftReadTargetV3 { height, block_id };
    let result = FormalBftReadPublicationV3::new(FormalBftReadPublicationInputV3 {
        handoff_anchor: h.anchor.clone(),
        target_config: h.target.clone(),
        exact_target: exact.clone(),
        certified_prefix: cut.certified_prefix(),
        committed_world: cut.world(),
        result_tree: cut.result_tree(),
    });
    assert!(
        matches!(result,Err(FormalReadError::NotReady(ref reason)) if reason.contains("private-input-safe"))
    );

    // Independently construct an otherwise valid QC/projection page to prove
    // the receiver also refuses to label raw-action evidence a safe light page.
    let verifier = oasis7_game_client::FormalBftReadVerifierV3::new(
        h.anchor.clone(),
        h.target.clone(),
        exact.clone(),
    )
    .unwrap();
    let request = verifier.request("public", [42; 32]).unwrap();
    let reader = fixtures::key(120);
    let signed = oasis7::world_service::authority::sign_read_request(
        FORMAL_BFT_READ_PATH_V3,
        request.clone(),
        &hex::encode(reader.to_bytes()),
    )
    .unwrap();
    let audience = oasis7::world_service::formal_projection::ProjectionAudienceV1::public();
    let tree =
        oasis7::world_service::formal_projection::commitments_for_world(cut.world()).unwrap();
    let inclusion = tree.inclusion(&audience).unwrap();
    let view = oasis7::world_service::projection::from_formal_committed_world(
        cut.world(),
        None,
        &signed.subject_public_key,
    )
    .unwrap();
    inclusion
        .verify(&audience, &view, cut.proof().commitment().projection_root())
        .unwrap();
    let block = cut.proof().certificate().block();
    let response = FormalBftReadResponseV3 {
        schema_version: 3,
        contract_version: 3,
        profile: FormalBftReadProfileV3::bft_v1(),
        world: request.world.clone(),
        scope_id: request.scope_id.clone(),
        observation_nonce: request.observation_nonce.clone(),
        handoff_anchor: request.handoff_anchor.clone(),
        target_config_digest: request.target_config_digest.clone(),
        activation_digest: request.activation_digest.clone(),
        exact_target: exact,
        head: FormalBftReadHeadV3 {
            height,
            block_id: cut.proof().certificate().block_id().into(),
            execution_block_hash: block.execution_block_hash.clone(),
            state_root: block.state_root.clone(),
            post_manifest_hash: block.post_manifest_hash.clone(),
            projection_root: cut.proof().commitment().projection_root().into(),
            logical_tick: height,
        },
        certified_prefix_cbor_hex: hex::encode(prefix.canonical_bytes().unwrap()),
        projection: FormalProjectionInclusionV1 {
            schema_version: inclusion.schema_version,
            audience: FormalProjectionAudienceV1 {
                scope_id: inclusion.audience.scope_id,
                reader_public_key: inclusion.audience.reader_public_key,
            },
            leaf_index: inclusion.leaf_index,
            leaf_count: inclusion.leaf_count,
            siblings: inclusion.siblings,
        },
        view,
    };
    response.validate_structure(&request).unwrap();
    let error = verifier
        .verify_response(&signed, response)
        .err()
        .expect("raw actions must not acquire a light-read capability");
    assert!(error.contains("private-input-safe"));
}
