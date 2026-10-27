//! Protocol-only V3 validator-transition checks.
//!
//! The qualified H fixture is real and commits to the exact V3 target. The
//! post-H commitments and validator transition are synthetic protocol vectors;
//! they do not claim that a World executed or authorized that transition.
use super::*;
use oasis7_formal_protocol::bft::{
    BftHeaderHeightScheduleV3, BftProfileTargetV2, BftValidatorSetV2, verify_formal_bft_handoff_v2,
    verify_formal_bft_header_certificate_v3,
};
use oasis7_proto::distributed_bft::v2::{BftValidatorTransitionV2, BftValidatorV2};
use oasis7_proto::distributed_bft::v3::{
    BftProposalHeaderV3, BftVotePhaseV3, BftVoteV3, BftVoteValueV3,
};
use std::collections::BTreeMap;

#[test]
fn v3_qc_rotation_keeps_old_set_at_h_and_h_plus_one_then_uses_new_keys_at_h_plus_two() {
    run_on_large_test_stack(
        "formal-bft-v3-validator-transition",
        v3_qc_rotation_keeps_old_set_at_h_and_h_plus_one_then_uses_new_keys_at_h_plus_two_case,
    );
}

fn v3_qc_rotation_keeps_old_set_at_h_and_h_plus_one_then_uses_new_keys_at_h_plus_two_case() {
    let qualified = qualified_handoff_with_target_and_setup(
        102,
        |world| {
            let mut policy = world.governance_execution_policy().clone();
            policy.epoch_length_ticks = 3;
            world
                .set_governance_execution_policy(policy)
                .expect("short deterministic epoch fixture");
        },
        |scope, policy| {
            let validators = (0..4)
                .map(|index| validator_with_seed(validator_id(index), 120 + index, 1).0)
                .collect();
            let target = BftProfileTargetV2::new(scope, 1, validators, policy)?;
            let handoff = ProfileHandoffV1 {
                next_profile: target.profile.clone(),
                next_profile_version: target.profile_version,
                next_authority_digest: target.target_digest()?,
                valid_from_position: 3,
            };
            Ok((target, handoff))
        },
    );
    let handoff = verify_formal_bft_handoff_v2(&qualified.anchor, qualified.target.clone())
        .expect("H signed this exact V3 target");
    let activation_schedule = initial_schedule(&handoff);
    assert_eq!(activation_schedule.height(), 3);
    assert_eq!(activation_schedule.policy().epoch_length_ticks, 3);

    let first_post_h_entry = signed_entry(
        &activation_schedule,
        commitment_for(&activation_schedule, &handoff, 0),
        0,
        None,
        &[0, 1, 2],
        None,
    );
    let first_post_h_token =
        verify_formal_bft_header_certificate_v3(&activation_schedule, &first_post_h_entry)
            .expect("first V3 QC extends the qualified H schedule");
    let transition_h_schedule = first_post_h_token.next_height_schedule().clone();
    assert_eq!(transition_h_schedule.height(), 4);
    assert_eq!(
        transition_h_schedule.active_set().set_hash(),
        activation_schedule.active_set().set_hash()
    );

    let new_validators = vec![
        validator_with_seed("validator-2".into(), 121, 1).0,
        validator_with_seed("validator-3".into(), 122, 1).0,
        validator_with_seed("validator-4".into(), 140, 1).0,
        validator_with_seed("validator-5".into(), 141, 1).0,
    ];
    let transition = transition_for(&transition_h_schedule, new_validators);
    assert_eq!(transition.scheduled_height, transition_h_schedule.height());
    let expected_activation = transition.activation_height;
    assert_eq!(expected_activation, transition_h_schedule.height() + 2);

    // `transition_h_schedule` is h=4. The QC commits the change, but h still
    // uses the old active set and old validator-4 key.
    let mut transition_h_commitment = commitment_for(&transition_h_schedule, &handoff, 1);
    transition_h_commitment.transition = Some(transition.clone());
    let transition_h_entry = signed_entry(
        &transition_h_schedule,
        transition_h_commitment,
        0,
        None,
        &[0, 1, 3],
        None,
    );
    let transition_h_token =
        verify_formal_bft_header_certificate_v3(&transition_h_schedule, &transition_h_entry)
            .expect("scheduled transition is committed by the old set at h");
    let scheduled_ref = transition_h_token
        .next_height_schedule()
        .pending_transition()
        .expect("h QC leaves exactly one pending transition")
        .clone();
    assert_eq!(
        scheduled_ref.source_block_id,
        transition_h_token.block_id(),
        "the transition reference uses the verified h block ID"
    );
    assert_eq!(
        scheduled_ref.transition_digest,
        transition.transition_digest().unwrap()
    );
    assert_eq!(
        scheduled_ref.scheduled_height,
        transition_h_schedule.height()
    );
    assert_eq!(scheduled_ref.activation_height, expected_activation);
    let transition_h1_schedule = transition_h_token.next_height_schedule().clone();
    assert_eq!(transition_h1_schedule.height(), 5);
    assert_eq!(
        transition_h1_schedule.active_set().set_hash(),
        transition_h_schedule.active_set().set_hash(),
        "the old validator set remains active at h+1"
    );

    // h+1 still uses the old set and must carry that exact pending reference.
    let mut transition_h1_commitment = commitment_for(&transition_h1_schedule, &handoff, 2);
    let missing_ref_entry = signed_entry(
        &transition_h1_schedule,
        transition_h1_commitment.clone(),
        0,
        None,
        &[0, 1, 3],
        None,
    );
    assert!(
        verify_formal_bft_header_certificate_v3(&transition_h1_schedule, &missing_ref_entry)
            .is_err()
    );

    let mut wrong_ref = scheduled_ref.clone();
    wrong_ref.source_block_id = hash("wrong-scheduled-transition-source");
    wrong_ref.validate().unwrap();
    transition_h1_commitment.transition_ref = Some(wrong_ref);
    let wrong_ref_entry = signed_entry(
        &transition_h1_schedule,
        transition_h1_commitment.clone(),
        0,
        None,
        &[0, 1, 3],
        None,
    );
    wrong_ref_entry
        .certificate
        .proposal
        .verify_signature()
        .unwrap();
    for vote in &wrong_ref_entry.certificate.precommits {
        vote.verify_signature().unwrap();
    }
    assert!(
        verify_formal_bft_header_certificate_v3(&transition_h1_schedule, &wrong_ref_entry).is_err()
    );

    transition_h1_commitment.transition_ref = Some(scheduled_ref);
    let transition_h1_entry = signed_entry(
        &transition_h1_schedule,
        transition_h1_commitment,
        0,
        None,
        &[0, 1, 3],
        None,
    );
    let transition_h1_token =
        verify_formal_bft_header_certificate_v3(&transition_h1_schedule, &transition_h1_entry)
            .expect("h+1 QC names the one pending transition and is signed by old keys");
    let transition_h2_schedule = transition_h1_token.next_height_schedule().clone();
    let new_set = transition_h2_schedule.active_set();
    assert_eq!(transition_h2_schedule.height(), 6);
    assert_eq!(
        transition_h2_schedule.last_activation_height(),
        expected_activation
    );
    assert!(transition_h2_schedule.pending_transition().is_none());
    assert_ne!(
        new_set.set_hash(),
        transition_h_schedule.active_set().set_hash()
    );
    assert!(
        new_set.validator("validator-1").is_none(),
        "removed identity is inactive"
    );
    assert_eq!(
        new_set.validator("validator-4").unwrap().public_key_hex,
        validator_with_seed("validator-4".into(), 140, 1)
            .0
            .public_key_hex,
        "the same validator ID is bound to its rotated key"
    );

    let new_keys = active_new_keys();
    // At h+2 the rotated key is active and the removed identity is absent.
    let transition_h2_commitment = commitment_for(&transition_h2_schedule, &handoff, 3);
    let valid_new_set_entry = signed_entry_with_keys(
        &transition_h2_schedule,
        transition_h2_commitment.clone(),
        &["validator-3", "validator-4", "validator-5"],
        &new_keys,
    );
    valid_new_set_entry
        .certificate
        .proposal
        .verify_signature()
        .unwrap();
    for vote in &valid_new_set_entry.certificate.precommits {
        vote.verify_signature().unwrap();
    }
    let new_token =
        verify_formal_bft_header_certificate_v3(&transition_h2_schedule, &valid_new_set_entry)
            .expect("h+2 quorum uses the rotated active-set keys");
    assert_eq!(
        new_token.schedule().active_set().set_hash(),
        new_set.set_hash()
    );

    let mut old_rotation_key = new_keys.clone();
    old_rotation_key.insert("validator-4".into(), fixtures::key(123));
    let wrong_rotation_entry = signed_entry_with_keys(
        &transition_h2_schedule,
        transition_h2_commitment.clone(),
        &["validator-3", "validator-4", "validator-5"],
        &old_rotation_key,
    );
    wrong_rotation_entry
        .certificate
        .proposal
        .verify_signature()
        .expect("the rotated validator's old-key proposal signature is valid");
    for vote in &wrong_rotation_entry.certificate.precommits {
        vote.verify_signature()
            .expect("old rotated-key signature is cryptographically valid");
    }
    assert!(
        verify_formal_bft_header_certificate_v3(&transition_h2_schedule, &wrong_rotation_entry)
            .unwrap_err()
            .contains("key does not match")
    );

    let mut include_removed = new_keys;
    include_removed.insert("validator-1".into(), fixtures::key(120));
    let removed_member_entry = signed_entry_with_keys(
        &transition_h2_schedule,
        transition_h2_commitment,
        &["validator-1", "validator-2", "validator-3"],
        &include_removed,
    );
    for vote in &removed_member_entry.certificate.precommits {
        vote.verify_signature()
            .expect("removed member's old-key vote is still a valid signature");
    }
    assert!(
        verify_formal_bft_header_certificate_v3(&transition_h2_schedule, &removed_member_entry)
            .unwrap_err()
            .contains("not in the active set")
    );
}

