use super::*;

pub(super) const PUBLICATION_TEST_OBSERVED_AT_MS: i64 = 1_700_000_100_000;
pub(super) const PUBLICATION_TEST_HEIGHT: u64 = 101;
pub(super) fn publication_test_manifest(
    tier: &str,
    target_validator_count: u64,
) -> LoadedNetworkTierManifest {
    LoadedNetworkTierManifest {
        source_path: format!("test-{tier}-manifest.json"),
        manifest: NetworkTierManifest {
            release_policy: None,
            world_policy: None,
            authority_policy: None,
            schema_version: NETWORK_TIER_MANIFEST_SCHEMA_V1.to_string(),
            tier: tier.to_string(),
            status: "live".to_string(),
            network_id: format!("oasis7-{tier}"),
            chain_id: format!("oasis7-{tier}"),
            runtime_refs: NetworkTierRuntimeRefs {
                release_candidate_bundle_ref: "bundle.json".to_string(),
                genesis_ref: "genesis.json".to_string(),
                bootstrap_peer_ref: "bootstrap.txt".to_string(),
            },
            endpoint_policy: NetworkTierEndpointPolicy {
                rpc_ref: "https://rpc.example.invalid".to_string(),
                explorer_ref: "https://explorer.example.invalid".to_string(),
                faucet_ref: None,
            },
            validator_policy: NetworkTierValidatorPolicy {
                governance_mode: "governance_registry".to_string(),
                validator_admission: "governance_registry_only".to_string(),
                target_validator_count,
                allow_observer_nodes: true,
            },
            token_policy: NetworkTierTokenPolicy {
                symbol: "OC".to_string(),
                faucet_mode: "disabled".to_string(),
                reset_policy: Some("never".to_string()),
                value_semantics: "test_only".to_string(),
            },
            claims_policy: NetworkTierClaimsPolicy {
                allowed_claims: Vec::new(),
                denied_claims: Vec::new(),
            },
            promotion_policy: NetworkTierPromotionPolicy {
                promote_from: Vec::new(),
                required_gates: Vec::new(),
            },
            evidence_refs: Vec::new(),
        },
        bootstrap_peers: vec!["/ip4/127.0.0.1/tcp/4100".to_string()],
    }
}
#[expect(
    clippy::field_reassign_with_default,
    reason = "Test fixture construction intentionally starts from canonical defaults before overriding scenario-specific fields."
)]
pub(super) fn publication_test_snapshot(role: NodeRole, commit_age_ms: i64) -> NodeSnapshot {
    let mut consensus = NodeConsensusSnapshot::default();
    consensus.latest_height = PUBLICATION_TEST_HEIGHT;
    consensus.committed_height = PUBLICATION_TEST_HEIGHT;
    consensus.network_committed_height = PUBLICATION_TEST_HEIGHT;
    consensus.replication_persisted_height = PUBLICATION_TEST_HEIGHT;
    consensus.last_committed_at_ms = Some(PUBLICATION_TEST_OBSERVED_AT_MS - commit_age_ms);
    consensus.last_block_hash = Some("block-h101".to_string());
    consensus.last_execution_height = PUBLICATION_TEST_HEIGHT;
    consensus.last_execution_block_hash = Some("execution-h101".to_string());
    consensus.last_execution_state_root = Some("state-h101".to_string());
    consensus.known_peer_heads = 1;
    consensus.peer_heads = vec![NodePeerCommittedHead {
        node_id: "validator-b".to_string(),
        validator_id: Some("validator-b".to_string()),
        height: PUBLICATION_TEST_HEIGHT - 1,
        block_hash: "block-h100".to_string(),
        committed_at_ms: PUBLICATION_TEST_OBSERVED_AT_MS - commit_age_ms - 1_000,
        observed_at_ms: PUBLICATION_TEST_OBSERVED_AT_MS - 100,
        execution_block_hash: Some("execution-h100".to_string()),
        execution_state_root: Some("state-h100".to_string()),
    }];
    consensus.validator_stakes = BTreeMap::from([("validator-b".to_string(), 67)]);
    consensus.required_stake = 67;
    consensus.total_stake = 100;
    consensus.validator_set_hash = "validator-set-hash".to_string();
    consensus.validator_stake_root = "validator-stake-root".to_string();
    consensus.validator_stake_proofs = vec![NodeValidatorStakeProofSnapshot {
        validator_id: "validator-b".to_string(),
        player_id: "player-b".to_string(),
        stake: 67,
        signer_public_key_hex: Some("02".repeat(33)),
        leaf_hash: "validator-b-leaf".to_string(),
        proof: Vec::new(),
    }];
    NodeSnapshot {
        node_id: "validator-a".to_string(),
        player_id: "player-a".to_string(),
        world_id: "world-public-testnet".to_string(),
        role,
        replication_enabled: true,
        running: true,
        tick_count: 1,
        last_tick_unix_ms: Some(PUBLICATION_TEST_OBSERVED_AT_MS),
        consensus,
        consensus_progress_observer_error: None,
        last_error: None,
    }
}
pub(super) fn publication_test_replication() -> super::super::ChainReplicationDebugStatus {
    super::super::ChainReplicationDebugStatus {
        local_peer_id: "validator-a".to_string(),
        connected_peers: vec!["validator-b".to_string()],
        peer_healths: vec![super::super::ChainPeerHealthStatus {
            peer_id: "validator-b".to_string(),
            status: "active".to_string(),
            issues: Vec::new(),
            discovery_sources: vec!["static_bootstrap".to_string()],
            active_path_kind: Some("direct".to_string()),
            source_operator: Some("validator".to_string()),
            source_asn: None,
        }],
        registered_protocols: vec![
            "/aw/node/replication/fetch-blob/1.0.0".to_string(),
            "/aw/node/replication/fetch-commit/1.0.0".to_string(),
        ],
        protocol_retry_cooldown_peers: BTreeMap::new(),
        transport_retry_cooldown_peers: Vec::new(),
        request_peer_scores: BTreeMap::from([("validator-b".to_string(), 100)]),
        connection_events: Vec::new(),
        recent_errors: Vec::new(),
    }
}
pub(super) fn publication_test_status(
    snapshot: &NodeSnapshot,
    manifest: Option<&LoadedNetworkTierManifest>,
) -> (
    super::super::status_payload::ChainConsensusNetworkHeadStatus,
    super::super::status_payload::ChainNodeObservabilityStatus,
    super::super::status_payload::ChainReadinessStatus,
) {
    let network_head = super::super::status_payload::build_network_head_status(
        snapshot,
        PUBLICATION_TEST_OBSERVED_AT_MS,
        manifest,
    );
    let policy = super::super::status_payload::readiness_policy(snapshot, manifest);
    let observability = super::super::status_payload::build_chain_node_observability_status(
        snapshot,
        &super::super::observability_tests::sample_observability_storage_metrics(),
        &super::super::observability_tests::sample_observability_reward_runtime_metrics(),
        &publication_test_replication(),
        &network_head,
        &super::super::observability_tests::sample_observability_p2p_status(),
        &policy,
        None,
        PUBLICATION_TEST_OBSERVED_AT_MS,
    );
    let readiness = super::super::status_payload::build_readiness_status(&observability, policy);
    (network_head, observability, readiness)
}

