use super::super::local_io;
use super::{
    direct_http,
    operation_ledger::{Direction, Identity, Ledger, Policy},
};
use std::{
    io::{Read, Write},
    net::TcpListener,
    time::{Duration, Instant},
};

struct TestDir(std::path::PathBuf);
impl TestDir {
    fn new() -> Self {
        let path = std::env::temp_dir().canonicalize().unwrap().join(format!(
            "oasis7-operation-test-{}",
            local_io::random_id().unwrap()
        ));
        local_io::private_dir(&path).unwrap();
        Self(path)
    }
    fn path(&self) -> &std::path::Path {
        &self.0
    }
}
impl Drop for TestDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn policy() -> Policy {
    Policy {
        api_version: "oasis7.formal-full-sync-transfer-policy/v1".into(),
        attempt_timeout_ms: 10_000,
        max_attempts: 2,
        max_encoded_io_bytes: 1024,
        max_exchanges: 4,
        max_proof_pages: 2,
        max_proof_bytes: 1024,
        max_inflight_requests: 1,
        reopen_timeout_ms: 10_000,
    }
}
fn identity(policy: &Policy) -> Identity {
    Identity {
        operation_id: local_io::random_id().unwrap(),
        request_digest: "11".repeat(32),
        policy_digest: local_io::digest(policy).unwrap(),
        binding_digest: "22".repeat(32),
    }
}

#[test]
fn zero_attempt_ledger_rejects_forged_activity_on_cold_open() {
    let temp = TestDir::new();
    let path = temp.path().join("operation.json");
    let policy = policy();
    let identity = identity(&policy);
    let ledger =
        Ledger::create_with_material_limit(&path, identity.clone(), policy.clone(), 10).unwrap();
    let pristine = ledger.snapshot().clone();
    assert_eq!(pristine.attempts_used, 0);
    drop(ledger);

    // These fields can only be populated after a durable begin_attempt.
    // Test each independently without changing the fixed identity or policy.
    for field in [
        "active_attempt_nonce",
        "encoded_tx_actual",
        "encoded_rx_actual",
        "unknown_final_charged",
        "exchanges_started",
        "window_seq",
        "proof_pages",
        "proof_bytes",
        "material_bytes",
        "verified_target_json",
    ] {
        let mut forged = serde_json::to_value(&pristine).unwrap();
        forged[field] = match field {
            "active_attempt_nonce" => serde_json::json!("ab".repeat(16)),
            "verified_target_json" => serde_json::json!([91, 93]),
            _ => serde_json::json!(1),
        };
        if field == "material_bytes" {
            forged["materials"]
                .as_object_mut()
                .unwrap()
                .insert("ab".repeat(32), serde_json::json!(1));
        }
        let forged: super::operation_snapshot::Snapshot =
            serde_json::from_value(forged).unwrap();
        // Structural validation is performed before attaching a recovered
        // owner, so a fabricated operation cannot be resumed as original.
        assert!(forged.validate().is_err(), "{field}");
        local_io::write(&path, &forged).unwrap();
        assert!(Ledger::open(&path, &identity, &policy, 10).is_err(), "{field}");
    }
    local_io::write(&path, &pristine).unwrap();
    let reopened = Ledger::open(&path, &identity, &policy, 10).unwrap();
    assert_eq!(reopened.snapshot(), &pristine);
}

#[test]
fn cold_open_rejects_zero_exchange_sequence_but_keeps_real_exchange() {
    let temp = TestDir::new();
    let path = temp.path().join("operation.json");
    let policy = policy();
    let identity = identity(&policy);
    let mut ledger = Ledger::create(&path, identity.clone(), policy.clone()).unwrap();
    let _attempt = ledger.begin_attempt().unwrap();
    let pristine = ledger.snapshot().clone();
    assert_eq!(pristine.attempts_used, 1);
    assert_eq!(pristine.exchanges_started, 0);
    drop(ledger);

    // A forged request #0 has an otherwise valid live attempt and request
    // nonce. No actual begin_exchange could have created this combination.
    let mut forged = pristine.clone();
    forged.active_request = Some((pristine.attempts_used, 0));
    forged.active_request_nonce = Some("ab".repeat(16));
    assert!(forged.validate().is_err());
    local_io::write(&path, &forged).unwrap();
    assert!(Ledger::open(&path, &identity, &policy, 0).is_err());

    // Restoring the original admitted attempt is valid; a real persisted
    // exchange must also remain recoverable as an observation.
    local_io::write(&path, &pristine).unwrap();
    let reopened = Ledger::open(&path, &identity, &policy, 0).unwrap();
    assert_eq!(reopened.snapshot(), &pristine);
    drop(reopened);

    // A newly admitted operation on a separate ledger exercises the normal
    // one-based sequence producer without reusing the recovered handle.
    let live_path = temp.path().join("real-operation.json");
    let mut live = Ledger::create(&live_path, identity.clone(), policy.clone()).unwrap();
    let real_attempt = live.begin_attempt().unwrap();
    let _exchange = live.begin_exchange(&real_attempt, 1, 1).unwrap();
    let real_snapshot = live.snapshot().clone();
    assert_eq!(real_snapshot.active_request, Some((1, 1)));
    drop(live);
    let reopened = Ledger::open(&live_path, &identity, &policy, 0).unwrap();
    assert_eq!(reopened.snapshot(), &real_snapshot);
}

