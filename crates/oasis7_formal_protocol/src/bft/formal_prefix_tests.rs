use super::*;
use crate::bft::{
    BFT_FORMAL_PREFIX_MAX_HEIGHTS_V1, BftHeightProposerScheduleV2, FormalBftCertifiedEntryV1,
    FormalBftCertifiedPrefixV1,
};
use oasis7_proto::distributed_bft::v2::{
    BftCommitCertificateV2, BftProposalHeaderV2, BftProposalV2, BftVotePhaseV2, BftVoteV2,
    BftVoteValueV2,
};

fn signed_entry(
    cap: &super::super::VerifiedBftHandoffTargetV1,
    target: &BftProfileTargetV1,
    schedule: &BftHeightProposerScheduleV2,
    parent_state_root: Option<String>,
) -> FormalBftCertifiedEntryV1 {
    let key = SigningKey::from_bytes(&[31; 32]);
    let mut block = first_block(cap, target);
    block.height = schedule.height();
    block.parent_commit_ref = schedule.parent_commit_ref().into();
    block.authority_epoch = schedule.authority_epoch();
    block.active_validator_set_hash = schedule.active_set().set_hash().into();
    block.manifest_hash = schedule.parent_manifest_hash().into();
    let mut preimage = commitment(cap, &block);
    preimage.parent_tick = schedule.parent_tick();
    preimage.parent_semantic_state_root =
        parent_state_root.unwrap_or_else(|| schedule.parent_state_root().into());
    block.execution_block_hash = preimage.digest().unwrap();
    FormalBftCertifiedEntryV1 {
        certificate: sign_block(block, schedule, &key),
        commitment: preimage,
    }
}

fn sign_block(
    block: BftBlockV2,
    schedule: &BftHeightProposerScheduleV2,
    key: &SigningKey,
) -> BftCommitCertificateV2 {
    let block_id = block.block_id().unwrap();
    let proposer = schedule.expected_proposer_for_round(0).unwrap();
    let header = BftProposalHeaderV2::sign(
        block.scope.clone(),
        block.height,
        0,
        schedule.active_set().set_hash().into(),
        schedule.policy_hash().into(),
        proposer.clone(),
        block_id.clone(),
        None,
        key,
    )
    .unwrap();
    let vote = BftVoteV2::sign(
        block.scope.clone(),
        block.height,
        0,
        BftVotePhaseV2::Precommit,
        schedule.active_set().set_hash().into(),
        schedule.policy_hash().into(),
        BftVoteValueV2::Block { block_id },
        proposer,
        key,
    )
    .unwrap();
    BftCommitCertificateV2 {
        proposal: Box::new(BftProposalV2 {
            header,
            block,
            valid_round_evidence: None,
        }),
        precommits: vec![vote],
    }
}

fn prefix(
    count: usize,
) -> (
    super::super::VerifiedBftHandoffTargetV1,
    FormalBftCertifiedPrefixV1,
) {
    let (cap, target, _) = cap_and_target();
    let mut schedule = cap
        .initial_schedule_after_handoff_replay(&hash(101), cap.governing_manifest_digest())
        .unwrap();
    let mut proof = FormalBftCertifiedPrefixV1 {
        schema_version: 1,
        entries: vec![],
    };
    for _ in 0..count {
        let entry = signed_entry(&cap, &target, &schedule, None);
        let token =
            crate::bft::verify_commit_certificate_token_v2(&schedule, &entry.certificate).unwrap();
        schedule = token.next_height_schedule().clone();
        proof.entries.push(entry);
    }
    (cap, proof)
}

#[test]
fn certified_prefix_chains_real_qcs_from_h_and_keeps_exact_tip() {
    let (cap, proof) = prefix(3);
    let bytes = proof.canonical_bytes().unwrap();
    let decoded = FormalBftCertifiedPrefixV1::decode_canonical(&bytes).unwrap();
    assert_eq!(decoded, proof);
    let verified = decoded.verify(&cap).unwrap();
    assert_eq!(verified.heights(), 3);
    assert_eq!(verified.encoded_bytes(), bytes.len());
    assert_eq!(
        verified.handoff().activation_digest(),
        cap.activation_digest()
    );
    assert_eq!(verified.certificate().block().height, 5);
    assert_eq!(verified.next_height_schedule().height(), 6);
    assert_eq!(
        verified.certificate().block_id(),
        proof.entries[2]
            .certificate
            .proposal
            .block
            .block_id()
            .unwrap()
    );
    assert_eq!(verified.commitment(), &proof.entries[2].commitment);
}

#[test]
fn certified_prefix_rejects_missing_reordered_duplicate_and_detached_parent_claims() {
    let (cap, proof) = prefix(3);
    let mut gap = proof.clone();
    gap.entries.remove(1);
    assert!(gap.verify(&cap).is_err());
    let mut reordered = proof.clone();
    reordered.entries.swap(0, 1);
    assert!(reordered.verify(&cap).is_err());
    let mut duplicate = proof.clone();
    duplicate.entries[1] = proof.entries[0].clone();
    assert!(duplicate.verify(&cap).is_err());

    let first = FormalBftCertifiedPrefixV1 {
        schema_version: 1,
        entries: vec![proof.entries[0].clone()],
    }
    .verify(&cap)
    .unwrap();
    let bad = signed_entry(
        &cap,
        cap.target(),
        first.next_height_schedule(),
        Some(hash(201)),
    );
    assert!(
        crate::bft::verify_commit_certificate_token_v2(
            first.next_height_schedule(),
            &bad.certificate,
        )
        .is_ok(),
        "the ordinary QC alone does not bind the preimage parent root"
    );
    bad.commitment
        .verify_block(&bad.certificate.proposal.block, cap.activation_digest())
        .unwrap();
    let detached = FormalBftCertifiedPrefixV1 {
        schema_version: 1,
        entries: vec![proof.entries[0].clone(), bad],
    };
    assert!(detached.verify(&cap).is_err());
}

