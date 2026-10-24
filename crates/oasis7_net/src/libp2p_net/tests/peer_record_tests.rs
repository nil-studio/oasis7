use super::super::discovery::peer_record_enables_rendezvous;
use super::super::peer_record::{build_configured_peer_record, verify_signed_peer_record};
use super::*;
use crate::util::to_canonical_cbor;
use libp2p::kad::RecordKey;
use oasis7_proto::distributed::dht_peer_record_key;
use serde::Serialize;

#[derive(Serialize)]
struct LegacySignedPeerRecordV1<'a> {
    record: &'a PeerRecord,
    identity_public_key_protobuf_hex: &'a str,
    signature_hex: &'a str,
}
use oasis7_proto::distributed_dht::{
    PeerRecordPublicationStateV2, PeerRecordPublicationV2, SignedPeerRecord,
};

#[test]
fn sign_and_verify_peer_record_round_trip() {
    let keypair = Keypair::generate_ed25519();
    let record = PeerRecord {
        peer_id: PeerId::from(keypair.public()).to_string(),
        node_id: "node-a".to_string(),
        world_id: "world-a".to_string(),
        network_id: "network-a".to_string(),
        node_role: PeerNodeRole::ValidatorCore.as_str().to_string(),
        deployment_mode: PeerDeploymentMode::ValidatorHidden,
        reachability_class: crate::dht::PeerReachabilityClass::Private,
        direct_addrs: Vec::new(),
        hole_punch_addrs: Vec::new(),
        relay_addrs: Vec::new(),
        discovery_sources: vec![
            crate::dht::PeerDiscoverySource::StaticBootstrap,
            crate::dht::PeerDiscoverySource::Dht,
        ],
        capability_lanes: PeerNodeRole::ValidatorCore.default_capability_lanes(),
        source_operator: None,
        source_asn: None,
        published_at_ms: 42,
        ttl_ms: 60_000,
    };

    let signed = sign_peer_record(&record, &keypair).expect("sign peer record");
    verify_signed_peer_record(&signed).expect("verify peer record");
    let legacy_shape = LegacySignedPeerRecordV1 {
        record: &signed.record,
        identity_public_key_protobuf_hex: &signed.identity_public_key_protobuf_hex,
        signature_hex: &signed.signature_hex,
    };
    assert_eq!(
        to_canonical_cbor(&signed).expect("encode V1 struct"),
        to_canonical_cbor(&legacy_shape).expect("encode pre-V2 wire struct"),
        "V1 CBOR must remain byte-identical to the three-field wire shape"
    );
}

#[test]
fn legacy_peer_record_v1_signing_vector_remains_stable() {
    let keypair = Keypair::ed25519_from_bytes([7u8; 32]).expect("fixed test key");
    let record = PeerRecord {
        peer_id: PeerId::from(keypair.public()).to_string(),
        node_id: "node-vector".into(),
        world_id: "world-vector".into(),
        network_id: "network-vector".into(),
        node_role: PeerNodeRole::FullStorage.as_str().into(),
        deployment_mode: PeerDeploymentMode::Hybrid,
        reachability_class: crate::dht::PeerReachabilityClass::Hybrid,
        direct_addrs: Vec::new(),
        hole_punch_addrs: Vec::new(),
        relay_addrs: Vec::new(),
        discovery_sources: vec![crate::dht::PeerDiscoverySource::Dht],
        capability_lanes: PeerNodeRole::FullStorage.default_capability_lanes(),
        source_operator: None,
        source_asn: None,
        published_at_ms: 42,
        ttl_ms: 60_000,
    };
    let signed = sign_peer_record(&record, &keypair).expect("sign fixed V1 vector");
    verify_signed_peer_record(&signed).expect("verify fixed V1 vector");
    assert_eq!(signed.publication, None);
    let mut legacy_payload = b"oasis7-peer-record-v1|".to_vec();
    legacy_payload.extend_from_slice(&to_canonical_cbor(&record).expect("encode old V1 payload"));
    let legacy_signature = keypair.sign(&legacy_payload).expect("sign old V1 payload");
    let expected_signature = "72bce61a227e248f8d25b930c9a8953d85d70bdf702e40e980781090b872e786bae2e033c174a33f5f137ac91d93ef603b76749ba097cf576542c16aa1a17203";
    assert_eq!(hex::encode(legacy_signature), expected_signature);
    assert_eq!(signed.signature_hex, expected_signature);
}

