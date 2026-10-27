use ed25519_dalek::SigningKey;
use oasis7_proto::distributed_bft::v2::{
    BFT_PROTOCOL_VERSION_V2, BftBlockV2, BftCommitCertificateV2, BftProposalHeaderV2,
    BftProposalV2, BftScopeV2, BftValidRoundEvidenceV2, BftValidatorTransitionRefV2,
    BftValidatorTransitionV2, BftValidatorV2, BftVotePhaseV2, BftVoteV2, BftVoteValueV2,
};

use super::*;

struct Fixture {
    policy: BftSchedulePolicyV2,
    set: BftValidatorSetV2,
    keys: Vec<(String, SigningKey)>,
}

impl Fixture {
    fn new(epoch_length_ticks: u64) -> Self {
        let scope = BftScopeV2 {
            world_id: "v2-qc-world".into(),
            chain_id: "v2-qc-chain".into(),
            genesis_hash: hash("1"),
        };
        let validators = validator_keys(1..=4, "v");
        let set = BftValidatorSetV2::new(
            scope.clone(),
            validators
                .iter()
                .enumerate()
                .map(|(index, (id, key))| BftValidatorV2 {
                    validator_id: id.clone(),
                    public_key_hex: hex::encode(key.verifying_key().to_bytes()),
                    voting_power: [1, 2, 3, 5][index],
                })
                .collect(),
        )
        .expect("weighted active set");
        Self {
            policy: BftSchedulePolicyV2::new(epoch_length_ticks, "controlled-bft-v2")
                .expect("schedule policy"),
            set,
            keys: validators,
        }
    }

    fn genesis(&self) -> BftHeightProposerScheduleV2 {
        BftHeightProposerScheduleV2::genesis(
            self.set.clone(),
            self.policy.clone(),
            7,
            hash("2"),
            hash("3"),
        )
        .expect("genesis schedule")
    }

    fn block(
        &self,
        schedule: &BftHeightProposerScheduleV2,
        transition: Option<BftValidatorTransitionV2>,
        transition_ref: Option<BftValidatorTransitionRefV2>,
        salt: u8,
    ) -> BftBlockV2 {
        BftBlockV2 {
            protocol_version: BFT_PROTOCOL_VERSION_V2,
            scope: schedule.scope().clone(),
            height: schedule.height(),
            parent_commit_ref: schedule.parent_commit_ref().to_string(),
            authority_epoch: schedule.authority_epoch(),
            active_validator_set_hash: schedule.active_set().set_hash().to_string(),
            schedule_policy_hash: schedule.policy_hash().to_string(),
            manifest_hash: schedule.parent_manifest_hash().to_string(),
            post_manifest_hash: hash("a"),
            actions_cbor: vec![0x80],
            action_root: hash_digit(salt),
            event_root: hash_digit(salt.wrapping_add(1)),
            execution_block_hash: hash_digit(salt.wrapping_add(2)),
            execution_state_root: hash_digit(salt.wrapping_add(3)),
            state_root: hash_digit(salt.wrapping_add(4)),
            transition,
            transition_ref,
        }
    }

    fn certificate(
        &self,
        schedule: &BftHeightProposerScheduleV2,
        block: BftBlockV2,
        keys: &[(String, SigningKey)],
        round: u64,
    ) -> BftCommitCertificateV2 {
        let proposer_id = schedule
            .expected_proposer_for_round(round)
            .expect("scheduled proposer");
        let proposer_key = keys
            .iter()
            .find(|(id, _)| id == &proposer_id)
            .expect("proposer key");
        let header = BftProposalHeaderV2::sign(
            block.scope.clone(),
            block.height,
            round,
            schedule.active_set().set_hash().to_string(),
            schedule.policy_hash().to_string(),
            proposer_id,
            block.block_id().expect("valid block id"),
            None,
            &proposer_key.1,
        )
        .expect("signed proposal header");
        let proposal = BftProposalV2 {
            header,
            block,
            valid_round_evidence: None,
        };
        let value = BftVoteValueV2::Block {
            block_id: proposal.header.block_id.clone(),
        };
        let mut precommits = Vec::new();
        let mut power = 0u64;
        for (validator_id, key) in keys {
            let Some(voting_power) = schedule.active_set().voting_power(validator_id) else {
                continue;
            };
            precommits.push(
                BftVoteV2::sign(
                    schedule.scope().clone(),
                    schedule.height(),
                    round,
                    BftVotePhaseV2::Precommit,
                    schedule.active_set().set_hash().to_string(),
                    schedule.policy_hash().to_string(),
                    value.clone(),
                    validator_id.clone(),
                    key,
                )
                .expect("signed precommit"),
            );
            power += voting_power;
            if power >= schedule.active_set().quorum_voting_power() {
                break;
            }
        }
        assert!(power >= schedule.active_set().quorum_voting_power());
        BftCommitCertificateV2 {
            proposal: Box::new(proposal),
            precommits,
        }
    }
}

