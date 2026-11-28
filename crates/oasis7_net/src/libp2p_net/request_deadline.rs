//! Local request expiry at the existing application CBOR boundary.
use std::{
    future::Future,
    io,
    pin::pin,
    sync::{Arc, atomic::AtomicBool},
    time::Duration,
};

use futures::{
    AsyncRead, AsyncWrite, AsyncWriteExt, FutureExt,
    future::{Either, select},
};
use libp2p::{
    StreamProtocol,
    request_response::{Codec, cbor},
};
use oasis7_proto::distributed_net::{NetworkRequest, NetworkRequestContext, NetworkResponse};
use tokio::time::{Instant, sleep_until};

use super::runtime_loop::{Command, HandlerRegistration};

pub(super) fn reject_expired_command(command: Option<&Command>) -> bool {
    let Some(
        Command::Request {
            deadline, response, ..
        }
        | Command::RequestToPeer {
            deadline, response, ..
        },
    ) = command
    else {
        return false;
    };
    match check_dispatch_deadline(*deadline) {
        Ok(()) => false,
        Err(error) => {
            let _ = response.send(Err(error));
            true
        }
    }
}

pub(super) fn check_dispatch_deadline(deadline: Option<Instant>) -> Result<(), super::WorldError> {
    if deadline.is_some_and(|deadline| deadline <= Instant::now()) {
        return Err(super::outbound_failure::request_failure_world_error(
            libp2p::request_response::OutboundFailure::Timeout,
            "request expired before dispatch".into(),
        ));
    }
    Ok(())
}

pub(super) fn dispatch_local_handler(
    handler: &HandlerRegistration,
    payload: &[u8],
    deadline: Option<Instant>,
) -> Result<Vec<u8>, super::WorldError> {
    check_dispatch_deadline(deadline)?;
    if let Some(admission) = &handler.admission {
        admission(payload)?;
    }
    check_dispatch_deadline(deadline)?;
    let context = NetworkRequestContext::new(
        deadline
            .unwrap_or_else(|| deadline_in(Duration::from_secs(30)))
            .into_std(),
        Arc::new(AtomicBool::new(false)),
    );
    (handler.handler)(&context, payload)
}

#[derive(Debug)]
pub(super) struct LocalRequest {
    pub(super) message: NetworkRequest,
    pub(super) deadline: Option<Instant>,
    pub(super) exchange: Option<Arc<super::encoded_exchange::Exchange>>,
    pub(super) response_resources: super::authenticated_reply::ReplyResources,
}

pub(super) fn deadline_in(timeout: Duration) -> Instant {
    let now = Instant::now();
    now.checked_add(timeout).unwrap_or(now)
}

#[derive(Debug)]
pub(super) struct RequestDeadlineElapsed;

impl std::fmt::Display for RequestDeadlineElapsed {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("local request deadline expired")
    }
}

impl std::error::Error for RequestDeadlineElapsed {}

pub(super) fn elapsed() -> io::Error {
    io::Error::new(io::ErrorKind::TimedOut, RequestDeadlineElapsed)
}

async fn before_deadline<T>(
    deadline: Instant,
    operation: impl Future<Output = io::Result<T>>,
) -> io::Result<T> {
    if Instant::now() >= deadline {
        return Err(elapsed());
    }
    match select(pin!(sleep_until(deadline)), pin!(operation)).await {
        Either::Left(_) => Err(elapsed()),
        Either::Right((result, _)) => result,
    }
}

#[derive(Default)]
pub(super) struct RequestCodec {
    inner: cbor::codec::Codec<NetworkRequest, NetworkResponse>,
    // The request-response handler clones one codec per stream. This is populated
    // only when that instance writes an outbound application request.
    outbound_deadline: Option<Instant>,
    outbound_exchange: Option<Arc<super::encoded_exchange::Exchange>>,
    // Inbound libp2p owns this codec until write_response AND stream.close
    // finish, or its stream future is cancelled. Never reset on callback return.
    inbound_response_resources: Option<super::authenticated_reply::ReplyResources>,
}

