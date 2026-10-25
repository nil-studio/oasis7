use ed25519_dalek::SigningKey;
use oasis7_proto::distributed_bft::{
    BftBlockV1, BftCommitCertificateV1, BftProposalHeaderV1, BftProposalV1, BftScopeV1,
    BftVotePhaseV1, BftVoteV1, BftVoteValueV1,
};

use super::*;
use crate::tendermint_v1::BftValidatorV1;

fn keys() -> Vec<SigningKey> {
    (1..=4)
        .map(|seed| SigningKey::from_bytes(&[seed; 32]))
        .collect()
}

fn scope() -> BftScopeV1 {
    BftScopeV1 {
        world_id: "verified-schedule-world".into(),
        chain_id: "verified-schedule-chain".into(),
        genesis_hash: "11".repeat(32),
    }
}

fn validator_set(keys: &[SigningKey]) -> BftValidatorSetV1 {
    BftValidatorSetV1::new(
        scope(),
        keys.iter()
            .enumerate()
            .map(|(index, key)| BftValidatorV1 {
                validator_id: format!("v{index}"),
                public_key_hex: hex::encode(key.verifying_key().to_bytes()),
                voting_power: 1,
            })
            .collect(),
    )
    .expect("valid validator set")
}

fn block(height: u64, parent_commit_ref: String, authority_epoch: u64) -> BftBlockV1 {
    BftBlockV1 {
        scope: scope(),
        height,
        parent_commit_ref,
        authority_epoch,
        manifest_hash: "22".repeat(32),
        actions_cbor: vec![0x80],
        action_root: "33".repeat(32),
        event_root: "44".repeat(32),
        execution_block_hash: "55".repeat(32),
        execution_state_root: "66".repeat(32),
        state_root: "77".repeat(32),
    }
}

fn proposer_for_round(
    set: &BftValidatorSetV1,
    starting_priority: &BftProposerPriorityV1,
    round: u64,
) -> String {
    let mut priority = starting_priority.clone();
    let mut proposer_id = String::new();
    for _ in 0..=round {
        proposer_id = priority
            .next_proposer(set)
            .expect("deterministic proposer")
            .validator_id;
    }
    proposer_id
}

fn certificate(
    set: &BftValidatorSetV1,
    keys: &[SigningKey],
    schedule: &BftHeightProposerScheduleV1,
    round: u64,
) -> BftCommitCertificateV1 {
    let block = block(
        schedule.height(),
        schedule.parent_commit_ref().to_string(),
        schedule.authority_epoch(),
    );
    let block_id = block.block_id().expect("block ID");
    let proposer_id = proposer_for_round(set, schedule.starting_priority(), round);
    let proposer_index = proposer_id[1..].parse::<usize>().expect("validator index");
    let proposal = BftProposalV1 {
        header: BftProposalHeaderV1::sign(
            set.scope().clone(),
            schedule.height(),
            round,
            set.set_hash().to_string(),
            proposer_id,
            block_id.clone(),
            None,
            &keys[proposer_index],
        )
        .expect("signed proposal"),
        block,
        valid_round_evidence: None,
    };
    let precommits = keys
        .iter()
        .enumerate()
        .take(3)
        .map(|(index, key)| {
            BftVoteV1::sign(
                set.scope().clone(),
                schedule.height(),
                round,
                BftVotePhaseV1::Precommit,
                set.set_hash().to_string(),
                BftVoteValueV1::Block {
                    block_id: block_id.clone(),
                },
                format!("v{index}"),
                key,
            )
            .expect("signed precommit")
        })
        .collect();
    BftCommitCertificateV1 {
        proposal: Box::new(proposal),
        precommits,
    }
}

#[test]
fn round_commit_continues_height_schedule_and_cold_replay_exactly() {
    let keys = keys();
    let set = validator_set(&keys);
    let genesis = BftHeightProposerScheduleV1::genesis(&set, 9).expect("genesis schedule");
    let height_one = certificate(&set, &keys, &genesis, 2);
    let first = verify_commit_certificate_token_v1(&set, &genesis, &height_one)
        .expect("round two certificate verifies");
    assert_eq!(first.next_height_schedule().height(), 2);
    assert_eq!(
        first.next_height_schedule().parent_commit_ref(),
        first.block_id()
    );

    let second = certificate(&set, &keys, first.next_height_schedule(), 0);
    let warm = verify_commit_certificate_token_v1(&set, first.next_height_schedule(), &second)
        .expect("next-height certificate uses continued priority");

    let cold_first = verify_commit_certificate_token_v1(&set, &genesis, &height_one)
        .expect("cold replay re-verifies height one");
    assert_eq!(
        cold_first.next_height_schedule(),
        first.next_height_schedule()
    );
    let cold_second =
        verify_commit_certificate_token_v1(&set, cold_first.next_height_schedule(), &second)
            .expect("cold replay re-verifies height two");
    assert_eq!(
        cold_second.next_height_schedule(),
        warm.next_height_schedule()
    );
    assert_eq!(cold_second.block_id(), warm.block_id());

    assert!(verify_commit_certificate_token_v1(&set, &genesis, &second).is_err());
}

