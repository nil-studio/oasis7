//! Actual signed TCP reads reach the production periodic completion fence.
//! Internal session/pending substitution is a test drive, not a successful public admission.
use super::*;
use oasis7::viewer::{ViewerRuntimeLiveServer, ViewerRuntimeLiveServerConfig};

struct GateRelease(std::path::PathBuf);
impl Drop for GateRelease {
    fn drop(&mut self) {
        let _ = fs::write(self.0.join("world-periodic-view-release"), b"release");
    }
}

#[test]
fn real_tcp_periodic_late_completion_fences_session_cursor_and_original_pending_payload() {
    for change in [
        "config",
        "cursor",
        "player",
        "subscription",
        "event_filter",
        "protocol",
        "pending",
        "matching",
    ] {
        run_periodic_late_completion_case(change, None);
    }
}

#[test]
fn real_tcp_slow_matching_periodic_read_exceeds_default_worker_budget() {
    run_periodic_late_completion_case(
        "matching",
        Some((Duration::from_secs(5), Duration::from_millis(2_200))),
    );
}

fn run_periodic_late_completion_case(change: &str, slow_case: Option<(Duration, Duration)>) {
    let fixture = Fixture::with_controlled_commits(true);
    let original = fixture.delegation();
    fixture.client.submit(original.clone()).unwrap();
    // Controlled fixtures deliberately gate the automatic Node clock.
    // Publish the genuinely submitted delegation through the real driver
    // before requiring its authenticated Lookup/minimum View result.
    commit_request(
        &mut fixture.driver.lock().unwrap(),
        2,
        Some(original.clone()),
    );
    let original_commit = fixture.committed(&original);
    let baseline = fixture
        .client
        .read_view(fixture.view(Some(original_commit)))
        .unwrap();
    // A legitimately signed successor is used only as an internal pending-map
    // substitution. This test does not claim the mismatched correlation pair
    // is admitted by the public Submit API.
    let WorldServicePayloadV1::Delegation(signed) = &original.signed_payload else {
        unreachable!()
    };
    let mut successor = signed.request.clone();
    successor.nonce += 1;
    let successor_payload = WorldServicePayloadV1::Delegation(
        sign_read_request("delegation", successor, &fixture.owner).unwrap(),
    );
    let successor_request = fixture.request(successor_payload.clone());
    let originals = vec![(
        original.correlation.clone(),
        original.signed_payload.clone(),
    )];
    let replacement = vec![
        (original.correlation.clone(), successor_payload.clone()),
        (successor_request.correlation, successor_payload),
    ];
    let root = fixture.root.join("late-periodic-gate");
    fs::create_dir(&root).unwrap();
    *fixture.world_gate.root.lock().unwrap() = Some(root.clone());
    fixture.concurrent_dispatch.store(true, Ordering::SeqCst);
    fs::write(root.join("world-concurrent-ready"), b"ready").unwrap();
    fs::write(root.join("world-periodic-view-arm"), b"arm").unwrap();
    if slow_case.is_some() {
        fs::write(root.join("world-periodic-slow-view"), b"slow case").unwrap();
    }
    let release = GateRelease(root.clone());
    let mut config = ViewerRuntimeLiveServerConfig::new(oasis7_simulator::WorldScenario::Minimal);
    config.world_id = "w1".into();
    let mut world_service = fixture.client.config().clone();
    if let Some((timeout, _)) = slow_case {
        world_service.timeout = timeout;
    }
    config.world_service = Some(world_service);
    config.auto_play_on_connect = false;
    config.chain_poll_interval = Duration::from_secs(60);
    let server = ViewerRuntimeLiveServer::new(config).unwrap();
    let shared = Arc::new(Mutex::new(server));
    let canonical_before = fixture.driver.lock().unwrap().execution_world.snapshot();
    let evidence = ViewerRuntimeLiveServer::test_drive_periodic_late_completion(
        &shared,
        baseline,
        originals,
        replacement,
        change,
        |stage| match stage {
            "held" => {
                let deadline = Instant::now() + Duration::from_secs(1);
                while !root.join("world-periodic-view-started").exists() {
                    assert!(
                        Instant::now() < deadline,
                        "actual signed periodic ReadView never reached gate"
                    );
                    thread::sleep(Duration::from_millis(2));
                }
                let parsed: serde_json::Value = serde_json::from_slice(
                    &fs::read(root.join("world-periodic-view-started")).unwrap(),
                )
                .unwrap();
                assert_eq!(parsed["signature_verified"], true);
                assert!(parsed["min_commit_position"].as_u64().unwrap() > 0);
                assert!(!root.join("world-periodic-view-release").exists());
                if let Some((_, delay)) = slow_case {
                    thread::sleep(delay);
                    assert!(!root.join("world-periodic-view-release").exists());
                }
                if change == "cursor" {
                    let mut driver = fixture.driver.lock().unwrap();
                    let height = driver.state.last_applied_committed_height + 1;
                    commit_request(&mut driver, height, None);
                    drop(driver);
                    Some(fixture.client.read_view(fixture.view(None)).unwrap())
                } else {
                    None
                }
            }
            "release" => {
                fs::write(root.join("world-periodic-view-release"), b"release").unwrap();
                None
            }
            other => panic!("unexpected drive boundary {other}"),
        },
    );
    drop(release);
    fixture.finish_http_workers().unwrap();
    if change != "cursor" {
        assert_eq!(
            fixture.driver.lock().unwrap().execution_world.snapshot(),
            canonical_before,
            "periodic reads/session changes may not mutate canonical authority"
        );
    }
    assert_eq!(evidence["actual_lookup_count"], 1);
    assert_eq!(evidence["actual_signed_view_changes_validated"], true);
    println!("PRE2_ACTUAL_PERIODIC_LATE_COMPLETION_PASSED change={change} evidence={evidence}");
}