impl Clone for RequestCodec {
    fn clone(&self) -> Self {
        // libp2p clones the configured codec for each new stream. Local exchange
        // identity and deadline must never propagate into another stream.
        Self {
            inner: self.inner.clone(),
            outbound_deadline: None,
            outbound_exchange: None,
            inbound_response_resources: None,
        }
    }
}

/// Encode once with the actual pinned codec before command dispatch. The runtime
/// admits after its existing local peer gate and before dialing or negotiation;
/// the stream later consumes these same bytes.
pub(super) fn prepare_encoded_request(
    message: NetworkRequest,
    exchange: &Arc<super::encoded_exchange::Exchange>,
) -> io::Result<()> {
    exchange.check_active()?;
    let mut codec = cbor::codec::Codec::<NetworkRequest, NetworkResponse>::default();
    let mut encoded = futures::io::Cursor::new(Vec::new());
    codec
        .write_request(
            &StreamProtocol::new(oasis7_proto::distributed::RR_PROTOCOL_PREFIX),
            &mut encoded,
            message,
        )
        .now_or_never()
        .ok_or_else(|| io::Error::other("in-memory request encoding did not complete"))??;
    exchange.prepare(encoded.into_inner())
}

impl Codec for RequestCodec {
    type Protocol = StreamProtocol;
    type Request = LocalRequest;
    type Response = NetworkResponse;

    async fn read_request<T>(
        &mut self,
        protocol: &Self::Protocol,
        io: &mut T,
    ) -> io::Result<LocalRequest>
    where
        T: AsyncRead + Unpin + Send,
    {
        let message = self.inner.read_request(protocol, io).await?;
        let response_resources = super::authenticated_reply::ReplyResources::default();
        self.inbound_response_resources = Some(response_resources.clone());
        Ok(LocalRequest {
            message,
            deadline: None,
            exchange: None,
            response_resources,
        })
    }

    async fn read_response<T>(
        &mut self,
        protocol: &Self::Protocol,
        io: &mut T,
    ) -> io::Result<NetworkResponse>
    where
        T: AsyncRead + Unpin + Send,
    {
        if let Some(exchange) = &self.outbound_exchange {
            let exchange = Arc::clone(exchange);
            let original = Instant::from_std(exchange.deadline());
            let deadline = self
                .outbound_deadline
                .map_or(original, |bound| bound.min(original));
            let result = {
                let mut metered = super::encoded_exchange::MeteredIo::new(
                    io,
                    Arc::clone(&exchange),
                    deadline.into_std(),
                );
                before_deadline(deadline, self.inner.read_response(protocol, &mut metered)).await
            };
            // All read futures and their reservation guards have stopped. The
            // caller's synchronous timeout does not run this cleanup early.
            let finished = exchange.finish();
            let response = result?;
            finished?;
            exchange.check_deadline()?;
            if Instant::now() >= deadline {
                return Err(elapsed());
            }
            return Ok(response);
        }
        let read = self.inner.read_response(protocol, io);
        match self.outbound_deadline {
            Some(deadline) => before_deadline(deadline, read).await,
            None => read.await,
        }
    }

