use super::*;
use std::{collections::BTreeMap, sync::Mutex, time::Duration};

use futures::{Future, io::Cursor, task::noop_waker};
use libp2p::{StreamProtocol, request_response::Codec};
use oasis7_proto::distributed_net::NetworkRequest;

use super::super::request_deadline::{LocalRequest, RequestCodec};

#[derive(Default, Debug)]
pub(super) struct Ledger {
    pub(super) limit: usize,
    pub(super) tx: usize,
    pub(super) rx: usize,
    pub(super) unknown: usize,
    pub(super) active: bool,
    pub(super) starts: Vec<usize>,
    pub(super) finishes: usize,
    pub(super) zeros: usize,
    next: u64,
    pending: BTreeMap<u64, usize>,
}

impl Ledger {
    fn spent(&self) -> usize {
        self.tx + self.rx + self.unknown + self.pending.values().sum::<usize>()
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Delay {
    None,
    Active,
    Start,
    Reserve,
    Settle,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum CancelAt {
    None,
    Start,
    Reserve,
    Settle,
}

pub(super) struct Budget {
    pub(super) ledger: Arc<Mutex<Ledger>>,
    deadline: Instant,
    admitted: AtomicBool,
    window: usize,
    delay: Delay,
    cancelled: Arc<AtomicBool>,
    cancel_at: CancelAt,
}

impl Budget {
    pub(super) fn new(ledger: Arc<Mutex<Ledger>>, deadline: Instant) -> Self {
        Self {
            ledger,
            deadline,
            admitted: AtomicBool::new(false),
            window: 7,
            delay: Delay::None,
            cancelled: Arc::new(AtomicBool::new(false)),
            cancel_at: CancelAt::None,
        }
    }
}

struct Reservation {
    ledger: Arc<Mutex<Ledger>>,
    id: u64,
    capacity: usize,
    direction: EncodedIoDirection,
    settled: bool,
    delay: bool,
    cancelled: Arc<AtomicBool>,
    cancel_on_settle: bool,
}

impl EncodedIoReservation for Reservation {
    fn capacity(&self) -> usize {
        self.capacity
    }
    fn settle(mut self: Box<Self>, actual: usize) -> io::Result<()> {
        assert!(actual <= self.capacity);
        {
            let mut ledger = self.ledger.lock().unwrap();
            assert_eq!(ledger.pending.remove(&self.id), Some(self.capacity));
            if actual == 0 {
                ledger.zeros += 1;
            }
            match self.direction {
                EncodedIoDirection::Tx => ledger.tx += actual,
                EncodedIoDirection::Rx => ledger.rx += actual,
            }
        }
        self.settled = true;
        if self.cancel_on_settle {
            self.cancelled.store(true, Ordering::Release);
        }
        if self.delay {
            std::thread::sleep(Duration::from_millis(300));
        }
        Ok(())
    }
}

impl Drop for Reservation {
    fn drop(&mut self) {
        if !self.settled {
            let mut ledger = self.ledger.lock().unwrap();
            assert_eq!(ledger.pending.remove(&self.id), Some(self.capacity));
            ledger.unknown += self.capacity;
        }
    }
}

impl EncodedExchangeBudget for Budget {
    fn deadline(&self) -> Instant {
        self.deadline
    }
    fn check_active(&self) -> io::Result<()> {
        if self.delay == Delay::Active {
            std::thread::sleep(Duration::from_millis(300));
        }
        if self.cancelled.load(Ordering::Acquire) {
            return Err(io::Error::other("original operation cancelled"));
        }
        Ok(())
    }
    fn start(&self, bytes: usize) -> io::Result<()> {
        {
            let mut ledger = self.ledger.lock().unwrap();
            if ledger.active
                || ledger.unknown != 0
                || bytes > ledger.limit.saturating_sub(ledger.spent())
            {
                return Err(io::Error::other(
                    "original operation unavailable or exhausted",
                ));
            }
            if self.admitted.swap(true, Ordering::AcqRel) {
                return Err(io::Error::other("exchange already admitted"));
            }
            ledger.active = true;
            ledger.starts.push(bytes);
        }
        if self.cancel_at == CancelAt::Start {
            self.cancelled.store(true, Ordering::Release);
        }
        if self.delay == Delay::Start {
            std::thread::sleep(Duration::from_millis(300));
        }
        Ok(())
    }
    fn reserve(
        &self,
        direction: EncodedIoDirection,
        max_bytes: usize,
    ) -> io::Result<Box<dyn EncodedIoReservation>> {
        let mut ledger = self.ledger.lock().unwrap();
        assert!(ledger.active && self.admitted.load(Ordering::Acquire));
        let capacity = max_bytes
            .min(self.window)
            .min(ledger.limit.saturating_sub(ledger.spent()));
        if capacity == 0 {
            return Err(io::Error::other("original operation exhausted"));
        }
        ledger.next += 1;
        let id = ledger.next;
        ledger.pending.insert(id, capacity);
        drop(ledger);
        if self.cancel_at == CancelAt::Reserve {
            self.cancelled.store(true, Ordering::Release);
        }
        if self.delay == Delay::Reserve {
            std::thread::sleep(Duration::from_millis(300));
        }
        Ok(Box::new(Reservation {
            ledger: Arc::clone(&self.ledger),
            id,
            capacity,
            direction,
            settled: false,
            delay: self.delay == Delay::Settle,
            cancelled: Arc::clone(&self.cancelled),
            cancel_on_settle: self.cancel_at == CancelAt::Settle,
        }))
    }
    fn finish(&self) -> io::Result<()> {
        if self.admitted.swap(false, Ordering::AcqRel) {
            let mut ledger = self.ledger.lock().unwrap();
            ledger.active = false;
            ledger.finishes += 1;
            if ledger.unknown != 0 || !ledger.pending.is_empty() {
                return Err(io::Error::other("original progress remains unknown"));
            }
        }
        Ok(())
    }
}

pub(super) fn ledger(limit: usize) -> Arc<Mutex<Ledger>> {
    Arc::new(Mutex::new(Ledger {
        limit,
        ..Ledger::default()
    }))
}

// Independent CBOR oracle for these concrete NetworkRequest/NetworkResponse
// fields. In particular Vec<u8> is an array of integer values, not raw bytes.
fn argument(out: &mut Vec<u8>, major: u8, n: usize) {
    if n < 24 {
        out.push(major | n as u8);
    } else if n <= 255 {
        out.extend([major | 24, n as u8]);
    } else {
        out.push(major | 25);
        out.extend((n as u16).to_be_bytes());
    }
}
fn text(out: &mut Vec<u8>, value: &str) {
    argument(out, 0x60, value.len());
    out.extend(value.as_bytes());
}
fn payload(out: &mut Vec<u8>, value: &[u8]) {
    text(out, "payload");
    argument(out, 0x80, value.len());
    for byte in value {
        argument(out, 0, usize::from(*byte));
    }
}
pub(super) fn request_bytes(protocol: &str, value: &[u8]) -> Vec<u8> {
    let mut bytes = vec![0xa2];
    text(&mut bytes, "protocol");
    text(&mut bytes, protocol);
    payload(&mut bytes, value);
    bytes
}
pub(super) fn response_bytes(value: &[u8]) -> Vec<u8> {
    let mut bytes = vec![0xa1];
    payload(&mut bytes, value);
    bytes
}

#[derive(Clone, Copy)]
enum Stop {
    None,
    Pending,
    Error,
    Zero,
}
struct PartialIo {
    ledger: Arc<Mutex<Ledger>>,
    written: Vec<u8>,
    input: Cursor<Vec<u8>>,
    chunk: usize,
    stop_after: usize,
    stop: Stop,
    read_stop: bool,
    calls: usize,
}
impl PartialIo {
    fn new(ledger: Arc<Mutex<Ledger>>, input: Vec<u8>) -> Self {
        Self {
            ledger,
            input: Cursor::new(input),
            written: Vec::new(),
            chunk: 2,
            stop_after: usize::MAX,
            stop: Stop::None,
            read_stop: false,
            calls: 0,
        }
    }
    fn entered(&mut self) {
        // The durable admission lock is not held while actual I/O is polled.
        let ledger = self.ledger.try_lock().expect("budget lock held across I/O");
        assert!(ledger.active && !ledger.pending.is_empty());
        self.calls += 1;
    }
    fn stopped(&self) -> Option<Poll<io::Result<usize>>> {
        match self.stop {
            Stop::None => None,
            Stop::Pending => Some(Poll::Pending),
            Stop::Error => Some(Poll::Ready(Err(io::Error::other(
                "uncertain transport progress",
            )))),
            Stop::Zero => Some(Poll::Ready(Ok(0))),
        }
    }
}
impl AsyncWrite for PartialIo {
    fn poll_write(
        mut self: Pin<&mut Self>,
        _: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<io::Result<usize>> {
        self.entered();
        if !self.read_stop
            && self.written.len() >= self.stop_after
            && let Some(result) = self.stopped()
        {
            return result;
        }
        let n = self.chunk.min(buf.len());
        self.written.extend_from_slice(&buf[..n]);
        Poll::Ready(Ok(n))
    }
    fn poll_flush(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<io::Result<()>> {
        Poll::Ready(Ok(()))
    }
    fn poll_close(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<io::Result<()>> {
        Poll::Ready(Ok(()))
    }
}
impl AsyncRead for PartialIo {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut [u8],
    ) -> Poll<io::Result<usize>> {
        self.entered();
        if self.read_stop
            && self.input.position() as usize >= self.stop_after
            && let Some(result) = self.stopped()
        {
            return result;
        }
        let n = self.chunk.min(buf.len());
        Pin::new(&mut self.input).poll_read(cx, &mut buf[..n])
    }
}

fn runtime() -> tokio::runtime::Runtime {
    tokio::runtime::Builder::new_current_thread()
        .enable_time()
        .build()
        .unwrap()
}
fn local(budget: Budget) -> LocalRequest {
    LocalRequest {
        message: NetworkRequest {
            protocol: "/meter".into(),
            payload: vec![0, 24, 255],
        },
        deadline: Some(tokio::time::Instant::from_std(budget.deadline)),
        exchange: Some(Exchange::new(Box::new(budget))),
    }
}

#[test]
fn actual_codec_counts_complete_cbor_and_short_io_with_known_eof() {
    runtime().block_on(async {
        let state = ledger(1024);
        let expected_request = request_bytes("/meter", &[0, 24, 255]);
        let expected_response = response_bytes(&[255, 0, 24]);
        let mut io = PartialIo::new(Arc::clone(&state), expected_response.clone());
        let mut codec = RequestCodec::default();
        let protocol = StreamProtocol::new("/test/actual-codec");
        codec
            .write_request(
                &protocol,
                &mut io,
                local(Budget::new(
                    Arc::clone(&state),
                    Instant::now() + Duration::from_secs(2),
                )),
            )
            .await
            .unwrap();
        let result = codec.read_response(&protocol, &mut io).await.unwrap();
        assert_eq!(result.payload, [255, 0, 24]);
        assert_eq!(io.written, expected_request);
        let actual = state.lock().unwrap();
        assert_eq!(actual.starts, [expected_request.len()]);
        assert_eq!(
            (actual.tx, actual.rx),
            (expected_request.len(), expected_response.len())
        );
        assert_eq!((actual.finishes, actual.unknown), (1, 0));
        assert!(actual.zeros >= 1 && actual.pending.is_empty());
        assert!(actual.tx > 3 && actual.rx > 3);
    });
}

#[test]
fn codec_partial_error_and_timeout_keep_only_unconfirmed_progress_unknown() {
    for stop in [Stop::Error, Stop::Pending] {
        runtime().block_on(async {
            let state = ledger(1024);
            let mut io = PartialIo::new(Arc::clone(&state), vec![]);
            io.stop_after = 2;
            io.stop = stop;
            let mut codec = RequestCodec::default();
            let result = codec
                .write_request(
                    &StreamProtocol::new("/test/partial"),
                    &mut io,
                    local(Budget::new(
                        Arc::clone(&state),
                        Instant::now() + Duration::from_millis(300),
                    )),
                )
                .await;
            assert!(result.is_err());
            drop(codec);
            let actual = state.lock().unwrap();
            assert_eq!(actual.tx, 2);
            assert_eq!(actual.unknown, 7);
            assert_eq!(actual.finishes, 1);
            assert!(actual.pending.is_empty());
        });
    }
}

#[test]
fn dropping_pending_codec_future_does_not_refund_or_finish_its_live_owner() {
    runtime().block_on(async {
        let state = ledger(1024);
        let mut io = PartialIo::new(Arc::clone(&state), vec![]);
        io.stop_after = 2;
        io.stop = Stop::Pending;
        let mut codec = RequestCodec::default();
        let protocol = StreamProtocol::new("/test/cancel");
        let mut future = Box::pin(codec.write_request(
            &protocol,
            &mut io,
            local(Budget::new(
                Arc::clone(&state),
                Instant::now() + Duration::from_secs(2),
            )),
        ));
        let waker = noop_waker();
        assert!(
            future
                .as_mut()
                .poll(&mut Context::from_waker(&waker))
                .is_pending()
        );
        drop(future);
        assert_eq!(state.lock().unwrap().finishes, 0);
        assert_eq!(state.lock().unwrap().unknown, 7);
        drop(codec);
        assert_eq!(state.lock().unwrap().finishes, 1);
    });
}

#[test]
fn pending_write_rechecks_original_cancellation_before_reusing_its_reservation() {
    runtime().block_on(async {
        let state = ledger(1024);
        let mut io = PartialIo::new(Arc::clone(&state), vec![]);
        io.stop_after = 2;
        io.stop = Stop::Pending;
        let deadline = Instant::now() + Duration::from_secs(2);
        let budget = Budget::new(Arc::clone(&state), deadline);
        let cancelled = Arc::clone(&budget.cancelled);
        let mut codec = RequestCodec::default();
        let protocol = StreamProtocol::new("/test/pending-write-cancel");
        let mut future = Box::pin(codec.write_request(&protocol, &mut io, local(budget)));
        let waker = noop_waker();
        let mut context = Context::from_waker(&waker);
        assert!(future.as_mut().poll(&mut context).is_pending());
        {
            let actual = state.lock().unwrap();
            assert_eq!(actual.tx, 2);
            assert_eq!(actual.pending.values().sum::<usize>(), 7);
            assert_eq!(actual.unknown, 0);
        }
        cancelled.store(true, Ordering::Release);
        assert!(matches!(
            future.as_mut().poll(&mut context),
            Poll::Ready(Err(_))
        ));
        drop(future);
        assert!(Instant::now() < deadline);
        assert_eq!(
            io.calls, 2,
            "cancelled pending credit cannot poll the writer again"
        );
        assert_eq!(state.lock().unwrap().finishes, 0);
        drop(codec);
        let actual = state.lock().unwrap();
        assert_eq!(
            (actual.tx, actual.rx, actual.unknown, actual.finishes),
            (2, 0, 7, 1)
        );
    });
}

#[test]
fn pending_response_rechecks_original_cancellation_and_keeps_unknown_credit() {
    runtime().block_on(async {
        let state = ledger(1024);
        let mut io = PartialIo::new(Arc::clone(&state), response_bytes(&[255; 16]));
        let deadline = Instant::now() + Duration::from_secs(2);
        let budget = Budget::new(Arc::clone(&state), deadline);
        let cancelled = Arc::clone(&budget.cancelled);
        let mut codec = RequestCodec::default();
        let protocol = StreamProtocol::new("/test/pending-read-cancel");
        codec
            .write_request(&protocol, &mut io, local(budget))
            .await
            .unwrap();
        let write_calls = io.calls;
        io.read_stop = true;
        io.stop_after = 2;
        io.stop = Stop::Pending;
        let mut future = Box::pin(codec.read_response(&protocol, &mut io));
        let waker = noop_waker();
        let mut context = Context::from_waker(&waker);
        assert!(future.as_mut().poll(&mut context).is_pending());
        assert_eq!(state.lock().unwrap().rx, 2);
        cancelled.store(true, Ordering::Release);
        assert!(matches!(
            future.as_mut().poll(&mut context),
            Poll::Ready(Err(_))
        ));
        drop(future);
        assert!(Instant::now() < deadline);
        assert_eq!(
            io.calls,
            write_calls + 2,
            "cancelled credit cannot poll the reader again"
        );
        let actual = state.lock().unwrap();
        assert_eq!((actual.rx, actual.unknown, actual.finishes), (2, 7, 1));
        assert!(actual.pending.is_empty());
    });
}

#[test]
fn write_zero_is_known_zero_but_partial_response_error_is_not() {
    runtime().block_on(async {
        let state = ledger(1024);
        let mut io = PartialIo::new(Arc::clone(&state), vec![]);
        io.stop_after = 0;
        io.stop = Stop::Zero;
        let mut codec = RequestCodec::default();
        let protocol = StreamProtocol::new("/test/zero");
        assert_eq!(
            codec
                .write_request(
                    &protocol,
                    &mut io,
                    local(Budget::new(
                        Arc::clone(&state),
                        Instant::now() + Duration::from_secs(2)
                    ))
                )
                .await
                .unwrap_err()
                .kind(),
            io::ErrorKind::WriteZero
        );
        drop(codec);
        {
            let actual = state.lock().unwrap();
            assert_eq!((actual.tx, actual.unknown), (0, 0));
        }
        let state = ledger(1024);
        let mut io = PartialIo::new(Arc::clone(&state), response_bytes(&[255; 16]));
        io.read_stop = true;
        io.stop_after = 2;
        io.stop = Stop::Error;
        let mut codec = RequestCodec::default();
        codec
            .write_request(
                &protocol,
                &mut io,
                local(Budget::new(
                    Arc::clone(&state),
                    Instant::now() + Duration::from_secs(2),
                )),
            )
            .await
            .unwrap();
        assert!(codec.read_response(&protocol, &mut io).await.is_err());
        let actual = state.lock().unwrap();
        assert_eq!(actual.rx, 2);
        assert!(actual.unknown > 0 && actual.pending.is_empty());
        assert_eq!(actual.finishes, 1);
    });
}

#[test]
fn late_activity_check_start_reserve_and_settlement_never_authorize_more_io() {
    for delay in [Delay::Active, Delay::Start, Delay::Reserve, Delay::Settle] {
        runtime().block_on(async {
            let state = ledger(1024);
            let mut io = PartialIo::new(Arc::clone(&state), vec![]);
            let mut budget = Budget::new(
                Arc::clone(&state),
                Instant::now() + Duration::from_millis(200),
            );
            budget.delay = delay;
            let mut codec = RequestCodec::default();
            assert!(
                codec
                    .write_request(
                        &StreamProtocol::new("/test/late-durable"),
                        &mut io,
                        local(budget)
                    )
                    .await
                    .is_err()
            );
            drop(codec);
            let actual = state.lock().unwrap();
            assert_eq!(actual.unknown, 0);
            assert_eq!(io.calls, usize::from(delay == Delay::Settle));
            assert_eq!(actual.tx, if delay == Delay::Settle { 2 } else { 0 });
        });
    }
}

#[test]
fn cancellation_during_durable_callbacks_never_authorizes_more_io() {
    for cancel_at in [CancelAt::Start, CancelAt::Reserve, CancelAt::Settle] {
        runtime().block_on(async {
            let state = ledger(1024);
            let mut io = PartialIo::new(Arc::clone(&state), vec![]);
            let deadline = Instant::now() + Duration::from_secs(2);
            let mut budget = Budget::new(Arc::clone(&state), deadline);
            budget.cancel_at = cancel_at;
            let mut codec = RequestCodec::default();
            assert!(
                codec
                    .write_request(
                        &StreamProtocol::new("/test/cancel-durable-return"),
                        &mut io,
                        local(budget)
                    )
                    .await
                    .is_err()
            );
            assert!(Instant::now() < deadline);
            drop(codec);
            let actual = state.lock().unwrap();
            assert_eq!(actual.unknown, 0);
            assert_eq!(io.calls, usize::from(cancel_at == CancelAt::Settle));
            assert_eq!(actual.tx, if cancel_at == CancelAt::Settle { 2 } else { 0 });
            assert_eq!(actual.zeros, usize::from(cancel_at == CancelAt::Reserve));
        });
    }
}

#[path = "encoded_exchange_network_tests.rs"]
mod network_tests;
