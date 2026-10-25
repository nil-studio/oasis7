use super::*;

#[test]
fn equal_valid_round_authorizes_local_prevote_after_lock() {
    let keys = keys();
    let set = validator_set(&keys);
    let local_id = "v0";
    let quorum = set.quorum_voting_power() as usize;
    let (round_zero_proposer, round_zero_key) = proposer_at(&set, 0);
    let mut machine = BftConsensusMachineV1::new(set.clone(), local_id, 1, None, 0, TEST_TIMEOUTS)
        .expect("machine");

    let locked_block = block(scope());
    let first_proposal = signed_proposal(
        &set,
        0,
        locked_block.clone(),
        &round_zero_proposer,
        &round_zero_key,
        None,
    );
    let actions = machine
        .receive_proposal(first_proposal, true, 1)
        .expect("validated round-zero proposal");
    let first_prevote = actions
        .into_iter()
        .find_map(|action| match action {
            BftConsensusActionV1::SignVote(intent) if intent.phase == BftVotePhaseV1::Prevote => {
                Some(intent)
            }
            _ => None,
        })
        .expect("local round-zero prevote intent");
    machine
        .record_local_vote(
            first_prevote
                .sign(&set, &keys[0])
                .expect("sign first local prevote"),
            2,
        )
        .expect("record first local prevote");

    let locked_id = locked_block.block_id().expect("locked block id");
    let mut precommit_intent = None;
    for (index, key) in keys.iter().enumerate().skip(1) {
        let actions = machine
            .receive_vote(
                vote(
                    &set,
                    key,
                    &format!("v{index}"),
                    0,
                    BftVotePhaseV1::Prevote,
                    BftVoteValueV1::Block {
                        block_id: locked_id.clone(),
                    },
                ),
                2 + index as i64,
            )
            .expect("round-zero prevote");
        precommit_intent = precommit_intent.or_else(|| {
            actions.into_iter().find_map(|action| match action {
                BftConsensusActionV1::SignVote(intent)
                    if intent.phase == BftVotePhaseV1::Precommit =>
                {
                    Some(intent)
                }
                _ => None,
            })
        });
        if precommit_intent.is_some() {
            break;
        }
    }
    assert_eq!(machine.locked(), Some((locked_id.as_str(), 0)));
    machine
        .record_local_vote(
            precommit_intent
                .expect("lock emits local precommit intent")
                .sign(&set, &keys[0])
                .expect("sign first local precommit"),
            6,
        )
        .expect("record first local precommit");

    // The fixture uses a hostile >1/3 equivocation: the same q validators
    // first lock A and then provide a separate q proof for B at validRound 0.
    // It tests signer/replay consistency, not safety under the BFT fault bound.
    for (index, key) in keys.iter().enumerate().skip(1).take(quorum - 1) {
        machine
            .receive_vote(
                vote(
                    &set,
                    key,
                    &format!("v{index}"),
                    0,
                    BftVotePhaseV1::Precommit,
                    BftVoteValueV1::Nil,
                ),
                6 + index as i64,
            )
            .expect("nil precommit contributes only to mixed-value timeout quorum");
    }
    let deadline = machine
        .step_deadline_ms()
        .expect("mixed precommits arm round timeout");
    machine
        .on_timeout(deadline, local_id)
        .expect("precommit timeout advances to next round");
    assert_eq!(machine.round(), 1);

    let mut reproposed_block = block(scope());
    reproposed_block.state_root = "99".repeat(32);
    let reproposed_id = reproposed_block.block_id().expect("reproposal block id");
    let proof_proposal = signed_proposal(
        &set,
        0,
        reproposed_block.clone(),
        &round_zero_proposer,
        &round_zero_key,
        None,
    );
    let proof = BftValidRoundEvidenceV1 {
        proposal: proof_proposal.header,
        prevotes: keys
            .iter()
            .enumerate()
            .take(quorum)
            .map(|(index, key)| {
                vote(
                    &set,
                    key,
                    &format!("v{index}"),
                    0,
                    BftVotePhaseV1::Prevote,
                    BftVoteValueV1::Block {
                        block_id: reproposed_id.clone(),
                    },
                )
            })
            .collect(),
    };
    let (round_one_proposer, round_one_key) = proposer_at(&set, 1);
    let reproposal = signed_proposal(
        &set,
        1,
        reproposed_block,
        &round_one_proposer,
        &round_one_key,
        Some(proof),
    );
    let actions = machine
        .receive_proposal(reproposal, true, deadline + 1)
        .expect("equal valid round permits replacing the locked value");
    let local_prevote = actions
        .into_iter()
        .find_map(|action| match action {
            BftConsensusActionV1::SignVote(intent) if intent.phase == BftVotePhaseV1::Prevote => {
                Some(intent)
            }
            _ => None,
        })
        .expect("round-one local prevote intent");
    assert_eq!(
        local_prevote.value,
        BftVoteValueV1::Block {
            block_id: reproposed_id,
        }
    );
    let signed_vote = local_prevote
        .sign(&set, &keys[0])
        .expect("sign authorized equal-valid-round vote");
    machine
        .record_local_vote(signed_vote.clone(), deadline + 2)
        .expect("locally authorized prevote must also pass signer validation");
    assert_eq!(
        machine.local_votes.get(&(1, BftVotePhaseV1::Prevote)),
        Some(&signed_vote)
    );
}