#[test]
fn v3_handoff_target_uses_weighted_quorum_not_signature_count() {
    run_on_large_test_stack(
        "formal-bft-v3-weighted-quorum",
        v3_handoff_target_uses_weighted_quorum_not_signature_count_case,
    );
}

fn v3_handoff_target_uses_weighted_quorum_not_signature_count_case() {
    let qualified = qualified_handoff_with_target_and_setup(
        102,
        |_| {},
        |scope, policy| {
            let powers = [1, 1, 1, 3];
            let validators = powers
                .into_iter()
                .enumerate()
                .map(|(index, power)| {
                    validator_with_seed(validator_id(index as u8), 120 + index as u8, power).0
                })
                .collect();
            let target = BftProfileTargetV2::new(scope, 1, validators, policy)?;
            let handoff = ProfileHandoffV1 {
                next_profile: target.profile.clone(),
                next_profile_version: target.profile_version,
                next_authority_digest: target.target_digest()?,
                valid_from_position: 3,
            };
            Ok((target, handoff))
        },
    );
    let handoff = verify_formal_bft_handoff_v2(&qualified.anchor, qualified.target.clone())
        .expect("weighted V3 target is committed by its qualified H");
    let schedule = initial_schedule(&handoff);
    assert_eq!(schedule.active_set().total_voting_power(), 6);
    assert_eq!(schedule.active_set().quorum_voting_power(), 5);

    let commitment = commitment_for(&schedule, &handoff, 0);
    let three_valid_signatures =
        signed_entry(&schedule, commitment.clone(), 0, None, &[0, 1, 2], None);
    for vote in &three_valid_signatures.certificate.precommits {
        vote.verify_signature().unwrap();
    }
    assert!(
        verify_formal_bft_header_certificate_v3(&schedule, &three_valid_signatures)
            .unwrap_err()
            .contains("lacks quorum voting power")
    );

    let four_valid_signatures = signed_entry(&schedule, commitment, 0, None, &[0, 1, 2, 3], None);
    for vote in &four_valid_signatures.certificate.precommits {
        vote.verify_signature().unwrap();
    }
    verify_formal_bft_header_certificate_v3(&schedule, &four_valid_signatures)
        .expect("the fourth validator supplies the missing voting power");
}