#[test]
fn peer_record_v2_valid_signature_uses_the_full_versioned_tuple() {
    let keypair = Keypair::generate_ed25519();
    let peer_id = PeerId::from(keypair.public());
    let record = PeerRecord {
        peer_id: peer_id.to_string(),
        node_id: "node-v2".to_string(),
        world_id: "world-v2".to_string(),
        network_id: "network-v2".to_string(),
        node_role: PeerNodeRole::FullStorage.as_str().to_string(),
        deployment_mode: PeerDeploymentMode::Hybrid,
        reachability_class: crate::dht::PeerReachabilityClass::Hybrid,
        direct_addrs: vec!["/ip4/192.0.2.7/tcp/4103".to_string()],
        hole_punch_addrs: Vec::new(),
        relay_addrs: Vec::new(),
        discovery_sources: Vec::new(),
        capability_lanes: PeerNodeRole::FullStorage.default_capability_lanes(),
        source_operator: None,
        source_asn: None,
        published_at_ms: 42,
        ttl_ms: 60_000,
    };
    let signed = sign_v2_peer_record(&record, &keypair, 1);

    verify_signed_peer_record(&signed).expect("valid V2 signature must verify");
}

#[test]
fn peer_record_v2_rejects_sequence_and_payload_tampering() {
    let keypair = Keypair::generate_ed25519();
    let peer_id = PeerId::from(keypair.public());
    let record = PeerRecord {
        peer_id: peer_id.to_string(),
        node_id: "node-v2".to_string(),
        world_id: "world-v2".to_string(),
        network_id: "network-v2".to_string(),
        node_role: PeerNodeRole::FullStorage.as_str().to_string(),
        deployment_mode: PeerDeploymentMode::Hybrid,
        reachability_class: crate::dht::PeerReachabilityClass::Hybrid,
        direct_addrs: vec!["/ip4/192.0.2.7/tcp/4103".to_string()],
        hole_punch_addrs: Vec::new(),
        relay_addrs: Vec::new(),
        discovery_sources: Vec::new(),
        capability_lanes: PeerNodeRole::FullStorage.default_capability_lanes(),
        source_operator: None,
        source_asn: None,
        published_at_ms: 42,
        ttl_ms: 60_000,
    };
    let signed = sign_v2_peer_record(&record, &keypair, 7);

    let mut changed_sequence = signed.clone();
    changed_sequence
        .publication
        .as_mut()
        .expect("V2 publication")
        .sequence += 1;
    assert!(verify_signed_peer_record(&changed_sequence).is_err());

    let mut changed_version = signed.clone();
    changed_version
        .publication
        .as_mut()
        .expect("V2 publication")
        .schema_version += 1;
    assert!(verify_signed_peer_record(&changed_version).is_err());

    let mut changed_record = signed.clone();
    changed_record.record.direct_addrs[0] = "/ip4/192.0.2.8/tcp/4103".to_string();
    assert!(verify_signed_peer_record(&changed_record).is_err());
}

#[test]
fn peer_record_v2_signature_is_not_accepted_by_legacy_v1_domain() {
    let keypair = Keypair::generate_ed25519();
    let peer_id = PeerId::from(keypair.public());
    let record = PeerRecord {
        peer_id: peer_id.to_string(),
        node_id: "node-v2".to_string(),
        world_id: "world-v2".to_string(),
        network_id: "network-v2".to_string(),
        node_role: PeerNodeRole::FullStorage.as_str().to_string(),
        deployment_mode: PeerDeploymentMode::Hybrid,
        reachability_class: crate::dht::PeerReachabilityClass::Hybrid,
        direct_addrs: Vec::new(),
        hole_punch_addrs: Vec::new(),
        relay_addrs: Vec::new(),
        discovery_sources: Vec::new(),
        capability_lanes: PeerNodeRole::FullStorage.default_capability_lanes(),
        source_operator: None,
        source_asn: None,
        published_at_ms: 42,
        ttl_ms: 60_000,
    };
    let signed = sign_v2_peer_record(&record, &keypair, 1);

    let mut legacy_payload = b"oasis7-peer-record-v1|".to_vec();
    legacy_payload.extend_from_slice(&to_canonical_cbor(&signed.record).expect("legacy payload"));
    let signature = hex::decode(&signed.signature_hex).expect("signature hex");
    assert!(
        !keypair
            .public()
            .verify(&legacy_payload, signature.as_slice())
    );
}

