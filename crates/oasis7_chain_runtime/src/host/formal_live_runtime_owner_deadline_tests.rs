//! Stop work must leave time for an authenticated reply within the caller budget.
use super::*;
use super::control::send_control_reply;
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
    for (total_ms, expected_ms) in [(2, 1), (2_000, 1_000), (10_000, 8_000), (60_000, 58_000)] {
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
    for total in [Duration::ZERO, Duration::from_nanos(1), Duration::from_millis(1)] {
        assert!(control_work_timeout("stop", total).is_err());
    }
}

#[test]
fn freeze_reserves_reply_time_and_returns_only_exact_owner_pin() {
    use super::super::super::formal_live_original_pending_producer::{
        OriginalPendingOwnerV1, OriginalPendingProducerV1,
    };
    use std::os::unix::fs::PermissionsExt;

    let expected = instance();
    let owner = OriginalPendingOwnerV1 {
        authority_id: expected.authority_id.clone(),
        operation_id: expected.operation_id.clone(),
        request_digest: expected.request_digest.clone(),
        config_digest: expected.config_digest.clone(),
        incarnation: expected.incarnation.clone(),
    };
    let root = std::env::temp_dir().join(format!(
        "o7-owner-freeze-reply-{}-{}",
        std::process::id(),
        crate::node_lifecycle::local_io::random_id().unwrap()
    ));
    std::fs::create_dir(&root).unwrap();
    std::fs::set_permissions(&root, std::fs::Permissions::from_mode(0o700)).unwrap();
    let mut producer = OriginalPendingProducerV1::open(&root, owner).unwrap();
    let pin = producer.begin_freeze(&root).unwrap();
    assert_eq!(pin.count, 0);

    let (client, mut server) = UnixStream::pair().unwrap();
    let peer_instance = expected.clone();
    let peer_pin = pin.clone();
    let peer = std::thread::spawn(move || {
        configure_control_stream(&server).unwrap();
        let request: ControlRequest = read_message(&mut server).unwrap();
        validate_control(&request, &peer_instance).unwrap();
        assert_eq!(request.action, "freeze_original_pending");
        assert!(request.timeout_ms < 2_000);
        write_message(
            &mut server,
            &ControlReply {
                api_version: OWNER_API.into(),
                snapshot: FormalRuntimeOwnerSnapshotV1 {
                    api_version: OWNER_API.into(),
                    instance: peer_instance,
                    state: "stopped".into(),
                    last_error: None,
                },
                durable_set_status: None,
                prepared_transition: None,
                original_pending_pin: Some(peer_pin),
                protected_observation: None,
                control_error: None,
            },
        )
        .unwrap();
    });
    let reply = send_control_reply(
        &absent_state_path(),
        &expected,
        "freeze_original_pending",
        Instant::now() + Duration::from_secs(2),
        client,
    )
    .unwrap();
    peer.join().unwrap();
    assert_eq!(reply.original_pending_pin, Some(pin));
    std::fs::remove_dir_all(&root).unwrap();
    // Exercise the lost-reply path inside this already registered native test.
    check_lost_original_freeze_reply_preserves_exact_frozen_original();
}

