use super::*;
use libp2p::{core::PeerRecord as Libp2pPeerRecord, rendezvous};

fn handle_cached_peer_exchange_response(
    local_key: &Keypair,
    signed: &SignedPeerRecord,
    kind: super::super::discovery::PendingPeerRecordRequest,
    response_source: PeerId,
    connected_peers: &[PeerId],
) -> (
    PeerRecordCache,
    super::super::peer_discovery_observations::DiscoveryObservations,
    Vec<String>,
) {
    let requested_peer = match &kind {
        super::super::discovery::PendingPeerRecordRequest::CachedPeerRecord { peer_id, .. } => {
            *peer_id
        }
        _ => panic!("test helper only accepts cached peer-record requests"),
    };
    let payload = to_canonical_cbor(signed).expect("encode signed peer record");
    let mut swarm = super::super::swarm_behaviour::build_swarm(
        local_key,
        false,
        true,
        Duration::from_secs(30),
        super::super::wire_bytes::init_shared_wire_byte_counters(),
    );
    let mut pending_peer_record_requests = HashMap::new();
    let mut pending_dht = HashMap::new();
    let mut cache = PeerRecordCache::default();
    let mut observations =
        super::super::peer_discovery_observations::DiscoveryObservations::default();
    let mut known_paths = HashMap::new();
    let mut last_dialed_paths = HashMap::new();
    let active_paths = HashMap::new();
    let mut failed_path_labels = HashSet::new();
    let mut pending_discovery_peer_records = HashSet::new();
    let mut pending_connected_peer_records = HashSet::new();
    let mut pending_cached_peer_records = HashSet::from([requested_peer]);
    let mut pending_cached_discovery_peers = HashSet::new();
    let mut cached_peer_record_cooldowns = HashMap::new();
    let event_errors = Arc::new(Mutex::new(Vec::new()));
    let traffic_metrics = super::super::traffic_metrics::init_shared_traffic_metrics();
    let template = signed_discovery_peer_record(local_key, Vec::new(), 1).record;

    super::super::discovery::handle_peer_record_response(
        &mut swarm,
        kind,
        response_source,
        &payload,
        &mut pending_peer_record_requests,
        &mut pending_dht,
        &mut cache,
        &mut observations,
        &mut known_paths,
        &mut last_dialed_paths,
        &active_paths,
        connected_peers,
        &traffic_metrics,
        &mut failed_path_labels,
        &mut pending_discovery_peer_records,
        &mut cached_peer_record_cooldowns,
        Some(&template),
        PeerId::from(local_key.public()),
        &mut pending_connected_peer_records,
        &mut pending_cached_peer_records,
        &mut pending_cached_discovery_peers,
        16,
        &event_errors,
        &PeerManagerPolicy::default(),
        &HashSet::new(),
    );
    let errors = event_errors.lock().expect("event errors lock").clone();
    (cache, observations, errors)
}

fn cached_peer_record_request(
    peer_id: PeerId,
    proxy: PeerId,
    rendezvous_triggers: Option<super::super::rendezvous_trigger::RendezvousTriggerContext>,
) -> super::super::discovery::PendingPeerRecordRequest {
    super::super::discovery::PendingPeerRecordRequest::CachedPeerRecord {
        ask_peer: proxy,
        peer_id,
        tried_proxies: vec![proxy],
        rendezvous_triggers,
    }
}

#[cfg(unix)]
fn initialized_receiver(
    local_key: &Keypair,
    template: &PeerRecord,
) -> (
    tempfile::TempDir,
    super::super::receiver_floor::SharedPeerRecordReceiver,
) {
    use std::os::unix::fs::PermissionsExt;

    let physical_temp_dir =
        std::fs::canonicalize(std::env::temp_dir()).expect("resolve temporary directory");
    let root = tempfile::tempdir_in(physical_temp_dir).expect("receiver root");
    let receiver_dir = root.path().join("receiver");
    std::fs::create_dir(&receiver_dir).expect("create receiver directory");
    std::fs::set_permissions(&receiver_dir, std::fs::Permissions::from_mode(0o700))
        .expect("restrict receiver directory");
    let config = super::super::config::PeerRecordReceiverConfig {
        mode: super::super::config::PeerRecordReceiverMode::DurableCompatible,
        directory: Some(receiver_dir),
    };
    super::super::receiver_floor::PeerRecordReceiver::initialize(&config, local_key, template)
        .expect("initialize durable receiver");
    let receiver =
        super::super::receiver_floor::PeerRecordReceiver::open(&config, local_key, Some(template))
            .expect("open durable receiver");
    (root, receiver)
}