fn sign_v2_peer_record(record: &PeerRecord, keypair: &Keypair, sequence: u64) -> SignedPeerRecord {
    let publication = PeerRecordPublicationV2 {
        schema_version: 2,
        sequence,
        state: PeerRecordPublicationStateV2::Live,
    };
    let mut payload = b"oasis7-peer-record-v2|".to_vec();
    payload.extend_from_slice(
        &to_canonical_cbor(&(
            publication.schema_version,
            publication.sequence,
            publication.state,
            record,
        ))
        .expect("V2 signing payload"),
    );
    let signature = keypair.sign(&payload).expect("sign V2 record");
    SignedPeerRecord {
        record: record.clone(),
        identity_public_key_protobuf_hex: hex::encode(keypair.public().encode_protobuf()),
        signature_hex: hex::encode(signature),
        publication: Some(publication),
    }
}

#[test]
fn build_configured_peer_record_excludes_loopback_direct_addrs_by_default() {
    let keypair = Keypair::generate_ed25519();
    let reachability = Arc::new(Mutex::new(Libp2pReachabilitySnapshot::default()));
    let listening_addrs = Arc::new(Mutex::new(vec![
        "/ip4/127.0.0.1/udp/4103/quic-v1"
            .parse()
            .expect("direct listen addr"),
        format!(
            "/dns4/relay.example/tcp/443/p2p/{}/p2p-circuit",
            PeerId::random()
        )
        .parse()
        .expect("relay listen addr"),
    ]));
    let signed = build_configured_peer_record(
        &keypair,
        &PeerRecord {
            peer_id: String::new(),
            node_id: "node-a".to_string(),
            world_id: "world-a".to_string(),
            network_id: "network-a".to_string(),
            node_role: PeerNodeRole::FullStorage.as_str().to_string(),
            deployment_mode: PeerDeploymentMode::Hybrid,
            reachability_class: crate::dht::PeerReachabilityClass::Hybrid,
            direct_addrs: Vec::new(),
            hole_punch_addrs: Vec::new(),
            relay_addrs: Vec::new(),
            discovery_sources: vec![crate::dht::PeerDiscoverySource::Dht],
            capability_lanes: PeerNodeRole::FullStorage.default_capability_lanes(),
            source_operator: None,
            source_asn: None,
            published_at_ms: 0,
            ttl_ms: 60_000,
        },
        &listening_addrs,
        &reachability,
        false,
    )
    .expect("build peer record");
    assert!(signed.record.direct_addrs.is_empty());
    assert_eq!(signed.record.hole_punch_addrs, Vec::<String>::new());
    assert_eq!(signed.record.relay_addrs.len(), 1);
    assert!(signed.record.relay_addrs[0].contains("/p2p-circuit"));
}