#[test]
fn elapsed_admission_persistence_does_not_renew_attempt_or_refund_its_slot() {
    let temp = TestDir::new();
    let path = temp.path().join("operation.json");
    let mut policy = policy();
    policy.attempt_timeout_ms = 100;
    let identity = identity(&policy);
    let mut ledger = Ledger::create(&path, identity.clone(), policy.clone()).unwrap();

    // Deterministically model a filesystem operation that finishes after the
    // original 100ms admission window. No sleep or transport timing is needed.
    let started = Instant::now()
        .checked_sub(Duration::from_millis(200))
        .unwrap();
    assert_eq!(
        ledger.begin_attempt_from(started).err().as_deref(),
        Some("attempt_expired")
    );
    // Commit was durable; the original first attempt is spent, not refunded or
    // reissued with a new deadline after the slow persistence completes.
    let spent = ledger.snapshot().clone();
    assert_eq!(spent.attempts_used, 1);
    assert!(spent.active_attempt_nonce.is_some());
    assert!(ledger.begin_attempt().is_err());
    drop(ledger);

    // Only the original ledger can be reopened for explicit fenced recovery.
    let reopened = Ledger::open(&path, &identity, &policy, 0).unwrap();
    assert_eq!(reopened.snapshot(), &spent);
}

#[test]
fn exchange_persisted_after_deadline_cannot_issue_a_new_io_permit() {
    let temp = TestDir::new();
    let path = temp.path().join("operation.json");
    let policy = policy();
    let identity = identity(&policy);
    let mut ledger = Ledger::create(&path, identity.clone(), policy.clone()).unwrap();
    let attempt = ledger.begin_attempt().unwrap();

    // A deterministic post-write deadline observation models slow fsync
    // without sleeping or depending on scheduler latency.
    let error = ledger
        .begin_exchange_with_finished_at(&attempt, 32, 32, || attempt.deadline())
        .err()
        .unwrap();
    assert_eq!(error, "attempt_expired");
    let spent = ledger.snapshot().clone();
    assert_eq!(spent.exchanges_started, 1);
    assert_eq!(spent.active_request, Some((1, 1)));
    assert_eq!(spent.tx_credit, 32);
    assert_eq!(spent.rx_credit, 32);
    assert!(ledger.begin_exchange(&attempt, 1, 1).is_err());
    drop(ledger);

    let reopened = Ledger::open(&path, &identity, &policy, 0).unwrap();
    assert_eq!(reopened.snapshot(), &spent);
}

#[test]
fn reserved_body_window_persisted_after_deadline_never_returns_token() {
    let temp = TestDir::new();
    let path = temp.path().join("operation.json");
    let policy = policy();
    let identity = identity(&policy);
    let mut ledger = Ledger::create(&path, identity.clone(), policy.clone()).unwrap();
    let attempt = ledger.begin_attempt().unwrap();
    let request = ledger.begin_exchange(&attempt, 32, 32).unwrap();
    let paid_before = ledger.snapshot().charged().unwrap();

    let error = ledger
        .reserve_with_finished_at(&attempt, &request, Direction::Rx, 20, || attempt.deadline())
        .err()
        .unwrap();
    assert_eq!(error, "attempt_expired");
    let spent = ledger.snapshot().clone();
    assert_eq!(spent.window_seq, 1);
    assert_eq!(spent.reservation.as_ref().unwrap().bytes, 20);
    assert_eq!(spent.charged().unwrap(), paid_before);
    assert!(ledger.reserve(&attempt, &request, Direction::Rx, 1).is_err());
    drop(ledger);

    let reopened = Ledger::open(&path, &identity, &policy, 0).unwrap();
    assert_eq!(reopened.snapshot(), &spent);
    assert_eq!(reopened.snapshot().charged().unwrap(), paid_before);
}

