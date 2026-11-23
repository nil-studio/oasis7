//! Software-only durable operation accounting consumed by the real Net codec.
//! This does not supply a wire-3 FullReadOnly grant, protected 4 GiB backend,
//! signer lease, native owner takeover or a successor installer.
#![cfg(all(feature = "node-libp2p", not(target_arch = "wasm32")))]

use super::super::operation_failure::LocalBlocker;
use super::super::operation_ledger::{
    AttemptPermit, Direction, ExchangePermit, Identity, Ledger, Policy, Reservation,
};
use super::{TestDir, identity, policy};
use oasis7_net::{
    EncodedExchangeBudget, EncodedIoDirection, EncodedIoReservation, Libp2pNetwork,
    Libp2pNetworkConfig,
};
use oasis7_proto::distributed_net::DistributedNetwork as _;
use oasis7_proto::network_failure::{FailureCause, FailureOrigin, NetworkFailure};
use oasis7_proto::world_error::WorldError;
use sha2::Digest;
use std::{
    io,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicUsize, Ordering},
        mpsc,
    },
    time::{Duration, Instant},
};

struct Original {
    ledger: Ledger,
    attempt: AttemptPermit,
    exchange: Option<ExchangePermit>,
    stopped_exchanges: usize,
}

struct SoftwareExchange {
    original: Arc<Mutex<Original>>,
    deadline: Instant,
    admitted: AtomicBool,
}

impl SoftwareExchange {
    fn new(original: Arc<Mutex<Original>>) -> Self {
        let deadline = original.lock().unwrap().attempt.deadline();
        Self {
            original,
            deadline,
            admitted: AtomicBool::new(false),
        }
    }
}

struct SoftwareReservation {
    original: Arc<Mutex<Original>>,
    token: Reservation,
    capacity: usize,
}

impl EncodedIoReservation for SoftwareReservation {
    fn capacity(&self) -> usize {
        self.capacity
    }
    fn settle(self: Box<Self>, actual_bytes: usize) -> io::Result<()> {
        self.original
            .lock()
            .unwrap()
            .ledger
            .settle(&self.token, actual_bytes as u64)
            .map_err(io::Error::other)
    }
    // Dropping the owned token deliberately does not edit the durable ledger.
    // The exact original reservation remains there with unknown progress.
}

impl EncodedExchangeBudget for SoftwareExchange {
    fn deadline(&self) -> Instant {
        self.deadline
    }
    fn check_active(&self) -> io::Result<()> {
        let original = self.original.lock().unwrap();
        original
            .ledger
            .check_attempt(&original.attempt)
            .map_err(io::Error::other)
    }
    fn start(&self, encoded_request_bytes: usize) -> io::Result<()> {
        let mut state = self.original.lock().unwrap();
        let Original {
            ledger,
            attempt,
            exchange,
            ..
        } = &mut *state;
        if exchange.is_some() || self.admitted.load(Ordering::Acquire) {
            return Err(io::Error::other("original exchange remains active"));
        }
        let permit = ledger
            .begin_exchange(attempt, encoded_request_bytes as u64, 64 * 1024)
            .map_err(io::Error::other)?;
        *exchange = Some(permit);
        self.admitted.store(true, Ordering::Release);
        Ok(())
    }
    fn reserve(
        &self,
        direction: EncodedIoDirection,
        max_bytes: usize,
    ) -> io::Result<Box<dyn EncodedIoReservation>> {
        let direction = match direction {
            EncodedIoDirection::Tx => Direction::Tx,
            EncodedIoDirection::Rx => Direction::Rx,
        };
        let mut state = self.original.lock().unwrap();
        let Original {
            ledger,
            attempt,
            exchange,
            ..
        } = &mut *state;
        let permit = exchange
            .as_ref()
            .ok_or_else(|| io::Error::other("original exchange absent"))?;
        let capacity = ledger
            .body_window_limit(direction, max_bytes)
            .map_err(io::Error::other)?;
        let token = ledger
            .reserve(attempt, permit, direction, capacity as u64)
            .map_err(io::Error::other)?;
        Ok(Box::new(SoftwareReservation {
            original: Arc::clone(&self.original),
            token,
            capacity,
        }))
    }
    fn finish(&self) -> io::Result<()> {
        if !self.admitted.swap(false, Ordering::AcqRel) {
            return Ok(());
        }
        let mut state = self.original.lock().unwrap();
        state.stopped_exchanges += 1;
        let Original {
            ledger,
            attempt,
            exchange,
            ..
        } = &mut *state;
        let permit = exchange
            .as_ref()
            .ok_or_else(|| io::Error::other("original exchange absent"))?;
        ledger
            .finish_exchange(attempt, permit)
            .map_err(io::Error::other)?;
        *exchange = None;
        Ok(())
    }
}

