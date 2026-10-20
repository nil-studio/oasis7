use std::collections::BTreeMap;

#[test]
fn classify_transport_stability_marks_repeated_transport_errors_unstable() {
    let stability =
        super::status_payload::classify_transport_stability(&super::ChainReplicationDebugStatus {
            local_peer_id: "peer-local".to_string(),
            connected_peers: Vec::new(),
            peer_healths: Vec::new(),
            registered_protocols: Vec::new(),
            protocol_retry_cooldown_peers: BTreeMap::new(),
            transport_retry_cooldown_peers: Vec::new(),
            request_peer_scores: BTreeMap::new(),
            connection_events: Vec::new(),
            recent_errors: vec![
                "libp2p connection closed peer=peer-a".to_string(),
                "libp2p connection closed peer=peer-a".to_string(),
                "libp2p publish failed topic=x: InsufficientPeers".to_string(),
                "request failed: Timeout".to_string(),
                "request failed: Timeout".to_string(),
            ],
        });

    assert!(!stability.stable);
    assert!(stability.score < 70);
    assert_eq!(stability.connection_closed_count, 2);
    assert_eq!(stability.insufficient_peers_count, 1);
    assert_eq!(stability.timeout_count, 2);
}

#[test]
fn classify_transport_stability_ignores_reachability_diagnostics() {
    let stability =
        super::status_payload::classify_transport_stability(&super::ChainReplicationDebugStatus {
            local_peer_id: "peer-local".to_string(),
            connected_peers: vec!["peer-a".to_string(), "peer-b".to_string()],
            peer_healths: vec![super::ChainPeerHealthStatus {
                peer_id: "peer-a".to_string(),
                status: "active".to_string(),
                issues: Vec::new(),
                discovery_sources: vec!["static_bootstrap".to_string()],
                active_path_kind: Some("direct".to_string()),
                source_operator: None,
                source_asn: None,
            }, super::ChainPeerHealthStatus {
                peer_id: "peer-b".to_string(),
                status: "active".to_string(),
                issues: Vec::new(),
                discovery_sources: vec!["static_bootstrap".to_string()],
                active_path_kind: Some("direct".to_string()),
                source_operator: None,
                source_asn: None,
            }],
            registered_protocols: Vec::new(),
            protocol_retry_cooldown_peers: BTreeMap::from([(
                "/aw/node/replication/fetch-commit/1.0.0".to_string(),
                vec!["peer-b".to_string()],
            )]),
            transport_retry_cooldown_peers: Vec::new(),
            request_peer_scores: BTreeMap::new(),
            connection_events: Vec::new(),
            recent_errors: vec![
                "libp2p autonat event OutboundProbe(Error NoServer)".to_string(),
                "libp2p autonat event OutboundProbe(Error UnsupportedProtocols)".to_string(),
                "peer record request failed: ConnectionClosed".to_string(),
                "libp2p peer manager quarantine suppresses failover peer=peer-a".to_string(),
                "dial condition peer=peer-a already connected or dial in progress".to_string(),
                "libp2p connection established peer=peer-a".to_string(),
                "libp2p routing updated peer=peer-a addrs=[/ip4/203.0.113.10/tcp/4001]".to_string(),
                "libp2p transport active peer=peer-a kind=direct flavor=tcp+noise+yamux addr=/ip4/203.0.113.10/tcp/4001".to_string(),
                "libp2p connection closed peer=peer-a num_established=1 active_path=/ip4/203.0.113.10/tcp/4001".to_string(),
                "libp2p outgoing connection error peer=None: Transport([(/ip4/203.0.113.10/tcp/4001, Other(Custom { kind: Other, error: Connection refused }))])".to_string(),
                "libp2p redundant connections pruned peer=peer-a count=1".to_string(),
            ],
        });

    assert!(stability.stable);
    assert_eq!(stability.score, 100);
    assert_eq!(stability.blocking_error_count, 0);
    assert_eq!(stability.connection_closed_count, 0);
    assert_eq!(stability.protocol_error_count, 0);
}

