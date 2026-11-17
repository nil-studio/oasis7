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
fn pre_target_recovery_preserves_unknown_credit_and_original_owner() {
    let temp = TestDir::new();
    drop(local_io::private_file(&temp.path().join("process.lock")).unwrap());
    local_io::private_dir(&temp.path().join("state")).unwrap();
    let state = temp.path().join("state/full-sync");
    local_io::private_dir(&state).unwrap();
    let path = state.join("operation.json");
    let policy = policy();
    let request = serde_json::json!({"minimum": 5});
    let mut id = identity(&policy);
    id.request_digest = local_io::digest(&request).unwrap();
    let owner = std::sync::Arc::new(super::super::process::lock(temp.path()).unwrap());
    let mut original = Ledger::create(&path, id.clone(), policy.clone()).unwrap();
    original.bind_owner(owner.clone(), temp.path()).unwrap();
    original.bind_original_request(&request).unwrap();
    let attempt = original.begin_attempt().unwrap();
    let request_permit = original.begin_exchange(&attempt, 19, 23).unwrap();
    let pending_window = original
        .reserve(&attempt, &request_permit, Direction::Tx, 10)
        .unwrap();
    let before = original.snapshot().clone();
    drop(original);
    drop(owner);
    let owner = std::sync::Arc::new(super::super::process::lock(temp.path()).unwrap());
    let mut reopened = Ledger::open(&path, &id, &policy, 0).unwrap();
    assert!(
        reopened
            .resume_before_target(
                owner.clone(),
                temp.path(),
                &serde_json::json!({"minimum": 4})
            )
            .is_err()
    );
    assert_eq!(reopened.snapshot(), &before);
    // A copied ledger cannot borrow a different native directory's valid lock.
    let other = TestDir::new();
    local_io::private_dir(&other.path().join("state")).unwrap();
    local_io::private_dir(&other.path().join("state/full-sync")).unwrap();
    drop(local_io::private_file(&other.path().join("process.lock")).unwrap());
    let copied_path = other.path().join("state/full-sync/operation.json");
    std::fs::copy(&path, &copied_path).unwrap();
    let copied_owner = std::sync::Arc::new(super::super::process::lock(other.path()).unwrap());
    let mut copied = Ledger::open(&copied_path, &id, &policy, 0).unwrap();
    assert!(
        copied
            .resume_before_target(copied_owner, other.path(), &request)
            .is_err()
    );
    assert_eq!(copied.snapshot(), &before);
    drop(copied);
    let next = reopened
        .resume_before_target(owner.clone(), temp.path(), &request)
        .unwrap();
    let after = reopened.snapshot();
    assert_eq!(after.revision, before.revision + 1);
    assert_eq!(after.attempts_used, 2);
    assert_eq!(after.interrupted_attempts.len(), 1);
    assert_eq!(
        serde_json::to_value(&after.interrupted_attempts[0]).unwrap()["reservation"],
        serde_json::to_value(&pending_window).unwrap()
    );
    assert_eq!(after.encoded_tx_actual, before.encoded_tx_actual);
    assert_eq!(after.encoded_rx_actual, before.encoded_rx_actual);
    assert_eq!(after.unknown_final_charged, 0);
    // 42 unresolved bytes still count against the original 1024 cap.
    assert!(reopened.begin_exchange(&next, 500, 500).is_err());
    drop(owner);
    assert!(super::super::process::lock(temp.path()).is_err());
    drop(reopened);
    drop(super::super::process::lock(temp.path()).unwrap());
    let mut observed = Ledger::open(&path, &id, &policy, 0).unwrap();
    assert!(observed.begin_attempt().is_err());
    assert_eq!(observed.snapshot().interrupted_attempts.len(), 1);
}

#[test]
fn pre_target_recovery_rejects_selected_expired_changed_or_replaced_state() {
    for case in ["selected", "expired", "predecessor", "owner", "unbound"] {
        let temp = TestDir::new();
        drop(local_io::private_file(&temp.path().join("process.lock")).unwrap());
        local_io::private_dir(&temp.path().join("state")).unwrap();
        local_io::private_dir(&temp.path().join("state/full-sync")).unwrap();
        let path = temp.path().join("state/full-sync/operation.json");
        let mut policy = policy();
        policy.reopen_timeout_ms = if case == "expired" { 100 } else { 10_000 };
        let request = serde_json::json!({"minimum": 9});
        let mut id = identity(&policy);
        id.request_digest = local_io::digest(&request).unwrap();
        let owner = std::sync::Arc::new(super::super::process::lock(temp.path()).unwrap());
        let mut ledger = Ledger::create(&path, id.clone(), policy.clone()).unwrap();
        if case != "unbound" {
            ledger.bind_owner(owner.clone(), temp.path()).unwrap();
        }
        ledger.bind_original_request(&request).unwrap();
        let attempt = ledger.begin_attempt().unwrap();
        if case == "selected" {
            ledger
                .bind_verified_target(&attempt, b"{\"cut\":10}")
                .unwrap();
        }
        drop(ledger);
        let mut reopened = Ledger::open(&path, &id, &policy, 0).unwrap();
        if case == "expired" {
            std::thread::sleep(Duration::from_millis(120));
        }
        if case == "predecessor" {
            let mut replaced = reopened.snapshot().clone();
            replaced.revision += 1;
            local_io::write(&path, &replaced).unwrap();
        }
        if case == "owner" {
            std::fs::rename(
                temp.path().join("process.lock"),
                temp.path().join("old.lock"),
            )
            .unwrap();
            drop(local_io::private_file(&temp.path().join("process.lock")).unwrap());
        }
        let before = std::fs::read(&path).unwrap();
        assert!(
            reopened
                .resume_before_target(owner, temp.path(), &request)
                .is_err(),
            "{case}"
        );
        assert_eq!(std::fs::read(&path).unwrap(), before, "{case}");
        assert!(reopened.begin_attempt().is_err(), "{case}");
    }
}

