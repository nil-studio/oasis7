use super::super::BftSchedulePolicyV2;
use super::{
    BFT_FORMAL_EXECUTION_DOMAIN_V1, BFT_FORMAL_PROFILE_V1, BFT_FORMAL_PROFILE_VERSION_V1,
    BftProfileTargetV1, FormalBftExecutionCommitmentV1, verify_formal_bft_handoff_v1,
};
use crate::controlled_authority::formal::test_fixtures as formal_fixtures;
use crate::controlled_authority::formal::{
    FormalCompletionBranchV1, InitialActivationTrustV1, ProfileHandoffV1,
    VerifiedFormalReadAnchorV1, verify_formal_qualified,
};
use ed25519_dalek::SigningKey;
use oasis7_proto::distributed_bft::v2::{
    BFT_MAX_SIGNED_MATERIAL_BYTES_V2, BFT_PROTOCOL_VERSION_V2, BftBlockV2, BftScopeV2,
    BftValidatorV2,
};

#[path = "formal_prefix_tests.rs"]
mod prefix_tests;

fn hash(byte: u8) -> String {
    hex::encode([byte; 32])
}

fn target() -> BftProfileTargetV1 {
    let scope = BftScopeV2 {
        world_id: "world".into(),
        chain_id: "chain".into(),
        genesis_hash: hash(8),
    };
    let key = SigningKey::from_bytes(&[31; 32]);
    BftProfileTargetV1::new(
        scope,
        2,
        vec![BftValidatorV2 {
            validator_id: "validator-a".into(),
            public_key_hex: hex::encode(key.verifying_key().to_bytes()),
            voting_power: 7,
        }],
        BftSchedulePolicyV2::new(4, BFT_FORMAL_PROFILE_V1).unwrap(),
    )
    .unwrap()
}

fn qualified_handoff(target: &BftProfileTargetV1) -> (VerifiedFormalReadAnchorV1, String, String) {
    let (certificate, authority, initial_proof, initial_commit) = formal_fixtures::initial();
    let trust = InitialActivationTrustV1 {
        issuer_public_key: formal_fixtures::public(9),
        world: certificate.body.authority.world.clone(),
        initial_state_root: certificate.body.initial_state_root.clone(),
        governing_manifest_digest: certificate.body.governing_manifest_digest.clone(),
        recovery_policy_digest: certificate.body.recovery_policy.digest().unwrap(),
    };
    let initial_anchor =
        VerifiedFormalReadAnchorV1::from_initial(&certificate, &trust, &initial_proof).unwrap();
    let target_digest = target.target_digest().unwrap();
    let handoff = ProfileHandoffV1 {
        next_profile: BFT_FORMAL_PROFILE_V1.into(),
        next_profile_version: BFT_FORMAL_PROFILE_VERSION_V1,
        next_authority_digest: target_digest,
        valid_from_position: 3,
    };
    let frozen = formal_fixtures::frozen(&authority, &initial_commit, Some(handoff));
    let record = formal_fixtures::execution(&authority, &frozen);
    let proof = formal_fixtures::proof(
        record.clone(),
        &authority,
        FormalCompletionBranchV1::Normal,
        None,
        Some(frozen),
        4,
        5,
        6,
    );
    let h_commit =
        verify_formal_qualified(&proof, &authority, Some(&initial_commit), None).unwrap();
    let h_id = h_commit.head().record_id;
    let anchor = initial_anchor
        .after_verified_commit(authority, h_commit)
        .unwrap();
    (anchor, record.after_state_root, h_id)
}

fn commitment(
    handoff: &super::VerifiedBftHandoffTargetV1,
    block: &BftBlockV2,
) -> FormalBftExecutionCommitmentV1 {
    FormalBftExecutionCommitmentV1 {
        domain: BFT_FORMAL_EXECUTION_DOMAIN_V1.into(),
        schema_version: 1,
        profile: BFT_FORMAL_PROFILE_V1.into(),
        profile_version: BFT_FORMAL_PROFILE_VERSION_V1,
        activation_digest: handoff.activation_digest().to_owned(),
        scope: block.scope.clone(),
        height: block.height,
        parent_commit_ref: block.parent_commit_ref.clone(),
        parent_tick: handoff.handoff_position(),
        parent_semantic_state_root: hash(101),
        authority_epoch: block.authority_epoch,
        active_validator_set_hash: block.active_validator_set_hash.clone(),
        schedule_policy_hash: block.schedule_policy_hash.clone(),
        manifest_hash: block.manifest_hash.clone(),
        post_manifest_hash: block.post_manifest_hash.clone(),
        action_root: block.action_root.clone(),
        event_root: block.event_root.clone(),
        execution_state_root: block.execution_state_root.clone(),
        state_root: block.state_root.clone(),
        projection_root: hash(111),
        result_root: hash(112),
        transition: None,
        transition_ref: None,
    }
}