#[test]
fn classify_transport_stability_ignores_local_peer_id_startup_noise() {
    let stability =
        super::status_payload::classify_transport_stability(&super::ChainReplicationDebugStatus {
            local_peer_id: "validator-a".to_string(),
            connected_peers: vec!["validator-b".to_string()],
            peer_healths: vec![super::ChainPeerHealthStatus {
                peer_id: "validator-b".to_string(),
                status: "active".to_string(),
                issues: Vec::new(),
                discovery_sources: vec!["static_bootstrap".to_string()],
                active_path_kind: Some("direct".to_string()),
                source_operator: Some("validator".to_string()),
                source_asn: None,
            }],
            registered_protocols: Vec::new(),
            protocol_retry_cooldown_peers: BTreeMap::new(),
            transport_retry_cooldown_peers: Vec::new(),
            request_peer_scores: BTreeMap::from([("validator-b".to_string(), 100)]),
            connection_events: Vec::new(),
            recent_errors: vec![
                "libp2p incoming connection error: LocalPeerId { endpoint: Listener { local_addr: /ip4/172.26.53.91/tcp/6831, send_back_addr: /ip4/39.104.204.172/tcp/34306 } }".to_string(),
                "libp2p outgoing connection error peer=Some(PeerId(\"validator-a\")): LocalPeerId { endpoint: Dialer { address: /ip4/39.104.204.172/tcp/6831/p2p/validator-a, role_override: Dialer } }".to_string(),
                "libp2p connection established peer=validator-b direction=outbound addr=/ip4/39.104.205.67/tcp/6832/p2p/validator-b".to_string(),
            ],
        });

    assert!(stability.stable);
    assert_eq!(stability.score, 100);
    assert_eq!(stability.blocking_error_count, 0);
    assert_eq!(stability.connection_closed_count, 0);
    assert_eq!(stability.protocol_error_count, 0);
}

#[test]
fn classify_transport_stability_recovers_when_active_request_peer_is_healthy() {
    let stability =
        super::status_payload::classify_transport_stability(&super::ChainReplicationDebugStatus {
            local_peer_id: "peer-local".to_string(),
            connected_peers: vec!["peer-a".to_string()],
            peer_healths: vec![super::ChainPeerHealthStatus {
                peer_id: "peer-a".to_string(),
                status: "active".to_string(),
                issues: Vec::new(),
                discovery_sources: vec!["static_bootstrap".to_string()],
                active_path_kind: Some("direct".to_string()),
                source_operator: None,
                source_asn: None,
            }],
            registered_protocols: Vec::new(),
            protocol_retry_cooldown_peers: BTreeMap::new(),
            transport_retry_cooldown_peers: Vec::new(),
            request_peer_scores: BTreeMap::from([("peer-a".to_string(), 100)]),
            connection_events: Vec::new(),
            recent_errors: vec![
                "request failed: Timeout protocol=/aw/node/replication/fetch-commit/1.0.0 peer=peer-b".to_string(),
                "request failed: Io(Custom { kind: UnexpectedEof, error: Eof { name: \"map\" } }) protocol=/aw/node/replication/fetch-commit/head/1.0.0 peer=peer-b".to_string(),
            ],
        });

    assert!(stability.stable);
    assert_eq!(stability.score, 100);
    assert_eq!(stability.blocking_error_count, 0);
    assert_eq!(stability.timeout_count, 0);
    assert_eq!(stability.protocol_error_count, 0);
}

#[test]
fn classify_transport_stability_tolerates_noisy_external_peer_with_healthy_validator_path() {
    let stability =
        super::status_payload::classify_transport_stability(&super::ChainReplicationDebugStatus {
            local_peer_id: "validator-a".to_string(),
            connected_peers: vec!["validator-b".to_string(), "observer-noisy".to_string()],
            peer_healths: vec![
                super::ChainPeerHealthStatus {
                    peer_id: "validator-b".to_string(),
                    status: "active".to_string(),
                    issues: Vec::new(),
                    discovery_sources: vec!["static_bootstrap".to_string()],
                    active_path_kind: Some("direct".to_string()),
                    source_operator: Some("validator".to_string()),
                    source_asn: None,
                },
                super::ChainPeerHealthStatus {
                    peer_id: "observer-noisy".to_string(),
                    status: "active".to_string(),
                    issues: Vec::new(),
                    discovery_sources: vec!["dht".to_string()],
                    active_path_kind: Some("direct".to_string()),
                    source_operator: Some("observer".to_string()),
                    source_asn: None,
                },
            ],
            registered_protocols: Vec::new(),
            protocol_retry_cooldown_peers: BTreeMap::new(),
            transport_retry_cooldown_peers: Vec::new(),
            request_peer_scores: BTreeMap::from([
                ("validator-b".to_string(), 100),
                ("observer-noisy".to_string(), 0),
            ]),
            connection_events: Vec::new(),
            recent_errors: vec![
                "libp2p inbound failure from PeerId(\"observer-noisy\"): Timeout".to_string(),
                "libp2p inbound failure from PeerId(\"observer-noisy\"): Timeout".to_string(),
                "libp2p inbound failure from PeerId(\"observer-noisy\"): Timeout".to_string(),
                "libp2p inbound failure from PeerId(\"observer-noisy\"): Timeout".to_string(),
                "libp2p incoming connection error: Transport(Other(Custom { kind: Other, error: ProtocolError(InvalidMessage) }))".to_string(),
            ],
        });

    assert!(stability.stable);
    assert_eq!(stability.score, 100);
    assert_eq!(stability.blocking_error_count, 0);
    assert_eq!(stability.timeout_count, 0);
    assert_eq!(stability.protocol_error_count, 0);
}