fn validator_keys(range: std::ops::RangeInclusive<u8>, prefix: &str) -> Vec<(String, SigningKey)> {
    range
        .map(|seed| {
            (
                format!("{prefix}{seed}"),
                SigningKey::from_bytes(&[seed; 32]),
            )
        })
        .collect()
}

fn hash(byte: &str) -> String {
    byte.repeat(64)
}

fn hash_digit(value: u8) -> String {
    format!("{:x}", value % 16).repeat(64)
}

fn alternate_policy_hash(fixture: &Fixture) -> String {
    let policy = BftSchedulePolicyV2::new(
        fixture.policy.epoch_length_ticks + 1,
        fixture.policy.profile_id.clone(),
    )
    .expect("alternate valid policy");
    let hash = policy.hash().expect("alternate policy hash");
    assert_ne!(hash, fixture.policy.hash().expect("fixture policy hash"));
    hash
}

fn quorum_votes(
    fixture: &Fixture,
    schedule: &BftHeightProposerScheduleV2,
    block_id: &str,
    round: u64,
    phase: BftVotePhaseV2,
    policy_hash: &str,
) -> Vec<BftVoteV2> {
    let value = BftVoteValueV2::Block {
        block_id: block_id.to_string(),
    };
    let mut votes = Vec::new();
    let mut power = 0u64;
    for (validator_id, key) in &fixture.keys {
        let Some(voting_power) = schedule.active_set().voting_power(validator_id) else {
            continue;
        };
        votes.push(
            BftVoteV2::sign(
                schedule.scope().clone(),
                schedule.height(),
                round,
                phase,
                schedule.active_set().set_hash().to_string(),
                policy_hash.to_string(),
                value.clone(),
                validator_id.clone(),
                key,
            )
            .expect("valid validator signature"),
        );
        power += voting_power;
        if power >= schedule.active_set().quorum_voting_power() {
            break;
        }
    }
    assert!(power >= schedule.active_set().quorum_voting_power());
    votes
}

fn transition_for(
    schedule: &BftHeightProposerScheduleV2,
    new_validators: Vec<BftValidatorV2>,
    target_epoch: u64,
) -> BftValidatorTransitionV2 {
    let policy = schedule.policy();
    let activation_height = target_epoch * policy.epoch_length_ticks;
    let new_set =
        BftValidatorSetV2::new(schedule.scope().clone(), new_validators.clone()).expect("new set");
    BftValidatorTransitionV2 {
        source_commit_ref: schedule.parent_commit_ref().to_string(),
        source_state_root: schedule.parent_state_root().to_string(),
        source_profile: policy.profile_id.clone(),
        source_epoch: schedule.parent_tick() / policy.epoch_length_ticks,
        target_epoch,
        epoch_length_ticks: policy.epoch_length_ticks,
        mapping_ruleset_hash: schedule.policy_hash().to_string(),
        scheduled_height: activation_height - 2,
        activation_height,
        old_validator_set_hash: schedule.active_set().set_hash().to_string(),
        new_validator_set_hash: new_set.set_hash().to_string(),
        new_validators,
        governance_action_digest: hash("b"),
        projected_registry_digest: hash("c"),
    }
}

