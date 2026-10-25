use super::*;

#[test]
fn mixed_value_quorums_arm_prevote_and_precommit_timeouts() {
    let keys = keys();
    let set = validator_set(&keys);
    let (local_id, _) = proposer(&set);
    let mut machine =
        BftConsensusMachineV1::new(set.clone(), local_id.clone(), 1, None, 100, TEST_TIMEOUTS)
            .expect("machine");

    machine
        .on_timeout(110, local_id.as_str())
        .expect("propose timeout enters prevote");
    assert_eq!(machine.step(), BftStepV1::Prevote);
    let block_value = BftVoteValueV1::Block {
        block_id: "99".repeat(32),
    };
    for (index, value) in [
        (0, block_value.clone()),
        (1, BftVoteValueV1::Nil),
        (2, block_value.clone()),
    ] {
        let actions = machine
            .receive_vote(
                vote(
                    &set,
                    &keys[index],
                    &format!("v{index}"),
                    0,
                    BftVotePhaseV1::Prevote,
                    value,
                ),
                111 + index as i64,
            )
            .expect("mixed prevote");
        assert!(
            actions
                .iter()
                .all(|action| !matches!(action, BftConsensusActionV1::Commit(_))),
            "mixed values must not count as a same-value commit quorum"
        );
    }
    assert_eq!(machine.locked(), None);
    assert_eq!(
        machine.step_deadline_ms(),
        Some(123),
        "unique prevote power reaches quorum although neither value does"
    );

    let prevote_timeout = machine
        .on_timeout(123, local_id.as_str())
        .expect("mixed prevotes timeout into precommit");
    assert_eq!(machine.step(), BftStepV1::Precommit);
    assert!(prevote_timeout.iter().any(|action| matches!(action,
        BftConsensusActionV1::SignVote(intent)
            if intent.phase == BftVotePhaseV1::Precommit
                && intent.value == BftVoteValueV1::Nil)));

    for (index, value) in [
        (0, BftVoteValueV1::Nil),
        (1, block_value.clone()),
        (2, block_value),
    ] {
        let actions = machine
            .receive_vote(
                vote(
                    &set,
                    &keys[index],
                    &format!("v{index}"),
                    0,
                    BftVotePhaseV1::Precommit,
                    value,
                ),
                124 + index as i64,
            )
            .expect("mixed precommit");
        assert!(
            actions
                .iter()
                .all(|action| !matches!(action, BftConsensusActionV1::Commit(_))),
            "mixed values must not produce a commit certificate"
        );
    }
    assert_eq!(
        machine.step_deadline_ms(),
        Some(136),
        "unique precommit power reaches quorum although neither value does"
    );
    let precommit_timeout = machine
        .on_timeout(136, local_id.as_str())
        .expect("mixed precommits timeout into next round");
    assert_eq!(machine.round(), 1);
    assert!(
        precommit_timeout
            .iter()
            .any(|action| matches!(action, BftConsensusActionV1::RoundAdvanced { round: 1, .. }))
    );
}

#[test]
fn equivocator_cannot_supply_two_units_of_any_value_quorum_power() {
    let keys = keys();
    let set = validator_set(&keys);
    let (local_id, _) = proposer(&set);
    let mut machine =
        BftConsensusMachineV1::new(set.clone(), local_id.clone(), 1, None, 100, TEST_TIMEOUTS)
            .expect("machine");
    machine
        .on_timeout(110, local_id.as_str())
        .expect("propose timeout enters prevote");
    let block_value = BftVoteValueV1::Block {
        block_id: "aa".repeat(32),
    };

    machine
        .receive_vote(
            vote(
                &set,
                &keys[0],
                "v0",
                0,
                BftVotePhaseV1::Prevote,
                BftVoteValueV1::Nil,
            ),
            111,
        )
        .expect("equivocator first value");
    machine
        .receive_vote(
            vote(
                &set,
                &keys[0],
                "v0",
                0,
                BftVotePhaseV1::Prevote,
                block_value.clone(),
            ),
            112,
        )
        .expect("equivocator conflicting value");
    machine
        .receive_vote(
            vote(
                &set,
                &keys[1],
                "v1",
                0,
                BftVotePhaseV1::Prevote,
                block_value.clone(),
            ),
            113,
        )
        .expect("second unique validator");
    assert_eq!(machine.equivocation_evidence().len(), 1);
    assert_eq!(
        machine.step_deadline_ms(),
        None,
        "three vote messages from two validators are below unique-power quorum"
    );

    machine
        .receive_vote(
            vote(
                &set,
                &keys[2],
                "v2",
                0,
                BftVotePhaseV1::Prevote,
                BftVoteValueV1::Nil,
            ),
            114,
        )
        .expect("third unique validator reaches any-value quorum");
    assert_eq!(
        machine.step_deadline_ms(),
        Some(124),
        "equivocation evidence must not erase or duplicate the other validators' power"
    );
    assert_eq!(machine.step(), BftStepV1::Prevote);
}

#[test]
fn prevote_timeout_arms_when_phase_enters_with_buffered_quorum() {
    let keys = keys();
    let set = validator_set(&keys);
    let (local_id, local_key) = proposer(&set);
    let mut machine =
        BftConsensusMachineV1::new(set.clone(), local_id, 1, None, 100, TEST_TIMEOUTS)
            .expect("machine");
    let unrelated = BftVoteValueV1::Block {
        block_id: "99".repeat(32),
    };
    for (index, key) in keys
        .iter()
        .enumerate()
        .take(set.quorum_voting_power() as usize)
    {
        machine
            .receive_vote(
                vote(
                    &set,
                    key,
                    &format!("v{index}"),
                    0,
                    BftVotePhaseV1::Prevote,
                    unrelated.clone(),
                ),
                101,
            )
            .expect("buffer quorum while still proposing");
    }
    assert_eq!(machine.step(), BftStepV1::Propose);
    assert_eq!(machine.step_deadline_ms(), Some(110));

    let proposal = machine
        .make_proposal(
            block(scope()),
            machine.current_proposer_id().expect("proposer"),
            &local_key,
        )
        .expect("proposal");
    machine
        .receive_proposal(proposal, true, 105)
        .expect("enter prevote and replay buffered tally");
    assert_eq!(machine.step(), BftStepV1::Prevote);
    assert_eq!(machine.step_deadline_ms(), Some(115));
}