#[test]
fn peer_reachability_contract_normalizes_record_identity_policy_and_ranked_paths() {
    let keypair = Keypair::generate_ed25519();
    let peer_id = PeerId::from(keypair.public());
    let relay_peer = PeerId::random();
    let signed = sign_peer_record(
        &PeerRecord {
            peer_id: peer_id.to_string(),
            node_id: "node-a".to_string(),
            world_id: "world-a".to_string(),
            network_id: "network-a".to_string(),
            node_role: PeerNodeRole::FullStorage.as_str().to_string(),
            deployment_mode: PeerDeploymentMode::Hybrid,
            reachability_class: crate::dht::PeerReachabilityClass::Hybrid,
            direct_addrs: vec!["/ip4/127.0.0.1/udp/4103/quic-v1".to_string()],
            hole_punch_addrs: vec!["/ip4/10.0.0.7/tcp/4104".to_string()],
            relay_addrs: vec![format!(
                "/dns4/relay.example/tcp/443/p2p/{relay_peer}/p2p-circuit"
            )],
            discovery_sources: vec![crate::dht::PeerDiscoverySource::Dht],
            capability_lanes: vec![NetworkLane::BlobState, NetworkLane::Control],
            source_operator: None,
            source_asn: None,
            published_at_ms: 0,
            ttl_ms: 60_000,
        },
        &keypair,
    )
    .expect("sign peer record");

    let contract = PeerReachabilityContract::from_signed_peer_record(&signed).expect("contract");

    assert_eq!(contract.peer_id, peer_id);
    assert_eq!(contract.node_role, PeerNodeRole::FullStorage);
    assert_eq!(contract.deployment_mode, PeerDeploymentMode::Hybrid);
    assert_eq!(
        contract.reachability_class,
        crate::dht::PeerReachabilityClass::Hybrid
    );
    assert!(contract.capability_lanes.contains(&NetworkLane::Control));
    assert!(contract.capability_lanes.contains(&NetworkLane::BlobState));
    assert_eq!(contract.publish_source.direct_path_count, 1);
    assert_eq!(contract.publish_source.hole_punch_path_count, 1);
    assert_eq!(contract.publish_source.relay_path_count, 1);
    assert_eq!(
        contract
            .ranked_paths
            .iter()
            .map(|path| path.kind)
            .collect::<Vec<_>>(),
        vec![
            TransportPathKind::Direct,
            TransportPathKind::HolePunched,
            TransportPathKind::RelayReserved
        ]
    );
}

#[test]
fn build_configured_peer_record_keeps_private_validator_without_direct_addrs() {
    let keypair = Keypair::generate_ed25519();
    let reachability = Arc::new(Mutex::new(Libp2pReachabilitySnapshot::default()));
    let listening_addrs = Arc::new(Mutex::new(vec![
        "/ip4/127.0.0.1/tcp/4202"
            .parse()
            .expect("direct listen addr"),
        format!(
            "/dns4/relay.example/tcp/443/p2p/{}/p2p-circuit",
            PeerId::random()
        )
        .parse()
        .expect("relay listen addr"),
    ]));
    let signed = build_configured_peer_record(
        &keypair,
        &PeerRecord {
            peer_id: String::new(),
            node_id: "node-private-validator".to_string(),
            world_id: "world-a".to_string(),
            network_id: "network-a".to_string(),
            node_role: PeerNodeRole::ValidatorCore.as_str().to_string(),
            deployment_mode: PeerDeploymentMode::Private,
            reachability_class: crate::dht::PeerReachabilityClass::Private,
            direct_addrs: Vec::new(),
            hole_punch_addrs: Vec::new(),
            relay_addrs: Vec::new(),
            discovery_sources: vec![
                crate::dht::PeerDiscoverySource::StaticBootstrap,
                crate::dht::PeerDiscoverySource::Dht,
            ],
            capability_lanes: PeerNodeRole::ValidatorCore.default_capability_lanes(),
            source_operator: None,
            source_asn: None,
            published_at_ms: 0,
            ttl_ms: 60_000,
        },
        &listening_addrs,
        &reachability,
        false,
    )
    .expect("build private validator peer record");

    assert!(signed.record.direct_addrs.is_empty());
    assert_eq!(signed.record.relay_addrs.len(), 1);
}