#[test]
fn round_zero_and_round_one_qcs_produce_same_next_height_seed_and_verify_next_qc() {
    let fixture = Fixture::new(16);
    let schedule = fixture.genesis();
    let block = fixture.block(&schedule, None, None, 4);
    let round_zero = fixture.certificate(&schedule, block.clone(), &fixture.keys, 0);
    let round_one = fixture.certificate(&schedule, block, &fixture.keys, 1);
    let token_zero = verify_commit_certificate_token_v2(&schedule, &round_zero).unwrap();
    let token_one = verify_commit_certificate_token_v2(&schedule, &round_one).unwrap();
    assert_eq!(
        token_zero.next_height_schedule(),
        token_one.next_height_schedule(),
        "QC round must not change the block-based height seed"
    );

    let key_ids = fixture
        .keys
        .iter()
        .map(|(id, key)| (id.as_str(), hex::encode(key.verifying_key().to_bytes())))
        .collect::<std::collections::BTreeMap<_, _>>();
    let priorities = token_zero
        .next_height_schedule()
        .starting_priority()
        .priorities();
    for (id, expected) in [("v1", 1), ("v2", 2), ("v3", 3), ("v4", -6)] {
        assert_eq!(priorities[&key_ids[id]], expected, "priority for {id}");
    }

    let next_schedule = token_zero.next_height_schedule();
    let next_block = fixture.block(next_schedule, None, None, 7);
    let next_qc = fixture.certificate(next_schedule, next_block, &fixture.keys, 0);
    let next_token = verify_commit_certificate_token_v2(next_schedule, &next_qc)
        .expect("both same-block source QCs authorize the actual next-height QC");
    assert_eq!(next_token.schedule().height(), 2);
    assert_eq!(next_token.block().parent_commit_ref, token_zero.block_id());
}

#[test]
fn weighted_key_rotation_activates_only_after_h_plus_one_qc_and_verifies_new_set_qc() {
    let fixture = Fixture::new(4);
    let mut schedule = fixture.genesis();
    while schedule.height() < 6 {
        let block = fixture.block(&schedule, None, None, schedule.height() as u8);
        let qc = fixture.certificate(&schedule, block, &fixture.keys, 0);
        schedule = verify_commit_certificate_token_v2(&schedule, &qc)
            .expect("pre-transition QC")
            .next_height_schedule()
            .clone();
    }

    let new_keys = vec![
        ("renamed-v2".to_string(), fixture.keys[1].1.clone()),
        ("v1".to_string(), SigningKey::from_bytes(&[11; 32])),
        ("n13".to_string(), SigningKey::from_bytes(&[13; 32])),
        ("n14".to_string(), SigningKey::from_bytes(&[14; 32])),
    ];
    let new_validators = new_keys
        .iter()
        .map(|(id, key)| BftValidatorV2 {
            validator_id: id.clone(),
            public_key_hex: hex::encode(key.verifying_key().to_bytes()),
            voting_power: match id.as_str() {
                "n13" => 5,
                "n14" => 3,
                "renamed-v2" => 2,
                "v1" => 1,
                _ => unreachable!(),
            },
        })
        .collect::<Vec<_>>();
    let mut new_validators = new_validators;
    new_validators.sort_by(|left, right| left.validator_id.cmp(&right.validator_id));
    let transition = transition_for(&schedule, new_validators, 2);
    assert_eq!(transition.scheduled_height, 6);
    assert_eq!(transition.activation_height, 8);
    let h_block = fixture.block(&schedule, Some(transition), None, 8);
    let h_qc = fixture.certificate(&schedule, h_block, &fixture.keys, 0);
    let h_token = verify_commit_certificate_token_v2(&schedule, &h_qc).unwrap();
    let h_plus_one = h_token.next_height_schedule().clone();
    assert_eq!(h_plus_one.height(), 7);
    assert_eq!(h_plus_one.active_set().set_hash(), fixture.set.set_hash());
    assert!(h_plus_one.pending_transition().is_some());

    let mut missing_ref = fixture.block(&h_plus_one, None, None, 9);
    let invalid_h1 = fixture.certificate(&h_plus_one, missing_ref.clone(), &fixture.keys, 0);
    assert!(verify_commit_certificate_token_v2(&h_plus_one, &invalid_h1).is_err());
    missing_ref.transition_ref = h_plus_one.pending_transition().cloned();
    let h1_qc = fixture.certificate(&h_plus_one, missing_ref, &fixture.keys, 0);
    let h1_token = verify_commit_certificate_token_v2(&h_plus_one, &h1_qc).unwrap();

    let h_plus_two = h1_token.next_height_schedule();
    assert_eq!(h_plus_two.height(), 8);
    assert!(h_plus_two.pending_transition().is_none());
    assert_eq!(h_plus_two.active_set().validators().len(), 4);
    assert!(h_plus_two.active_set().validator("n13").is_some());
    assert!(h_plus_two.active_set().validator("renamed-v2").is_some());
    assert!(h_plus_two.active_set().validator("n14").is_some());
    assert!(h_plus_two.active_set().validator("v1").is_some());
    assert_eq!(
        h_plus_two
            .active_set()
            .validator("renamed-v2")
            .unwrap()
            .public_key_hex,
        hex::encode(fixture.keys[1].1.verifying_key().to_bytes()),
        "priority identity follows public key across a validator label change"
    );
    assert_ne!(
        h_plus_two
            .active_set()
            .validator("v1")
            .unwrap()
            .public_key_hex,
        hex::encode(fixture.keys[0].1.verifying_key().to_bytes()),
        "same label with a new key is a key rotation, not continuation"
    );
    assert!(
        h_plus_two
            .active_set()
            .validator_by_public_key(&hex::encode(fixture.keys[0].1.verifying_key().to_bytes()))
            .is_none()
    );
    let new_block = fixture.block(h_plus_two, None, None, 10);
    let new_set_qc = fixture.certificate(h_plus_two, new_block, &new_keys, 0);
    let new_set_token = verify_commit_certificate_token_v2(h_plus_two, &new_set_qc)
        .expect("new rotated key set can form the activation-height QC");
    assert_eq!(new_set_token.block().height, 8);
}