fn wait_until(mut condition: impl FnMut() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(5);
    while Instant::now() < deadline {
        if condition() {
            return;
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    assert!(condition(), "real wire operation did not stop");
}

fn pair(quic: bool) -> (Libp2pNetwork, Libp2pNetwork) {
    let address = if quic {
        "/ip4/127.0.0.1/udp/0/quic-v1"
    } else {
        "/ip4/127.0.0.1/tcp/0"
    };
    let server = Libp2pNetwork::new(Libp2pNetworkConfig {
        listen_addrs: vec![address.parse().unwrap()],
        ..Libp2pNetworkConfig::default()
    });
    wait_until(|| !server.listening_addrs().is_empty());
    let remote = format!("{}/p2p/{}", server.listening_addrs()[0], server.peer_id())
        .parse()
        .unwrap();
    let client = Libp2pNetwork::new(Libp2pNetworkConfig {
        listen_addrs: vec![address.parse().unwrap()],
        bootstrap_peers: vec![remote],
        ..Libp2pNetworkConfig::default()
    });
    wait_until(|| client.connected_peers().contains(&server.peer_id()));
    (server, client)
}

// These independent size calculations follow the fixed outer CBOR DTO fields,
// including integer-array expansion, protocol text and this payload's prefix.
fn cbor_argument(value: usize) -> usize {
    if value < 24 {
        1
    } else if value <= 255 {
        2
    } else {
        3
    }
}
fn encoded_payload(bytes: &[u8]) -> usize {
    1 + "payload".len()
        + cbor_argument(bytes.len())
        + bytes
            .iter()
            .map(|b| cbor_argument(usize::from(*b)))
            .sum::<usize>()
}
fn encoded_request(protocol: &str, bytes: &[u8]) -> usize {
    1 + 1
        + "protocol".len()
        + cbor_argument(protocol.len())
        + protocol.len()
        + encoded_payload(bytes)
}
fn encoded_response(bytes: &[u8]) -> usize {
    1 + encoded_payload(bytes)
}

fn original(
    path: &std::path::Path,
    policy: &Policy,
    protocol: &str,
) -> (Arc<Mutex<Original>>, Identity) {
    let request =
        serde_json::json!({"scope":"software-codec-accounting-test", "protocol":protocol});
    let mut identity = identity(policy);
    identity.request_digest =
        hex::encode(sha2::Sha256::digest(serde_json::to_vec(&request).unwrap()));
    let mut ledger = Ledger::create(path, identity.clone(), policy.clone()).unwrap();
    ledger.bind_original_request(&request).unwrap();
    let attempt = ledger.begin_attempt().unwrap();
    (
        Arc::new(Mutex::new(Original {
            ledger,
            attempt,
            exchange: None,
            stopped_exchanges: 0,
        })),
        identity,
    )
}

#[test]
fn real_wire_two_exchanges_use_one_durable_original_budget_and_cold_totals() {
    for quic in [false, true] {
        let (server, client) = pair(quic);
        let temp = TestDir::new();
        let path = temp.path().join("operation.json");
        let protocol = "/aw/rr/1.0.0/durable-full-sync-account";
        let body = [0, 0, 0, 3, 24, 255, 0];
        let tx = encoded_request(protocol, &body) as u64;
        let rx = encoded_response(&body) as u64;
        let mut policy = policy();
        policy.max_encoded_io_bytes = 2 * (tx + rx) + tx - 1;
        let (state, identity) = original(&path, &policy, protocol);
        let calls = Arc::new(AtomicUsize::new(0));
        let observed = Arc::clone(&calls);
        server
            .register_handler(
                protocol,
                Box::new(move |bytes| {
                    observed.fetch_add(1, Ordering::AcqRel);
                    Ok(bytes.to_vec())
                }),
            )
            .unwrap();
        let deadline = state.lock().unwrap().attempt.deadline();
        for _ in 0..2 {
            assert_eq!(
                client
                    .request_to_peer_with_encoded_budget(
                        protocol,
                        &body,
                        server.peer_id(),
                        Box::new(SoftwareExchange::new(Arc::clone(&state)))
                    )
                    .unwrap(),
                body
            );
            assert_eq!(state.lock().unwrap().attempt.deadline(), deadline);
        }
        assert!(
            client
                .request_to_peer_with_encoded_budget(
                    protocol,
                    &body,
                    server.peer_id(),
                    Box::new(SoftwareExchange::new(Arc::clone(&state)))
                )
                .is_err()
        );
        wait_until(|| Arc::strong_count(&state) == 1);
        let before = state.lock().unwrap().ledger.snapshot().clone();
        assert_eq!(before.identity, identity);
        assert_eq!(
            (before.encoded_tx_actual, before.encoded_rx_actual),
            (2 * tx, 2 * rx)
        );
        assert_eq!((before.exchanges_started, before.attempts_used), (2, 1));
        assert_eq!(before.charged().unwrap(), 2 * (tx + rx));
        assert!(before.reservation.is_none() && before.active_request.is_none());
        assert_eq!(calls.load(Ordering::Acquire), 2);
        drop(state);
        let reopened = Ledger::open(&path, &identity, &policy, 0).unwrap();
        assert_eq!(reopened.snapshot(), &before);
    }
}

#[test]
fn real_wire_cancel_keeps_unknown_original_reservation_after_cold_open() {
    for independent_cancel in [false, true] {
        wire_cancel_case(independent_cancel);
    }
}

struct CancelAfterAdmission {
    inner: SoftwareExchange,
    cancelled: Arc<AtomicBool>,
}

impl EncodedExchangeBudget for CancelAfterAdmission {
    fn deadline(&self) -> Instant {
        self.inner.deadline()
    }
    fn check_active(&self) -> io::Result<()> {
        self.inner.check_active()?;
        if self.cancelled.load(Ordering::Acquire) {
            return Err(io::Error::other(
                "this exchange was cancelled after durable admission",
            ));
        }
        Ok(())
    }
    fn start(&self, bytes: usize) -> io::Result<()> {
        self.inner.start(bytes)?;
        self.cancelled.store(true, Ordering::Release);
        Ok(())
    }
    fn reserve(
        &self,
        direction: EncodedIoDirection,
        bytes: usize,
    ) -> io::Result<Box<dyn EncodedIoReservation>> {
        self.inner.reserve(direction, bytes)
    }
    fn finish(&self) -> io::Result<()> {
        self.inner.finish()
    }
}

fn assert_local_admission_failure(error: &WorldError, protocol: &str) {
    assert_eq!(
        error.network_failure(Some(protocol)),
        NetworkFailure::observed(
            FailureOrigin::LocalTransport,
            FailureCause::Unknown,
            Some(protocol),
        )
    );
    assert!(matches!(
        error.diagnostic_source(),
        WorldError::NetworkProtocolUnavailable { protocol: phase }
            if phase == "encoded exchange admission failed"
    ));
}

#[test]
fn real_predispatch_cancel_spends_exchange_limit_and_cold_ledger_preserves_known_zero() {
    let (server, client) = pair(false);
    let protocol = "/aw/rr/1.0.0/durable-admitted-cancel";
    let calls = Arc::new(AtomicUsize::new(0));
    let observed = Arc::clone(&calls);
    server
        .register_handler(
            protocol,
            Box::new(move |body| {
                observed.fetch_add(1, Ordering::AcqRel);
                Ok(body.to_vec())
            }),
        )
        .unwrap();
    let temp = TestDir::new();
    let path = temp.path().join("operation.json");
    let mut policy = policy();
    policy.max_exchanges = 1;
    let (state, identity) = original(&path, &policy, protocol);
    let deadline = state.lock().unwrap().attempt.deadline();
    let cancelled = Arc::new(AtomicBool::new(false));
    let error = client
        .request_to_peer_with_encoded_budget(
            protocol,
            &[24, 255],
            server.peer_id(),
            Box::new(CancelAfterAdmission {
                inner: SoftwareExchange::new(Arc::clone(&state)),
                cancelled: Arc::clone(&cancelled),
            }),
        )
        .unwrap_err();
    assert_local_admission_failure(&error, protocol);
    assert!(cancelled.load(Ordering::Acquire));
    wait_until(|| Arc::strong_count(&state) == 1);
    let before = state.lock().unwrap().ledger.snapshot().clone();
    assert_eq!((before.exchanges_started, before.attempts_used), (1, 1));
    assert_eq!(
        (
            before.encoded_tx_actual,
            before.encoded_rx_actual,
            before.unknown_final_charged
        ),
        (0, 0, 0)
    );
    assert_eq!(before.charged().unwrap(), 0);
    assert!(before.reservation.is_none() && before.active_request.is_none());
    assert_eq!(state.lock().unwrap().stopped_exchanges, 1);
    // Only that exchange was cancelled. The original operation and attempt are
    // still active, so this independent refusal must be the spent count cap.
    {
        let original = state.lock().unwrap();
        original.ledger.check_attempt(&original.attempt).unwrap();
        assert!(original.exchange.is_none());
        assert_eq!(
            original.ledger.failure_reason(&original.attempt),
            LocalBlocker::RecoveryRequired
        );
    }
    let error = client
        .request_to_peer_with_encoded_budget(
            protocol,
            &[24, 255],
            server.peer_id(),
            Box::new(SoftwareExchange::new(Arc::clone(&state))),
        )
        .unwrap_err();
    assert_local_admission_failure(&error, protocol);
    wait_until(|| Arc::strong_count(&state) == 1);
    {
        let original = state.lock().unwrap();
        assert_eq!(original.ledger.snapshot(), &before);
        assert_eq!(original.attempt.deadline(), deadline);
        assert_eq!(
            original.ledger.failure_reason(&original.attempt),
            LocalBlocker::BudgetExhausted
        );
        assert_eq!(original.stopped_exchanges, 1);
        assert!(original.exchange.is_none());
    }
    assert_eq!(
        client
            .traffic_metrics_snapshot()
            .by_protocol
            .get(protocol)
            .map_or(0, |lane| lane.outbound.messages),
        0
    );
    assert_eq!(calls.load(Ordering::Acquire), 0);
    assert!(Instant::now() < deadline);
    drop(state);
    let mut reopened = Ledger::open(&path, &identity, &policy, 0).unwrap();
    assert_eq!(reopened.snapshot(), &before);
    assert!(reopened.begin_attempt().is_err());
    assert_eq!(reopened.snapshot(), &before);
}

fn wire_cancel_case(independent_cancel: bool) {
    let (server, client) = pair(false);
    let temp = TestDir::new();
    let path = temp.path().join("operation.json");
    let protocol = "/aw/rr/1.0.0/durable-full-sync-cancel";
    let (entered, seen) = mpsc::channel();
    let (release, permits) = mpsc::channel();
    let permits = Mutex::new(permits);
    server
        .register_handler(
            protocol,
            Box::new(move |bytes| {
                entered.send(()).unwrap();
                permits
                    .lock()
                    .unwrap()
                    .recv_timeout(Duration::from_secs(3))
                    .unwrap();
                Ok(bytes.to_vec())
            }),
        )
        .unwrap();
    let mut policy = policy();
    if !independent_cancel {
        policy.attempt_timeout_ms = 1_000;
    }
    let (state, identity) = original(&path, &policy, protocol);
    let cancelled = Arc::new(AtomicBool::new(false));
    state
        .lock()
        .unwrap()
        .ledger
        .bind_cancellation(Arc::clone(&cancelled))
        .unwrap();
    let deadline = state.lock().unwrap().attempt.deadline();
    std::thread::scope(|threads| {
        let request = threads.spawn(|| {
            client.request_to_peer_with_encoded_budget(
                protocol,
                &[255, 24],
                server.peer_id(),
                Box::new(SoftwareExchange::new(Arc::clone(&state))),
            )
        });
        seen.recv_timeout(Duration::from_secs(2)).unwrap();
        wait_until(|| {
            state
                .lock()
                .unwrap()
                .ledger
                .snapshot()
                .reservation
                .is_some()
        });
        let active = state.lock().unwrap().ledger.snapshot().clone();
        assert!(
            client
                .request_to_peer_with_encoded_budget(
                    protocol,
                    &[255, 24],
                    server.peer_id(),
                    Box::new(SoftwareExchange::new(Arc::clone(&state))),
                )
                .is_err()
        );
        {
            let original = state.lock().unwrap();
            assert_eq!(original.ledger.snapshot(), &active);
            assert_eq!(original.stopped_exchanges, 0);
            assert_eq!(active.exchanges_started, 1);
            assert_eq!(
                active.reservation.as_ref().unwrap().direction,
                Direction::Rx
            );
        }
        assert!(matches!(seen.try_recv(), Err(mpsc::TryRecvError::Empty)));
        if independent_cancel {
            cancelled.store(true, Ordering::SeqCst);
            // A real socket wake must recheck the operation even though it
            // already has durable response credit and time remains.
            release.send(()).unwrap();
        }
        assert!(request.join().unwrap().is_err());
    });
    wait_until(|| Arc::strong_count(&state) == 1);
    let before = state.lock().unwrap().ledger.snapshot().clone();
    assert_eq!(
        before.encoded_tx_actual,
        encoded_request(protocol, &[255, 24]) as u64
    );
    assert_eq!(before.encoded_rx_actual, 0);
    assert!(before.reservation.is_some() && before.active_request.is_some());
    assert!(before.charged().unwrap() > before.encoded_tx_actual);
    assert_eq!(state.lock().unwrap().stopped_exchanges, 1);
    if independent_cancel {
        assert!(Instant::now() < deadline);
    } else {
        release.send(()).unwrap();
    }
    drop(state);
    let mut reopened = Ledger::open(&path, &identity, &policy, 0).unwrap();
    assert_eq!(reopened.snapshot(), &before);
    assert!(reopened.begin_attempt().is_err());
    assert_eq!(reopened.snapshot(), &before);
}