#[test]
fn rendezvous_discovery_keeps_server_to_peer_trigger_association() {
    let local_key = Keypair::generate_ed25519();
    let local_peer = PeerId::from(local_key.public());
    let rendezvous_server = PeerId::random();
    let remote_key = Keypair::generate_ed25519();
    let registration_record = Libp2pPeerRecord::new(
        &remote_key,
        vec!["/memory/79".parse().expect("valid memory address")],
    )
    .expect("signed registration record");
    let registered_peer = registration_record.peer_id();
    let registration = rendezvous::Registration {
        namespace: rendezvous::Namespace::from_static("trigger-test"),
        record: registration_record,
        ttl: 60,
    };
    let template =
        signed_discovery_peer_record(&local_key, vec![crate::dht::PeerDiscoverySource::Dht], 1)
            .record;
    let mut swarm = super::super::swarm_behaviour::build_swarm(
        &local_key,
        false,
        true,
        Duration::from_secs(30),
        super::super::wire_bytes::init_shared_wire_byte_counters(),
    );
    let mut pending_dht = HashMap::new();
    let mut pending_peer_record_requests = HashMap::new();
    let mut pending_discovery_peer_records = HashSet::new();
    let mut pending_cached_peer_records = HashSet::new();
    let mut cached_peer_record_cooldowns = HashMap::new();
    let traffic_metrics = super::super::traffic_metrics::init_shared_traffic_metrics();
    let event_errors = Arc::new(Mutex::new(Vec::new()));

    super::super::rendezvous_trigger::handle_rendezvous_discovered(
        &mut swarm,
        rendezvous_server,
        vec![registration],
        &mut pending_dht,
        &mut pending_peer_record_requests,
        &mut pending_discovery_peer_records,
        &mut pending_cached_peer_records,
        &mut cached_peer_record_cooldowns,
        &traffic_metrics,
        &[],
        &PeerRecordCache::default(),
        local_peer,
        Some(&template),
        16,
        &event_errors,
    );

    let second_rendezvous_server = PeerId::random();
    assert_ne!(second_rendezvous_server, rendezvous_server);
    for _ in 0..2 {
        let duplicate_registration = rendezvous::Registration {
            namespace: rendezvous::Namespace::from_static("trigger-test"),
            record: Libp2pPeerRecord::new(
                &remote_key,
                vec!["/memory/79".parse().expect("valid memory address")],
            )
            .expect("signed duplicate registration record"),
            ttl: 60,
        };
        super::super::rendezvous_trigger::handle_rendezvous_discovered(
            &mut swarm,
            second_rendezvous_server,
            vec![duplicate_registration],
            &mut pending_dht,
            &mut pending_peer_record_requests,
            &mut pending_discovery_peer_records,
            &mut pending_cached_peer_records,
            &mut cached_peer_record_cooldowns,
            &traffic_metrics,
            &[],
            &PeerRecordCache::default(),
            local_peer,
            Some(&template),
            16,
            &event_errors,
        );
    }

    assert!(pending_discovery_peer_records.contains(&registered_peer));

    let dht_upstream = PeerId::from(Keypair::generate_ed25519().public());
    assert_ne!(dht_upstream, rendezvous_server);
    let signed = signed_discovery_peer_record(
        &remote_key,
        vec![crate::dht::PeerDiscoverySource::Dht],
        crate::libp2p_net::now_ms(),
    );
    let (_, mut pending) = pending_dht
        .drain()
        .next()
        .expect("rendezvous queued DHT lookup");
    let dht_key = "rendezvous-target".to_string();
    let found = kad::QueryResult::GetRecord(Ok(kad::GetRecordOk::FoundRecord(kad::PeerRecord {
        peer: Some(dht_upstream),
        record: kad::Record {
            key: RecordKey::new(&dht_key),
            value: to_canonical_cbor(&signed).expect("encode signed peer record"),
            publisher: None,
            expires: None,
        },
    })));
    let DhtProgressAction::DiscoverPeerRecord {
        upstream: Some(observed_upstream),
        result,
        rendezvous_triggers,
        ..
    } = handle_dht_progress(
        &mut pending,
        found,
        true,
        &super::super::peer_record_time::DiscoveryClock::default(),
    )
    else {
        panic!("valid DHT record should complete the rendezvous-triggered lookup");
    };
    assert_eq!(observed_upstream, dht_upstream);
    let admitted = result
        .expect("valid signed DHT result")
        .expect("record found");
    let attempt_trigger_evidence = rendezvous_triggers
        .as_ref()
        .expect("remote DHT lookup carries rendezvous context")
        .active_evidence(Instant::now());
    let mut cache = PeerRecordCache::default();
    let mut observations =
        super::super::peer_discovery_observations::DiscoveryObservations::default();
    let mut known_paths = HashMap::new();
    let mut last_dialed_paths = HashMap::new();
    let active_paths = HashMap::new();
    let mut failed_path_labels = HashSet::new();

    super::super::peer_record_candidate::install_admitted_peer_record_with_rendezvous_triggers(
        &mut swarm,
        &mut cache,
        &mut observations,
        &mut known_paths,
        &mut last_dialed_paths,
        &active_paths,
        &mut failed_path_labels,
        Some(&template),
        &PeerManagerPolicy::default(),
        &HashSet::new(),
        admitted,
        Some(dht_upstream),
        super::super::peer_record_admission::PeerRecordAdmissionWorkUnit::KadDiscoveryPeerRecordResult,
        super::super::peer_discovery_observations::DiscoveryIngress::Dht {
            upstream: Some(dht_upstream),
        },
        rendezvous_triggers.as_ref(),
    )
    .expect("verified DHT record installs");

    assert_eq!(
        observations.observed_sources(&signed),
        vec![crate::dht::PeerDiscoverySource::Dht]
    );
    assert!(observations.trusted_source_labels(&signed).is_empty());
    let trigger_servers = observations.rendezvous_trigger_servers(&signed, Instant::now());
    assert_eq!(
        trigger_servers.len(),
        2,
        "duplicate servers are deduplicated"
    );
    assert!(trigger_servers.contains(&rendezvous_server));
    assert!(trigger_servers.contains(&second_rendezvous_server));
    let diagnostic = super::super::rendezvous_trigger::linked_observation_diagnostic(
        &signed,
        &format!("dht-upstream={dht_upstream}"),
        &observations,
        &attempt_trigger_evidence,
    )
    .expect("successful exact live record exposes bounded trigger diagnostics");
    assert!(diagnostic.contains(&signed.record.peer_id));
    assert!(diagnostic.contains(&rendezvous_server.to_string()));
    assert!(diagnostic.contains(&dht_upstream.to_string()));

    let before_unrelated_query = event_errors
        .lock()
        .expect("event errors lock")
        .iter()
        .filter(|message| message.starts_with("rendezvous-trigger-observation"))
        .count();
    let payload = to_canonical_cbor(&signed).expect("encode repeated DHT result");
    let admitted = super::super::peer_record_admission::AdmittedPeerRecord::admit_and_decode_with(
        &super::super::peer_record_admission::PeerRecordAdmissionBudget::shared(),
        Some(dht_upstream),
        super::super::peer_record_admission::PeerRecordAdmissionWorkUnit::KadDiscoveryPeerRecordResult,
        &payload,
        super::super::peer_record::MAX_SIGNED_PEER_RECORD_BYTES,
        super::super::peer_record::decode_peer_record,
    )
    .expect("admit unrelated repeated DHT result");
    super::super::peer_record_candidate::install_dht_discovered_peer_record(
        &mut swarm,
        &mut cache,
        &mut observations,
        &mut known_paths,
        &mut last_dialed_paths,
        &active_paths,
        &mut failed_path_labels,
        Some(&template),
        &PeerManagerPolicy::default(),
        &HashSet::new(),
        admitted,
        Some(dht_upstream),
        None,
        &event_errors,
        16,
    )
    .expect("unrelated exact DHT result still follows normal idempotent path");
    let after_unrelated_query = event_errors
        .lock()
        .expect("event errors lock")
        .iter()
        .filter(|message| message.starts_with("rendezvous-trigger-observation"))
        .count();
    assert_eq!(
        after_unrelated_query, before_unrelated_query,
        "an unrelated query must not re-emit an old rendezvous association"
    );
}