#[test]
fn real_tcp_body_accounting_survives_original_identity_reopen() {
    let temp = TestDir::new();
    let path = temp.path().join("operation.json");
    let policy = policy();
    let identity = identity(&policy);
    let mut ledger = Ledger::create(&path, identity.clone(), policy.clone()).unwrap();
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let peer = std::thread::spawn(move || {
        listener.set_nonblocking(true).unwrap();
        let until = Instant::now() + Duration::from_secs(5);
        let mut socket = loop {
            match listener.accept() {
                Ok((socket, _)) => break socket,
                Err(error)
                    if error.kind() == std::io::ErrorKind::WouldBlock && Instant::now() < until =>
                {
                    std::thread::sleep(Duration::from_millis(5))
                }
                Err(error) => panic!("bounded test accept failed: {error}"),
            }
        };
        socket
            .set_read_timeout(Some(Duration::from_secs(5)))
            .unwrap();
        let mut headers = Vec::new();
        while !headers.ends_with(b"\r\n\r\n") {
            let mut b = [0];
            socket.read_exact(&mut b).unwrap();
            headers.push(b[0]);
        }
        let mut body = [0; 7];
        socket.read_exact(&mut body).unwrap();
        assert_eq!(&body, b"{\"x\":1}");
        socket
            .write_all(
                b"HTTP/1.1 403 Forbidden\r\nContent-Length: 5\r\nConnection: close\r\n\r\ndenie",
            )
            .unwrap();
    });
    let attempt = ledger.begin_attempt().unwrap();
    // Error-status bodies are still charged; parsing authorization is a later layer.
    let result = direct_http::post(
        direct_http::Request {
            address,
            path: "/read",
            body: b"{\"x\":1}",
            response_bound: 16,
            request_timeout: Duration::from_secs(2),
        },
        &mut ledger,
        &attempt,
        Instant::now() + Duration::from_secs(5),
    );
    peer.join().unwrap();
    assert_eq!(result.unwrap(), (403, b"denie".to_vec()));
    assert_eq!(ledger.snapshot().encoded_tx_actual, 7);
    assert_eq!(ledger.snapshot().encoded_rx_actual, 5);
    ledger.finish_attempt(&attempt).unwrap();
    drop(ledger);
    let restored = Ledger::open(&path, &identity, &policy, 0).unwrap();
    assert_eq!(restored.snapshot().encoded_tx_actual, 7);
    assert_eq!(restored.snapshot().encoded_rx_actual, 5);
}

#[test]
fn unresolved_reservation_is_not_refunded_by_reopen() {
    let temp = TestDir::new();
    let path = temp.path().join("operation.json");
    let policy = policy();
    let identity = identity(&policy);
    let mut ledger = Ledger::create(&path, identity.clone(), policy.clone()).unwrap();
    let attempt = ledger.begin_attempt().unwrap();
    let request = ledger.begin_exchange(&attempt, 32, 32).unwrap();
    let token = ledger
        .reserve(&attempt, &request, Direction::Rx, 32)
        .unwrap();
    drop(ledger); // no settlement: models process death at the I/O boundary
    let mut restored = Ledger::open(&path, &identity, &policy, 0).unwrap();
    assert_eq!(restored.snapshot().reservation.as_ref(), Some(&token));
    assert_eq!(restored.snapshot().encoded_rx_actual, 0);
    assert!(restored.settle(&token, 0).is_err());
    assert!(restored.begin_attempt().is_err());
}

#[test]
fn foreign_token_and_policy_changes_are_rejected() {
    let temp = TestDir::new();
    let policy = policy();
    let mut first = Ledger::create(
        &temp.path().join("a.json"),
        identity(&policy),
        policy.clone(),
    )
    .unwrap();
    let mut second = Ledger::create(
        &temp.path().join("b.json"),
        identity(&policy),
        policy.clone(),
    )
    .unwrap();
    let a = first.begin_attempt().unwrap();
    let b = second.begin_attempt().unwrap();
    let ar = first.begin_exchange(&a, 10, 10).unwrap();
    let br = second.begin_exchange(&b, 10, 10).unwrap();
    let token = first.reserve(&a, &ar, Direction::Tx, 10).unwrap();
    second.reserve(&b, &br, Direction::Tx, 10).unwrap();
    assert!(second.settle(&token, 10).is_err());
    let original = first.snapshot().identity.clone();
    drop(first);
    let mut changed = policy.clone();
    changed.max_encoded_io_bytes += 1;
    assert!(Ledger::open(&temp.path().join("a.json"), &original, &changed, 0).is_err());
}

#[test]
fn inadequate_initial_credit_never_starts_network_or_charges_exchange() {
    let temp = TestDir::new();
    let mut policy = policy();
    policy.max_encoded_io_bytes = 7;
    let mut ledger = Ledger::create(
        &temp.path().join("operation.json"),
        identity(&policy),
        policy,
    )
    .unwrap();
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let attempt = ledger.begin_attempt().unwrap();
    let error = direct_http::post(
        direct_http::Request {
            address: listener.local_addr().unwrap(),
            path: "/read",
            body: b"1234567",
            response_bound: 16,
            request_timeout: Duration::from_secs(1),
        },
        &mut ledger,
        &attempt,
        attempt.deadline(),
    )
    .unwrap_err();
    assert!(error.contains("initial HTTP body credit"));
    assert_eq!(ledger.snapshot().exchanges_started, 0);
    assert!(ledger.snapshot().active_request.is_none());
    assert_eq!(
        listener.accept().unwrap_err().kind(),
        std::io::ErrorKind::WouldBlock
    );
}