    async fn write_request<T>(
        &mut self,
        protocol: &Self::Protocol,
        io: &mut T,
        request: LocalRequest,
    ) -> io::Result<()>
    where
        T: AsyncWrite + Unpin + Send,
    {
        self.outbound_deadline = request.deadline;
        if let Some(exchange) = request.exchange {
            self.outbound_exchange = Some(Arc::clone(&exchange));
            exchange.check_active()?;
            let original = Instant::from_std(exchange.deadline());
            let deadline = request
                .deadline
                .map_or(original, |bound| bound.min(original));
            if Instant::now() >= deadline {
                return Err(elapsed());
            }
            // The API encoded and the runtime admitted before network dispatch.
            // Missing or reused bytes fail closed, never encode
            // or admit a second time after stream negotiation.
            let encoded = exchange.take_encoded_request()?;
            if Instant::now() >= deadline {
                return Err(elapsed());
            }
            let mut metered =
                super::encoded_exchange::MeteredIo::new(io, exchange, deadline.into_std());
            return before_deadline(deadline, async {
                metered.write_all(&encoded).await?;
                metered.close().await
            })
            .await;
        }
        self.outbound_exchange = None;
        match request.deadline {
            Some(deadline) => {
                before_deadline(deadline, async {
                    self.inner
                        .write_request(protocol, io, request.message)
                        .await?;
                    // The receiver decodes after EOF. Include the writable half-close
                    // in the same deadline; the instrumented stream fuses the handler's
                    // subsequent close poll. An incomplete write returns an error and
                    // the handler drops the stream instead of continuing the message.
                    io.close().await
                })
                .await
            }
            None => {
                self.inner
                    .write_request(protocol, io, request.message)
                    .await
            }
        }
    }

    async fn write_response<T>(
        &mut self,
        protocol: &Self::Protocol,
        io: &mut T,
        response: NetworkResponse,
    ) -> io::Result<()>
    where
        T: AsyncWrite + Unpin + Send,
    {
        self.inner.write_response(protocol, io, response).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        pin::Pin,
        task::{Context, Poll},
    };

    fn request() -> NetworkRequest {
        NetworkRequest {
            protocol: "/test/legacy-cbor".into(),
            payload: vec![0, 1, 127, 255],
        }
    }

    #[test]
    fn legacy_cbor_request_is_byte_identical_with_local_deadline_metadata() {
        let protocol = StreamProtocol::new("/test/request-encoding");
        tokio::runtime::Builder::new_current_thread()
            .enable_time()
            .build()
            .unwrap()
            .block_on(async {
                let mut original = Vec::new();
                cbor::codec::Codec::<NetworkRequest, NetworkResponse>::default()
                    .write_request(&protocol, &mut original, request())
                    .await
                    .unwrap();
                for deadline in [None, Some(deadline_in(Duration::from_secs(3)))] {
                    let mut wrapped = Vec::new();
                    RequestCodec::default()
                        .write_request(
                            &protocol,
                            &mut wrapped,
                            LocalRequest {
                                message: request(),
                                deadline,
                                exchange: None,
                                response_resources: Default::default(),
                            },
                        )
                        .await
                        .unwrap();
                    assert_eq!(wrapped, original);
                    let mut input = futures::io::Cursor::new(wrapped);
                    let received = RequestCodec::default()
                        .read_request(&protocol, &mut input)
                        .await
                        .unwrap();
                    assert_eq!(received.message, request());
                    assert!(received.deadline.is_none());
                }
            });
    }

    #[test]
    fn handler_fallback_rechecks_original_deadline_after_admission() {
        use oasis7_proto::network_failure::FailureCause;
        use std::sync::atomic::Ordering;

        let deadline = deadline_in(Duration::from_millis(250));
        let admitted = Arc::new(AtomicBool::new(false));
        let callback = Arc::clone(&admitted);
        let handler = HandlerRegistration {
            admission: Some(Arc::new(move |_| {
                callback.store(true, Ordering::Release);
                std::thread::sleep(
                    deadline.saturating_duration_since(Instant::now()) + Duration::from_millis(5),
                );
                Ok(())
            })),
            handler: Arc::new(|_, _| panic!("expired local request reached its handler")),
            authenticated: None,
        };
        let error = dispatch_local_handler(&handler, b"expired", Some(deadline)).unwrap_err();
        assert!(admitted.load(Ordering::Acquire));
        assert_eq!(error.network_failure(None).cause, FailureCause::Timeout);
    }

    #[derive(Default)]
    struct PartialWriter {
        bytes: usize,
        close_calls: u32,
    }

