use super::*;

fn publication_renewal_snapshot(
    local_height: u64,
    local_committed_at_ms: i64,
    peer_height: u64,
    peer_committed_at_ms: i64,
    peer_observed_at_ms: i64,
) -> NodeSnapshot {
    let mut snapshot = publication_test_snapshot(NodeRole::Sequencer, 0);
    snapshot.consensus.latest_height = local_height;
    snapshot.consensus.committed_height = local_height;
    snapshot.consensus.network_committed_height = local_height;
    snapshot.consensus.replication_persisted_height = local_height;
    snapshot.consensus.last_execution_height = local_height;
    snapshot.consensus.last_committed_at_ms = Some(local_committed_at_ms);
    snapshot.consensus.last_block_hash = Some(format!("block-h{local_height}"));
    snapshot.consensus.last_execution_block_hash = Some(format!("execution-h{local_height}"));
    snapshot.consensus.last_execution_state_root = Some(format!("state-h{local_height}"));
    snapshot.consensus.peer_heads[0].height = peer_height;
    snapshot.consensus.peer_heads[0].block_hash = format!("block-h{peer_height}");
    snapshot.consensus.peer_heads[0].committed_at_ms = peer_committed_at_ms;
    snapshot.consensus.peer_heads[0].observed_at_ms = peer_observed_at_ms;
    snapshot.consensus.peer_heads[0].execution_block_hash =
        Some(format!("execution-h{peer_height}"));
    snapshot.consensus.peer_heads[0].execution_state_root = Some(format!("state-h{peer_height}"));
    snapshot
}

fn publication_renewal_write_record(records_dir: &std::path::Path, height: u64, timestamp_ms: i64) {
    publication_test_write_execution_record(
        records_dir,
        &publication_test_execution_record_at(
            height,
            format!("block-h{height}").as_str(),
            format!("block-h{}", height - 1).as_str(),
            format!("execution-h{height}").as_str(),
            format!("state-h{height}").as_str(),
            timestamp_ms,
        ),
    );
}

fn publication_renewal_is_critical_not_ready(
    payload: &super::super::super::status_payload::ChainStatusResponse,
) -> bool {
    payload.observability.status == "critical"
        && !payload.observability.ready
        && payload.readiness.status == "not_ready"
        && !payload.readiness.ready
        && payload
            .observability
            .alerts
            .iter()
            .any(|alert| alert.code == "local_chain_ahead_of_network_head")
        && !payload.observability.alerts.iter().any(|alert| {
            alert.code == "sequencer_head_publication_pending" && alert.severity == "warn"
        })
}

fn publication_renewal_is_warning_ready(
    payload: &super::super::super::status_payload::ChainStatusResponse,
) -> bool {
    payload.observability.status == "warn"
        && payload.observability.ready
        && payload.readiness.status == "ready"
        && payload.readiness.ready
        && payload.readiness.failed_gates.is_empty()
        && payload.observability.alerts.iter().any(|alert| {
            alert.code == "sequencer_head_publication_pending" && alert.severity == "warn"
        })
        && !payload
            .observability
            .alerts
            .iter()
            .any(|alert| alert.code == "local_chain_ahead_of_network_head")
}

#[test]
fn public_testnet_sequencer_renewable_observation_does_not_reset_uninterrupted_lag() {
    let manifest = publication_test_manifest("public_testnet", 2);
    let dir = publication_test_temp_dir("renewable-observation-uninterrupted");
    let records_dir = dir.join("records");
    fs::create_dir_all(&records_dir).expect("create renewable observation records dir");
    let now_ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock after epoch")
        .as_millis() as i64;
    let chronology = [
        (500, now_ms - 31_100),
        (501, now_ms - 31_000),
        (502, now_ms - 20_000),
        (503, now_ms - 10_000),
        (504, now_ms),
    ];
    for (height, timestamp_ms) in chronology {
        publication_renewal_write_record(&records_dir, height, timestamp_ms);
    }

    let mut last_local_commit = None;
    let mut final_payload = None;
    let mut failures = Vec::new();
    for window in chronology.windows(2) {
        let (parent_height, parent_committed_at_ms) = window[0];
        let (local_height, local_committed_at_ms) = window[1];
        let peer_observed_at_ms =
            parent_committed_at_ms + (local_committed_at_ms - parent_committed_at_ms) / 2;
        let snapshot = publication_renewal_snapshot(
            local_height,
            local_committed_at_ms,
            parent_height,
            parent_committed_at_ms,
            peer_observed_at_ms,
        );
        if !(parent_committed_at_ms <= peer_observed_at_ms
            && peer_observed_at_ms <= local_committed_at_ms)
        {
            failures.push(format!(
                "height {local_height} did not exercise renewable observation fast path: parent={parent_committed_at_ms} observed={peer_observed_at_ms} local={local_committed_at_ms}"
            ));
        }
        if last_local_commit.is_some_and(|previous| local_committed_at_ms <= previous) {
            failures.push(format!(
                "height {local_height} did not refresh last_committed_at_ms: previous={last_local_commit:?} current={local_committed_at_ms}"
            ));
        }
        if snapshot.consensus.peer_heads[0].height == snapshot.consensus.committed_height {
            failures.push(format!(
                "height {local_height} accidentally recorded equal-head catch-up"
            ));
        }
        last_local_commit = Some(local_committed_at_ms);
        final_payload = Some(publication_test_full_payload_from_records(
            snapshot,
            &manifest,
            dir.as_path(),
            records_dir.as_path(),
        ));
    }

    let payload = final_payload.expect("continuous lag produced a final payload");
    if !publication_renewal_is_critical_not_ready(&payload) {
        failures.push(format!(
            "uninterrupted lag older than 30000ms renewed from latest commit: observability={{status:{},ready:{},alerts:{:?}}} readiness={{status:{},ready:{},failed_gates:{:?}}}",
            payload.observability.status,
            payload.observability.ready,
            payload
                .observability
                .alerts
                .iter()
                .map(|alert| format!("{}:{}", alert.severity, alert.code))
                .collect::<Vec<_>>(),
            payload.readiness.status,
            payload.readiness.ready,
            payload.readiness.failed_gates,
        ));
    }

    fs::remove_dir_all(dir).expect("remove renewable observation records dir");
    assert!(
        failures.is_empty(),
        "renewable observation uninterrupted-lag contract mismatches:\n{}",
        failures.join("\n")
    );
}