#[test]
fn initial_duplex_credit_survives_crash_and_foreign_finish_cannot_release_it() {
    let temp = TestDir::new();
    let policy = policy();
    let a_path = temp.path().join("a.json");
    let b_path = temp.path().join("b.json");
    let a_id = identity(&policy);
    let mut a = Ledger::create(&a_path, a_id.clone(), policy.clone()).unwrap();
    let mut b = Ledger::create(&b_path, identity(&policy), policy.clone()).unwrap();
    let aa = a.begin_attempt().unwrap();
    let ba = b.begin_attempt().unwrap();
    let ar = a.begin_exchange(&aa, 17, 29).unwrap();
    let br = b.begin_exchange(&ba, 17, 29).unwrap();
    assert!(b.finish_exchange(&ba, &ar).is_err());
    assert_eq!(b.snapshot().tx_credit, 17);
    assert_eq!(b.snapshot().rx_credit, 29);
    b.finish_exchange(&ba, &br).unwrap();
    assert_eq!(b.snapshot().tx_credit, 0);
    assert_eq!(b.snapshot().rx_credit, 0);
    drop(a);
    let restored = Ledger::open(&a_path, &a_id, &policy, 0).unwrap();
    assert_eq!(restored.snapshot().tx_credit, 17);
    assert_eq!(restored.snapshot().rx_credit, 29);
    assert_eq!(restored.snapshot().encoded_tx_actual, 0);
    assert_eq!(restored.snapshot().encoded_rx_actual, 0);
}

#[test]
fn proof_and_material_charges_are_durable_and_exhaustion_does_not_reset_them() {
    let temp = TestDir::new();
    let path = temp.path().join("operation.json");
    let policy = policy();
    let identity = identity(&policy);
    let mut ledger =
        Ledger::create_with_material_limit(&path, identity.clone(), policy.clone(), 7).unwrap();
    let attempt = ledger.begin_attempt().unwrap();
    ledger.charge_proof(&attempt, 512).unwrap();
    ledger.charge_proof(&attempt, 512).unwrap();
    let before = ledger.snapshot().clone();
    assert!(ledger.charge_proof(&attempt, 1).is_err());
    assert_eq!(ledger.snapshot(), &before);
    let digest = "ab".repeat(32);
    ledger.charge_material(&attempt, &digest, 7).unwrap();
    let charged = ledger.snapshot().clone();
    ledger.charge_material(&attempt, &digest, 7).unwrap();
    assert_eq!(ledger.snapshot(), &charged);
    assert!(ledger.charge_material(&attempt, &digest, 6).is_err());
    assert!(
        ledger
            .charge_material(&attempt, &"cd".repeat(32), 1)
            .is_err()
    );
    assert_eq!(ledger.snapshot(), &charged);
    ledger.finish_attempt(&attempt).unwrap();
    assert!(ledger.check_attempt(&attempt).is_err());
    let final_state = ledger.snapshot().clone();
    drop(ledger);
    assert!(Ledger::open(&path, &identity, &policy, 6).is_err());
    let reopened = Ledger::open(&path, &identity, &policy, 7).unwrap();
    assert_eq!(reopened.snapshot(), &final_state);
}

#[test]
fn uncertain_proof_persistence_invalidates_active_permit() {
    let temp = TestDir::new();
    let path = temp.path().join("operation.json");
    let policy = policy();
    let identity = identity(&policy);
    let mut ledger = Ledger::create(&path, identity, policy).unwrap();
    let attempt = ledger.begin_attempt().unwrap();
    std::fs::write(&path, b"{broken").unwrap();
    assert!(ledger.charge_proof(&attempt, 1).is_err());
    assert!(ledger.check_attempt(&attempt).is_err());
    assert!(ledger.finish_attempt(&attempt).is_err());
    assert!(ledger.begin_attempt().is_err());
    assert_eq!(std::fs::read(&path).unwrap(), b"{broken");
}