fn first_block(
    handoff: &super::VerifiedBftHandoffTargetV1,
    target: &BftProfileTargetV1,
) -> BftBlockV2 {
    BftBlockV2 {
        protocol_version: BFT_PROTOCOL_VERSION_V2,
        scope: target.scope().clone(),
        height: handoff.activation_height().unwrap(),
        parent_commit_ref: handoff.handoff_head().record_id.clone(),
        authority_epoch: target.authority_epoch(),
        active_validator_set_hash: target.initial_validator_set_hash().unwrap(),
        schedule_policy_hash: target.schedule_policy_hash().unwrap(),
        manifest_hash: handoff.governing_manifest_digest().to_owned(),
        post_manifest_hash: hash(104),
        actions_cbor: vec![0x80],
        action_root: hash(105),
        event_root: hash(106),
        execution_block_hash: hash(107),
        execution_state_root: hash(108),
        state_root: hash(109),
        transition: None,
        transition_ref: None,
    }
}

fn cap_and_target() -> (
    super::VerifiedBftHandoffTargetV1,
    BftProfileTargetV1,
    String,
) {
    let target = target();
    let (anchor, raw_snapshot_root, h_id) = qualified_handoff(&target);
    let cap = verify_formal_bft_handoff_v1(&anchor, target.clone()).unwrap();
    assert_eq!(cap.handoff_head().record_id, h_id);
    assert_eq!(cap.raw_snapshot_root(), raw_snapshot_root);
    (cap, target, raw_snapshot_root)
}

#[test]
fn qualified_handoff_derives_h_plus_one_schedule_from_formal_preimage() {
    let (cap, target, raw_snapshot_root) = cap_and_target();
    assert_eq!(
        target.canonical_bytes().unwrap(),
        oasis7_cas::to_canonical_cbor(&target).unwrap(),
        "moved target bytes remain identical to the historical CAS encoder"
    );
    assert_eq!(
        target.target_digest().unwrap(),
        "cd82956be58f748793a19a29f675b298dda09290fa91b7a9bfaac39e863990b6"
    );
    assert_eq!(
        cap.activation_digest(),
        "cc4e2d3c96fba8c1310da86e672197ca4788577ac8dcf343eb2c7fc126cbb04f"
    );
    let replayed_schedule = cap
        .initial_schedule_after_handoff_replay(&hash(101), cap.governing_manifest_digest())
        .unwrap();
    assert_eq!(replayed_schedule.height(), cap.activation_height().unwrap());
    assert_eq!(replayed_schedule.parent_state_root(), hash(101));
    assert!(
        cap.initial_schedule_after_handoff_replay(&hash(101), "invalid-digest")
            .is_err()
    );

    let mut block = first_block(&cap, &target);
    let preimage = commitment(&cap, &block);
    assert_eq!(
        preimage.canonical_bytes().unwrap(),
        oasis7_cas::to_canonical_cbor(&preimage).unwrap(),
        "moved commitment bytes remain identical to the historical CAS encoder"
    );
    assert_eq!(
        preimage.digest().unwrap(),
        "4f4753e84bf53a0dee43c15f5feba5bbff045dc248525c766a0d9dfbd9e1faf6"
    );
    block.execution_block_hash = preimage.digest().unwrap();
    let schedule = cap
        .initial_schedule_for_commitment(&preimage, &block)
        .unwrap();

    assert_eq!(schedule.height(), 3);
    assert_eq!(schedule.parent_tick(), 2);
    assert_eq!(schedule.parent_commit_ref(), cap.handoff_head().record_id);
    assert_eq!(schedule.parent_state_root(), hash(101));
    assert_eq!(
        schedule.parent_manifest_hash(),
        cap.governing_manifest_digest()
    );
    assert_ne!(
        schedule.parent_state_root(),
        raw_snapshot_root,
        "formal raw Snapshot root must not be confused with semantic World root"
    );
}