#[test]
fn public_testnet_sequencer_publication_grace_requires_authoritative_nonzero_stake() {
    let manifest = publication_test_manifest("public_testnet", 2);
    let mut zero_required_stake = publication_test_snapshot(NodeRole::Sequencer, 1_000);
    zero_required_stake.consensus.required_stake = 0;
    let mut missing_stake_authority = zero_required_stake.clone();
    missing_stake_authority.consensus.total_stake = 0;
    missing_stake_authority.consensus.validator_stakes.clear();
    missing_stake_authority.consensus.validator_set_hash.clear();
    missing_stake_authority
        .consensus
        .validator_stake_root
        .clear();
    missing_stake_authority
        .consensus
        .validator_stake_proofs
        .clear();
    let mut failures = Vec::new();

    for (label, snapshot) in [
        (
            "zero authoritative required stake despite count quorum",
            &zero_required_stake,
        ),
        (
            "missing authoritative stake configuration despite count quorum",
            &missing_stake_authority,
        ),
    ] {
        let (network_head, observability, readiness) =
            publication_test_status(snapshot, Some(&manifest));
        assert_eq!(network_head.quorum_mode, "count", "{label}");
        assert_eq!(network_head.source, "peer_quorum", "{label}");
        assert_eq!(network_head.decision, "ready", "{label}");
        assert_eq!(network_head.conflicting_peer_count, 0, "{label}");
        assert!(
            network_head.fresh_peer_count >= network_head.required_peer_count,
            "{label}"
        );
        assert!(network_head.stake_quorum_met, "{label}");

        let has_publication_warning = observability.alerts.iter().any(|alert| {
            alert.severity == "warn" && alert.code == "sequencer_head_publication_pending"
        });
        if observability.status != "critical"
            || observability.ready
            || readiness.status != "not_ready"
            || readiness.ready
            || has_publication_warning
        {
            failures.push(format!(
                "{label}: required_stake={} observed_stake={} observability={{status:{},ready:{},alerts:{:?}}} readiness={{status:{},ready:{},failed_gates:{:?}}}",
                snapshot.consensus.required_stake,
                network_head.observed_stake,
                observability.status,
                observability.ready,
                observability
                    .alerts
                    .iter()
                    .map(|alert| format!("{}:{}", alert.severity, alert.code))
                    .collect::<Vec<_>>(),
                readiness.status,
                readiness.ready,
                readiness.failed_gates,
            ));
        }
    }

    assert!(
        failures.is_empty(),
        "publication grace accepted missing or zero authoritative stake:\n{}",
        failures.join("\n")
    );
}
pub(super) fn publication_test_temp_dir(label: &str) -> PathBuf {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock after epoch")
        .as_nanos();
    std::env::temp_dir().join(format!(
        "oasis7-publication-grace-{label}-{}-{unique}",
        std::process::id()
    ))
}
pub(super) fn publication_test_execution_record(
    height: u64,
    node_block_hash: &str,
    prev_node_block_hash: &str,
    execution_block_hash: &str,
    execution_state_root: &str,
) -> serde_json::Value {
    publication_test_execution_record_at(
        height,
        node_block_hash,
        prev_node_block_hash,
        execution_block_hash,
        execution_state_root,
        PUBLICATION_TEST_OBSERVED_AT_MS - 1_000,
    )
}
pub(super) fn publication_test_execution_record_at(
    height: u64,
    node_block_hash: &str,
    prev_node_block_hash: &str,
    execution_block_hash: &str,
    execution_state_root: &str,
    timestamp_ms: i64,
) -> serde_json::Value {
    serde_json::json!({
        "schema_version": 3,
        "world_id": "world-public-testnet",
        "height": height,
        "node_block_hash": node_block_hash,
        "prev_node_block_hash": prev_node_block_hash,
        "proposer_id": "validator-a",
        "action_root": format!("action-root-h{height}"),
        "execution_block_hash": execution_block_hash,
        "execution_state_root": execution_state_root,
        "journal_len": 0,
        "latest_state_ref": format!("state-h{height}.json"),
        "snapshot_ref": format!("state-h{height}.json"),
        "journal_ref": format!("journal-h{height}.json"),
        "timestamp_ms": timestamp_ms,
    })
}
pub(super) fn publication_test_write_execution_record(
    records_dir: &std::path::Path,
    record: &serde_json::Value,
) {
    let height = record["height"].as_u64().expect("record height");
    let bytes = serde_json::to_vec_pretty(record).expect("serialize execution record");
    fs::write(records_dir.join(format!("{height:020}.json")), &bytes)
        .expect("write execution record");
    fs::write(records_dir.join("latest.json"), bytes).expect("write latest execution record");
}
pub(super) fn publication_test_full_payload(
    snapshot: NodeSnapshot,
    manifest: &LoadedNetworkTierManifest,
    local_h_record: serde_json::Value,
    parent_record: serde_json::Value,
) -> super::super::status_payload::ChainStatusResponse {
    let dir = publication_test_temp_dir("records");
    let records_dir = dir.join("records");
    fs::create_dir_all(&records_dir).expect("create publication test records dir");
    let local_height = local_h_record["height"]
        .as_u64()
        .expect("local record height");
    let parent_height = parent_record["height"]
        .as_u64()
        .expect("parent record height");
    let parent_timestamp_ms = parent_record["timestamp_ms"]
        .as_i64()
        .expect("parent record timestamp");
    for height in (0..parent_height).rev() {
        let node_block_hash = format!("block-h{height}");
        let prev_node_block_hash = if height == 0 {
            "block-before-genesis".to_string()
        } else {
            format!("block-h{}", height - 1)
        };
        let record = publication_test_execution_record_at(
            height,
            node_block_hash.as_str(),
            prev_node_block_hash.as_str(),
            format!("execution-h{height}").as_str(),
            format!("state-h{height}").as_str(),
            parent_timestamp_ms - (parent_height - height) as i64,
        );
        publication_test_write_execution_record(&records_dir, &record);
    }
    fs::write(
        records_dir.join(format!("{parent_height:020}.json")),
        serde_json::to_vec_pretty(&parent_record).expect("serialize parent record"),
    )
    .expect("write parent record");
    let local_bytes = serde_json::to_vec_pretty(&local_h_record).expect("serialize local record");
    fs::write(
        records_dir.join(format!("{local_height:020}.json")),
        &local_bytes,
    )
    .expect("write local record");
    fs::write(records_dir.join("latest.json"), local_bytes).expect("write latest record");
    let execution_world_dir = dir.join("execution-world");
    fs::create_dir_all(&execution_world_dir).expect("create publication lifecycle state dir");
    let lifecycle_state = serde_json::json!({
        "schema_version": 1,
        "world_id": snapshot.world_id.clone(),
        "episode": null,
        "catch_up": {
            "height": parent_height,
            "node_block_hash": parent_record["node_block_hash"].clone(),
            "execution_block_hash": parent_record["execution_block_hash"].clone(),
            "execution_state_root": parent_record["execution_state_root"].clone(),
            "timestamp_ms": parent_timestamp_ms,
        },
    });
    fs::write(
        execution_world_dir.join(super::super::publication_lifecycle::PUBLICATION_LAG_STATE_FILE),
        serde_json::to_vec_pretty(&lifecycle_state).expect("serialize publication lifecycle state"),
    )
    .expect("write publication lifecycle state");
    let payload = publication_test_full_payload_from_records(
        snapshot,
        manifest,
        dir.as_path(),
        records_dir.as_path(),
    );
    fs::remove_dir_all(dir).expect("remove publication test dir");
    payload
}
pub(super) fn publication_test_full_payload_from_records(
    snapshot: NodeSnapshot,
    manifest: &LoadedNetworkTierManifest,
    dir: &std::path::Path,
    records_dir: &std::path::Path,
) -> super::super::status_payload::ChainStatusResponse {
    let recommendation = NodeNetworkPolicy::recommend_for_user_mode(
        NodeRole::Sequencer,
        NodeUserMode::PrivateSafe,
        NodeReachabilityAutoDetection {
            observed_reachability: Some(PeerReachabilityClass::Public),
            hole_punch_viability: NodeHolePunchViability::Viable,
            relay_available: true,
            probe_stable: true,
            autonat_status: NodeAutoNatStatus::Public,
            public_port_reachability: NodePublicPortReachability::Reachable,
        },
        false,
    )
    .expect("publication test recommendation");
    super::super::status_payload::build_chain_status_payload(
        snapshot,
        dir.join("execution-world").as_path(),
        Some(records_dir),
        Some(manifest),
        &recommendation,
        Some("private_safe".to_string()),
        NodeNetworkPolicy {
            deployment_mode: PeerDeploymentMode::Public,
            node_role_claim: PeerNodeRole::ValidatorCore,
        },
        &Libp2pReachabilitySnapshot {
            active_transport_kind: Some(LiveTransportKind::Direct),
            active_transport_kind_since_unix_ms: Some(PUBLICATION_TEST_OBSERVED_AT_MS),
            active_direct_path_count: 1,
            autonat_status: LiveAutoNatStatus::Public,
            public_port_reachability: LivePublicPortReachability::Reachable,
            observed_public_addr: Some("/ip4/203.0.113.10/tcp/4001".to_string()),
            confirmed_external_direct_addrs: vec!["/ip4/203.0.113.10/tcp/4001".to_string()],
            ..Libp2pReachabilitySnapshot::default()
        },
        NodeReachabilityAutoDetection {
            observed_reachability: Some(PeerReachabilityClass::Public),
            hole_punch_viability: NodeHolePunchViability::Viable,
            relay_available: true,
            probe_stable: true,
            autonat_status: NodeAutoNatStatus::Public,
            public_port_reachability: NodePublicPortReachability::Reachable,
        },
        ReleaseSecurityPolicy::default(),
        super::super::observability_tests::sample_observability_reward_runtime_metrics(),
        super::super::observability_tests::sample_observability_storage_metrics(),
        super::super::observability_tests::sample_wasm_status(),
        None,
        super::super::ChainTrafficStatus {
            udp_gossip: None,
            libp2p_replication: oasis7_node::Libp2pTrafficMetricsSnapshot::default(),
        },
        super::super::transfer_submit_api::ChainTransferMetricsStatus {
            tracked_records: 0,
            accepted_count: 0,
            pending_count: 0,
            confirmed_count: 0,
            failed_count: 0,
            timeout_count: 0,
            inflight_count: 0,
            oldest_inflight_age_ms: None,
            recent_confirmation_latency:
                super::super::transfer_submit_api::ChainTransferLatencySummaryStatus {
                    sample_count: 0,
                    avg_latency_ms: None,
                    max_latency_ms: None,
                    p50_latency_ms: None,
                    p95_latency_ms: None,
                },
        },
        publication_test_replication(),
    )
}