#[test]
fn late_target_persistence_keeps_original_selection_without_granting_more_io() {
    let temp = TestDir::new();
    let path = temp.path().join("operation.json");
    let policy = policy();
    let identity = identity(&policy);
    let mut ledger = Ledger::create(&path, identity.clone(), policy.clone()).unwrap();
    let attempt = ledger.begin_attempt().unwrap();
    let target = br#"[{"position":7},{"record":"original"}]"#;

    // Deterministically model slow target persistence ending at the already
    // fixed attempt deadline. There is no sleep and no manufactured renewal.
    let error = ledger
        .bind_verified_target_with_finished_at(&attempt, target, || attempt.deadline())
        .unwrap_err();
    assert_eq!(error, "attempt_expired");
    let saved = ledger.snapshot().clone();
    assert_eq!(saved.verified_target_json.as_deref(), Some(target.as_slice()));
    assert_eq!(saved.attempts_used, 1);
    assert!(saved.active_attempt_nonce.is_some());
    assert_eq!(saved.exchanges_started, 0);
    assert!(ledger.check_attempt(&attempt).is_err());
    assert!(ledger.bind_verified_target(&attempt, target).is_err());
    assert!(ledger.begin_exchange(&attempt, 1, 1).is_err());
    assert!(
        ledger
            .begin_publication(&attempt, &serde_json::json!({"position":7}), &"ab".repeat(32))
            .is_err()
    );
    drop(ledger);

    // The same durable target is visible for explicit fenced recovery, not
    // silently repinned or replaced under the old attempt.
    let recovered = Ledger::open(&path, &identity, &policy, 0).unwrap();
    assert_eq!(recovered.snapshot(), &saved);
}

