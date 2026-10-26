use oasis7_proto::distributed_bft::{BftCommitCertificateV1, BftVotePhaseV1, BftVoteValueV1};

use super::*;

#[test]
fn committed_block_advances_machine_priority_once_independent_of_qc_round() {
    let keys = keys();
    let set = validator_set(&keys);
    let (local_id, local_key) = proposer(&set);
    let candidate_block = block(scope());
    let round_zero_proposal =
        BftConsensusMachineV1::new(set.clone(), local_id.clone(), 1, None, 0, TEST_TIMEOUTS)
            .expect("round-zero machine")
            .make_proposal(candidate_block.clone(), local_id.as_str(), &local_key)
            .expect("round-zero proposal");
    let mut round_zero_machine =
        BftConsensusMachineV1::new(set.clone(), local_id.clone(), 1, None, 0, TEST_TIMEOUTS)
            .expect("round-zero machine");
    let round_zero_qc =
        decide_round_zero(&mut round_zero_machine, &set, &keys, &round_zero_proposal);
    round_zero_machine
        .mark_commit_applied(&round_zero_qc, 10, TEST_TIMEOUTS)
        .expect("apply round-zero QC");

    let mut round_one_machine =
        BftConsensusMachineV1::new(set.clone(), local_id, 1, None, 0, TEST_TIMEOUTS)
            .expect("round-one machine");
    round_one_machine
        .advance_to_round(1, 10)
        .expect("advance height-one proposer schedule");
    let (round_one_proposer, round_one_key) = proposer_at(&set, 1);
    assert_eq!(
        round_one_machine.current_proposer_id(),
        Some(round_one_proposer.as_str())
    );
    let round_one_proposal = signed_proposal(
        &set,
        1,
        candidate_block,
        round_one_proposer.as_str(),
        &round_one_key,
        None,
    );
    let block_id = round_one_proposal.block.block_id().expect("same block ID");
    let precommits = keys
        .iter()
        .enumerate()
        .take(3)
        .map(|(index, key)| {
            vote(
                &set,
                key,
                &format!("v{index}"),
                1,
                BftVotePhaseV1::Precommit,
                BftVoteValueV1::Block {
                    block_id: block_id.clone(),
                },
            )
        })
        .collect();
    let round_one_qc = BftCommitCertificateV1 {
        proposal: Box::new(round_one_proposal),
        precommits,
    };
    round_one_machine
        .mark_commit_applied(&round_one_qc, 11, TEST_TIMEOUTS)
        .expect("apply valid round-one QC");

    assert_eq!(round_zero_machine.height(), 2);
    assert_eq!(round_one_machine.height(), 2);
    assert_eq!(
        round_zero_machine.current_proposer_id(),
        round_one_machine.current_proposer_id(),
        "persistent proposer priority follows committed blocks, not QC rounds"
    );
}