fn publication_negative_snapshot(local_height: u64, local_timestamp_ms: i64) -> NodeSnapshot {
    let mut snapshot = publication_test_snapshot(NodeRole::Sequencer, 0);
    let parent_height = local_height - 1;
    snapshot.consensus.latest_height = local_height;
    snapshot.consensus.committed_height = local_height;
    snapshot.consensus.network_committed_height = local_height;
    snapshot.consensus.replication_persisted_height = local_height;
    snapshot.consensus.last_execution_height = local_height;
    snapshot.consensus.last_committed_at_ms = Some(local_timestamp_ms);
    snapshot.consensus.last_block_hash = Some(format!("block-h{local_height}"));
    snapshot.consensus.last_execution_block_hash = Some(format!("execution-h{local_height}"));
    snapshot.consensus.last_execution_state_root = Some(format!("state-h{local_height}"));
    snapshot.consensus.peer_heads[0].height = parent_height;
    snapshot.consensus.peer_heads[0].block_hash = format!("block-h{parent_height}");
    snapshot.consensus.peer_heads[0].committed_at_ms = local_timestamp_ms - 1;
    snapshot.consensus.peer_heads[0].observed_at_ms = i64::MAX;
    snapshot.consensus.peer_heads[0].execution_block_hash =
        Some(format!("execution-h{parent_height}"));
    snapshot.consensus.peer_heads[0].execution_state_root = Some(format!("state-h{parent_height}"));
    snapshot
}