#[test]
fn explicit_failure_clears_shared_trigger_before_parallel_valid_result() {
    let local_key = Keypair::generate_ed25519();
    let target_key = Keypair::generate_ed25519();
    let target = PeerId::from(target_key.public());
    let proxy = PeerId::random();
    let server = PeerId::random();
    let context = super::super::rendezvous_trigger::RendezvousTriggerContext::new(
        target,
        server,
        Instant::now(),
    );
    let signed = signed_discovery_peer_record(
        &target_key,
        vec![crate::dht::PeerDiscoverySource::PeerExchange],
        crate::libp2p_net::now_ms(),
    );
    let mut invalid = signed.clone();
    let mut invalid_signature = hex::decode(&invalid.signature_hex).expect("valid signature hex");
    invalid_signature[0] ^= 1;
    invalid.signature_hex = hex::encode(invalid_signature);

    let failed_kind = || super::super::discovery::PendingPeerRecordRequest::CachedPeerRecord {
        ask_peer: proxy,
        peer_id: target,
        tried_proxies: vec![proxy],
        rendezvous_triggers: Some(context.clone()),
    };
    let (failed_cache, failed_observations, failure_errors) =
        handle_cached_peer_exchange_response(&local_key, &invalid, failed_kind(), proxy, &[]);
    assert!(failed_cache.get(&target).is_none());
    assert!(
        failed_observations
            .rendezvous_trigger_servers(&signed, Instant::now())
            .is_empty()
    );
    assert!(context.active_evidence(Instant::now()).is_empty());
    assert!(failure_errors.iter().any(|message| {
        message.contains("peer record response rejected")
            && message.contains("signature verification failed")
    }));

    let valid_kind = || super::super::discovery::PendingPeerRecordRequest::CachedPeerRecord {
        ask_peer: proxy,
        peer_id: target,
        tried_proxies: vec![proxy],
        rendezvous_triggers: Some(context.clone()),
    };
    let (valid_cache, valid_observations, valid_errors) =
        handle_cached_peer_exchange_response(&local_key, &signed, valid_kind(), proxy, &[]);
    assert!(
        valid_cache.get(&target).is_some(),
        "valid peer record is still installed"
    );
    assert_eq!(
        valid_observations.observed_sources(&signed),
        vec![crate::dht::PeerDiscoverySource::PeerExchange]
    );
    assert!(
        valid_observations
            .rendezvous_trigger_servers(&signed, Instant::now())
            .is_empty(),
        "another path may install the valid record but must not inherit cleared evidence"
    );
    assert!(
        valid_errors
            .iter()
            .all(|message| !message.starts_with("rendezvous-trigger-observation"))
    );
}