#[test]
fn build_configured_peer_record_allows_loopback_direct_addrs_for_testing() {
    let keypair = Keypair::generate_ed25519();
    let reachability = Arc::new(Mutex::new(Libp2pReachabilitySnapshot::default()));
    let listening_addrs = Arc::new(Mutex::new(vec![
        "/ip4/127.0.0.1/udp/4103/quic-v1"
            .parse()
            .expect("direct listen addr"),
        format!(
            "/dns4/relay.example/tcp/443/p2p/{}/p2p-circuit",
            PeerId::random()
        )
        .parse()
        .expect("relay listen addr"),
    ]));
    let signed = build_configured_peer_record(
        &keypair,
        &PeerRecord {
            peer_id: String::new(),
            node_id: "node-loopback-test".to_string(),
            world_id: "world-a".to_string(),
            network_id: "network-a".to_string(),
            node_role: PeerNodeRole::FullStorage.as_str().to_string(),
            deployment_mode: PeerDeploymentMode::Hybrid,
            reachability_class: crate::dht::PeerReachabilityClass::Hybrid,
            direct_addrs: Vec::new(),
            hole_punch_addrs: Vec::new(),
            relay_addrs: Vec::new(),
            discovery_sources: vec![crate::dht::PeerDiscoverySource::Dht],
            capability_lanes: PeerNodeRole::FullStorage.default_capability_lanes(),
            source_operator: None,
            source_asn: None,
            published_at_ms: 0,
            ttl_ms: 60_000,
        },
        &listening_addrs,
        &reachability,
        true,
    )
    .expect("build loopback testing peer record");

    assert_eq!(
        signed.record.direct_addrs,
        vec!["/ip4/127.0.0.1/udp/4103/quic-v1".to_string()]
    );
    assert_eq!(signed.record.relay_addrs.len(), 1);
}

#[test]
fn build_configured_peer_record_prefers_confirmed_external_direct_addrs() {
    let keypair = Keypair::generate_ed25519();
    let reachability = Arc::new(Mutex::new(Libp2pReachabilitySnapshot {
        confirmed_external_direct_addrs: vec!["/dns4/public.example/tcp/4103".to_string()],
        ..Libp2pReachabilitySnapshot::default()
    }));
    let listening_addrs = Arc::new(Mutex::new(vec![
        "/ip4/10.0.0.2/udp/4103/quic-v1"
            .parse()
            .expect("private listen addr"),
        format!(
            "/dns4/relay.example/tcp/443/p2p/{}/p2p-circuit",
            PeerId::random()
        )
        .parse()
        .expect("relay listen addr"),
    ]));
    let signed = build_configured_peer_record(
        &keypair,
        &PeerRecord {
            peer_id: String::new(),
            node_id: "node-public".to_string(),
            world_id: "world-a".to_string(),
            network_id: "network-a".to_string(),
            node_role: PeerNodeRole::FullStorage.as_str().to_string(),
            deployment_mode: PeerDeploymentMode::Hybrid,
            reachability_class: crate::dht::PeerReachabilityClass::Hybrid,
            direct_addrs: Vec::new(),
            hole_punch_addrs: Vec::new(),
            relay_addrs: Vec::new(),
            discovery_sources: vec![crate::dht::PeerDiscoverySource::Dht],
            capability_lanes: PeerNodeRole::FullStorage.default_capability_lanes(),
            source_operator: None,
            source_asn: None,
            published_at_ms: 0,
            ttl_ms: 60_000,
        },
        &listening_addrs,
        &reachability,
        false,
    )
    .expect("build public peer record");

    assert_eq!(
        signed.record.direct_addrs,
        vec!["/dns4/public.example/tcp/4103".to_string()]
    );
    assert_eq!(signed.record.relay_addrs.len(), 1);
}

#[test]
fn peer_record_enables_rendezvous_only_when_source_is_declared() {
    let without_rendezvous = PeerRecord {
        peer_id: String::new(),
        node_id: "node-no-rendezvous".to_string(),
        world_id: "world-a".to_string(),
        network_id: "network-a".to_string(),
        node_role: PeerNodeRole::FullStorage.as_str().to_string(),
        deployment_mode: PeerDeploymentMode::Private,
        reachability_class: crate::dht::PeerReachabilityClass::Private,
        direct_addrs: Vec::new(),
        hole_punch_addrs: Vec::new(),
        relay_addrs: Vec::new(),
        discovery_sources: vec![
            crate::dht::PeerDiscoverySource::StaticBootstrap,
            crate::dht::PeerDiscoverySource::Dht,
        ],
        capability_lanes: PeerNodeRole::FullStorage.default_capability_lanes(),
        source_operator: None,
        source_asn: None,
        published_at_ms: 0,
        ttl_ms: 60_000,
    };
    assert!(!peer_record_enables_rendezvous(&without_rendezvous));

    let with_rendezvous = PeerRecord {
        discovery_sources: vec![
            crate::dht::PeerDiscoverySource::StaticBootstrap,
            crate::dht::PeerDiscoverySource::Rendezvous,
        ],
        ..without_rendezvous
    };
    assert!(peer_record_enables_rendezvous(&with_rendezvous));
}