#[test]
fn original_verified_target_is_durable_and_cannot_be_replaced() {
    let temp = TestDir::new();
    let path = temp.path().join("operation.json");
    let policy = policy();
    let identity = identity(&policy);
    let mut ledger = Ledger::create(&path, identity.clone(), policy.clone()).unwrap();
    let attempt = ledger.begin_attempt().unwrap();
    // Ledger retains caller-verified bytes; it does not confer proof validity.
    let target = br#"[{"position":7},{"record":"original"}]"#;
    ledger.bind_verified_target(&attempt, target).unwrap();
    let original = ledger.snapshot().clone();
    ledger.bind_verified_target(&attempt, target).unwrap();
    assert_eq!(ledger.snapshot(), &original);
    assert!(
        ledger
            .bind_verified_target(&attempt, br#"[{"position":8},{}]"#)
            .is_err()
    );
    assert_eq!(ledger.snapshot(), &original);
    drop(ledger);
    let reopened = Ledger::open(&path, &identity, &policy, 0).unwrap();
    assert_eq!(reopened.snapshot(), &original);
}

#[test]
fn original_request_is_retained_exactly_and_tampering_blocks_reopen() {
    let temp = TestDir::new();
    let path = temp.path().join("operation.json");
    let policy = policy();
    let request = serde_json::json!({"origin":"https://example.invalid","minimum":{"position":7}});
    let mut identity = identity(&policy);
    identity.request_digest = local_io::digest(&request).unwrap();
    let mut ledger = Ledger::create(&path, identity.clone(), policy.clone()).unwrap();
    ledger.bind_original_request(&request).unwrap();
    let original = ledger.snapshot().clone();
    assert_eq!(
        original.original_request_json,
        Some(serde_json::to_vec(&request).unwrap())
    );
    ledger.bind_original_request(&request).unwrap();
    assert_eq!(ledger.snapshot(), &original);
    assert!(
        ledger
            .bind_original_request(&serde_json::json!({"minimum":6}))
            .is_err()
    );
    assert_eq!(ledger.snapshot(), &original);
    drop(ledger);
    let reopened = Ledger::open(&path, &identity, &policy, 0).unwrap();
    assert_eq!(reopened.snapshot(), &original);
    drop(reopened);
    let mut corrupted = original;
    corrupted.original_request_json = Some(b"{}".to_vec());
    local_io::write(&path, &corrupted).unwrap();
    assert!(Ledger::open(&path, &identity, &policy, 0).is_err());
}

#[test]
fn original_request_cannot_be_backfilled_after_attempt_start() {
    let temp = TestDir::new();
    let path = temp.path().join("operation.json");
    let policy = policy();
    let request = serde_json::json!({"original":"request"});
    let mut identity = identity(&policy);
    identity.request_digest = local_io::digest(&request).unwrap();
    let mut ledger = Ledger::create(&path, identity, policy).unwrap();
    let _attempt = ledger.begin_attempt().unwrap();
    let before = ledger.snapshot().clone();
    assert!(ledger.bind_original_request(&request).is_err());
    assert_eq!(ledger.snapshot(), &before);
}

#[test]
fn late_publication_intent_persistence_never_authorizes_external_pin_cas() {
    let temp = TestDir::new();
    let path = temp.path().join("operation.json");
    let policy = policy();
    let identity = identity(&policy);
    let mut ledger = Ledger::create(&path, identity.clone(), policy.clone()).unwrap();
    let attempt = ledger.begin_attempt().unwrap();
    let cut = serde_json::json!({"record_id": "original"});
    let target =
        serde_json::to_vec(&(cut.clone(), serde_json::json!({"original_record": true}))).unwrap();
    let roots = "ab".repeat(32);
    ledger.bind_verified_target(&attempt, &target).unwrap();

    // A slow durable intent write reaches the exclusive attempt deadline.
    // The caller must NOT receive success and start external root publication.
    let error = ledger
        .begin_publication_with_finished_at(&attempt, &cut, &roots, || attempt.deadline())
        .unwrap_err();
    assert_eq!(error, "attempt_expired");
    let pending = ledger.snapshot().clone();
    assert_eq!(
        pending.publication.as_ref().map(|journal| journal.completed),
        Some(false)
    );
    assert!(ledger.check_attempt(&attempt).is_err());
    assert!(ledger.complete_publication(&attempt, &cut, &roots).is_err());
    assert!(ledger.begin_publication(&attempt, &cut, &roots).is_err());
    drop(ledger);

    // The exact pending intent survives cold observation; no fabricated
    // acknowledgement or replay permission may appear on the reopened handle.
    let mut reopened = Ledger::open(&path, &identity, &policy, 0).unwrap();
    assert_eq!(reopened.snapshot(), &pending);
    assert!(reopened.complete_publication(&attempt, &cut, &roots).is_err());
    assert_eq!(reopened.snapshot(), &pending);
}

#[test]
fn publication_intent_is_exact_and_unknown_reopen_cannot_replay() {
    let temp = TestDir::new();
    let path = temp.path().join("operation.json");
    let policy = policy();
    let identity = identity(&policy);
    let mut ledger = Ledger::create(&path, identity.clone(), policy.clone()).unwrap();
    let attempt = ledger.begin_attempt().unwrap();
    let cut = serde_json::json!({"record_id": "original"});
    let target =
        serde_json::to_vec(&(cut.clone(), serde_json::json!({"original_record": true}))).unwrap();
    let roots = "ab".repeat(32);
    assert!(ledger.begin_publication(&attempt, &cut, &roots).is_err());
    ledger.bind_verified_target(&attempt, &target).unwrap();
    let selected = ledger.snapshot().clone();
    assert!(
        ledger
            .begin_publication(
                &attempt,
                &serde_json::json!({"record_id":"replacement"}),
                &roots
            )
            .is_err()
    );
    assert_eq!(ledger.snapshot(), &selected);
    ledger.begin_publication(&attempt, &cut, &roots).unwrap();
    let pending = ledger.snapshot().clone();
    assert!(!pending.publication.as_ref().unwrap().completed);
    assert!(ledger.finish_attempt(&attempt).is_err());
    assert!(ledger.begin_publication(&attempt, &cut, &roots).is_err());
    assert!(
        ledger
            .complete_publication(&attempt, &cut, &"cd".repeat(32))
            .is_err()
    );
    assert_eq!(ledger.snapshot(), &pending);
    drop(ledger);
    let mut ledger = Ledger::open(&path, &identity, &policy, 0).unwrap();
    assert_eq!(ledger.snapshot(), &pending);
    assert!(ledger.complete_publication(&attempt, &cut, &roots).is_err());
    assert!(ledger.begin_attempt().is_err());
    assert_eq!(ledger.snapshot(), &pending);
}

#[test]
fn publication_intent_and_ack_block_new_real_http_exchange() {
    let temp = TestDir::new();
    let path = temp.path().join("operation.json");
    let policy = policy();
    let identity = identity(&policy);
    let mut ledger = Ledger::create(&path, identity.clone(), policy.clone()).unwrap();
    let attempt = ledger.begin_attempt().unwrap();
    let cut = serde_json::json!({"record_id": "original"});
    let target =
        serde_json::to_vec(&(cut.clone(), serde_json::json!({"original_record":true}))).unwrap();
    let roots = "ab".repeat(32);
    ledger.bind_verified_target(&attempt, &target).unwrap();
    ledger.begin_publication(&attempt, &cut, &roots).unwrap();

    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    // Exercise the real controlled HTTP consumer, not only Ledger's public
    // method. Neither an unknown intent nor an acknowledged publication
    // authorizes another network request on the original operation.
    for acknowledged in [false, true] {
        let before = ledger.snapshot().clone();
        let error = direct_http::post(
            direct_http::Request {
                address: listener.local_addr().unwrap(),
                path: "/read",
                body: b"{}",
                response_bound: 16,
                request_timeout: Duration::from_secs(1),
            },
            &mut ledger,
            &attempt,
            Instant::now() + Duration::from_secs(2),
        )
        .unwrap_err();
        assert!(
            error.contains("publication has already started"),
            "acknowledged={acknowledged}: {error}"
        );
        assert_eq!(ledger.snapshot(), &before);
        assert_eq!(ledger.snapshot().exchanges_started, 0);
        assert_eq!(
            listener.accept().unwrap_err().kind(),
            std::io::ErrorKind::WouldBlock
        );
        if !acknowledged {
            ledger.complete_publication(&attempt, &cut, &roots).unwrap();
        }
    }
    ledger.finish_attempt(&attempt).unwrap();
    let finished = ledger.snapshot().clone();
    drop(ledger);

    // A forged durable exchange after publication must also be refused on
    // cold read, even when all other counters and original pins are valid.
    let mut forged = finished.clone();
    forged.exchanges_started = 1;
    forged.active_attempt_nonce = Some("ab".repeat(16));
    forged.active_request = Some((1, 1));
    forged.active_request_nonce = Some("cd".repeat(16));
    forged.tx_credit = 1;
    assert!(forged.validate().is_err());
    local_io::write(&path, &forged).unwrap();
    assert!(Ledger::open(&path, &identity, &policy, 0).is_err());

    local_io::write(&path, &finished).unwrap();
    let reopened = Ledger::open(&path, &identity, &policy, 0).unwrap();
    assert_eq!(reopened.snapshot(), &finished);
}

#[test]
fn publication_intent_freezes_proof_and_material_progress_through_cold_reopen() {
    let temp = TestDir::new();
    let path = temp.path().join("operation.json");
    let policy = policy();
    let identity = identity(&policy);
    let mut ledger =
        Ledger::create_with_material_limit(&path, identity.clone(), policy.clone(), 16).unwrap();
    let attempt = ledger.begin_attempt().unwrap();
    let first_material = "ab".repeat(32);
    let other_material = "cd".repeat(32);

    // Both accounting operations are allowed during the real sync phase.
    ledger.charge_proof(&attempt, 7).unwrap();
    ledger.charge_material(&attempt, &first_material, 4).unwrap();
    let cut = serde_json::json!({"record_id": "original"});
    let target =
        serde_json::to_vec(&(cut.clone(), serde_json::json!({"original_record": true}))).unwrap();
    let roots = "ef".repeat(32);
    ledger.bind_verified_target(&attempt, &target).unwrap();
    ledger.begin_publication(&attempt, &cut, &roots).unwrap();

    // A pending publication may already have had an external effect. Neither
    // new proof/material work nor a repeated prior material may advance its
    // original budget after this point, including after acknowledgement.
    for completed in [false, true] {
        let before = ledger.snapshot().clone();
        for result in [
            ledger.charge_proof(&attempt, 1),
            ledger.charge_material(&attempt, &other_material, 1),
            ledger.charge_material(&attempt, &first_material, 4),
        ] {
            let error = result.unwrap_err();
            assert!(
                error.contains("publication has already started"),
                "completed={completed}: {error}"
            );
        }
        assert_eq!(ledger.snapshot(), &before);
        if !completed {
            ledger.complete_publication(&attempt, &cut, &roots).unwrap();
        }
    }
    ledger.finish_attempt(&attempt).unwrap();
    let completed = ledger.snapshot().clone();
    assert_eq!(completed.proof_pages, 1);
    assert_eq!(completed.proof_bytes, 7);
    assert_eq!(completed.material_bytes, 4);
    drop(ledger);
    let reopened = Ledger::open(&path, &identity, &policy, 16).unwrap();
    assert_eq!(reopened.snapshot(), &completed);
}

#[test]
fn publication_acknowledgement_is_durable_but_not_install_authority() {
    let temp = TestDir::new();
    let path = temp.path().join("operation.json");
    let policy = policy();
    let identity = identity(&policy);
    let mut ledger = Ledger::create(&path, identity.clone(), policy.clone()).unwrap();
    let attempt = ledger.begin_attempt().unwrap();
    let cut = serde_json::json!({"record_id":"original"});
    let target =
        serde_json::to_vec(&(cut.clone(), serde_json::json!({"original_record":true}))).unwrap();
    let roots = "ab".repeat(32);
    ledger.bind_verified_target(&attempt, &target).unwrap();
    ledger.begin_publication(&attempt, &cut, &roots).unwrap();
    ledger.complete_publication(&attempt, &cut, &roots).unwrap();
    assert!(ledger.snapshot().publication.as_ref().unwrap().completed);
    assert!(ledger.complete_publication(&attempt, &cut, &roots).is_err());
    ledger.finish_attempt(&attempt).unwrap();
    let completed = ledger.snapshot().clone();
    assert!(ledger.begin_attempt().is_err());
    drop(ledger);
    let mut ledger = Ledger::open(&path, &identity, &policy, 0).unwrap();
    assert_eq!(ledger.snapshot(), &completed);
    assert!(ledger.begin_attempt().is_err());
}

#[test]
fn fenced_pre_target_reopen_preserves_the_original_pending_reservation() {
    use super::super::process;

    let temp = TestDir::new();
    let data = temp.path();
    local_io::private_file(&data.join("process.lock")).unwrap();
    local_io::private_dir(&data.join("state")).unwrap();
    let sync = data.join("state/full-sync");
    local_io::private_dir(&sync).unwrap();
    let path = sync.join("operation.json");
    let owner = process::lock(data).unwrap();
    let binding = owner.binding_digest().unwrap();
    let request = serde_json::json!({
        "native_owner_binding": binding,
        "original": "pinned request"
    });
    let policy = policy();
    let mut identity = identity(&policy);
    identity.request_digest = local_io::digest(&request).unwrap();
    identity.binding_digest = binding;
    let mut ledger = Ledger::create(&path, identity.clone(), policy.clone()).unwrap();
    ledger.bind_original_request(&request).unwrap();
    let old_attempt = ledger.begin_attempt().unwrap();
    let old_exchange = ledger.begin_exchange(&old_attempt, 32, 48).unwrap();
    let old_reservation = ledger
        .reserve(&old_attempt, &old_exchange, Direction::Rx, 24)
        .unwrap();
    let before = ledger.snapshot().clone();
    assert_eq!(before.unresolved_reserved().unwrap(), 80);
    assert!(process::lock(data).is_err());
    drop(ledger);
    drop(owner);

    let owner = process::lock(data).unwrap();
    let mut reopened = Ledger::open(&path, &identity, &policy, 0).unwrap();
    let deadline = Instant::now() + Duration::from_secs(10);
    let mut wrong = request.clone();
    wrong["original"] = serde_json::json!("different");
    assert!(
        reopened
            .resume_before_target(&owner, data, &wrong, deadline)
            .is_err()
    );
    assert_eq!(reopened.snapshot(), &before);
    let next = reopened
        .resume_before_target(&owner, data, &request, deadline)
        .unwrap();
    let retained = reopened.snapshot();
    assert_eq!(retained.attempts_used, 2);
    assert_eq!(retained.identity, identity);
    assert_eq!(retained.unresolved_reserved().unwrap(), 80);
    assert_eq!(
        retained.interrupted_attempts[0].reservation,
        Some(old_reservation.clone())
    );
    assert_eq!(retained.exchanges_started, 1);
    assert!(retained.reservation.is_none());
    assert!(reopened.settle(&old_reservation, 0).is_err());
    assert!(reopened.begin_exchange(&old_attempt, 1, 1).is_err());
    reopened.finish_attempt(&next).unwrap();
    let closed = reopened.snapshot().clone();
    drop(reopened);

    let mut reopened = Ledger::open(&path, &identity, &policy, 0).unwrap();
    assert!(
        reopened
            .resume_before_target(&owner, data, &request, deadline)
            .is_err()
    );
    assert_eq!(reopened.snapshot(), &closed);
}

#[test]
fn native_initial_ledger_durably_publishes_the_complete_original_request() {
    use super::super::process;

    let temp = TestDir::new();
    let data = temp.path();
    local_io::private_file(&data.join("process.lock")).unwrap();
    local_io::private_dir(&data.join("state")).unwrap();
    let sync = data.join("state/full-sync");
    local_io::private_dir(&sync).unwrap();
    let path = sync.join("operation.json");
    let owner = process::lock(data).unwrap();
    let binding = owner.binding_digest().unwrap();
    let request = serde_json::json!({
        "native_owner_binding": binding,
        "original": {"minimum": 37}
    });
    let policy = policy();
    let mut identity = identity(&policy);
    identity.request_digest = local_io::digest(&request).unwrap();
    let wrong_owner = Ledger::create_bound(
        &path,
        identity.clone(),
        policy.clone(),
        1,
        &request,
        &owner,
        data,
    );
    assert!(wrong_owner.is_err());
    assert!(!path.exists());

    identity.binding_digest = binding;
    let ledger = Ledger::create_bound(
        &path,
        identity.clone(),
        policy.clone(),
        1,
        &request,
        &owner,
        data,
    )
    .unwrap();
    let original = ledger.snapshot().clone();
    assert_eq!(original.revision, 0);
    assert_eq!(original.attempts_used, 0);
    assert_eq!(
        original.original_request_json.as_deref(),
        Some(serde_json::to_vec(&request).unwrap().as_slice())
    );
    drop(ledger);

    let repeated = Ledger::create_bound(
        &path,
        identity.clone(),
        policy.clone(),
        1,
        &request,
        &owner,
        data,
    );
    assert!(repeated.is_err());
    let observed = Ledger::open(&path, &identity, &policy, 1).unwrap();
    assert_eq!(observed.snapshot(), &original);
}
