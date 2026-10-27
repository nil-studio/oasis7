//! V3 public block identity is bound to a real execution commitment and the
//! exact private action bytes without exposing those bytes in public metadata.
use super::*;
use oasis7::consensus_action_payload::{
    ConsensusActionPayloadEnvelope, encode_consensus_action_payload,
};
use oasis7_consensus::formal_bft_payload_v3::ValidatedBftPrivatePayloadV3;
use oasis7_formal_protocol::bft::{BoundFormalBftHeaderV3, FormalBftExecutionCommitmentV1};
use oasis7_node::NodeConsensusAction;
use oasis7_proto::distributed_bft::v3::{BftBlockHeaderV3, actions_cbor_digest_v3};

const PRIVATE_CANARY: &str = "private-header-canary-4975-never-public";

#[test]
fn formal_bft_header_identity_v3_binds_actual_nonempty_execution_and_payload() {
    std::thread::Builder::new()
        .name("formal-bft-header-identity".into())
        .stack_size(32 * 1024 * 1024)
        .spawn(header_identity_case)
        .unwrap()
        .join()
        .unwrap();
}

fn header_identity_case() {
    let h = qualified_handoff();
    let scratch = Scratch::new();
    let execution = open_formal_read(&h, &scratch, h.reader.clone(), true).unwrap();
    let private_action_cbor = encode_consensus_action_payload(
        &ConsensusActionPayloadEnvelope::from_runtime_action(Action::RegisterAgent {
            agent_id: PRIVATE_CANARY.into(),
            pos: oasis7::geometry::GeoPos::new(7, 8, 9),
        }),
    )
    .unwrap();
    assert!(
        private_action_cbor
            .windows(PRIVATE_CANARY.len())
            .any(|part| part == PRIVATE_CANARY.as_bytes())
    );
    let action =
        NodeConsensusAction::from_payload(1, "v3-private", private_action_cbor.clone()).unwrap();
    let block_v2 = execution.build_candidate(vec![action]).unwrap();
    let executed = execution.execute_candidate(&block_v2).unwrap();
    assert!(executed.world.state().agents.contains_key(PRIVATE_CANARY));
    assert_eq!(executed.commitment.action_root(), block_v2.action_root);
    assert_eq!(
        executed.commitment.digest().unwrap(),
        block_v2.execution_block_hash
    );

    let header = BftBlockHeaderV3 {
        protocol_version: 3,
        execution_commitment_hash: executed.commitment.digest().unwrap(),
        actions_cbor_digest: actions_cbor_digest_v3(&block_v2.actions_cbor).unwrap(),
    };
    let header_id = header.block_id().unwrap();
    let bound = BoundFormalBftHeaderV3::bind(header.clone(), executed.commitment.clone())
        .expect("bind public header to the executed preimage");
    let validated = ValidatedBftPrivatePayloadV3::decode(bound.clone(), &block_v2.actions_cbor)
        .expect("accept exact canonical private action bytes");
    assert_eq!(validated.actions().len(), 1);
    assert_eq!(
        validated.actions()[0].payload_cbor,
        private_action_cbor,
        "validated private payload must retain the exact executed input"
    );

    let header_cbor = header.canonical_bytes().unwrap();
    let header_json = serde_json::to_vec(&header).unwrap();
    assert!(!contains_bytes(&header_cbor, PRIVATE_CANARY.as_bytes()));
    assert!(!contains_bytes(&header_json, PRIVATE_CANARY.as_bytes()));
    assert!(!format!("{header:?}").contains(PRIVATE_CANARY));
    assert!(!format!("{bound:?}").contains(PRIVATE_CANARY));
    assert!(!format!("{validated:?}").contains(PRIVATE_CANARY));

    let mut tampered_payload = block_v2.actions_cbor.clone();
    *tampered_payload.last_mut().unwrap() ^= 1;
    let payload_error = header
        .verify_private_payload(&tampered_payload)
        .unwrap_err();
    assert!(payload_error.contains("private payload digest mismatch"));
    assert!(ValidatedBftPrivatePayloadV3::decode(bound.clone(), &tampered_payload).is_err());

    let mut wrong_action_root = executed.commitment.clone();
    wrong_action_root.action_root = if wrong_action_root.action_root == "00".repeat(32) {
        "01".repeat(32)
    } else {
        "00".repeat(32)
    };
    let action_root_header = header_for(&wrong_action_root, &block_v2.actions_cbor);
    let action_root_bound = BoundFormalBftHeaderV3::bind(action_root_header, wrong_action_root)
        .expect("the public header consistently binds the altered preimage");
    let action_root_error =
        ValidatedBftPrivatePayloadV3::decode(action_root_bound, &block_v2.actions_cbor)
            .unwrap_err();
    assert!(action_root_error.contains("private action root mismatch"));

    // A matching raw-byte digest does not make an ignored typed field canonical.
    let mut unknown_field_payload: serde_cbor::Value =
        serde_cbor::from_slice(&block_v2.actions_cbor).unwrap();
    let serde_cbor::Value::Array(encoded_actions) = &mut unknown_field_payload else {
        panic!("candidate actions are a CBOR sequence");
    };
    let serde_cbor::Value::Map(action_fields) = &mut encoded_actions[0] else {
        panic!("candidate action is a CBOR map");
    };
    action_fields.insert(
        serde_cbor::Value::Text("ignored_unknown_field".into()),
        serde_cbor::Value::Bool(true),
    );
    let unknown_field_bytes = serde_cbor::to_vec(&unknown_field_payload).unwrap();
    let unknown_field_header = header_for(&executed.commitment, &unknown_field_bytes);
    let unknown_field_bound =
        BoundFormalBftHeaderV3::bind(unknown_field_header, executed.commitment.clone()).unwrap();
    let canonicality_error =
        ValidatedBftPrivatePayloadV3::decode(unknown_field_bound, &unknown_field_bytes)
            .unwrap_err();
    assert!(canonicality_error.contains("noncanonical BFT private actions"));

    // The byte digest is also consistent for malformed typed action semantics.
    let mut invalid_payload_hash_actions: Vec<NodeConsensusAction> =
        serde_cbor::from_slice(&block_v2.actions_cbor).unwrap();
    invalid_payload_hash_actions[0].payload_cbor[0] ^= 1;
    let invalid_payload_hash_bytes = serde_cbor::to_vec(&invalid_payload_hash_actions).unwrap();
    let invalid_payload_hash_header = header_for(&executed.commitment, &invalid_payload_hash_bytes);
    let invalid_payload_hash_bound =
        BoundFormalBftHeaderV3::bind(invalid_payload_hash_header, executed.commitment.clone())
            .unwrap();
    let payload_hash_error = ValidatedBftPrivatePayloadV3::decode(
        invalid_payload_hash_bound,
        &invalid_payload_hash_bytes,
    )
    .unwrap_err();
    assert_eq!(
        payload_hash_error,
        "invalid BFT private action ordering or payload"
    );
    assert!(!payload_hash_error.contains(PRIVATE_CANARY));

    let mut duplicate_id_actions: Vec<NodeConsensusAction> =
        serde_cbor::from_slice(&block_v2.actions_cbor).unwrap();
    duplicate_id_actions.push(duplicate_id_actions[0].clone());
    let duplicate_id_bytes = serde_cbor::to_vec(&duplicate_id_actions).unwrap();
    let duplicate_id_header = header_for(&executed.commitment, &duplicate_id_bytes);
    let duplicate_id_bound =
        BoundFormalBftHeaderV3::bind(duplicate_id_header, executed.commitment.clone()).unwrap();
    let duplicate_id_error =
        ValidatedBftPrivatePayloadV3::decode(duplicate_id_bound, &duplicate_id_bytes).unwrap_err();
    assert_eq!(
        duplicate_id_error,
        "invalid BFT private action ordering or payload"
    );
    assert!(!duplicate_id_error.contains(PRIVATE_CANARY));

    let mut wrong_execution_hash = header.clone();
    wrong_execution_hash.execution_commitment_hash =
        if header.execution_commitment_hash == "00".repeat(32) {
            "01".repeat(32)
        } else {
            "00".repeat(32)
        };
    let execution_error =
        BoundFormalBftHeaderV3::bind(wrong_execution_hash, executed.commitment.clone())
            .unwrap_err();
    assert!(execution_error.contains("execution commitment mismatch"));

    // V2 identifies its full block encoding; it is not a V3 header or a
    // quorum proof for this new identity.
    assert_eq!(block_v2.protocol_version, 2);
    assert_ne!(block_v2.block_id().unwrap(), header_id);
    let v2_bytes = serde_cbor::to_vec(&block_v2).unwrap();
    assert!(BftBlockHeaderV3::decode_canonical(&v2_bytes).is_err());
}

fn header_for(
    commitment: &FormalBftExecutionCommitmentV1,
    actions_cbor: &[u8],
) -> BftBlockHeaderV3 {
    BftBlockHeaderV3 {
        protocol_version: 3,
        execution_commitment_hash: commitment.digest().unwrap(),
        actions_cbor_digest: actions_cbor_digest_v3(actions_cbor).unwrap(),
    }
}

fn contains_bytes(haystack: &[u8], needle: &[u8]) -> bool {
    haystack
        .windows(needle.len())
        .any(|window| window == needle)
}