#[test]
fn certified_prefix_requires_canonical_versioned_bounded_evidence() {
    let (cap, mut proof) = prefix(BFT_FORMAL_PREFIX_MAX_HEIGHTS_V1);
    assert_eq!(proof.verify(&cap).unwrap().heights(), 64);
    let bytes = proof.canonical_bytes().unwrap();
    assert!(
        FormalBftCertifiedPrefixV1::decode_canonical(&[bytes.as_slice(), &[0]].concat()).is_err()
    );
    proof.entries.push(proof.entries.last().unwrap().clone());
    assert!(proof.canonical_bytes().is_err());
    proof.entries.clear();
    assert!(proof.verify(&cap).is_err());
    let (_, mut wrong_version) = prefix(1);
    wrong_version.schema_version = 2;
    assert!(wrong_version.canonical_bytes().is_err());
}

#[test]
fn certified_prefix_cumulative_bytes_are_bounded_before_canonicalization() {
    let (cap, target, _) = cap_and_target();
    let schedule = cap
        .initial_schedule_after_handoff_replay(&hash(101), cap.governing_manifest_digest())
        .unwrap();
    let mut entry = signed_entry(&cap, &target, &schedule, None);
    entry.certificate.proposal.block.actions_cbor = vec![0; 1_048_576];
    entry.certificate = sign_block(
        entry.certificate.proposal.block.clone(),
        &schedule,
        &SigningKey::from_bytes(&[31; 32]),
    );
    entry.certificate.validate_wire_bounds().unwrap();
    // Encoding capacity is checked before ancestry: repeated entries are
    // intentionally only a bounded wire-size fixture, not a valid prefix.
    let oversized = FormalBftCertifiedPrefixV1 {
        schema_version: 1,
        entries: vec![entry; 17],
    };
    assert!(
        oversized
            .canonical_bytes()
            .unwrap_err()
            .contains("cumulative byte bound")
    );
    assert!(
        FormalBftCertifiedPrefixV1::decode_canonical(&vec![0; 17 * 1024 * 1024])
            .unwrap_err()
            .contains("byte bound")
    );
}

#[test]
fn certified_prefix_keeps_old_signers_through_h_plus_one_and_activates_at_h_plus_two() {
    use oasis7_proto::distributed_bft::v2::BftValidatorTransitionV2;
    let (cap, target, _) = cap_and_target();
    let replacement = SigningKey::from_bytes(&[32; 32]);
    let new_validators = vec![BftValidatorV2 {
        validator_id: "validator-b".into(),
        public_key_hex: hex::encode(replacement.verifying_key().to_bytes()),
        voting_power: 7,
    }];
    let new_set =
        crate::bft::BftValidatorSetV2::new(cap.scope().clone(), new_validators.clone()).unwrap();
    let mut schedule = cap
        .initial_schedule_after_handoff_replay(&hash(101), cap.governing_manifest_digest())
        .unwrap();
    let mut proof = FormalBftCertifiedPrefixV1 {
        schema_version: 1,
        entries: vec![],
    };
    for height in 3..=8 {
        let mut entry = signed_entry(&cap, &target, &schedule, None);
        if height == 6 {
            let transition = BftValidatorTransitionV2 {
                source_commit_ref: schedule.parent_commit_ref().into(),
                source_state_root: schedule.parent_state_root().into(),
                source_profile: schedule.policy().profile_id.clone(),
                source_epoch: schedule.parent_tick() / 4,
                target_epoch: 2,
                epoch_length_ticks: 4,
                mapping_ruleset_hash: schedule.policy_hash().into(),
                scheduled_height: 6,
                activation_height: 8,
                old_validator_set_hash: schedule.active_set().set_hash().into(),
                new_validators: new_validators.clone(),
                new_validator_set_hash: new_set.set_hash().into(),
                governance_action_digest: hash(211),
                projected_registry_digest: hash(212),
            };
            entry.certificate.proposal.block.transition = Some(transition.clone());
            entry.commitment.transition = Some(transition);
        }
        let reference = schedule.pending_transition().cloned();
        entry.certificate.proposal.block.transition_ref = reference.clone();
        entry.commitment.transition_ref = reference;
        entry.certificate.proposal.block.execution_block_hash = entry.commitment.digest().unwrap();
        let key = if height == 8 {
            &replacement
        } else {
            &SigningKey::from_bytes(&[31; 32])
        };
        entry.certificate = sign_block(entry.certificate.proposal.block.clone(), &schedule, key);
        let token =
            crate::bft::verify_commit_certificate_token_v2(&schedule, &entry.certificate).unwrap();
        assert_eq!(
            token.active_validator_set().set_hash() == new_set.set_hash(),
            height == 8
        );
        schedule = token.next_height_schedule().clone();
        proof.entries.push(entry);
    }
    let verified = proof.verify(&cap).unwrap();
    assert_eq!(verified.certificate().block().height, 8);
    assert_eq!(verified.certificate().active_validator_set(), &new_set);
    let mut early = proof.clone();
    let prior = FormalBftCertifiedPrefixV1 {
        schema_version: 1,
        entries: proof.entries[..4].to_vec(),
    }
    .verify(&cap)
    .unwrap();
    early.entries[4].certificate = sign_block(
        early.entries[4].certificate.proposal.block.clone(),
        prior.next_height_schedule(),
        &replacement,
    );
    assert!(
        early.verify(&cap).is_err(),
        "the replacement key cannot sign the old set's h+1"
    );
}
