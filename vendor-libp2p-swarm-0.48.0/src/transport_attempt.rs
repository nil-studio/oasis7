use std::{
    future::Future,
    io,
    pin::Pin,
    task::{Context, Poll, ready},
};

use futures::{FutureExt, future::poll_fn};
use libp2p_core::{
    Multiaddr, Transport,
    muxing::{StreamMuxer, StreamMuxerBox, StreamMuxerEvent, SubstreamBox},
    transport::TransportError,
};
use libp2p_identity::PeerId;
use web_time::Instant;

use crate::{ConnectionDenied, ConnectionId, connection::pool::concurrent_dial::DialFuture};

/// An admission token whose destructor releases the caller's transport attempt resources.
pub type TransportAttemptPermit = Box<dyn Send>;

/// The token returned by outbound admission.
pub type OutboundAttemptPermit = TransportAttemptPermit;

/// Resources and an optional absolute deadline for a transport handshake.
pub struct TransportAdmission {
    permit: TransportAttemptPermit,
    deadline: Option<Instant>,
}

impl TransportAdmission {
    /// Retain the permit while the attempt runs and terminate at the given instant.
    pub fn until(permit: TransportAttemptPermit, deadline: Instant) -> Self {
        Self {
            permit,
            deadline: Some(deadline),
        }
    }

    pub(crate) fn unbounded(permit: TransportAttemptPermit) -> Self {
        Self {
            permit,
            deadline: None,
        }
    }

    pub(crate) fn check(self) -> Result<Self, ConnectionDenied> {
        if self
            .deadline
            .is_some_and(|deadline| Instant::now() >= deadline)
        {
            return Err(ConnectionDenied::new(timed_out()));
        }
        Ok(self)
    }
}

pub(crate) type OutboundAttemptAdmission = Box<
    dyn FnMut(
            ConnectionId,
            Option<PeerId>,
            &Multiaddr,
        ) -> Result<TransportAdmission, ConnectionDenied>
        + Send,
>;

pub(crate) type InboundAttemptAdmission = Box<
    dyn FnMut(ConnectionId, &Multiaddr, &Multiaddr) -> Result<TransportAdmission, ConnectionDenied>
        + Send,
>;

type ListenerUpgrade =
    <libp2p_core::transport::Boxed<(PeerId, StreamMuxerBox)> as Transport>::ListenerUpgrade;

pub(crate) fn hold_attempt_permit(
    future: DialFuture,
    admission: TransportAdmission,
    address: Multiaddr,
) -> DialFuture {
    let future = if let Some(deadline) = admission.deadline {
        async move {
            until(future, deadline)
                .await
                .unwrap_or_else(|error| (address, Err(TransportError::Other(error))))
        }
        .boxed()
    } else {
        future
    };
    Box::pin(PermittedAttempt {
        future,
        permit: Some(admission.permit),
        on_ready: |(address, result), permit| (address, retain_connected(result, permit)),
    })
}

pub(crate) fn hold_listener_permit(
    future: ListenerUpgrade,
    admission: TransportAdmission,
) -> ListenerUpgrade {
    let future = if let Some(deadline) = admission.deadline {
        async move { until(future, deadline).await? }.boxed()
    } else {
        future
    };
    Box::pin(PermittedAttempt {
        future,
        permit: Some(admission.permit),
        on_ready: retain_connected,
    })
}

fn timed_out() -> io::Error {
    io::Error::new(
        io::ErrorKind::TimedOut,
        "transport handshake deadline expired",
    )
}

async fn until<F: Future + Unpin>(mut future: F, deadline: Instant) -> io::Result<F::Output> {
    let mut timer = futures_timer::Delay::new(deadline.saturating_duration_since(Instant::now()));
    poll_fn(|cx| {
        if Instant::now() >= deadline {
            return Poll::Ready(Err(timed_out()));
        }
        if Pin::new(&mut timer).poll(cx).is_ready() {
            // Re-arm if timer granularity wakes the task before the absolute deadline.
            timer.reset(deadline.saturating_duration_since(Instant::now()));
            cx.waker().wake_by_ref();
        }
        let result = ready!(Pin::new(&mut future).poll(cx));
        if Instant::now() >= deadline {
            return Poll::Ready(Err(timed_out()));
        }
        Poll::Ready(Ok(result))
    })
    .await
}

type AttemptResult<E> = Result<(PeerId, StreamMuxerBox), E>;
type ReadyMapper<T> = fn(T, &mut Option<TransportAttemptPermit>) -> T;

struct PermittedAttempt<F: Future> {
    // Drop the transport future before releasing its permit on cancellation.
    future: F,
    permit: Option<TransportAttemptPermit>,
    on_ready: ReadyMapper<F::Output>,
}

fn retain_connected<E>(
    result: AttemptResult<E>,
    permit: &mut Option<TransportAttemptPermit>,
) -> AttemptResult<E> {
    result.map(|(peer, muxer)| {
        let permit = permit.take().expect("attempt completes only once");
        (
            peer,
            StreamMuxerBox::new(PermittedMuxer {
                muxer,
                _permit: permit,
            }),
        )
    })
}

impl<F: Future + Unpin> Future for PermittedAttempt<F> {
    type Output = F::Output;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let result = ready!(Pin::new(&mut self.future).poll(cx));
        Poll::Ready((self.on_ready)(result, &mut self.permit))
    }
}

struct PermittedMuxer {
    // Drop the underlying connection before releasing the caller's permit.
    muxer: StreamMuxerBox,
    _permit: TransportAttemptPermit,
}

impl StreamMuxer for PermittedMuxer {
    type Substream = SubstreamBox;
    type Error = io::Error;

    fn poll_inbound(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<Result<Self::Substream, Self::Error>> {
        Pin::new(&mut self.muxer).poll_inbound(cx)
    }

    fn poll_outbound(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<Result<Self::Substream, Self::Error>> {
        Pin::new(&mut self.muxer).poll_outbound(cx)
    }

    fn poll_close(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Pin::new(&mut self.muxer).poll_close(cx)
    }

    fn poll(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<Result<StreamMuxerEvent, Self::Error>> {
        Pin::new(&mut self.muxer).poll(cx)
    }
}