    impl AsyncWrite for PartialWriter {
        fn poll_write(
            mut self: Pin<&mut Self>,
            _: &mut Context<'_>,
            buffer: &[u8],
        ) -> Poll<io::Result<usize>> {
            if self.bytes != 0 {
                return Poll::Pending;
            }
            assert!(!buffer.is_empty());
            self.bytes = 1;
            Poll::Ready(Ok(1))
        }
        fn poll_flush(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<io::Result<()>> {
            Poll::Ready(Ok(()))
        }
        fn poll_close(mut self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<io::Result<()>> {
            self.close_calls += 1;
            Poll::Ready(Ok(()))
        }
    }

    #[test]
    fn interrupted_partial_request_never_closes_as_a_completed_write() {
        tokio::runtime::Builder::new_current_thread()
            .enable_time()
            .build()
            .unwrap()
            .block_on(async {
                let mut stream = PartialWriter::default();
                let error = RequestCodec::default()
                    .write_request(
                        &StreamProtocol::new("/test/request-encoding"),
                        &mut stream,
                        LocalRequest {
                            message: request(),
                            deadline: Some(deadline_in(Duration::from_millis(30))),
                            exchange: None,
                            response_resources: Default::default(),
                        },
                    )
                    .await
                    .unwrap_err();
                assert!(error.get_ref().unwrap().is::<RequestDeadlineElapsed>());
                assert_eq!(stream.bytes, 1);
                assert_eq!(stream.close_calls, 0);
            });
    }

    struct ResponseHold(Arc<std::sync::atomic::AtomicUsize>);
    impl Drop for ResponseHold {
        fn drop(&mut self) {
            self.0.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        }
    }

    fn held_response() -> (
        RequestCodec,
        super::super::authenticated_reply::ReplyGuard,
        Arc<std::sync::atomic::AtomicUsize>,
    ) {
        let protocol = StreamProtocol::new("/test/response-hold");
        let mut encoded = Vec::new();
        cbor::codec::Codec::<NetworkRequest, NetworkResponse>::default()
            .write_request(&protocol, &mut encoded, request())
            .now_or_never()
            .unwrap()
            .unwrap();
        let mut codec = RequestCodec::default();
        let request = codec
            .read_request(&protocol, &mut futures::io::Cursor::new(encoded))
            .now_or_never()
            .unwrap()
            .unwrap();
        let guard = super::super::authenticated_reply::ReplyGuard::new(
            Arc::new(AtomicBool::new(true)),
            request.response_resources,
        );
        guard
            .initialize(&NetworkRequestContext::new(
                std::time::Instant::now() + Duration::from_secs(30),
                Arc::new(AtomicBool::new(false)),
            ))
            .unwrap();
        let drops = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        guard.retain(Box::new(ResponseHold(drops.clone()))).unwrap();
        (codec, guard, drops)
    }

    struct ResponseWriter {
        bytes: Vec<u8>,
        drops: Arc<std::sync::atomic::AtomicUsize>,
        close_ready: bool,
        fail: bool,
    }
    impl AsyncWrite for ResponseWriter {
        fn poll_write(
            mut self: Pin<&mut Self>,
            _: &mut Context<'_>,
            bytes: &[u8],
        ) -> Poll<io::Result<usize>> {
            assert_eq!(self.drops.load(std::sync::atomic::Ordering::SeqCst), 0);
            if self.fail {
                return Poll::Ready(Err(io::Error::other("response write failed")));
            }
            self.bytes.extend_from_slice(bytes);
            Poll::Ready(Ok(bytes.len()))
        }
        fn poll_flush(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<io::Result<()>> {
            assert_eq!(self.drops.load(std::sync::atomic::Ordering::SeqCst), 0);
            Poll::Ready(Ok(()))
        }
        fn poll_close(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<io::Result<()>> {
            assert_eq!(self.drops.load(std::sync::atomic::Ordering::SeqCst), 0);
            if self.close_ready {
                Poll::Ready(Ok(()))
            } else {
                Poll::Pending
            }
        }
    }

    #[test]
    fn response_hold_survives_callback_encoding_and_pending_half_close() {
        use std::sync::atomic::Ordering;
        let (mut codec, guard, drops) = held_response();
        let rejected = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        assert!(
            guard
                .retain(Box::new(ResponseHold(rejected.clone())))
                .is_err()
        );
        assert_eq!(rejected.load(Ordering::SeqCst), 1);
        let unrelated_stream = codec.clone();
        drop(guard); // Callback and completion have returned; stream still owns the hold.
        let protocol = StreamProtocol::new("/test/response-hold");
        let response = NetworkResponse {
            payload: vec![42; 4096],
        };
        let mut expected = Vec::new();
        cbor::codec::Codec::<NetworkRequest, NetworkResponse>::default()
            .write_response(&protocol, &mut expected, response.clone())
            .now_or_never()
            .unwrap()
            .unwrap();
        let mut writer = ResponseWriter {
            bytes: Vec::new(),
            drops: drops.clone(),
            close_ready: false,
            fail: false,
        };
        codec
            .write_response(&protocol, &mut writer, response)
            .now_or_never()
            .unwrap()
            .unwrap();
        assert_eq!(writer.bytes, expected);
        assert!(writer.close().now_or_never().is_none());
        assert_eq!(drops.load(Ordering::SeqCst), 0);
        writer.close_ready = true;
        writer.close().now_or_never().unwrap().unwrap();
        drop(codec);
        assert_eq!(drops.load(Ordering::SeqCst), 1);
        drop(unrelated_stream); // A per-stream clone must not retain the old stream's hold.
        assert_eq!(drops.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn response_hold_releases_after_write_error_and_last_worker_on_abort() {
        use std::sync::atomic::Ordering;
        let protocol = StreamProtocol::new("/test/response-hold");
        let (mut codec, guard, drops) = held_response();
        drop(guard);
        let mut writer = ResponseWriter {
            bytes: Vec::new(),
            drops: drops.clone(),
            close_ready: true,
            fail: true,
        };
        assert!(
            codec
                .write_response(
                    &protocol,
                    &mut writer,
                    NetworkResponse { payload: vec![42] }
                )
                .now_or_never()
                .unwrap()
                .is_err()
        );
        drop(codec);
        assert_eq!(drops.load(Ordering::SeqCst), 1);

        let (mut codec, guard, drops) = held_response();
        let worker = guard.clone();
        drop(guard);
        let mut writer = PartialWriter::default();
        assert!(
            codec
                .write_response(
                    &protocol,
                    &mut writer,
                    NetworkResponse {
                        payload: vec![42; 4096]
                    }
                )
                .now_or_never()
                .is_none()
        ); // Cancels a real codec write after one byte.
        assert_eq!(writer.bytes, 1);
        assert_eq!(writer.close_calls, 0);
        drop(codec); // Stream timeout/abort cannot release a still-running worker.
        assert_eq!(drops.load(Ordering::SeqCst), 0);
        drop(worker);
        assert_eq!(drops.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn response_hold_survives_partial_write_timeout_until_stream_drop() {
        tokio::runtime::Builder::new_current_thread()
            .enable_time()
            .build()
            .unwrap()
            .block_on(async {
                let (mut codec, guard, drops) = held_response();
                drop(guard);
                let mut writer = PartialWriter::default();
                let error = before_deadline(
                    deadline_in(Duration::from_millis(30)),
                    codec.write_response(
                        &StreamProtocol::new("/test/response-hold"),
                        &mut writer,
                        NetworkResponse {
                            payload: vec![42; 4096],
                        },
                    ),
                )
                .await
                .unwrap_err();
                assert!(error.get_ref().unwrap().is::<RequestDeadlineElapsed>());
                assert_eq!(writer.bytes, 1);
                assert_eq!(writer.close_calls, 0);
                assert_eq!(drops.load(std::sync::atomic::Ordering::SeqCst), 0);
                drop(codec);
                assert_eq!(drops.load(std::sync::atomic::Ordering::SeqCst), 1);
            });
    }
}
