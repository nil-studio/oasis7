use super::*;

#[test]
fn proxy_chain_stops_at_four_and_new_lists_cannot_reset_target_cooldown() {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let _guard = runtime.enter();
    let keypair = Keypair::generate_ed25519();
    let local = PeerId::from(keypair.public());
    let mut swarm = super::super::swarm_behaviour::build_swarm(
        &keypair,
        false,
        false,
        std::time::Duration::from_secs(30),
        super::super::wire_bytes::init_shared_wire_byte_counters(),
    );
    let mut pending = HashMap::new();
    let mut targets = HashSet::new();
    let mut cooldowns = HashMap::new();
    let metrics = super::super::traffic_metrics::init_shared_traffic_metrics();
    let target = PeerId::random();
    let proxies: Vec<_> = (0..5).map(|_| PeerId::random()).collect();
    for index in 0..4 {
        assert!(request_cached_peer_record_via(
            &mut swarm,
            &mut pending,
            &mut targets,
            &mut cooldowns,
            &metrics,
            proxies[index],
            target,
            local,
            proxies[..index].to_vec()
        ));
        let id = *pending.keys().next().unwrap();
        pending.remove(&id);
        targets.remove(&target);
        swarm.behaviour_mut().discovery_budget.finish_request(id);
    }
    assert!(!request_cached_peer_record_via(
        &mut swarm,
        &mut pending,
        &mut targets,
        &mut cooldowns,
        &metrics,
        proxies[4],
        target,
        local,
        proxies[..4].to_vec()
    ));
    assert!(!request_cached_peer_record_via(
        &mut swarm,
        &mut pending,
        &mut targets,
        &mut cooldowns,
        &metrics,
        proxies[4],
        target,
        local,
        Vec::new()
    ));
    assert!(cooldowns.contains_key(&target));
    assert!(pending.is_empty());
    assert_eq!(metrics.lock().unwrap().request.outbound.messages, 4);
    assert_eq!(
        swarm.behaviour().discovery_budget.snapshot().window_starts,
        4
    );
}

#[test]
fn cooldown_capacity_refuses_new_targets_and_prunes_expired_entries() {
    let now = super::super::now_ms();
    let mut cooldowns: HashMap<_, _> = (0..1024)
        .map(|_| (PeerId::random(), now + 60_000))
        .collect();
    let peer = PeerId::random();
    assert!(peer_record_request_in_cooldown(&mut cooldowns, peer, now));
    assert_eq!(cooldowns.len(), 1024);
    assert!(!peer_record_request_in_cooldown(
        &mut cooldowns,
        peer,
        now + 60_000
    ));
    assert!(cooldowns.is_empty());
}