fn transition_for(
    schedule: &BftHeaderHeightScheduleV3,
    new_validators: Vec<BftValidatorV2>,
) -> BftValidatorTransitionV2 {
    let epoch_length = schedule.policy().epoch_length_ticks;
    let target_epoch = 2u64;
    let activation_height = target_epoch.checked_mul(epoch_length).unwrap();
    let scheduled_height = activation_height.checked_sub(2).unwrap();
    let new_set = BftValidatorSetV2::new(schedule.scope().clone(), new_validators.clone()).unwrap();
    BftValidatorTransitionV2 {
        source_commit_ref: schedule.parent_commit_ref().into(),
        source_state_root: schedule.parent_state_root().into(),
        source_profile: schedule.policy().profile_id.clone(),
        source_epoch: schedule.parent_tick() / epoch_length,
        target_epoch,
        epoch_length_ticks: epoch_length,
        mapping_ruleset_hash: schedule.policy_hash().into(),
        scheduled_height,
        activation_height,
        old_validator_set_hash: schedule.active_set().set_hash().into(),
        new_validator_set_hash: new_set.set_hash().into(),
        new_validators,
        governance_action_digest: hash("synthetic-protocol-transition-action"),
        projected_registry_digest: hash("synthetic-protocol-transition-registry"),
    }
}