#[test]
fn freeze_reply_rejects_pin_for_another_original_incarnation() {
    use super::super::super::formal_live_original_pending_producer::{
        OriginalPendingOwnerV1, OriginalPendingProducerV1,
    };
    use std::os::unix::fs::PermissionsExt;

    let expected = instance();
    let root = std::env::temp_dir().join(format!(
        "o7-owner-freeze-wrong-pin-{}-{}",
        std::process::id(),
        crate::node_lifecycle::local_io::random_id().unwrap()
    ));
    std::fs::create_dir(&root).unwrap();
    std::fs::set_permissions(&root, std::fs::Permissions::from_mode(0o700)).unwrap();
    let wrong_owner = OriginalPendingOwnerV1 {
        authority_id: expected.authority_id.clone(),
        operation_id: expected.operation_id.clone(),
        request_digest: expected.request_digest.clone(),
        config_digest: expected.config_digest.clone(),
        incarnation: "different-incarnation".into(),
    };
    let mut producer = OriginalPendingProducerV1::open(&root, wrong_owner).unwrap();
    let wrong_pin = producer.begin_freeze(&root).unwrap();

    let (client, mut server) = UnixStream::pair().unwrap();
    let peer_instance = expected.clone();
    let peer_pin = wrong_pin;
    let peer = std::thread::spawn(move || {
        configure_control_stream(&server).unwrap();
        let _: ControlRequest = read_message(&mut server).unwrap();
        write_message(
            &mut server,
            &ControlReply {
                api_version: OWNER_API.into(),
                snapshot: FormalRuntimeOwnerSnapshotV1 {
                    api_version: OWNER_API.into(),
                    instance: peer_instance,
                    state: "stopped".into(),
                    last_error: None,
                },
                durable_set_status: None,
                prepared_transition: None,
                original_pending_pin: Some(peer_pin),
                protected_observation: None,
                control_error: None,
            },
        )
        .unwrap();
    });
    let result = send_control_reply(
        &absent_state_path(),
        &expected,
        "freeze_original_pending",
        Instant::now() + Duration::from_secs(2),
        client,
    );
    peer.join().unwrap();
    assert!(result.unwrap_err().contains("original pending pin/owner mismatch"));
    std::fs::remove_dir_all(&root).unwrap();
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
                original_pending_pin: None,
                protected_observation: None,
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
                original_pending_pin: None,
                protected_observation: None,
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


fn check_lost_original_freeze_reply_preserves_exact_frozen_original() {
    use oasis7_distfs::controlled_authority::LocalRequestIdentity;
    use super::super::super::formal_live_original_pending_producer::{
        OriginalPendingOwnerV1, OriginalPendingProducerV1, cold_read_exact_frozen,
    };
    use std::os::unix::fs::PermissionsExt;

    let expected = instance();
    let owner = OriginalPendingOwnerV1 {
        authority_id: expected.authority_id.clone(),
        operation_id: expected.operation_id.clone(),
        request_digest: expected.request_digest.clone(),
        config_digest: expected.config_digest.clone(),
        incarnation: expected.incarnation.clone(),
    };
    let root = std::env::temp_dir().join(format!(
        "o7-owner-lost-freeze-{}-{}",
        std::process::id(),
        crate::node_lifecycle::local_io::random_id().unwrap()
    ));
    std::fs::create_dir(&root).unwrap();
    std::fs::set_permissions(&root, std::fs::Permissions::from_mode(0o700)).unwrap();
    let mut producer = OriginalPendingProducerV1::open(&root, owner.clone()).unwrap();
    let original_bytes = vec![0x31, 0x00, 0xff, 0x42];
    producer
        .note_enqueued_with_action(
            LocalRequestIdentity {
                verified_subject: "subject".into(),
                operation_domain: "submit".into(),
                nonce_scope: "nonce".into(),
                request_id: "signed-submit-1".into(),
            },
            original_bytes.clone(),
            vec![0x02, 0x45],
        )
        .unwrap();

    let (client, mut server) = UnixStream::pair().unwrap();
    let peer_instance = expected.clone();
    let peer_directory = root.clone();
    let peer_owner = owner.clone();
    let peer = std::thread::spawn(move || {
        configure_control_stream(&server).unwrap();
        let request: ControlRequest = read_message(&mut server).unwrap();
        validate_control(&request, &peer_instance).unwrap();
        assert_eq!(request.action, "freeze_original_pending");
        let prepared = producer.begin_freeze(&peer_directory).unwrap();
        assert_eq!(prepared.count, 1);
        // A durable preparation is never a completed Node Frozen marker.
        assert!(cold_read_exact_frozen(
            &peer_directory,
            &peer_owner,
            &prepared.root,
            prepared.count,
        )
        .is_err());
        // This isolated transport test represents the already-observed
        // Node-stop completion; live Node quiescence is tested separately.
        let frozen = producer.finish_node_freeze(&peer_directory).unwrap();
        assert_eq!(frozen, prepared);
        assert!(producer.is_frozen());
        // The owner finished the immutable write, then the socket vanished
        // before the caller could receive any reply.
        drop(server);
        frozen
    });
    let lost = send_control_reply(
        &absent_state_path(),
        &expected,
        "freeze_original_pending",
        Instant::now() + Duration::from_secs(2),
        client,
    );
    assert!(
        lost.is_err(),
        "lost freeze reply cannot report successful owner control"
    );
    let frozen = peer.join().unwrap();
    assert!(OriginalPendingProducerV1::open(&root, owner.clone()).is_err());

    // This is a pinned local data read, NOT independent protected recovery
    // entitlement. The real process E2E must supply its own root/count.
    let recovered =
        cold_read_exact_frozen(&root, &owner, &frozen.root, frozen.count).unwrap();
    assert_eq!(recovered.requests.len(), 1);
    assert_eq!(recovered.requests[0].signed_request, original_bytes);
    assert!(cold_read_exact_frozen(&root, &owner, &frozen.root, 0).is_err());
    let mut wrong = owner.clone();
    wrong.incarnation = "replacement-owner".into();
    assert!(
        cold_read_exact_frozen(&root, &wrong, &frozen.root, frozen.count).is_err()
    );
    std::fs::remove_dir_all(root).unwrap();
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