#[test]
fn handoff_rejects_a_different_target_or_unsupported_mapping() {
    let target = target();
    let (anchor, _, _) = qualified_handoff(&target);

    let mut different_target = target.clone();
    different_target.authority_epoch += 1;
    assert!(
        verify_formal_bft_handoff_v1(&anchor, different_target).is_err(),
        "H must reject a different target digest"
    );

    let mut unsupported_mapping = target.clone();
    unsupported_mapping.tick_mapping_ruleset = "position-is-not-world-time-v1".into();
    assert!(
        verify_formal_bft_handoff_v1(&anchor, unsupported_mapping).is_err(),
        "the first implementation only accepts the explicit aligned mapping"
    );

    let mut unsupported_profile = target.clone();
    unsupported_profile.profile_version += 1;
    assert!(verify_formal_bft_handoff_v1(&anchor, unsupported_profile).is_err());

    let mut wrong_scope = target;
    wrong_scope.scope.world_id = "different-world".into();
    assert!(verify_formal_bft_handoff_v1(&anchor, wrong_scope).is_err());
}

#[test]
fn formal_target_rejects_schedule_policy_for_another_profile() {
    let mut target = target();
    target.schedule_policy.profile_id = "engineering".into();

    assert!(target.validate().is_err());
    assert!(target.canonical_bytes().is_err());
}

#[test]
fn target_and_execution_preimages_require_canonical_bounded_closed_cbor() {
    let target = target();
    let target_bytes = target.canonical_bytes().unwrap();
    assert_eq!(
        BftProfileTargetV1::decode_canonical(&target_bytes).unwrap(),
        target
    );
    assert!(
        BftProfileTargetV1::decode_canonical(&[target_bytes.as_slice(), &[0]].concat()).is_err()
    );
    assert!(
        BftProfileTargetV1::decode_canonical(&vec![0; BFT_MAX_SIGNED_MATERIAL_BYTES_V2 + 1])
            .is_err()
    );
    let mut target_with_unknown: serde_cbor::Value = serde_cbor::value::to_value(&target).unwrap();
    let serde_cbor::Value::Map(entries) = &mut target_with_unknown else {
        panic!("profile target must encode as a CBOR map");
    };
    entries.insert(
        serde_cbor::Value::Text("unexpected".into()),
        serde_cbor::Value::Integer(1),
    );
    let unknown_target_bytes = oasis7_cas::to_canonical_cbor(&target_with_unknown).unwrap();
    assert!(BftProfileTargetV1::decode_canonical(&unknown_target_bytes).is_err());

    let (cap, target, _) = cap_and_target();
    let mut block = first_block(&cap, &target);
    let preimage = commitment(&cap, &block);
    block.execution_block_hash = preimage.digest().unwrap();
    let bytes = preimage.canonical_bytes().unwrap();
    assert_eq!(
        FormalBftExecutionCommitmentV1::decode_canonical(&bytes).unwrap(),
        preimage
    );
    assert!(
        FormalBftExecutionCommitmentV1::decode_canonical(&[bytes.as_slice(), &[0]].concat())
            .is_err()
    );
    assert!(
        FormalBftExecutionCommitmentV1::decode_canonical(&vec![
            0;
            super::BFT_FORMAL_EXECUTION_COMMITMENT_MAX_BYTES_V1
                + 1
        ])
        .is_err()
    );

    let mut unknown: serde_cbor::Value = serde_cbor::value::to_value(&preimage).unwrap();
    let serde_cbor::Value::Map(entries) = &mut unknown else {
        panic!("commitment preimage must encode as a CBOR map");
    };
    entries.insert(
        serde_cbor::Value::Text("unexpected".into()),
        serde_cbor::Value::Integer(1),
    );
    let unknown_bytes = oasis7_cas::to_canonical_cbor(&unknown).unwrap();
    assert!(FormalBftExecutionCommitmentV1::decode_canonical(&unknown_bytes).is_err());
}

#[test]
fn formal_commitment_rejects_engineering_domain_and_changed_block_roots() {
    let (cap, target, _) = cap_and_target();
    let mut block = first_block(&cap, &target);
    let preimage = commitment(&cap, &block);
    block.execution_block_hash = preimage.digest().unwrap();
    preimage
        .verify_block(&block, cap.activation_digest())
        .unwrap();

    let mut engineering = preimage.clone();
    engineering.domain = "oasis7.bft.execution.v2".into();
    let engineering_bytes = oasis7_cas::to_canonical_cbor(&engineering).unwrap();
    assert!(
        FormalBftExecutionCommitmentV1::decode_canonical(&engineering_bytes).is_err(),
        "engineering execution commitments must not be reinterpreted as formal profile commitments"
    );

    let mut wrong_output_root = block;
    wrong_output_root.state_root = hash(113);
    assert!(
        preimage
            .verify_block(&wrong_output_root, cap.activation_digest())
            .is_err()
    );
}