#[test]
fn rr_fallback_keeps_dht_miss_trigger_through_remote_record_install() {
    let local_key = Keypair::generate_ed25519();
    let target_key = Keypair::generate_ed25519();
    let target = PeerId::from(target_key.public());
    let rendezvous_server = PeerId::random();
    let proxy = PeerId::random();
    let context = super::super::rendezvous_trigger::RendezvousTriggerContext::new(
        target,
        rendezvous_server,
        Instant::now(),
    );
    let mut pending = super::super::kad_queries::PendingDhtQuery::DiscoverPeerRecord {
        peer_id: target,
        upstream: None,
        record: None,
        error: None,
        rendezvous_triggers: Some(context.clone()),
    };
    let key = "rendezvous-fallback".to_string();
    let DhtProgressAction::DiscoverPeerRecord {
        peer_id: completed_peer,
        upstream,
        result,
        rendezvous_triggers,
    } = super::super::kad_queries::handle_dht_progress(
        &mut pending,
        kad::QueryResult::GetRecord(Err(kad::GetRecordError::NotFound {
            key: RecordKey::new(&key),
            closest_peers: Vec::new(),
        })),
        true,
        &super::super::peer_record_time::DiscoveryClock::default(),
    )
    else {
        panic!("DHT not-found should return the pending peer-record lookup");
    };
    assert_eq!(completed_peer, target);
    assert_eq!(upstream, None);
    assert!(matches!(*result, Ok(None)));
    let context = super::super::rendezvous_trigger::context_for_dht_result(
        rendezvous_triggers,
        upstream,
        true,
    )
    .expect("a DHT miss remains eligible for its RR fallback");

    let mut swarm = super::super::swarm_behaviour::build_swarm(
        &local_key,
        false,
        true,
        Duration::from_secs(30),
        super::super::wire_bytes::init_shared_wire_byte_counters(),
    );
    let mut pending_dht = HashMap::new();
    let mut pending_rr = HashMap::new();
    let mut pending_cached = HashSet::new();
    let mut cooldowns = HashMap::new();
    let traffic_metrics = super::super::traffic_metrics::init_shared_traffic_metrics();
    super::super::rendezvous_trigger::queue_cached_record_fallback(
        &mut swarm,
        &mut pending_dht,
        &mut pending_rr,
        &mut pending_cached,
        &mut cooldowns,
        &traffic_metrics,
        &[proxy],
        target,
        PeerId::from(local_key.public()),
        Some(&context),
    );
    let request = pending_rr
        .into_values()
        .find(|request| matches!(request, super::super::discovery::PendingPeerRecordRequest::CachedPeerRecord { peer_id, .. } if *peer_id == target))
        .expect("the DHT miss queues the cached-record RR fallback");
    let signed = signed_discovery_peer_record(
        &target_key,
        vec![crate::dht::PeerDiscoverySource::PeerExchange],
        crate::libp2p_net::now_ms(),
    );
    let (cache, observations, diagnostics) =
        handle_cached_peer_exchange_response(&local_key, &signed, request, proxy, &[]);
    assert!(cache.get(&target).is_some());
    assert_eq!(
        observations.observed_sources(&signed),
        vec![crate::dht::PeerDiscoverySource::PeerExchange]
    );
    assert_eq!(
        observations.rendezvous_trigger_servers(&signed, Instant::now()),
        vec![rendezvous_server]
    );
    assert!(diagnostics.iter().any(|message| {
        message.starts_with("rendezvous-trigger-observation")
            && message.contains(&rendezvous_server.to_string())
            && message.contains(&proxy.to_string())
    }));
}

