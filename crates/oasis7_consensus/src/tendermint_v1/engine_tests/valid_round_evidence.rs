use super::*;

#[test]
fn proposal_valid_round_admission_rejects_duplicate_prevoters_like_qc_verification() {
    let keys = keys();
    let set = validator_set(&keys);
    let (valid_round_proposer, valid_round_key) = proposer_at(&set, 0);
    let (proposal_proposer, proposal_key) = proposer_at(&set, 1);
    let candidate = block(scope());
    let candidate_id = candidate.block_id().expect("candidate block id");
    let proof_proposal = signed_proposal(
        &set,
        0,
        candidate.clone(),
        &valid_round_proposer,
        &valid_round_key,
        None,
    );
    let mut prevotes = (0..set.quorum_voting_power() as usize)
        .map(|index| {
            vote(
                &set,
                &keys[index],
                &format!("v{index}"),
                0,
                BftVotePhaseV1::Prevote,
                BftVoteValueV1::Block {
                    block_id: candidate_id.clone(),
                },
            )
        })
        .collect::<Vec<_>>();
    prevotes.push(prevotes[0].clone());
    let proposal = signed_proposal(
        &set,
        1,
        candidate,
        &proposal_proposer,
        &proposal_key,
        Some(BftValidRoundEvidenceV1 {
            proposal: proof_proposal.header,
            prevotes,
        }),
    );

    let certificate = BftCommitCertificateV1 {
        proposal: Box::new(proposal.clone()),
        precommits: (0..set.quorum_voting_power() as usize)
            .map(|index| {
                vote(
                    &set,
                    &keys[index],
                    &format!("v{index}"),
                    1,
                    BftVotePhaseV1::Precommit,
                    BftVoteValueV1::Block {
                        block_id: proposal.header.block_id.clone(),
                    },
                )
            })
            .collect(),
    };
    let expected_proposers =
        std::collections::BTreeMap::from([(0, valid_round_proposer), (1, proposal_proposer)]);
    assert!(
        verify_commit_certificate_v1(&set, &expected_proposers, &certificate)
            .is_err_and(|error| error.contains("repeats a validator"))
    );

    let (local_id, _) = proposer(&set);
    let mut machine =
        BftConsensusMachineV1::new(set, local_id, 1, None, 0, TEST_TIMEOUTS).expect("machine");
    let admission = machine.receive_proposal(proposal, true, 1);
    assert!(
        admission.is_err_and(|error| error.contains("repeats a validator")),
        "proposal admission must reject the same duplicate signer that QC verification rejects"
    );
}
