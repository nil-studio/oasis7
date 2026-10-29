//! Software public-plane vectors; not quorum, private authorization or World evidence.
use super::*;
use oasis7_consensus::tendermint_v3::*;
use oasis7_proto::distributed_bft::{v2::BftScopeV2, v3::*};
fn h(n: u8) -> String {
    format!("{n:02x}").repeat(32)
}
fn context() -> BftPeerContextV3 {
    BftPeerContextV3 {
        activation_digest: h(1),
        target_digest: h(2),
        scope: BftScopeV2 {
            world_id: "peer-test".into(),
            chain_id: "chain".into(),
            genesis_hash: h(3),
        },
    }
}
fn vote() -> BftPublicPeerMessageV3 {
    BftPublicPeerMessageV3::Vote(Box::new(
        BftVoteV3::sign(
            context().scope,
            9,
            0,
            BftVotePhaseV3::Prevote,
            h(7),
            h(8),
            BftVoteValueV3::Nil,
            "validator".into(),
            &ed25519_dalek::SigningKey::from_bytes(&[17; 32]),
        )
        .unwrap(),
    ))
}
fn proposal() -> BftPublicPeerMessageV3 {
    let x = context();
    let c = FormalBftExecutionCommitmentV1 {
        domain: "oasis7.formal.bft.execution.v1".into(),
        schema_version: 1,
        profile: "bft".into(),
        profile_version: 1,
        activation_digest: x.activation_digest.clone(),
        scope: x.scope.clone(),
        height: 9,
        parent_commit_ref: h(5),
        parent_tick: 8,
        parent_semantic_state_root: h(6),
        authority_epoch: 1,
        active_validator_set_hash: h(7),
        schedule_policy_hash: h(8),
        manifest_hash: h(9),
        post_manifest_hash: h(10),
        action_root: h(11),
        event_root: h(12),
        execution_state_root: h(13),
        state_root: h(14),
        projection_root: h(15),
        result_root: h(16),
        transition: None,
        transition_ref: None,
    };
    let header = BftBlockHeaderV3 {
        protocol_version: 3,
        execution_commitment_hash: c.digest().unwrap(),
        actions_cbor_digest: actions_cbor_digest_v3(&[0x80]).unwrap(),
    };
    let signed = BftProposalHeaderV3::sign(
        x.scope,
        9,
        0,
        h(7),
        h(8),
        "validator".into(),
        header.block_id().unwrap(),
        None,
        &ed25519_dalek::SigningKey::from_bytes(&[17; 32]),
    )
    .unwrap();
    BftPublicPeerMessageV3::Proposal(Box::new(BftPublicProposalV3 {
        header: signed,
        block: BftHeaderValueV3 {
            header,
            execution_commitment: c,
        },
        valid_round_evidence: None,
    }))
}
fn wait(mut f: impl FnMut() -> bool) {
    let end = Instant::now() + Duration::from_secs(10);
    while !f() {
        assert!(Instant::now() < end);
        std::thread::sleep(Duration::from_millis(10));
    }
}
#[test]
fn public_vote_uses_two_noise_tcp_swarms_and_nonblocking_queue() {
    let a = Arc::new(Libp2pNetwork::new(oasis7_net::Libp2pNetworkConfig {
        listen_addrs: vec!["/ip4/127.0.0.1/tcp/0".parse().unwrap()],
        ..Default::default()
    }));
    wait(|| !a.listening_addrs().is_empty());
    let b = Arc::new(Libp2pNetwork::new(oasis7_net::Libp2pNetworkConfig {
        bootstrap_peers: vec![
            a.listening_addrs()[0]
                .clone()
                .with(libp2p::multiaddr::Protocol::P2p(a.peer_id())),
        ],
        ..Default::default()
    }));
    wait(|| b.connected_peers().contains(&a.peer_id()));
    let ta = NodeBftLibp2pPeerTransportV3::new(a.clone(), context(), vec![b.peer_id().to_string()])
        .unwrap();
    let tb = NodeBftLibp2pPeerTransportV3::new(
        b.clone(),
        context(),
        vec![a.peer_id().to_string(), PeerId::random().to_string()],
    )
    .unwrap();
    let msg = vote();
    let before = Instant::now();
    tb.broadcast_public(&msg).unwrap();
    assert!(before.elapsed() < Duration::from_secs(1));
    tb.send_public(&a.peer_id().to_string(), &proposal())
        .unwrap();
    let mut received = Vec::new();
    wait(|| {
        received.extend(ta.drain_public(128).unwrap());
        received.len() == 2
    });
    assert_eq!(received.len(), 2);
    assert_eq!(received[1].message, proposal());
    assert_eq!(received[0].peer_id, b.peer_id().to_string());
    assert_eq!(received[0].message, msg);
    assert!(a.published().is_empty() && b.published().is_empty());
    assert!(ta.send_public(&PeerId::random().to_string(), &msg).is_err());
    assert!(
        NodeBftLibp2pPeerTransportV3::new(a.clone(), context(), vec![b.peer_id().to_string()])
            .is_err()
    );
    drop(ta);
    let rebuilt =
        NodeBftLibp2pPeerTransportV3::new(a.clone(), context(), vec![b.peer_id().to_string()])
            .unwrap();
    tb.send_public(&a.peer_id().to_string(), &msg).unwrap();
    let mut recovered = Vec::new();
    wait(|| {
        recovered.extend(rebuilt.drain_public(128).unwrap());
        !recovered.is_empty()
    });
    assert_eq!(recovered[0].message, msg);
}
#[test]
fn assembly_rejects_order_conflict_budget_and_keeps_original_deadline() {
    let c = context();
    let ctx = context_digest(&c).unwrap();
    let raw = encode_bft_public_peer_message_v3(&c, &vote()).unwrap();
    let digest = message_digest(&ctx, &raw);
    let peer = PeerId::random();
    let mut inbox = Inbox::new();
    let mut f = Frame::decode(&Frame::encode_chunk(&ctx, &raw, &digest, 0).unwrap(), &ctx).unwrap();
    // Exact retransmission returns ACK and does not create another completed event.
    inbox.receive(peer, f, &c).unwrap();
    f = Frame::decode(&Frame::encode_chunk(&ctx, &raw, &digest, 0).unwrap(), &ctx).unwrap();
    inbox.receive(peer, f, &c).unwrap();
    assert_eq!(inbox.drain(128).len(), 1);
    assert!(Frame::decode(&vec![0; 65537], &ctx).is_err());
    assert!(Frame::decode(&[0xbb, 255, 255, 255, 255, 255, 255, 255, 255], &ctx).is_err());
    assert!(
        Frame::decode(
            &Frame::encode_chunk(&ctx, &raw, &digest, 0).unwrap(),
            &h(99)
        )
        .is_err()
    );
    let large = vec![0u8; CHUNK * 3];
    let d = message_digest(&ctx, &large);
    let mut i = Inbox::new();
    assert!(
        i.receive(
            peer,
            Frame::decode(&Frame::encode_chunk(&ctx, &large, &d, CHUNK).unwrap(), &ctx).unwrap(),
            &c
        )
        .is_err()
    );
    i.receive(
        peer,
        Frame::decode(&Frame::encode_chunk(&ctx, &large, &d, 0).unwrap(), &ctx).unwrap(),
        &c,
    )
    .unwrap();
    let deadlines = i.deadlines_for_test();
    i.receive(
        peer,
        Frame::decode(&Frame::encode_chunk(&ctx, &large, &d, 0).unwrap(), &ctx).unwrap(),
        &c,
    )
    .unwrap();
    assert_eq!(i.deadlines_for_test(), deadlines);
    i.expire_partials_for_test();
    assert!(
        i.receive(
            peer,
            Frame::decode(&Frame::encode_chunk(&ctx, &large, &d, CHUNK).unwrap(), &ctx).unwrap(),
            &c
        )
        .is_err()
    );
    i.receive(
        peer,
        Frame::decode(&Frame::encode_chunk(&ctx, &large, &d, 0).unwrap(), &ctx).unwrap(),
        &c,
    )
    .unwrap();
    let mut changed = large.clone();
    changed[0] = 1;
    assert!(
        i.receive(
            peer,
            Frame::decode(&Frame::encode_chunk(&ctx, &changed, &d, 0).unwrap(), &ctx).unwrap(),
            &c
        )
        .is_err()
    );
}

#[test]
fn public_partial_capacity_is_global_and_per_peer() {
    let c = context();
    let ctx = context_digest(&c).unwrap();
    let mut i = Inbox::new();
    let peer = PeerId::random();
    let raw = vec![0; CHUNK * 2];
    for n in 1..=2 {
        i.receive(
            peer,
            Frame::decode(&Frame::encode_chunk(&ctx, &raw, &h(n), 0).unwrap(), &ctx).unwrap(),
            &c,
        )
        .unwrap();
    }
    assert!(
        i.receive(
            peer,
            Frame::decode(&Frame::encode_chunk(&ctx, &raw, &h(3), 0).unwrap(), &ctx).unwrap(),
            &c
        )
        .is_err()
    );
    for n in 3..=8 {
        i.receive(
            PeerId::random(),
            Frame::decode(&Frame::encode_chunk(&ctx, &raw, &h(n), 0).unwrap(), &ctx).unwrap(),
            &c,
        )
        .unwrap();
    }
    assert!(
        i.receive(
            PeerId::random(),
            Frame::decode(&Frame::encode_chunk(&ctx, &raw, &h(9), 0).unwrap(), &ctx).unwrap(),
            &c
        )
        .is_err()
    );
}