#[test]
fn wrong_target_and_expired_rr_contexts_do_not_attribute_triggers() {
    let local_key = Keypair::generate_ed25519();
    let requested_key = Keypair::generate_ed25519();
    let requested_peer = PeerId::from(requested_key.public());
    let returned_key = Keypair::generate_ed25519();
    let proxy = PeerId::random();
    let server = PeerId::random();
    let wrong_target_context = super::super::rendezvous_trigger::RendezvousTriggerContext::new(
        requested_peer,
        server,
        Instant::now(),
    );
    let wrong_target_record = signed_discovery_peer_record(
        &returned_key,
        vec![crate::dht::PeerDiscoverySource::PeerExchange],
        crate::libp2p_net::now_ms(),
    );
    let (wrong_cache, wrong_observations, _) = handle_cached_peer_exchange_response(
        &local_key,
        &wrong_target_record,
        cached_peer_record_request(requested_peer, proxy, Some(wrong_target_context.clone())),
        proxy,
        &[],
    );
    assert!(wrong_cache.get(&requested_peer).is_none());
    assert!(
        wrong_target_context
            .active_evidence(Instant::now())
            .is_empty()
    );
    assert!(
        wrong_observations
            .rendezvous_trigger_servers(&wrong_target_record, Instant::now())
            .is_empty()
    );

    let expired_context = super::super::rendezvous_trigger::RendezvousTriggerContext::new(
        requested_peer,
        server,
        Instant::now() - Duration::from_secs(60),
    );
    let valid_record = signed_discovery_peer_record(
        &requested_key,
        vec![crate::dht::PeerDiscoverySource::PeerExchange],
        crate::libp2p_net::now_ms(),
    );
    let (valid_cache, valid_observations, diagnostics) = handle_cached_peer_exchange_response(
        &local_key,
        &valid_record,
        cached_peer_record_request(requested_peer, proxy, Some(expired_context.clone())),
        proxy,
        &[],
    );
    assert!(valid_cache.get(&requested_peer).is_some());
    assert!(expired_context.active_evidence(Instant::now()).is_empty());
    assert!(
        valid_observations
            .rendezvous_trigger_servers(&valid_record, Instant::now())
            .is_empty()
    );
    assert!(
        diagnostics
            .iter()
            .all(|message| !message.starts_with("rendezvous-trigger-observation"))
    );
}