#[test]
fn schedule_rejects_wrong_fixed_set_parent_epoch_and_height() {
    let keys = keys();
    let set = validator_set(&keys);
    let genesis = BftHeightProposerScheduleV1::genesis(&set, 9).expect("genesis schedule");
    let valid = certificate(&set, &keys, &genesis, 1);
    let other_keys = vec![
        SigningKey::from_bytes(&[21; 32]),
        SigningKey::from_bytes(&[22; 32]),
        SigningKey::from_bytes(&[23; 32]),
        SigningKey::from_bytes(&[24; 32]),
    ];
    let other_set = validator_set(&other_keys);
    assert!(verify_commit_certificate_token_v1(&other_set, &genesis, &valid).is_err());

    let wrong_parent = certificate_with_block_changes(&set, &keys, &genesis, 1, |candidate| {
        candidate.parent_commit_ref = "88".repeat(32);
    });
    assert!(verify_commit_certificate_token_v1(&set, &genesis, &wrong_parent).is_err());

    let wrong_epoch = certificate_with_block_changes(&set, &keys, &genesis, 1, |candidate| {
        candidate.authority_epoch = 10;
    });
    assert!(verify_commit_certificate_token_v1(&set, &genesis, &wrong_epoch).is_err());
}

#[test]
fn a_valid_signature_from_a_noncanonical_proposer_cannot_mint_a_token() {
    let keys = keys();
    let set = validator_set(&keys);
    let genesis = BftHeightProposerScheduleV1::genesis(&set, 9).expect("genesis schedule");
    let mut forged = certificate(&set, &keys, &genesis, 1);
    let canonical = proposer_for_round(&set, genesis.starting_priority(), 1);
    let wrong_index = (0..keys.len())
        .find(|index| format!("v{index}") != canonical)
        .expect("a different member exists");
    let block_id = forged.proposal.block.block_id().expect("block ID");
    forged.proposal.header = BftProposalHeaderV1::sign(
        set.scope().clone(),
        forged.proposal.block.height,
        1,
        set.set_hash().to_string(),
        format!("v{wrong_index}"),
        block_id,
        None,
        &keys[wrong_index],
    )
    .expect("valid signature from a noncanonical proposer");
    assert!(forged.proposal.header.verify_signature().is_ok());
    assert!(
        verify_commit_certificate_token_v1(&set, &genesis, &forged)
            .is_err_and(|error| error.contains("deterministic proposer"))
    );
}

fn certificate_with_block_changes(
    set: &BftValidatorSetV1,
    keys: &[SigningKey],
    schedule: &BftHeightProposerScheduleV1,
    round: u64,
    change: impl FnOnce(&mut BftBlockV1),
) -> BftCommitCertificateV1 {
    let mut candidate = block(
        schedule.height(),
        schedule.parent_commit_ref().to_string(),
        schedule.authority_epoch(),
    );
    change(&mut candidate);
    let block_id = candidate.block_id().expect("changed block ID");
    let proposer_id = proposer_for_round(set, schedule.starting_priority(), round);
    let proposer_index = proposer_id[1..].parse::<usize>().expect("validator index");
    let proposal = BftProposalV1 {
        header: BftProposalHeaderV1::sign(
            set.scope().clone(),
            schedule.height(),
            round,
            set.set_hash().to_string(),
            proposer_id,
            block_id.clone(),
            None,
            &keys[proposer_index],
        )
        .expect("signed proposal"),
        block: candidate,
        valid_round_evidence: None,
    };
    let precommits = keys
        .iter()
        .enumerate()
        .take(3)
        .map(|(index, key)| {
            BftVoteV1::sign(
                set.scope().clone(),
                schedule.height(),
                round,
                BftVotePhaseV1::Precommit,
                set.set_hash().to_string(),
                BftVoteValueV1::Block {
                    block_id: block_id.clone(),
                },
                format!("v{index}"),
                key,
            )
            .expect("signed precommit")
        })
        .collect();
    BftCommitCertificateV1 {
        proposal: Box::new(proposal),
        precommits,
    }
}