#[test]
fn first_formal_schedule_rejects_wrong_handoff_cursor_and_parent_bindings() {
    let (cap, target, _) = cap_and_target();

    let mut block = first_block(&cap, &target);
    let mut preimage = commitment(&cap, &block);
    preimage.parent_tick = 1;
    preimage.height = 2;
    block.height = 2;
    block.execution_block_hash = preimage.digest().unwrap();
    assert!(
        cap.initial_schedule_for_commitment(&preimage, &block)
            .is_err()
    );

    let mut block = first_block(&cap, &target);
    let preimage = commitment(&cap, &block);
    block.manifest_hash = hash(118);
    block.execution_block_hash = preimage.digest().unwrap();
    assert!(
        cap.initial_schedule_for_commitment(&preimage, &block)
            .is_err()
    );

    let mut block = first_block(&cap, &target);
    let mut preimage = commitment(&cap, &block);
    preimage.parent_commit_ref = hash(119);
    block.parent_commit_ref = preimage.parent_commit_ref.clone();
    block.execution_block_hash = preimage.digest().unwrap();
    assert!(
        cap.initial_schedule_for_commitment(&preimage, &block)
            .is_err()
    );

    let mut block = first_block(&cap, &target);
    let mut preimage = commitment(&cap, &block);
    preimage.schedule_policy_hash = hash(121);
    block.schedule_policy_hash = preimage.schedule_policy_hash.clone();
    block.execution_block_hash = preimage.digest().unwrap();
    assert!(
        cap.initial_schedule_for_commitment(&preimage, &block)
            .is_err()
    );

    let mut block = first_block(&cap, &target);
    let mut preimage = commitment(&cap, &block);
    preimage.activation_digest = hash(120);
    block.execution_block_hash = preimage.digest().unwrap();
    assert!(
        cap.initial_schedule_for_commitment(&preimage, &block)
            .is_err()
    );
}

#[test]
fn commitment_transition_must_bind_parent_semantic_root() {
    let (cap, target, _) = cap_and_target();
    let block = first_block(&cap, &target);
    let mut preimage = commitment(&cap, &block);
    preimage.transition = Some(
        oasis7_proto::distributed_bft::v2::BftValidatorTransitionV2 {
            source_commit_ref: preimage.parent_commit_ref.clone(),
            source_state_root: hash(114),
            source_profile: BFT_FORMAL_PROFILE_V1.into(),
            source_epoch: 0,
            target_epoch: 1,
            epoch_length_ticks: 4,
            mapping_ruleset_hash: hash(115),
            scheduled_height: 2,
            activation_height: 4,
            old_validator_set_hash: block.active_validator_set_hash.clone(),
            new_validators: target.initial_validators().to_vec(),
            new_validator_set_hash: target.initial_validator_set_hash().unwrap(),
            governance_action_digest: hash(116),
            projected_registry_digest: hash(117),
        },
    );
    assert!(preimage.validate().is_err());
}

#[test]
fn activation_digest_is_deterministic_for_the_same_qualified_h_and_target() {
    let target = target();
    let (anchor, _, _) = qualified_handoff(&target);
    let first = verify_formal_bft_handoff_v1(&anchor, target.clone()).unwrap();
    let second = verify_formal_bft_handoff_v1(&anchor, target).unwrap();
    assert_eq!(first.activation_digest(), second.activation_digest());
    assert_eq!(first.target_digest(), second.target_digest());
    assert_eq!(
        first.governing_manifest_digest(),
        second.governing_manifest_digest()
    );
}

#[test]
fn formal_handoff_keeps_runtime_governing_and_world_manifest_digests_distinct() {
    let (cap, target, _) = cap_and_target();
    let world_manifest = hash(119);
    assert_ne!(world_manifest, cap.governing_manifest_digest());
    let schedule = cap
        .initial_schedule_after_handoff_replay(&hash(101), &world_manifest)
        .unwrap();
    assert_eq!(schedule.parent_manifest_hash(), world_manifest);
    let mut block = first_block(&cap, &target);
    block.manifest_hash = world_manifest.clone();
    let preimage = commitment(&cap, &block);
    block.execution_block_hash = preimage.digest().unwrap();
    assert_eq!(
        cap.initial_schedule_for_commitment(&preimage, &block)
            .unwrap()
            .parent_manifest_hash(),
        world_manifest
    );
}
