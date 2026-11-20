use super::*;
#[test]
fn authenticated_inbound_releases_unknown_pool_but_retains_global_pending_permit() {
    let budget = ConnectionBudget::new(ConnectionLimits {
        total: 4,
        pending: 3,
        ..limits()
    })
    .unwrap();
    let address: Multiaddr = "/ip4/127.0.0.1/tcp/1".parse().unwrap();
    let first_peer = Keypair::generate_ed25519().public().to_peer_id();
    let second_peer = Keypair::generate_ed25519().public().to_peer_id();
    let first = dial_for(first_peer, address.clone()).connection_id();
    let second = dial_for(second_peer, address.clone()).connection_id();
    let third = dial_for(first_peer, address).connection_id();

    let first_permit = budget.reserve(AttemptKey::Inbound(first), None).unwrap();
    assert_eq!(budget.snapshot().unwrap().pending_inbound_unknown, 1);
    assert!(budget.reserve(AttemptKey::Inbound(second), None).is_err());
    assert_eq!(
        budget.0.diagnostics.latest().0,
        Some(ResourceBudgetRejection::PendingInbound)
    );

    // Peer authentication transfers the unknown slot, but child behaviours
    // have not accepted the connection: it must still occupy total + pending.
    let first_admission = budget.authorize_inbound(first, first_peer).unwrap();
    let snapshot = budget.snapshot().unwrap();
    assert_eq!(snapshot.total, 1);
    assert_eq!(snapshot.pending, 1);
    assert_eq!(snapshot.pending_inbound_unknown, 0);
    assert_eq!(occupancy(&budget, first_peer), (1, [1, 0], 1));

    // A different unknown peer can now handshake without exceeding global P.
    let second_permit = budget.reserve(AttemptKey::Inbound(second), None).unwrap();
    let snapshot = budget.snapshot().unwrap();
    assert_eq!(snapshot.total, 2);
    assert_eq!(snapshot.pending, 2);
    assert_eq!(snapshot.pending_inbound_unknown, 1);
    assert!(budget.reserve(AttemptKey::Inbound(third), None).is_err());
    assert_eq!(
        budget.0.diagnostics.latest().0,
        Some(ResourceBudgetRejection::PendingInbound)
    );

    let second_admission = budget.authorize_inbound(second, second_peer).unwrap();
    assert_eq!(budget.snapshot().unwrap().pending_inbound_unknown, 0);
    first_admission.establish().unwrap();
    assert_eq!(budget.snapshot().unwrap().pending, 1);
    second_admission.establish().unwrap();
    let snapshot = budget.snapshot().unwrap();
    assert_eq!(snapshot.total, 2);
    assert_eq!(snapshot.established, 2);
    assert_eq!(snapshot.pending, 0);
    assert_eq!(snapshot.pending_inbound_unknown, 0);
    drop((first_permit, second_permit));
    assert_eq!(budget.snapshot().unwrap().total, 0);
}

#[test]
fn denied_inbound_authentication_keeps_unknown_slot_until_transport_ends() {
    let budget = ConnectionBudget::new(limits()).unwrap();
    let address: Multiaddr = "/ip4/127.0.0.1/tcp/1".parse().unwrap();
    let peer = Keypair::generate_ed25519().public().to_peer_id();
    let outbound = dial_for(peer, address.clone()).connection_id();
    let outbound_permit = budget
        .reserve(AttemptKey::Outbound(outbound, address.clone()), Some(peer))
        .unwrap();
    budget
        .authorize_outbound(outbound, &address, peer)
        .unwrap()
        .establish()
        .unwrap();

    let incoming = dial_for(peer, address.clone()).connection_id();
    let refused = budget.reserve(AttemptKey::Inbound(incoming), None).unwrap();
    assert!(budget.authorize_inbound(incoming, peer).is_err());
    assert_eq!(
        budget.0.diagnostics.latest().0,
        Some(ResourceBudgetRejection::PerPeer)
    );
    assert_eq!(budget.snapshot().unwrap().pending_inbound_unknown, 1);
    let other = dial_for(peer, address).connection_id();
    assert!(budget.reserve(AttemptKey::Inbound(other), None).is_err());
    drop(refused);
    assert_eq!(budget.snapshot().unwrap().pending_inbound_unknown, 0);
    let recovered = budget.reserve(AttemptKey::Inbound(other), None).unwrap();
    drop((recovered, outbound_permit));
    assert_eq!(budget.snapshot().unwrap().total, 0);
}