#[test]
fn public_testnet_sequencer_renewable_observation_resets_only_after_durable_equal_head_proof() {
    fn reconcile(
        snapshot: &NodeSnapshot,
        manifest: &LoadedNetworkTierManifest,
        dir: &std::path::Path,
        records_dir: &std::path::Path,
        observed_at_ms: i64,
    ) {
        super::super::super::publication_lifecycle::reconcile(
            snapshot,
            Some(manifest),
            dir.join("execution-world").as_path(),
            records_dir,
            observed_at_ms,
        )
        .expect("publication lifecycle owner reconciles transition");
    }

    let manifest = publication_test_manifest("public_testnet", 2);
    let dir = publication_test_temp_dir("renewable-observation-catch-up");
    let records_dir = dir.join("records");
    fs::create_dir_all(&records_dir).expect("create catch-up proof records dir");
    let now_ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock after epoch")
        .as_millis() as i64;
    let old_episode_at_ms = now_ms - 31_000;
    for (height, timestamp_ms) in [
        (600, old_episode_at_ms - 100),
        (601, old_episode_at_ms),
        (602, now_ms - 1_000),
    ] {
        publication_renewal_write_record(&records_dir, height, timestamp_ms);
    }

    let initial_equal_head = publication_renewal_snapshot(
        600,
        old_episode_at_ms - 100,
        600,
        old_episode_at_ms - 100,
        old_episode_at_ms - 50,
    );
    reconcile(
        &initial_equal_head,
        &manifest,
        &dir,
        &records_dir,
        old_episode_at_ms - 50,
    );
    let old_episode = publication_renewal_snapshot(
        601,
        old_episode_at_ms,
        600,
        old_episode_at_ms - 100,
        old_episode_at_ms - 1,
    );
    reconcile(
        &old_episode,
        &manifest,
        &dir,
        &records_dir,
        old_episode_at_ms,
    );
    let stale_lag =
        publication_renewal_snapshot(602, now_ms - 1_000, 601, old_episode_at_ms, now_ms - 500);
    reconcile(&stale_lag, &manifest, &dir, &records_dir, now_ms - 500);
    let stale_payload = publication_test_full_payload_from_records(
        stale_lag,
        &manifest,
        dir.as_path(),
        records_dir.as_path(),
    );
    let equal_head =
        publication_renewal_snapshot(602, now_ms - 1_000, 602, now_ms - 1_000, now_ms - 400);
    reconcile(&equal_head, &manifest, &dir, &records_dir, now_ms - 400);
    let equal_payload = publication_test_full_payload_from_records(
        equal_head,
        &manifest,
        dir.as_path(),
        records_dir.as_path(),
    );

    publication_renewal_write_record(&records_dir, 603, now_ms);
    let fresh_lag = publication_renewal_snapshot(603, now_ms, 602, now_ms - 1_000, now_ms + 1);
    reconcile(&fresh_lag, &manifest, &dir, &records_dir, now_ms + 1);
    let fresh_payload = publication_test_full_payload_from_records(
        fresh_lag,
        &manifest,
        dir.as_path(),
        records_dir.as_path(),
    );

    let mut failures = Vec::new();
    if !publication_renewal_is_critical_not_ready(&stale_payload) {
        failures.push("pre-catch-up old lag was not critical/not-ready".to_string());
    }
    if !equal_payload.observability.ready
        || !equal_payload.readiness.ready
        || equal_payload
            .observability
            .alerts
            .iter()
            .any(|alert| alert.code == "local_chain_ahead_of_network_head")
    {
        failures.push(format!(
            "explicit equal-head proof was not accepted: observability={{status:{},ready:{},alerts:{:?}}} readiness={{status:{},ready:{}}}",
            equal_payload.observability.status,
            equal_payload.observability.ready,
            equal_payload
                .observability
                .alerts
                .iter()
                .map(|alert| format!("{}:{}", alert.severity, alert.code))
                .collect::<Vec<_>>(),
            equal_payload.readiness.status,
            equal_payload.readiness.ready,
        ));
    }
    if !publication_renewal_is_warning_ready(&fresh_payload) {
        failures.push(format!(
            "new lag after explicit equal-head proof did not receive fresh grace: observability={{status:{},ready:{},alerts:{:?}}} readiness={{status:{},ready:{},failed_gates:{:?}}}",
            fresh_payload.observability.status,
            fresh_payload.observability.ready,
            fresh_payload
                .observability
                .alerts
                .iter()
                .map(|alert| format!("{}:{}", alert.severity, alert.code))
                .collect::<Vec<_>>(),
            fresh_payload.readiness.status,
            fresh_payload.readiness.ready,
            fresh_payload.readiness.failed_gates,
        ));
    }

    fs::remove_dir_all(dir).expect("remove catch-up proof records dir");
    assert!(
        failures.is_empty(),
        "renewable observation catch-up contract mismatches:\n{}",
        failures.join("\n")
    );
}