#[test]
fn pre_target_recovery_never_admits_fifth_attempt() {
    let temp = TestDir::new();
    drop(local_io::private_file(&temp.path().join("process.lock")).unwrap());
    local_io::private_dir(&temp.path().join("state")).unwrap();
    local_io::private_dir(&temp.path().join("state/full-sync")).unwrap();
    let path = temp.path().join("state/full-sync/operation.json");
    let mut policy = policy();
    policy.max_attempts = 4;
    let request = serde_json::json!({"minimum": 11});
    let mut id = identity(&policy);
    id.request_digest = local_io::digest(&request).unwrap();
    let owner = std::sync::Arc::new(super::super::process::lock(temp.path()).unwrap());
    let mut ledger = Ledger::create(&path, id.clone(), policy.clone()).unwrap();
    ledger.bind_owner(owner.clone(), temp.path()).unwrap();
    ledger.bind_original_request(&request).unwrap();
    let first = ledger.begin_attempt().unwrap();
    ledger.begin_exchange(&first, 3, 4).unwrap();
    for attempt_no in 2..=4 {
        drop(ledger);
        ledger = Ledger::open(&path, &id, &policy, 0).unwrap();
        let attempt = ledger
            .resume_before_target(owner.clone(), temp.path(), &request)
            .unwrap();
        assert_eq!(ledger.snapshot().attempts_used, attempt_no);
        assert_eq!(
            ledger.snapshot().interrupted_attempts.len() as u64,
            attempt_no - 1
        );
        ledger.begin_exchange(&attempt, 3, 4).unwrap();
    }
    drop(ledger);
    let mut reopened = Ledger::open(&path, &id, &policy, 0).unwrap();
    let before = std::fs::read(&path).unwrap();
    assert!(
        reopened
            .resume_before_target(owner, temp.path(), &request)
            .is_err()
    );
    assert_eq!(std::fs::read(&path).unwrap(), before);
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
fn publication_ack_failure_keeps_pending_and_poisoned() {
    let temp = TestDir::new();
    let path = temp.path().join("operation.json");
    let policy = policy();
    let identity = identity(&policy);
    let mut ledger = Ledger::create(&path, identity.clone(), policy.clone()).unwrap();
    let attempt = ledger.begin_attempt().unwrap();
    let cut = serde_json::json!({"record_id": "original"});
    let target = serde_json::to_vec(&(cut.clone(), serde_json::json!({}))).unwrap();
    let roots = "ab".repeat(32);
    ledger.bind_verified_target(&attempt, &target).unwrap();
    ledger.begin_publication(&attempt, &cut, &roots).unwrap();
    let mut disk = ledger.snapshot().clone();
    disk.revision += 1;
    local_io::write(&path, &disk).unwrap();
    assert!(ledger.complete_publication(&attempt, &cut, &roots).is_err());
    assert!(ledger.check_attempt(&attempt).is_err());
    assert!(ledger.finish_attempt(&attempt).is_err());
    drop(ledger);
    let reopened = Ledger::open(&path, &identity, &policy, 0).unwrap();
    assert_eq!(reopened.snapshot(), &disk);
    assert!(!reopened.snapshot().publication.as_ref().unwrap().completed);
}

#[test]
fn legacy_selected_ledger_does_not_acquire_publication_capability() {
    let temp = TestDir::new();
    let path = temp.path().join("operation.json");
    let policy = policy();
    let identity = identity(&policy);
    let mut ledger = Ledger::create(&path, identity.clone(), policy.clone()).unwrap();
    let attempt = ledger.begin_attempt().unwrap();
    ledger.bind_verified_target(&attempt, b"[{},{}]").unwrap();
    let mut old = serde_json::to_value(ledger.snapshot()).unwrap();
    old.as_object_mut().unwrap().remove("publication");
    old.as_object_mut()
        .unwrap()
        .remove("publication_journal_version");
    drop(ledger);
    local_io::write(&path, &old).unwrap();
    let mut reopened = Ledger::open(&path, &identity, &policy, 0).unwrap();
    assert_eq!(reopened.snapshot().publication_journal_version, None);
    assert!(reopened.begin_attempt().is_err());
    assert!(
        reopened
            .begin_publication(&attempt, &serde_json::json!({}), &"ab".repeat(32))
            .is_err()
    );
    let after: serde_json::Value = local_io::read(&path).unwrap();
    assert_eq!(after, old);
}