fn publication_negative_record(height: u64, timestamp_ms: i64) -> serde_json::Value {
    publication_test_execution_record_at(
        height,
        format!("block-h{height}").as_str(),
        format!("block-h{}", height - 1).as_str(),
        format!("execution-h{height}").as_str(),
        format!("state-h{height}").as_str(),
        timestamp_ms,
    )
}

fn publication_write_chain(
    records_dir: &std::path::Path,
    local_height: u64,
    record_count: usize,
    local_timestamp_ms: i64,
) {
    for offset in 0..record_count {
        let height = local_height - offset as u64;
        publication_test_write_execution_record(
            records_dir,
            &publication_negative_record(height, local_timestamp_ms - offset as i64),
        );
    }
}

fn publication_payload_for_negative_records(
    label: &str,
    snapshot: NodeSnapshot,
    write_records: impl FnOnce(&std::path::Path),
) -> super::super::status_payload::ChainStatusResponse {
    let manifest = publication_test_manifest("public_testnet", 2);
    let dir = publication_test_temp_dir(label);
    let records_dir = dir.join("records");
    fs::create_dir_all(&records_dir).expect("create negative publication records dir");
    write_records(&records_dir);
    let payload = publication_test_full_payload_from_records(
        snapshot,
        &manifest,
        dir.as_path(),
        records_dir.as_path(),
    );
    fs::remove_dir_all(dir).expect("remove negative publication records dir");
    payload
}