#[test]
fn rr_success_links_only_the_current_attempt_evidence() {
    let local_key = Keypair::generate_ed25519();
    let target_key = Keypair::generate_ed25519();
    let target = PeerId::from(target_key.public());
    let proxy = PeerId::random();
    let server = PeerId::random();
    let context = super::super::rendezvous_trigger::RendezvousTriggerContext::new(
        target,
        server,
        Instant::now(),
    );
    let signed = signed_discovery_peer_record(
        &target_key,
        vec![crate::dht::PeerDiscoverySource::PeerExchange],
        crate::libp2p_net::now_ms(),
    );
    let (cache, observations, diagnostics) = handle_cached_peer_exchange_response(
        &local_key,
        &signed,
        cached_peer_record_request(target, proxy, Some(context.clone())),
        proxy,
        &[],
    );
    assert!(cache.get(&target).is_some());
    assert_eq!(
        observations.rendezvous_trigger_servers(&signed, Instant::now()),
        vec![server]
    );
    assert!(context.active_evidence(Instant::now()).is_empty());
    assert!(diagnostics.iter().any(|message| {
        message.starts_with("rendezvous-trigger-observation")
            && message.contains(&target.to_string())
            && message.contains(&proxy.to_string())
            && message.contains(&server.to_string())
    }));
}

#[cfg(unix)]
#[test]
fn exact_duplicate_remote_record_merges_trigger_without_refreshing_candidate() {
    use oasis7_proto::distributed_dht::{PeerRecordPublicationStateV2, PeerRecordPublicationV2};

    let local_key = Keypair::generate_ed25519();
    let local_template = signed_discovery_peer_record(&local_key, Vec::new(), 1).record;
    let (_root, receiver) = initialized_receiver(&local_key, &local_template);
    let target_key = Keypair::generate_ed25519();
    let target = PeerId::from(target_key.public());
    let upstream = PeerId::random();
    let mut body =
        signed_discovery_peer_record(&target_key, Vec::new(), crate::libp2p_net::now_ms()).record;
    body.direct_addrs.clear();
    let signed = super::super::peer_record::sign_peer_record_v2(
        &body,
        PeerRecordPublicationV2 {
            schema_version: 2,
            sequence: 1,
            state: PeerRecordPublicationStateV2::Live,
        },
        &target_key,
    )
    .expect("sign V2 record");
    let mut swarm = super::super::swarm_behaviour::build_swarm_with_receiver(
        &local_key,
        false,
        false,
        Duration::from_secs(3),
        super::super::wire_bytes::init_shared_wire_byte_counters(),
        receiver.clone(),
    );
    let mut cache = PeerRecordCache::with_dial_policy_and_receiver(
        super::super::peer_record_time::DiscoveryClock::default(),
        Default::default(),
        receiver,
    );
    let mut observations =
        super::super::peer_discovery_observations::DiscoveryObservations::default();
    let mut known_paths = HashMap::new();
    let mut last_dialed_paths = HashMap::new();
    let active_paths = HashMap::new();
    let mut failed_path_labels = HashSet::new();
    let admit = || {
        let payload = to_canonical_cbor(&signed).expect("encode exact signed record");
        super::super::peer_record_admission::AdmittedPeerRecord::admit_and_decode_with(
            &super::super::peer_record_admission::PeerRecordAdmissionBudget::shared(),
            Some(upstream),
            super::super::peer_record_admission::PeerRecordAdmissionWorkUnit::PeerRecordResponse,
            &payload,
            super::super::peer_record::MAX_SIGNED_PEER_RECORD_BYTES,
            super::super::peer_record::decode_peer_record,
        )
        .expect("admit exact signed record")
    };

    super::super::peer_record_candidate::install_admitted_peer_record_with_rendezvous_triggers(
        &mut swarm,
        &mut cache,
        &mut observations,
        &mut known_paths,
        &mut last_dialed_paths,
        &active_paths,
        &mut failed_path_labels,
        Some(&local_template),
        &PeerManagerPolicy::default(),
        &HashSet::new(),
        admit(),
        Some(upstream),
        super::super::peer_record_admission::PeerRecordAdmissionWorkUnit::PeerRecordResponse,
        super::super::peer_discovery_observations::DiscoveryIngress::PeerExchange { upstream },
        None,
    )
    .expect("first live record installs");
    let original_candidate = cache.get(&target).cloned().expect("candidate installed");
    let original_sources = observations.observed_sources(&signed);
    let server = PeerId::random();
    let context = super::super::rendezvous_trigger::RendezvousTriggerContext::new(
        target,
        server,
        Instant::now(),
    );
    super::super::peer_record_candidate::install_admitted_peer_record_with_rendezvous_triggers(
        &mut swarm,
        &mut cache,
        &mut observations,
        &mut known_paths,
        &mut last_dialed_paths,
        &active_paths,
        &mut failed_path_labels,
        Some(&local_template),
        &PeerManagerPolicy::default(),
        &HashSet::new(),
        admit(),
        Some(upstream),
        super::super::peer_record_admission::PeerRecordAdmissionWorkUnit::PeerRecordResponse,
        super::super::peer_discovery_observations::DiscoveryIngress::PeerExchange { upstream },
        Some(&context),
    )
    .expect("exact remote duplicate is idempotent");

    assert_eq!(cache.get(&target), Some(&original_candidate));
    assert_eq!(observations.observed_sources(&signed), original_sources);
    assert_eq!(
        observations.rendezvous_trigger_servers(&signed, Instant::now()),
        vec![server],
        "the exact successful remote response may merge its current trigger"
    );
    assert!(context.active_evidence(Instant::now()).is_empty());
}

