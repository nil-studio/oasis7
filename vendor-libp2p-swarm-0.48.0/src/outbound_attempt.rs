use std::{
    future::Future,
    io,
    pin::Pin,
    task::{Context, Poll, ready},
};

use libp2p_core::{
    Multiaddr,
    muxing::{StreamMuxer, StreamMuxerBox, StreamMuxerEvent, SubstreamBox},
};
use libp2p_identity::PeerId;

use crate::{ConnectionDenied, ConnectionId, connection::pool::concurrent_dial::DialFuture};

/// An admission token whose destructor releases the caller's outbound attempt resources.
pub type OutboundAttemptPermit = Box<dyn Send>;

pub(crate) type OutboundAttemptAdmission = Box<
    dyn FnMut(
            ConnectionId,
            Option<PeerId>,
            &Multiaddr,
        ) -> Result<OutboundAttemptPermit, ConnectionDenied>
        + Send,
>;

pub(crate) fn hold_attempt_permit(future: DialFuture, permit: OutboundAttemptPermit) -> DialFuture {
    Box::pin(OutboundAttempt {
        future,
        permit: Some(permit),
    })
}

struct OutboundAttempt {
    // Drop the transport future before releasing its permit on cancellation.
    future: DialFuture,
    permit: Option<OutboundAttemptPermit>,
}

impl Future for OutboundAttempt {
    type Output = <DialFuture as Future>::Output;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let (address, result) = ready!(self.future.as_mut().poll(cx));
        let result = result.map(|(peer, muxer)| {
            let permit = self.permit.take().expect("attempt completes only once");
            (
                peer,
                StreamMuxerBox::new(PermittedMuxer {
                    muxer,
                    _permit: permit,
                }),
            )
        });
        Poll::Ready((address, result))
    }
}

struct PermittedMuxer {
    // Drop the underlying connection before releasing the caller's permit.
    muxer: StreamMuxerBox,
    _permit: OutboundAttemptPermit,
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
