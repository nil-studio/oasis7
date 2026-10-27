//! Stop work must leave time for an authenticated reply within the caller budget.
use super::*;
use std::io::Read;

fn instance() -> FormalRuntimeInstanceV1 {
    FormalRuntimeInstanceV1 {
        api_version: OWNER_API.into(),
        operation_id: "deadline-fixture".into(),
        request_digest: "request".into(),
        incarnation: "original-owner".into(),
        config_digest: "config".into(),
        node_id: "node".into(),
        world_id: "world".into(),
        chain_id: "chain".into(),
        authority_id: "authority".into(),
        authority_epoch: 1,
        profile_version: 2,
        process_id: std::process::id(),
    }
}

fn absent_state_path() -> PathBuf {
    // This transport fixture has no durable owner record. Use an unused path
    // beneath a canonical temp root; no persistent file is created or removed.
    std::env::temp_dir()
        .canonicalize()
        .unwrap()
        .join(format!(
            "o7-owner-deadline-{}",
            crate::node_lifecycle::local_io::random_id().unwrap()
        ))
        .join("owner.json")
}

#[test]
fn stop_reserves_reply_time_without_extending_caller_budget() {
    for (total_ms, expected_ms) in [
        (2, 1),
        (2_000, 1_000),
        (10_000, 8_000),
        (60_000, 58_000),
    ] {
        let total = Duration::from_millis(total_ms);
        let work = control_work_timeout("stop", total).unwrap();
        assert_eq!(work, Duration::from_millis(expected_ms));
        assert!(work < total);
    }
    assert_eq!(
        control_work_timeout("stop", Duration::from_secs(120)).unwrap(),
        Duration::from_secs(58)
    );
    assert_eq!(
        control_work_timeout("query", Duration::from_secs(2)).unwrap(),
        Duration::from_secs(2)
    );
    for total in [
        Duration::ZERO,
        Duration::from_nanos(1),
        Duration::from_millis(1),
    ] {
        assert!(control_work_timeout("stop", total).is_err());
    }
}

#[test]
fn bounded_stop_receives_pending_after_server_uses_its_work_budget() {
    let expected = instance();
    let peer_instance = expected.clone();
    let (client, mut server) = UnixStream::pair().unwrap();
    let deadline = Instant::now() + Duration::from_secs(2);
    let peer = std::thread::spawn(move || {
        configure_control_stream(&server).unwrap();
        let request: ControlRequest = read_message(&mut server).unwrap();
        validate_control(&request, &peer_instance).unwrap();
        assert_eq!(request.action, "stop");
        assert!((1..=1_000).contains(&request.timeout_ms));
        // Includes time to publish the Pending result and frame its reply.
        // The old client requested its entire 2s wait budget as stop work and
        // timed out before this response could be sent.
        std::thread::sleep(Duration::from_millis(request.timeout_ms + 100));
        write_message(
            &mut server,
            &ControlReply {
                api_version: OWNER_API.into(),
                snapshot: FormalRuntimeOwnerSnapshotV1 {
                    api_version: OWNER_API.into(),
                    instance: peer_instance,
                    state: "unknown".into(),
                    last_error: Some(
                        "formal Node stop is still pending; runtime remains owned".into(),
                    ),
                },
                durable_set_status: None,
                prepared_transition: None,
                control_error: Some(
                    "formal Node stop is still pending; runtime remains owned".into(),
                ),
            },
        )
    });
    let result = send_control_on_stream(&absent_state_path(), &expected, "stop", deadline, client);
    let peer_result = peer.join().unwrap();
    let snapshot = result.unwrap();
    peer_result.unwrap();
    assert_eq!(snapshot.instance, expected);
    assert_eq!(snapshot.state, "unknown");
    assert_eq!(
        snapshot.last_error.as_deref(),
        Some("formal Node stop is still pending; runtime remains owned")
    );
}

#[test]
fn stop_reply_still_rejects_a_different_owner_incarnation() {
    let expected = instance();
    let mut wrong = expected.clone();
    wrong.incarnation = "replacement-owner".into();
    let (client, mut server) = UnixStream::pair().unwrap();
    let peer = std::thread::spawn(move || {
        configure_control_stream(&server).unwrap();
        let _: ControlRequest = read_message(&mut server).unwrap();
        write_message(
            &mut server,
            &ControlReply {
                api_version: OWNER_API.into(),
                snapshot: FormalRuntimeOwnerSnapshotV1 {
                    api_version: OWNER_API.into(),
                    instance: wrong,
                    state: "stopped".into(),
                    last_error: None,
                },
                durable_set_status: None,
                prepared_transition: None,
                control_error: None,
            },
        )
        .unwrap();
    });
    let result = send_control_on_stream(
        &absent_state_path(),
        &expected,
        "stop",
        Instant::now() + Duration::from_secs(2),
        client,
    );
    peer.join().unwrap();
    assert!(result.unwrap_err().contains("exact operation/instance"));
}

#[test]
fn expired_stop_sends_no_request_and_never_becomes_a_success() {
    let (client, mut server) = UnixStream::pair().unwrap();
    let result = send_control_on_stream(
        &absent_state_path(),
        &instance(),
        "stop",
        Instant::now(),
        client,
    );
    assert!(result.unwrap_err().contains("deadline exhausted"));
    let mut byte = [0];
    assert_eq!(server.read(&mut byte).unwrap(), 0);
}

#[test]
fn missing_stop_reply_is_an_error_not_a_synthetic_pending_receipt() {
    let (client, _silent_peer) = UnixStream::pair().unwrap();
    let result = send_control_on_stream(
        &absent_state_path(),
        &instance(),
        "stop",
        Instant::now() + Duration::from_millis(50),
        client,
    );
    assert!(result.is_err());
}
