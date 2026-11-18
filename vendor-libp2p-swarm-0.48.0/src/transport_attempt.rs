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
    })
}

pub(crate) fn hold_listener_permit(
    future: ListenerUpgrade,
    permit: TransportAttemptPermit,
) -> ListenerUpgrade {
    Box::pin(PermittedAttempt {
        future,
        permit: Some(permit),
    })
}

struct PermittedAttempt<F> {
    // Drop the transport future before releasing its permit on cancellation.
    future: F,
    permit: Option<TransportAttemptPermit>,
}

impl<F> PermittedAttempt<F> {
    fn retain_connected(
        &mut self,
        peer: PeerId,
        muxer: StreamMuxerBox,
    ) -> (PeerId, StreamMuxerBox) {
        let permit = self.permit.take().expect("attempt completes only once");
        (
            peer,
            StreamMuxerBox::new(PermittedMuxer {
                muxer,
                _permit: permit,
            }),
        )
    }
}

impl Future for PermittedAttempt<DialFuture> {
    type Output = <DialFuture as Future>::Output;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let (address, result) = ready!(self.future.as_mut().poll(cx));
        let result = result.map(|(peer, muxer)| self.retain_connected(peer, muxer));
        Poll::Ready((address, result))
    }
}

impl Future for PermittedAttempt<ListenerUpgrade> {
    type Output = <ListenerUpgrade as Future>::Output;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let result = ready!(self.future.as_mut().poll(cx));
        Poll::Ready(result.map(|(peer, muxer)| self.retain_connected(peer, muxer)))
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
