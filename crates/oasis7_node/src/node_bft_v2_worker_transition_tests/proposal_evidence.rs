use super::*;
use oasis7_proto::distributed_bft::v2::{BftMessageV2, BftProposalHeaderV2, BftProposalV2};

#[test]
fn worker_retains_conflicting_signed_proposal_evidence_across_restart() {
    let root = PrivateRoot::new();
    let identities = identities();
    let addresses = reserve_udp_addresses(identities.len());
    let set =
        BftValidatorSetV2::new(scope(), validators(&[11, 12, 13, 14])).expect("valid genesis set");
    let schedule = BftHeightProposerScheduleV2::genesis(
        set.clone(),
        BftSchedulePolicyV2::new(EPOCH_LENGTH, PROFILE_ID).expect("valid schedule policy"),
        1,
        hash_byte(0x22),
        hash_byte(0x33),
    )
    .expect("valid genesis schedule");
    let proposer_id = schedule
        .expected_proposer_for_round(0)
        .expect("deterministic proposer");
    let proposer_seed = (11..=14)
        .find(|seed| validator_id(*seed) == proposer_id)
        .expect("proposer has a test signing key");
    let proposer_key = SigningKey::from_bytes(&[proposer_seed; 32]);
    let target_index = identities
        .iter()
        .take(4)
        .position(|identity| identity.id != proposer_id)
        .expect("an active non-proposer can observe the proposal");
    let target_identity = &identities[target_index];
    let mut node = make_node(target_identity, target_index, &addresses, root.path());
    node.start().expect("start real protected Node V2 worker");

    let hook = TransitionTestHook {
        ledger: Arc::clone(&target_identity.driver),
        new_validators: validators(&[11, 12, 13, 15, 16]),
    };
    let payload = serde_cbor::to_vec(&serde_json::json!({
        "kind": "transition-test-noop"
    }))
    .expect("encode a valid no-op test action");
    let first_action = NodeConsensusAction::from_payload(1, "player", payload.clone())
        .expect("first valid action");
    let second_action =
        NodeConsensusAction::from_payload(2, "player", payload).expect("second valid action");
    let proposals = [first_action, second_action].map(|action| {
        let block = hook
            .make_block(&schedule, &[action])
            .expect("build deterministic candidate");
        let header = BftProposalHeaderV2::sign(
            block.scope.clone(),
            block.height,
            0,
            set.set_hash().to_string(),
            schedule.policy_hash().to_string(),
            proposer_id.clone(),
            block.block_id().expect("candidate block id"),
            None,
            &proposer_key,
        )
        .expect("sign same-slot proposal header");
        BftProposalV2 {
            header,
            block,
            valid_round_evidence: None,
        }
    });
    assert_ne!(proposals[0].header, proposals[1].header);

    let sender = UdpSocket::bind("127.0.0.1:0").expect("bind proposal sender");
    for proposal in &proposals {
        let envelope = crate::gossip_udp::GossipMessage::BftV2 {
            message: BftMessageV2::Proposal(Box::new(proposal.clone())),
        };
        let bytes = serde_json::to_vec(&envelope).expect("encode signed proposal");
        sender
            .send_to(&bytes, addresses[target_index])
            .expect("send signed proposal to real worker");
    }

    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        let status = node.bft_v2_status().expect("BFT V2 status");
        if status.rejected_inbound_messages > 0 && status.proposal_equivocation_evidence.len() == 1
        {
            assert_eq!(
                status.proposal_equivocation_evidence[0],
                (proposals[0].header.clone(), proposals[1].header.clone())
            );
            break;
        }
        assert!(
            Instant::now() < deadline,
            "worker did not retain equivocation evidence: {status:?}"
        );
        thread::sleep(Duration::from_millis(10));
    }

    node.stop()
        .expect("stop worker before checking durable journal");
    let journal = identities[target_index]
        .provider
        .backend
        .snapshot()
        .state
        .journal
        .expect("provider keeps the current-height journal");
    assert!(journal.events().iter().any(|event| matches!(
        event,
        oasis7_consensus::tendermint_v2::BftSigningJournalEventV2::ProposalEquivocation {
            first,
            second,
            ..
        } if first.as_ref() == &proposals[0].header && second.as_ref() == &proposals[1].header
    )));

    let mut reopened = make_node(target_identity, target_index, &addresses, root.path());
    reopened
        .start()
        .expect("reopen Node by replaying provider journal");
    let reopened_status = reopened.bft_v2_status().expect("reopened BFT V2 status");
    assert_eq!(
        reopened_status.proposal_equivocation_evidence,
        vec![(proposals[0].header.clone(), proposals[1].header.clone())]
    );
    reopened.stop().expect("stop reopened worker");
}