#[test]
fn classify_transport_stability_keeps_degraded_core_peer_inbound_errors_blocking() {
    let stability =
        super::status_payload::classify_transport_stability(&super::ChainReplicationDebugStatus {
            local_peer_id: "validator-a".to_string(),
            connected_peers: vec!["validator-b".to_string(), "validator-c".to_string()],
            peer_healths: vec![
                super::ChainPeerHealthStatus {
                    peer_id: "validator-b".to_string(),
                    status: "active".to_string(),
                    issues: Vec::new(),
                    discovery_sources: vec!["static_bootstrap".to_string()],
                    active_path_kind: Some("direct".to_string()),
                    source_operator: Some("validator".to_string()),
                    source_asn: None,
                },
                super::ChainPeerHealthStatus {
                    peer_id: "validator-c".to_string(),
                    status: "active".to_string(),
                    issues: Vec::new(),
                    discovery_sources: vec!["static_bootstrap".to_string()],
                    active_path_kind: Some("direct".to_string()),
                    source_operator: Some("validator".to_string()),
                    source_asn: None,
                },
            ],
            registered_protocols: Vec::new(),
            protocol_retry_cooldown_peers: BTreeMap::new(),
            transport_retry_cooldown_peers: Vec::new(),
            request_peer_scores: BTreeMap::from([
                ("validator-b".to_string(), 100),
                ("validator-c".to_string(), 0),
            ]),
            connection_events: Vec::new(),
            recent_errors: vec![
                "libp2p inbound failure from PeerId(\"validator-c\"): Timeout".to_string(),
                "libp2p inbound failure from PeerId(\"validator-c\"): Timeout".to_string(),
                "libp2p inbound failure from PeerId(\"validator-c\"): Timeout".to_string(),
                "libp2p inbound failure from PeerId(\"validator-c\"): Timeout".to_string(),
            ],
        });

    assert!(!stability.stable);
    assert!(stability.score < 70);
    assert_eq!(stability.blocking_error_count, 4);
    assert_eq!(stability.timeout_count, 4);
}

#[test]
fn classify_transport_stability_keeps_request_errors_blocking_without_requestable_peer() {
    let stability =
        super::status_payload::classify_transport_stability(&super::ChainReplicationDebugStatus {
            local_peer_id: "peer-local".to_string(),
            connected_peers: vec!["peer-a".to_string()],
            peer_healths: vec![super::ChainPeerHealthStatus {
                peer_id: "peer-a".to_string(),
                status: "active".to_string(),
                issues: Vec::new(),
                discovery_sources: vec!["static_bootstrap".to_string()],
                active_path_kind: Some("direct".to_string()),
                source_operator: None,
                source_asn: None,
            }],
            registered_protocols: Vec::new(),
            protocol_retry_cooldown_peers: BTreeMap::new(),
            transport_retry_cooldown_peers: vec!["peer-a".to_string()],
            request_peer_scores: BTreeMap::from([("peer-a".to_string(), 100)]),
            connection_events: Vec::new(),
            recent_errors: vec![
                "request failed: Timeout protocol=/aw/node/replication/fetch-commit/1.0.0 peer=peer-a".to_string(),
                "request failed: Io(Custom { kind: UnexpectedEof, error: Eof { name: \"map\" } }) protocol=/aw/node/replication/fetch-commit/head/1.0.0 peer=peer-a".to_string(),
                "request failed: Timeout protocol=/aw/node/replication/fetch-commit/head/1.0.0 peer=peer-a".to_string(),
                "request failed: Timeout protocol=/aw/node/replication/fetch-commit/1.0.0 peer=peer-a".to_string(),
            ],
        });

    assert!(!stability.stable);
    assert!(stability.score < 70);
    assert_eq!(stability.blocking_error_count, 4);
    assert_eq!(stability.timeout_count, 3);
    assert_eq!(stability.protocol_error_count, 4);
}