fn publication_payload_for_durable_records(
    label: &str,
    snapshot: NodeSnapshot,
    write_records: impl FnOnce(&std::path::Path),
) -> super::super::status_payload::ChainStatusResponse {
    let manifest = publication_test_manifest("public_testnet", 2);
    let dir = publication_test_temp_dir(label);
    let records_dir = dir.join("records");
    let execution_world_dir = dir.join("execution-world");
    fs::create_dir_all(&records_dir).expect("create durable publication records dir");
    fs::create_dir_all(&execution_world_dir).expect("create durable publication state dir");
    write_records(&records_dir);
    let peer = &snapshot.consensus.peer_heads[0];
    let lifecycle_state = serde_json::json!({
        "schema_version": 1,
        "world_id": snapshot.world_id.clone(),
        "episode": null,
        "catch_up": {
            "height": peer.height,
            "node_block_hash": peer.block_hash.clone(),
            "execution_block_hash": peer.execution_block_hash.clone(),
            "execution_state_root": peer.execution_state_root.clone(),
            "timestamp_ms": peer.committed_at_ms,
        },
    });
    fs::write(
        execution_world_dir.join(super::super::publication_lifecycle::PUBLICATION_LAG_STATE_FILE),
        serde_json::to_vec_pretty(&lifecycle_state).expect("serialize durable publication state"),
    )
    .expect("write durable publication state");
    let payload = publication_test_full_payload_from_records(
        snapshot,
        &manifest,
        dir.as_path(),
        records_dir.as_path(),
    );
    fs::remove_dir_all(dir).expect("remove durable publication records dir");
    payload
}