#[test]
fn dht_get_peer_record_decodes_and_verifies_record() {
    let keypair = Keypair::generate_ed25519();
    let signed = sign_peer_record(
        &PeerRecord {
            peer_id: PeerId::from(keypair.public()).to_string(),
            node_id: "node-a".to_string(),
            world_id: "world-a".to_string(),
            network_id: "network-a".to_string(),
            node_role: PeerNodeRole::FullStorage.as_str().to_string(),
            deployment_mode: PeerDeploymentMode::Hybrid,
            reachability_class: crate::dht::PeerReachabilityClass::Hybrid,
            direct_addrs: vec!["/ip4/127.0.0.1/tcp/4102".to_string()],
            hole_punch_addrs: Vec::new(),
            relay_addrs: vec!["/dns4/relay.example/tcp/443".to_string()],
            discovery_sources: vec![crate::dht::PeerDiscoverySource::Dht],
            capability_lanes: PeerNodeRole::FullStorage.default_capability_lanes(),
            source_operator: None,
            source_asn: None,
            published_at_ms: crate::libp2p_net::now_ms(),
            ttl_ms: 60_000,
        },
        &keypair,
    )
    .expect("sign peer record");
    let payload = to_canonical_cbor(&signed).expect("encode peer record");
    let key = dht_peer_record_key("world-a", signed.record.peer_id.as_str());
    let mut pending = PendingDhtQuery::GetPeerRecord {
        peer_id: PeerId::from(keypair.public()),
        response: None,
        record: None,
        error: None,
    };
    let result = kad::QueryResult::GetRecord(Ok(kad::GetRecordOk::FoundRecord(kad::PeerRecord {
        peer: None,
        record: kad::Record {
            key: RecordKey::new(&key),
            value: payload,
            publisher: None,
            expires: None,
        },
    })));
    handle_dht_progress(
        &mut pending,
        result,
        true,
        &super::super::peer_record_time::DiscoveryClock::default(),
    );
    let PendingDhtQuery::GetPeerRecord { record, .. } = pending else {
        panic!("unexpected query state");
    };
    assert_eq!(record, Some(signed));
}

#[test]
fn dht_get_peer_record_rejects_signed_record_for_requested_target_mismatch() {
    let wrong_keypair = Keypair::generate_ed25519();
    let wrong_peer_id = PeerId::from(wrong_keypair.public());
    let requested_peer_id = PeerId::random();
    assert_ne!(wrong_peer_id, requested_peer_id);
    let signed = super::signed_discovery_peer_record(
        &wrong_keypair,
        vec![crate::dht::PeerDiscoverySource::Dht],
        77,
    );
    let payload = to_canonical_cbor(&signed).expect("encode peer record");
    let key = dht_peer_record_key("world-a", requested_peer_id.to_string().as_str());
    let (sender, receiver) = std::sync::mpsc::channel();
    let mut pending = PendingDhtQuery::GetPeerRecord {
        peer_id: requested_peer_id,
        response: Some(sender),
        record: None,
        error: None,
    };
    let result = kad::QueryResult::GetRecord(Ok(kad::GetRecordOk::FoundRecord(kad::PeerRecord {
        peer: None,
        record: kad::Record {
            key: RecordKey::new(&key),
            value: payload,
            publisher: None,
            expires: None,
        },
    })));
    handle_dht_progress(
        &mut pending,
        result,
        true,
        &super::super::peer_record_time::DiscoveryClock::default(),
    );
    let response = receiver.recv().expect("command response");
    assert!(response.is_err(), "mismatched record must fail closed");
    let PendingDhtQuery::GetPeerRecord { record, .. } = pending else {
        panic!("unexpected query state");
    };
    assert!(record.is_none(), "mismatched record must not be returned");
}
