// Copyright 2017-2020 Parity Technologies (UK) Ltd.
//
// Permission is hereby granted, free of charge, to any person obtaining a
// copy of this software and associated documentation files (the "Software"),
// to deal in the Software without restriction, including without limitation
// the rights to use, copy, modify, merge, publish, distribute, sublicense,
// and/or sell copies of the Software, and to permit persons to whom the
// Software is furnished to do so, subject to the following conditions:
//
// The above copyright notice and this permission notice shall be included in
// all copies or substantial portions of the Software.
//
// THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS
// OR IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
// FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
// AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
// LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING
// FROM, OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER
// DEALINGS IN THE SOFTWARE.

//! Future that drives a QUIC connection until is has performed its TLS handshake.

use std::{
    pin::Pin,
    task::{Context, Poll},
    time::Duration,
};

use futures::{future::FutureExt, prelude::*};
use futures_timer::Delay;
use libp2p_identity::PeerId;
use quinn::rustls::pki_types::CertificateDer;
use quinn_proto::TransportError;

use crate::{Connection, ConnectionError, Error};

/// A QUIC connection currently being negotiated.
#[derive(Debug)]
pub struct Connecting {
    incoming: Option<quinn::Incoming>,
    connecting: Option<quinn::Connecting>,
    timeout: Delay,
}

impl Connecting {
    pub(crate) fn new(connection: quinn::Connecting, timeout: Duration) -> Self {
        Connecting {
            incoming: None,
            connecting: Some(connection),
            timeout: Delay::new(timeout),
        }
    }
}

impl Connecting {
    /// Keep the server handshake unstarted until the caller admits and polls
    /// this upgrade. Dropping an unpolled Incoming refuses it in Quinn.
    pub(crate) fn incoming(incoming: quinn::Incoming, timeout: Duration) -> Self {
        Self {
            incoming: Some(incoming),
            connecting: None,
            // Start at receipt, not first poll: admission cannot extend it.
            timeout: Delay::new(timeout),
        }
    }

    fn remote_peer_id(connection: &quinn::Connection) -> Result<PeerId, Error> {
        fn transport_err(reason: &str) -> Error {
            Error::Connection(ConnectionError(quinn::ConnectionError::TransportError(
                TransportError {
                    code: quinn::TransportErrorCode::PROTOCOL_VIOLATION,
                    frame: None,
                    reason: reason.to_string(),
                },
            )))
        }

        let identity = connection
            .peer_identity()
            .ok_or_else(|| transport_err("No crypto identity in quinn's Connection"))?;
        let certificates: Box<Vec<CertificateDer>> = identity
            .downcast()
            .map_err(|_| transport_err("Could not downcast identity"))?;
        let end_entity = certificates
            .first()
            .ok_or_else(|| transport_err("No certificate found"))?;
        let p2p_cert = libp2p_tls::certificate::parse(end_entity)
            .map_err(|_| transport_err("Could not parse certificate"))?;
        Ok(p2p_cert.peer_id())
    }
}

impl Future for Connecting {
    type Output = Result<(PeerId, Connection), Error>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        if self.incoming.is_some() {
            if self.timeout.poll_unpin(cx).is_ready() {
                // Refuse without accepting an attempt whose deadline expired.
                self.incoming.take();
                return Poll::Ready(Err(Error::HandshakeTimedOut));
            }
            let incoming = self.incoming.take().expect("incoming checked above");
            self.connecting = Some(incoming.accept().map_err(ConnectionError)?);
        }
        let connection = match self
            .connecting
            .as_mut()
            .expect("connecting state")
            .poll_unpin(cx)
        {
            Poll::Ready(result) => result.map_err(ConnectionError)?,
            Poll::Pending => {
                if self.timeout.poll_unpin(cx).is_ready() {
                    return Poll::Ready(Err(Error::HandshakeTimedOut));
                }
                return Poll::Pending;
            }
        };

        let peer_id = Self::remote_peer_id(&connection)?;
        let muxer = Connection::new(connection);
        Poll::Ready(Ok((peer_id, muxer)))
    }
}