fn publication_rejection_failure(
    label: &str,
    expected_reason: &str,
    payload: &super::super::status_payload::ChainStatusResponse,
) -> Option<String> {
    let reason_alert = payload.observability.alerts.iter().find(|alert| {
        alert.code.contains(expected_reason) || alert.summary.contains(expected_reason)
    });
    let rejected = payload.observability.status == "critical"
        && !payload.observability.ready
        && payload.readiness.status == "not_ready"
        && !payload.readiness.ready
        && payload
            .observability
            .alerts
            .iter()
            .any(|alert| alert.severity == "critical")
        && !payload.observability.alerts.iter().any(|alert| {
            alert.code == "sequencer_head_publication_pending" && alert.severity == "warn"
        });
    let reason_is_bounded =
        reason_alert.is_some_and(|alert| alert.code.len() <= 96 && alert.summary.len() <= 512);
    (!rejected || !reason_is_bounded).then(|| {
        format!(
            "{label}: expected critical/not_ready with bounded reason `{expected_reason}`, got observability={{status:{},ready:{},alerts:{:?}}} readiness={{status:{},ready:{},failed_gates:{:?}}}",
            payload.observability.status,
            payload.observability.ready,
            payload
                .observability
                .alerts
                .iter()
                .map(|alert| format!("{}:{}:{}", alert.severity, alert.code, alert.summary))
                .collect::<Vec<_>>(),
            payload.readiness.status,
            payload.readiness.ready,
            payload.readiness.failed_gates,
        )
    })
}