#[test]
fn certificate_rejects_validly_signed_precommits_from_another_schedule_policy() {
    let fixture = Fixture::new(16);
    let schedule = fixture.genesis();
    let block = fixture.block(&schedule, None, None, 21);
    let valid = fixture.certificate(&schedule, block, &fixture.keys, 0);
    assert!(verify_commit_certificate_token_v2(&schedule, &valid).is_ok());

    let other_policy_hash = alternate_policy_hash(&fixture);
    let mut wrong_policy = valid.clone();
    wrong_policy.precommits = valid
        .precommits
        .iter()
        .map(|vote| {
            let key = fixture
                .keys
                .iter()
                .find(|(id, _)| id == &vote.validator_id)
                .expect("certificate signer key");
            BftVoteV2::sign(
                vote.scope.clone(),
                vote.height,
                vote.round,
                vote.phase,
                vote.active_validator_set_hash.clone(),
                other_policy_hash.clone(),
                vote.value.clone(),
                vote.validator_id.clone(),
                &key.1,
            )
            .expect("re-sign precommit under another valid policy hash")
        })
        .collect();

    assert_eq!(
        wrong_policy.proposal.block.block_id(),
        valid.proposal.block.block_id()
    );
    for vote in &wrong_policy.precommits {
        assert_eq!(vote.schedule_policy_hash, other_policy_hash);
        vote.verify_signature()
            .expect("wrong-policy vote remains cryptographically valid");
        fixture
            .set
            .validate_vote(vote)
            .expect("wrong-policy vote still has an authorized active signer");
    }
    assert!(
        verify_commit_certificate_token_v2(&schedule, &wrong_policy).is_err(),
        "the QC must reject valid signatures bound to a different schedule policy"
    );
}

