//! Local request expiry at the existing application CBOR boundary.
use std::{
    future::Future,
    io,
    pin::pin,
    sync::{Arc, atomic::AtomicBool},
    time::Duration,
};

use futures::{
    AsyncRead, AsyncWrite, AsyncWriteExt,
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

fn elapsed() -> io::Error {
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

#[derive(Clone, Default)]
pub(super) struct RequestCodec {
    inner: cbor::codec::Codec<NetworkRequest, NetworkResponse>,
    // The request-response handler clones one codec per stream. This is populated
    // only when that instance writes an outbound application request.
    outbound_deadline: Option<Instant>,
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
        Ok(LocalRequest {
            message: self.inner.read_request(protocol, io).await?,
            deadline: None,
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
                        },
                    )
                    .await
                    .unwrap_err();
                assert!(error.get_ref().unwrap().is::<RequestDeadlineElapsed>());
                assert_eq!(stream.bytes, 1);
                assert_eq!(stream.close_calls, 0);
            });
    }
}