fn validator_with_seed(
    validator_id: String,
    seed: u8,
    voting_power: u64,
) -> (BftValidatorV2, SigningKey) {
    let key = fixtures::key(seed);
    (
        BftValidatorV2 {
            validator_id,
            public_key_hex: hex::encode(key.verifying_key().to_bytes()),
            voting_power,
        },
        key,
    )
}

fn active_new_keys() -> BTreeMap<String, SigningKey> {
    [
        ("validator-2", 121),
        ("validator-3", 122),
        ("validator-4", 140),
        ("validator-5", 141),
    ]
    .into_iter()
    .map(|(validator_id, seed)| (validator_id.to_owned(), fixtures::key(seed)))
    .collect()
}

fn signed_entry_with_keys(
    schedule: &BftHeaderHeightScheduleV3,
    commitment: FormalBftExecutionCommitmentV1,
    signer_ids: &[&str],
    keys: &BTreeMap<String, SigningKey>,
) -> FormalBftCertifiedHeaderV3 {
    let header = BftBlockHeaderV3 {
        protocol_version: 3,
        execution_commitment_hash: commitment.digest().unwrap(),
        actions_cbor_digest: actions_cbor_digest_v3(EMPTY_ACTIONS_CBOR).unwrap(),
    };
    let block_id = header.block_id().unwrap();
    let proposer_id = schedule.expected_proposer_for_round(0).unwrap();
    let proposal = BftProposalHeaderV3::sign(
        commitment.scope.clone(),
        commitment.height,
        0,
        commitment.active_validator_set_hash.clone(),
        commitment.schedule_policy_hash.clone(),
        proposer_id.clone(),
        block_id.clone(),
        None,
        keys.get(&proposer_id).expect("active proposer key"),
    )
    .unwrap();
    let precommits = signer_ids
        .iter()
        .map(|validator_id| {
            BftVoteV3::sign(
                schedule.scope().clone(),
                schedule.height(),
                0,
                BftVotePhaseV3::Precommit,
                commitment.active_validator_set_hash.clone(),
                commitment.schedule_policy_hash.clone(),
                BftVoteValueV3::Block {
                    block_id: block_id.clone(),
                },
                (*validator_id).to_owned(),
                keys.get(*validator_id).expect("fixture vote key"),
            )
            .unwrap()
        })
        .collect();
    FormalBftCertifiedHeaderV3::new(
        BftHeaderCommitCertificateV3 {
            proposal,
            valid_round_evidence: None,
            precommits,
        },
        header,
        commitment,
    )
    .unwrap()
}

fn run_on_large_test_stack(name: &str, test_case: fn()) {
    std::thread::Builder::new()
        .name(name.to_owned())
        .stack_size(32 * 1024 * 1024)
        .spawn(test_case)
        .expect("spawn large-stack formal BFT test")
        .join()
        .expect("large-stack formal BFT test thread");
}
