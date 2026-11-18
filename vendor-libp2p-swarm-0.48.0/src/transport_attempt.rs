use std::{
    future::Future,
    io,
    pin::Pin,
    task::{Context, Poll, ready},
};

use libp2p_core::{
    Multiaddr, Transport,
    muxing::{StreamMuxer, StreamMuxerBox, StreamMuxerEvent, SubstreamBox},
};
use libp2p_identity::PeerId;

use crate::{ConnectionDenied, ConnectionId, connection::pool::concurrent_dial::DialFuture};

/// An admission token whose destructor releases the caller's transport attempt resources.
pub type TransportAttemptPermit = Box<dyn Send>;

/// The token returned by outbound admission.
pub type OutboundAttemptPermit = TransportAttemptPermit;

pub(crate) type OutboundAttemptAdmission = Box<
    dyn FnMut(
            ConnectionId,
            Option<PeerId>,
            &Multiaddr,
        ) -> Result<OutboundAttemptPermit, ConnectionDenied>
        + Send,
>;

pub(crate) type InboundAttemptAdmission = Box<
    dyn FnMut(
            ConnectionId,
            &Multiaddr,
            &Multiaddr,
        ) -> Result<TransportAttemptPermit, ConnectionDenied>
        + Send,
>;

type ListenerUpgrade =
    <libp2p_core::transport::Boxed<(PeerId, StreamMuxerBox)> as Transport>::ListenerUpgrade;

pub(crate) fn hold_attempt_permit(future: DialFuture, permit: OutboundAttemptPermit) -> DialFuture {
    Box::pin(PermittedAttempt {
        future,
        permit: Some(permit),
        on_ready: |(address, result), permit| (address, retain_connected(result, permit)),
    })
}

pub(crate) fn hold_listener_permit(
    future: ListenerUpgrade,
    permit: TransportAttemptPermit,
) -> ListenerUpgrade {
    Box::pin(PermittedAttempt {
        future,
        permit: Some(permit),
        on_ready: retain_connected,
    })
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