#[test]
fn public_testnet_sequencer_publication_proof_negative_matrix_fails_closed_with_reason() {
    const HEIGHT: u64 = 400;
    const FUTURE_MS: i64 = i64::MAX - 10_000;
    let mut failures = Vec::new();

    let timestamp_mismatch = publication_payload_for_negative_records(
        "timestamp-mismatch",
        publication_negative_snapshot(HEIGHT, FUTURE_MS),
        |records| {
            publication_write_chain(records, HEIGHT, 2, FUTURE_MS - 1);
        },
    );
    if let Some(failure) = publication_rejection_failure(
        "retained H timestamp mismatches snapshot",
        "timestamp_mismatch",
        &timestamp_mismatch,
    ) {
        failures.push(failure);
    }

    let chronology_inversion = publication_payload_for_negative_records(
        "chronology-inversion",
        publication_negative_snapshot(HEIGHT, FUTURE_MS - 1),
        |records| {
            publication_test_write_execution_record(
                records,
                &publication_negative_record(HEIGHT, FUTURE_MS - 1),
            );
            publication_test_write_execution_record(
                records,
                &publication_negative_record(HEIGHT - 1, FUTURE_MS),
            );
        },
    );
    if let Some(failure) = publication_rejection_failure(
        "H timestamp precedes H-1 timestamp",
        "chronology_invalid",
        &chronology_inversion,
    ) {
        failures.push(failure);
    }

    for (label, remove_hash) in [("empty", false), ("missing", true)] {
        let ancestry = publication_payload_for_negative_records(
            format!("{label}-ancestry-hash").as_str(),
            publication_negative_snapshot(HEIGHT, FUTURE_MS),
            |records| {
                publication_write_chain(records, HEIGHT, 3, FUTURE_MS);
                let parent_path = records.join(format!("{:020}.json", HEIGHT - 1));
                let mut parent: serde_json::Value = serde_json::from_slice(
                    &fs::read(&parent_path).expect("read parent ancestry record"),
                )
                .expect("parse parent ancestry record");
                let deep_path = records.join(format!("{:020}.json", HEIGHT - 2));
                let mut deep: serde_json::Value = serde_json::from_slice(
                    &fs::read(&deep_path).expect("read deep ancestry record"),
                )
                .expect("parse deep ancestry record");
                if remove_hash {
                    parent
                        .as_object_mut()
                        .expect("parent object")
                        .remove("prev_node_block_hash");
                    deep.as_object_mut()
                        .expect("deep object")
                        .remove("node_block_hash");
                } else {
                    parent["prev_node_block_hash"] = serde_json::json!("");
                    deep["node_block_hash"] = serde_json::json!("");
                }
                fs::write(
                    parent_path,
                    serde_json::to_vec(&parent).expect("serialize parent"),
                )
                .expect("rewrite parent ancestry record");
                fs::write(
                    deep_path,
                    serde_json::to_vec(&deep).expect("serialize deep"),
                )
                .expect("rewrite deep ancestry record");
            },
        );
        if let Some(failure) = publication_rejection_failure(
            format!("{label} deep ancestry hash").as_str(),
            "ancestry_invalid",
            &ancestry,
        ) {
            failures.push(failure);
        }
    }

    for (label, field, value) in [
        (
            "wrong world continuity",
            "world_id",
            serde_json::json!("other-world"),
        ),
        (
            "wrong height continuity",
            "height",
            serde_json::json!(HEIGHT - 9),
        ),
    ] {
        let continuity = publication_payload_for_negative_records(
            label,
            publication_negative_snapshot(HEIGHT, FUTURE_MS),
            |records| {
                publication_write_chain(records, HEIGHT, 3, FUTURE_MS);
                let path = records.join(format!("{:020}.json", HEIGHT - 2));
                let mut record: serde_json::Value =
                    serde_json::from_slice(&fs::read(&path).expect("read continuity record"))
                        .expect("parse continuity record");
                record[field] = value;
                fs::write(
                    path,
                    serde_json::to_vec(&record).expect("serialize continuity record"),
                )
                .expect("rewrite continuity record");
            },
        );
        if let Some(failure) =
            publication_rejection_failure(label, "continuity_invalid", &continuity)
        {
            failures.push(failure);
        }
    }

    let missing_record = publication_payload_for_negative_records(
        "missing-interior-record",
        publication_negative_snapshot(HEIGHT, FUTURE_MS),
        |records| publication_write_chain(records, HEIGHT, 2, FUTURE_MS),
    );
    if let Some(failure) =
        publication_rejection_failure("missing interior record", "record_missing", &missing_record)
    {
        failures.push(failure);
    }
    let malformed_record = publication_payload_for_negative_records(
        "malformed-interior-record",
        publication_negative_snapshot(HEIGHT, FUTURE_MS),
        |records| {
            publication_write_chain(records, HEIGHT, 3, FUTURE_MS);
            fs::write(
                records.join(format!("{:020}.json", HEIGHT - 2)),
                b"{not-json",
            )
            .expect("write malformed interior record");
        },
    );
    if let Some(failure) = publication_rejection_failure(
        "malformed interior record",
        "record_malformed",
        &malformed_record,
    ) {
        failures.push(failure);
    }

    let eligible_255 = publication_payload_for_negative_records(
        "scan-boundary-255",
        publication_negative_snapshot(HEIGHT, FUTURE_MS),
        |records| publication_write_chain(records, HEIGHT, 255, FUTURE_MS),
    );
    if let Some(failure) = publication_rejection_failure(
        "255-record exact scan boundary persistence gate",
        "state_persist_pending",
        &eligible_255,
    ) {
        failures.push(failure);
    }
    let rejected_256 = publication_payload_for_negative_records(
        "scan-boundary-256",
        publication_negative_snapshot(HEIGHT, FUTURE_MS),
        |records| publication_write_chain(records, HEIGHT, 256, FUTURE_MS),
    );
    if let Some(failure) = publication_rejection_failure(
        "256-record exact scan boundary",
        "scan_limit_exceeded",
        &rejected_256,
    ) {
        failures.push(failure);
    }

    let now_ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock after epoch")
        .as_millis() as i64;
    let expired_ms = now_ms - 30_100;
    let grace_expired = publication_payload_for_durable_records(
        "grace-expired",
        publication_negative_snapshot(HEIGHT, expired_ms),
        |records| publication_write_chain(records, HEIGHT, 2, expired_ms),
    );
    if let Some(failure) =
        publication_rejection_failure("publication grace expired", "grace_expired", &grace_expired)
    {
        failures.push(failure);
    }

    assert!(
        failures.is_empty(),
        "sequencer publication negative matrix mismatches:\n{}",
        failures.join("\n")
    );
}

#[path = "oasis7_chain_runtime_observability_readiness_publication_lifecycle_tests.rs"]
mod publication_lifecycle_tests;
#[path = "oasis7_chain_runtime_observability_readiness_publication_renewal_tests.rs"]
mod publication_renewal_tests;