#[test]
fn valid_round_evidence_rejects_validly_signed_wrong_policy_headers_and_prevotes() {
    let fixture = Fixture::new(16);
    let schedule = fixture.genesis();
    let block = fixture.block(&schedule, None, None, 22);
    let block_id = block.block_id().expect("block identity");
    let old_round_certificate = fixture.certificate(&schedule, block.clone(), &fixture.keys, 0);
    let old_round_proposal = old_round_certificate.proposal.header.clone();
    let valid_prevotes = quorum_votes(
        &fixture,
        &schedule,
        &block_id,
        0,
        BftVotePhaseV2::Prevote,
        schedule.policy_hash(),
    );

    let round_one_proposer = schedule
        .expected_proposer_for_round(1)
        .expect("round-one proposer");
    let round_one_key = fixture
        .keys
        .iter()
        .find(|(id, _)| id == &round_one_proposer)
        .expect("round-one proposer key");
    let round_one_header = BftProposalHeaderV2::sign(
        block.scope.clone(),
        block.height,
        1,
        schedule.active_set().set_hash().to_string(),
        schedule.policy_hash().to_string(),
        round_one_proposer,
        block_id.clone(),
        Some(0),
        &round_one_key.1,
    )
    .expect("round-one proposal with valid-round zero");
    let valid_certificate = BftCommitCertificateV2 {
        proposal: Box::new(BftProposalV2 {
            header: round_one_header,
            block: block.clone(),
            valid_round_evidence: Some(BftValidRoundEvidenceV2 {
                proposal: old_round_proposal.clone(),
                prevotes: valid_prevotes.clone(),
            }),
        }),
        precommits: quorum_votes(
            &fixture,
            &schedule,
            &block_id,
            1,
            BftVotePhaseV2::Precommit,
            schedule.policy_hash(),
        ),
    };
    assert!(verify_commit_certificate_token_v2(&schedule, &valid_certificate).is_ok());

    let other_policy_hash = alternate_policy_hash(&fixture);
    let old_proposer_key = fixture
        .keys
        .iter()
        .find(|(id, _)| id == &old_round_proposal.proposer_id)
        .expect("valid-round proposer key");
    let wrong_policy_old_proposal = BftProposalHeaderV2::sign(
        old_round_proposal.scope.clone(),
        old_round_proposal.height,
        old_round_proposal.round,
        old_round_proposal.active_validator_set_hash.clone(),
        other_policy_hash.clone(),
        old_round_proposal.proposer_id.clone(),
        old_round_proposal.block_id.clone(),
        old_round_proposal.valid_round,
        &old_proposer_key.1,
    )
    .expect("re-sign valid-round proposal under another valid policy hash");
    assert_eq!(wrong_policy_old_proposal.block_id, block_id);
    wrong_policy_old_proposal
        .verify_signature()
        .expect("wrong-policy valid-round proposal remains cryptographically valid");
    fixture
        .set
        .validate_proposal_header(&wrong_policy_old_proposal)
        .expect("wrong-policy header has an authorized proposer key");

    let mut wrong_proposal_policy = valid_certificate.clone();
    wrong_proposal_policy
        .proposal
        .valid_round_evidence
        .as_mut()
        .expect("valid-round evidence")
        .proposal = wrong_policy_old_proposal;
    assert!(
        verify_commit_certificate_token_v2(&schedule, &wrong_proposal_policy).is_err(),
        "valid-round proposal signatures must bind the current schedule policy"
    );

    let wrong_policy_prevotes = quorum_votes(
        &fixture,
        &schedule,
        &block_id,
        0,
        BftVotePhaseV2::Prevote,
        &other_policy_hash,
    );
    for vote in &wrong_policy_prevotes {
        assert_eq!(vote.schedule_policy_hash, other_policy_hash);
        vote.verify_signature()
            .expect("wrong-policy prevote remains cryptographically valid");
        fixture
            .set
            .validate_vote(vote)
            .expect("wrong-policy prevote still has an authorized active signer");
    }
    let mut wrong_prevote_policy = valid_certificate;
    wrong_prevote_policy
        .proposal
        .valid_round_evidence
        .as_mut()
        .expect("valid-round evidence")
        .prevotes = wrong_policy_prevotes;
    assert!(
        verify_commit_certificate_token_v2(&schedule, &wrong_prevote_policy).is_err(),
        "valid-round prevotes must bind the current schedule policy"
    );
}