#[test]
fn local_kad_record_drops_rendezvous_trigger_without_backfilling_observation() {
    let local_key = Keypair::generate_ed25519();
    let target_key = Keypair::generate_ed25519();
    let target = PeerId::from(target_key.public());
    let rendezvous_server = PeerId::random();
    let signed = signed_discovery_peer_record(
        &target_key,
        vec![crate::dht::PeerDiscoverySource::Dht],
        crate::libp2p_net::now_ms(),
    );
    let context = super::super::rendezvous_trigger::RendezvousTriggerContext::new(
        target,
        rendezvous_server,
        Instant::now(),
    );
    let mut pending = super::super::kad_queries::PendingDhtQuery::DiscoverPeerRecord {
        peer_id: target,
        upstream: None,
        record: None,
        error: None,
        rendezvous_triggers: Some(context.clone()),
    };
    let key = "local-cache-peer-record".to_string();
    let found = kad::QueryResult::GetRecord(Ok(kad::GetRecordOk::FoundRecord(kad::PeerRecord {
        peer: None,
        record: kad::Record {
            key: RecordKey::new(&key),
            value: to_canonical_cbor(&signed).expect("encode local cached record"),
            publisher: None,
            expires: None,
        },
    })));
    let DhtProgressAction::DiscoverPeerRecord {
        upstream: None,
        result,
        rendezvous_triggers,
        ..
    } = super::super::kad_queries::handle_dht_progress(
        &mut pending,
        found,
        true,
        &super::super::peer_record_time::DiscoveryClock::default(),
    )
    else {
        panic!("local Kademlia result should complete the query");
    };
    let triggers =
        super::super::rendezvous_trigger::context_for_dht_result(rendezvous_triggers, None, false);
    assert!(triggers.is_none());
    assert!(context.active_evidence(Instant::now()).is_empty());
    let admitted = result
        .expect("valid local signed record")
        .expect("record found");

    let template = signed_discovery_peer_record(&local_key, Vec::new(), 1).record;
    let mut swarm = super::super::swarm_behaviour::build_swarm(
        &local_key,
        false,
        false,
        Duration::from_secs(3),
        super::super::wire_bytes::init_shared_wire_byte_counters(),
    );
    let mut cache = PeerRecordCache::default();
    let mut observations =
        super::super::peer_discovery_observations::DiscoveryObservations::default();
    let mut known_paths = HashMap::new();
    let mut last_dialed_paths = HashMap::new();
    let active_paths = HashMap::new();
    let mut failed_path_labels = HashSet::new();
    super::super::peer_record_candidate::install_admitted_peer_record(
        &mut swarm,
        &mut cache,
        &mut observations,
        &mut known_paths,
        &mut last_dialed_paths,
        &active_paths,
        &mut failed_path_labels,
        Some(&template),
        &PeerManagerPolicy::default(),
        &HashSet::new(),
        admitted,
        None,
        super::super::peer_record_admission::PeerRecordAdmissionWorkUnit::KadDiscoveryPeerRecordResult,
        super::super::peer_discovery_observations::DiscoveryIngress::Dht { upstream: None },
    )
    .expect("local cache record retains normal candidate handling");
    assert!(
        observations
            .rendezvous_trigger_servers(&signed, Instant::now())
            .is_empty()
    );
}

