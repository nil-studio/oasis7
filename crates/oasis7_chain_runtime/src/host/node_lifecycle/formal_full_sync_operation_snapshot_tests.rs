//! Cold-open invariants against the actual durable operation ledger.
use super::super::operation_ledger::{Ledger, Policy};
use super::*;
use std::path::{Path, PathBuf};

struct TestDir(PathBuf);
impl TestDir {
    fn new() -> Self {
        let path = std::env::temp_dir().canonicalize().unwrap().join(format!(
            "oasis7-sync-snapshot-{}",
            local_io::random_id().unwrap()
        ));
        local_io::private_dir(&path).unwrap();
        Self(path)
    }

    fn ledger_path(&self) -> PathBuf {
        self.0.join("operation.json")
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
        max_proof_pages: 3,
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

fn reject_then_restore(path: &Path, original: &Snapshot, bad: &Snapshot) {
    assert!(original.validate().is_ok());
    assert!(bad.validate().is_err(), "fabricated ledger state accepted");
    local_io::write(path, bad).unwrap();
    assert!(read_snapshot(path).is_err(), "fabricated disk ledger accepted");
    local_io::write(path, original).unwrap();
    assert_eq!(read_snapshot(path).unwrap(), *original);
}

#[test]
fn cold_open_rejects_unmatched_proof_pages_and_bytes_after_admission() {
    let temp = TestDir::new();
    let path = temp.ledger_path();
    let policy = policy();
    let identity = identity(&policy);
    let mut ledger = Ledger::create(&path, identity.clone(), policy.clone()).unwrap();
    let attempt = ledger.begin_attempt().unwrap();
    ledger.charge_proof(&attempt, 3).unwrap();
    let original = ledger.snapshot().clone();
    assert_eq!((original.proof_pages, original.proof_bytes), (1, 3));
    drop(ledger);

    for (pages, bytes) in [(0, 3), (1, 0), (2, 1)] {
        let mut bad = original.clone();
        bad.proof_pages = pages;
        bad.proof_bytes = bytes;
        reject_then_restore(&path, &original, &bad);
        local_io::write(&path, &bad).unwrap();
        assert!(
            Ledger::open(&path, &identity, &policy, 0).is_err(),
            "cold open accepted pages={pages}, bytes={bytes}"
        );
        local_io::write(&path, &original).unwrap();
    }
    let reopened = Ledger::open(&path, &identity, &policy, 0).unwrap();
    assert_eq!(reopened.snapshot(), &original);
}

#[test]
fn cold_open_rejects_orphan_windows_and_settled_bytes_without_exchange() {
    let temp = TestDir::new();
    let path = temp.ledger_path();
    let policy = policy();
    let identity = identity(&policy);
    let mut ledger = Ledger::create(&path, identity.clone(), policy.clone()).unwrap();
    let attempt = ledger.begin_attempt().unwrap();
    let request = ledger.begin_exchange(&attempt, 1, 1).unwrap();
    let reservation = ledger
        .reserve(&attempt, &request, Direction::Rx, 1)
        .unwrap();
    ledger.settle(&reservation, 1).unwrap();
    ledger.finish_exchange(&attempt, &request).unwrap();
    let original = ledger.snapshot().clone();
    assert_eq!(original.exchanges_started, 1);
    assert_eq!(original.window_seq, 1);
    assert_eq!(original.encoded_rx_actual, 1);
    drop(ledger);

    for orphan in ["window", "settled_body"] {
        let mut bad = original.clone();
        bad.exchanges_started = 0;
        match orphan {
            "window" => bad.encoded_rx_actual = 0,
            "settled_body" => bad.window_seq = 0,
            _ => unreachable!(),
        }
        reject_then_restore(&path, &original, &bad);
        local_io::write(&path, &bad).unwrap();
        assert!(
            Ledger::open(&path, &identity, &policy, 0).is_err(),
            "cold open accepted orphan {orphan}"
        );
        local_io::write(&path, &original).unwrap();
    }
    let reopened = Ledger::open(&path, &identity, &policy, 0).unwrap();
    assert_eq!(reopened.snapshot(), &original);
}

#[test]
fn cold_open_rejects_zero_sequence_reservations_in_active_and_interrupted_attempts() {
    let temp = TestDir::new();
    let path = temp.ledger_path();
    let policy = policy();
    let identity = identity(&policy);
    let mut ledger = Ledger::create(&path, identity.clone(), policy.clone()).unwrap();
    let attempt = ledger.begin_attempt().unwrap();
    let exchange = ledger.begin_exchange(&attempt, 2, 2).unwrap();
    let token = ledger
        .reserve(&attempt, &exchange, Direction::Rx, 1)
        .unwrap();
    let live = ledger.snapshot().clone();
    assert_eq!(token.window_seq, 1);
    assert_eq!(live.window_seq, 1);
    drop(ledger);

    // A real reserve always increments its window sequence before persisting
    // the token. Forging both the token and the global counter to zero used to
    // pass structural validation; neither can be minted by the producer.
    let mut bad_live = live.clone();
    bad_live.window_seq = 0;
    bad_live.reservation.as_mut().unwrap().window_seq = 0;
    reject_then_restore(&path, &live, &bad_live);
    local_io::write(&path, &bad_live).unwrap();
    assert!(Ledger::open(&path, &identity, &policy, 0).is_err());

    // Represent the exact old attempt fields carried into a successor attempt
    // after an interrupted owner. The reservation comes from the real writer,
    // rather than a fabricated token; no protected takeover is claimed here.
    let mut interrupted = live.clone();
    let old_nonce = interrupted.active_attempt_nonce.take().unwrap();
    let old_request = interrupted.active_request.take();
    let old_request_nonce = interrupted.active_request_nonce.take();
    let old_reservation = interrupted.reservation.take();
    let tx_credit = std::mem::take(&mut interrupted.tx_credit);
    let rx_credit = std::mem::take(&mut interrupted.rx_credit);
    interrupted.interrupted_attempts.push(InterruptedAttempt {
        attempt_no: interrupted.attempts_used,
        attempt_nonce: old_nonce,
        request: old_request,
        request_nonce: old_request_nonce,
        tx_credit,
        rx_credit,
        reservation: old_reservation,
    });
    interrupted.attempts_used += 1;
    interrupted.active_attempt_nonce = Some("ab".repeat(16));
    assert!(interrupted.validate().is_ok());

    let mut bad_interrupted = interrupted.clone();
    bad_interrupted.window_seq = 0;
    bad_interrupted.interrupted_attempts[0]
        .reservation
        .as_mut()
        .unwrap()
        .window_seq = 0;
    reject_then_restore(&path, &interrupted, &bad_interrupted);
    local_io::write(&path, &bad_interrupted).unwrap();
    assert!(Ledger::open(&path, &identity, &policy, 0).is_err());
    local_io::write(&path, &interrupted).unwrap();
    let reopened = Ledger::open(&path, &identity, &policy, 0).unwrap();
    assert_eq!(reopened.snapshot(), &interrupted);
}
