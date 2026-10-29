use super::*;
fn h(n: u8) -> String {
    format!("{n:02x}").repeat(32)
}
fn body() -> BftCandidateReadBodyV3 {
    BftCandidateReadBodyV3 {
        schema_version: 3,
        activation_digest: h(1),
        target_digest: h(2),
        scope: BftScopeV2 {
            world_id: "read-proof-world".into(),
            chain_id: "chain".into(),
            genesis_hash: h(3),
        },
        validator_id: "validator".into(),
        validator_public_key_hex: hex::encode(
            SigningKey::from_bytes(&[17; 32]).verifying_key().to_bytes(),
        ),
        candidate_policy_digest: h(21),
        candidate_grant_generation: 1,
        authority_epoch: 1,
        active_validator_set_hash: h(4),
        schedule_policy_hash: h(5),
        candidate_height: 9,
        parent_height: 8,
        parent_commit_ref: h(6),
        parent_state_root: h(7),
        parent_manifest_hash: h(8),
        header_id: h(9),
        execution_commitment_hash: h(10),
        requester_peer_id: "QmYwAPJzv5CZsnAzt8auVZRnGiRAkCfE2xJznsUKWK8GWX".into(),
        serving_peer_id: "QmYwAPJzv5CZsnAzt8auVZRnGiRAkCfE2xJznsUKWK8GWX".into(),
        nonce: h(11),
        operation: BftCandidateReadOperationV3::Manifest,
    }
}
#[test]
fn candidate_read_proof_binds_exact_context_and_new_grant_generation() {
    let body = body();
    let key = SigningKey::from_bytes(&[17; 32]);
    let proof = BftCandidateReadProofV3::sign(body.clone(), &key).unwrap();
    proof
        .verify_for(&body, &body.validator_public_key_hex)
        .unwrap();
    for field in [
        "activation_digest",
        "target_digest",
        "candidate_policy_digest",
        "nonce",
        "header_id",
        "validator_public_key_hex",
    ] {
        let mut wrong = serde_json::to_value(&body).unwrap();
        wrong[field] = serde_json::Value::String(h(99));
        let wrong: BftCandidateReadBodyV3 = serde_json::from_value(wrong).unwrap();
        assert!(
            proof
                .verify_for(&wrong, &body.validator_public_key_hex)
                .is_err()
        );
    }
    let mut replacement = body.clone();
    replacement.candidate_grant_generation += 1;
    assert!(
        proof
            .verify_for(&replacement, &body.validator_public_key_hex)
            .is_err()
    );
    let mut forged = proof.clone();
    forged.body = replacement.clone();
    assert!(
        forged
            .verify_for(&replacement, &body.validator_public_key_hex)
            .is_err()
    );
    let mut crossed = proof;
    crossed.signature_hex = hex::encode(
        key.sign(&domain_framed("oasis7.bft.vote.v3.sign", &body).unwrap())
            .to_bytes(),
    );
    assert!(
        crossed
            .verify_for(&body, &body.validator_public_key_hex)
            .is_err()
    );
}
#[test]
fn candidate_read_manifest_cannot_authorize_package_or_out_of_range_chunk() {
    assert!(body().operation.chunk_range(0).is_err());
    let op = BftCandidateReadOperationV3::Package {
        package_digest: h(1),
        total_bytes: 16_385,
        chunk_size: 16_384,
        total_chunks: 2,
    };
    assert_eq!(op.chunk_range(0).unwrap(), (0, 16_384));
    assert_eq!(op.chunk_range(1).unwrap(), (16_384, 1));
    assert!(op.chunk_range(2).is_err());
    for (bytes, size, count) in [
        (0, 1, 0),
        (1, 0, 1),
        (1, 16_385, 1),
        (16_385, 16_384, 1),
        (u64::MAX, 1, 1),
        (BFT_CANDIDATE_READ_MAX_PACKAGE_BYTES_V3 + 1, 16_384, 1),
    ] {
        assert!(
            BftCandidateReadOperationV3::Package {
                package_digest: h(1),
                total_bytes: bytes,
                chunk_size: size,
                total_chunks: count
            }
            .validate()
            .is_err()
        );
    }
}
#[test]
fn candidate_read_structure_never_accepts_private_inputs_or_unbounded_strings() {
    let mut value = serde_json::to_value(body()).unwrap();
    value["actions_cbor"] = serde_json::json!([1, 2, 3]);
    assert!(serde_json::from_value::<BftCandidateReadBodyV3>(value).is_err());
    let mut value = serde_json::to_value(body()).unwrap();
    value["activation_digest"] = serde_json::Value::String("a".repeat(129));
    assert!(serde_json::from_value::<BftCandidateReadBodyV3>(value).is_err());
    let mut value = body();
    value.parent_height = u64::MAX;
    value.candidate_height = 0;
    assert!(value.validate().is_err());
    let mut value = body();
    value.candidate_grant_generation = 0;
    assert!(value.validate().is_err());
    let mut value = body();
    value.requester_peer_id = "self-reported-peer".into();
    assert!(value.validate().is_err());
    let mut value = body();
    value.requester_peer_id = "a".repeat(128);
    assert!(value.validate().is_err());
}