#[cfg(unix)]
#[test]
fn successful_tombstone_consumes_trigger_before_a_later_live_record() {
    use oasis7_proto::distributed_dht::{PeerRecordPublicationStateV2, PeerRecordPublicationV2};
    use std::os::unix::fs::PermissionsExt;

    let local_key = Keypair::generate_ed25519();
    let local_template = signed_discovery_peer_record(&local_key, Vec::new(), 1).record;
    let physical_temp_dir =
        std::fs::canonicalize(std::env::temp_dir()).expect("resolve temporary directory");
    let root = tempfile::tempdir_in(physical_temp_dir).expect("receiver root");
    let receiver_dir = root.path().join("receiver");
    std::fs::create_dir(&receiver_dir).expect("create receiver directory");
    std::fs::set_permissions(&receiver_dir, std::fs::Permissions::from_mode(0o700))
        .expect("restrict receiver directory");
    let receiver_config = super::super::config::PeerRecordReceiverConfig {
        mode: super::super::config::PeerRecordReceiverMode::DurableCompatible,
        directory: Some(receiver_dir),
    };
    super::super::receiver_floor::PeerRecordReceiver::initialize(
        &receiver_config,
        &local_key,
        &local_template,
    )
    .expect("initialize durable receiver");
    let receiver = super::super::receiver_floor::PeerRecordReceiver::open(
        &receiver_config,
        &local_key,
        Some(&local_template),
    )
    .expect("open durable receiver");

    let target_key = Keypair::generate_ed25519();
    let target = PeerId::from(target_key.public());
    let upstream = PeerId::random();
    let mut body =
        signed_discovery_peer_record(&target_key, Vec::new(), crate::libp2p_net::now_ms()).record;
    body.direct_addrs.clear();
    body.hole_punch_addrs.clear();
    body.relay_addrs.clear();
    let tombstone = super::super::peer_record::sign_peer_record_v2(
        &body,
        PeerRecordPublicationV2 {
            schema_version: 2,
            sequence: 1,
            state: PeerRecordPublicationStateV2::Tombstone,
        },
        &target_key,
    )
    .expect("sign tombstone");
    let live = super::super::peer_record::sign_peer_record_v2(
        &body,
        PeerRecordPublicationV2 {
            schema_version: 2,
            sequence: 2,
            state: PeerRecordPublicationStateV2::Live,
        },
        &target_key,
    )
    .expect("sign resumed live record");

    let trigger = super::super::rendezvous_trigger::RendezvousTriggerContext::new(
        target,
        PeerId::random(),
        Instant::now(),
    );
    let mut swarm = super::super::swarm_behaviour::build_swarm_with_receiver(
        &local_key,
        false,
        false,
        Duration::from_secs(3),
        super::super::wire_bytes::init_shared_wire_byte_counters(),
        receiver.clone(),
    );
    let mut cache = PeerRecordCache::with_dial_policy_and_receiver(
        super::super::peer_record_time::DiscoveryClock::default(),
        Default::default(),
        receiver,
    );
    let mut observations =
        super::super::peer_discovery_observations::DiscoveryObservations::default();
    let mut known_paths = HashMap::new();
    let mut last_dialed_paths = HashMap::new();
    let active_paths = HashMap::new();
    let mut failed_path_labels = HashSet::new();

    for (signed, sequence) in [(&tombstone, 1), (&live, 2)] {
        assert_eq!(signed.publication.as_ref().unwrap().sequence, sequence);
        let payload = to_canonical_cbor(signed).expect("encode signed V2 record");
        let admitted = super::super::peer_record_admission::AdmittedPeerRecord::admit_and_decode_with(
            &super::super::peer_record_admission::PeerRecordAdmissionBudget::shared(),
            Some(upstream),
            super::super::peer_record_admission::PeerRecordAdmissionWorkUnit::PeerRecordResponse,
            &payload,
            super::super::peer_record::MAX_SIGNED_PEER_RECORD_BYTES,
            super::super::peer_record::decode_peer_record,
        )
        .expect("admit signed record");
        super::super::peer_record_candidate::install_admitted_peer_record_with_rendezvous_triggers(
            &mut swarm,
            &mut cache,
            &mut observations,
            &mut known_paths,
            &mut last_dialed_paths,
            &active_paths,
            &mut failed_path_labels,
            Some(&local_template),
            &PeerManagerPolicy::default(),
            &HashSet::new(),
            admitted,
            Some(upstream),
            super::super::peer_record_admission::PeerRecordAdmissionWorkUnit::PeerRecordResponse,
            super::super::peer_discovery_observations::DiscoveryIngress::PeerExchange { upstream },
            Some(&trigger),
        )
        .expect("tombstone and higher live record are both accepted");
    }
    assert!(trigger.active_evidence(Instant::now()).is_empty());
    assert!(
        observations
            .rendezvous_trigger_servers(&live, Instant::now())
            .is_empty()
    );
}
