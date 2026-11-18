use std::convert::TryFrom as _;
use std::io;
use std::pin::Pin;
use std::sync::{
    Arc,
    atomic::{AtomicU64, Ordering},
};
use std::task::{Context, Poll};

use futures::io::{AsyncRead, AsyncWrite, IoSlice, IoSliceMut};
use futures::ready;
use libp2p::core::muxing::{StreamMuxer, StreamMuxerEvent};

use super::wire_byte_window::{BudgetWait, Direction, WireByteWindow};

#[cfg(test)]
#[path = "wire_bytes_tests.rs"]
mod tests;

const MAX_VECTORS_PER_POLL: usize = 16;

#[derive(Debug, Default)]
pub(crate) struct Libp2pWireByteCounters {
    inbound: AtomicU64,
    outbound: AtomicU64,
    // Standard hosts leave this unset until connection admission is available.
    window: Option<Arc<WireByteWindow>>,
}

pub(crate) type SharedLibp2pWireByteCounters = Arc<Libp2pWireByteCounters>;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) struct Libp2pWireByteSnapshot {
    pub inbound_bytes: u64,
    pub outbound_bytes: u64,
}

pub(crate) fn init_shared_wire_byte_counters() -> SharedLibp2pWireByteCounters {
    Arc::new(Libp2pWireByteCounters::default())
}

impl Libp2pWireByteCounters {
    #[cfg(test)]
    pub(super) fn with_window(window: Arc<WireByteWindow>) -> SharedLibp2pWireByteCounters {
        Arc::new(Self {
            window: Some(window),
            ..Self::default()
        })
    }

    pub(super) fn window(&self) -> Option<Arc<WireByteWindow>> {
        self.window.clone()
    }
}

pub(crate) fn snapshot_wire_byte_counters(
    counters: &SharedLibp2pWireByteCounters,
) -> Libp2pWireByteSnapshot {
    Libp2pWireByteSnapshot {
        inbound_bytes: counters.inbound.load(Ordering::Relaxed),
        outbound_bytes: counters.outbound.load(Ordering::Relaxed),
    }
}

pub(crate) fn record_inbound_wire_bytes(counters: &SharedLibp2pWireByteCounters, num_bytes: usize) {
    saturating_atomic_add(
        &counters.inbound,
        u64::try_from(num_bytes).unwrap_or(u64::MAX),
    );
}

pub(crate) fn record_outbound_wire_bytes(
    counters: &SharedLibp2pWireByteCounters,
    num_bytes: usize,
) {
    saturating_atomic_add(
        &counters.outbound,
        u64::try_from(num_bytes).unwrap_or(u64::MAX),
    );
}

fn saturating_atomic_add(counter: &AtomicU64, delta: u64) {
    let mut observed = counter.load(Ordering::Relaxed);
    loop {
        let next = observed.saturating_add(delta);
        match counter.compare_exchange_weak(observed, next, Ordering::Relaxed, Ordering::Relaxed) {
            Ok(_) => return,
            Err(current) => observed = current,
        }
    }
}

#[derive(Clone)]
#[pin_project::pin_project]
pub(crate) struct InstrumentedStreamMuxer<SMInner> {
    #[pin]
    inner: SMInner,
    counters: SharedLibp2pWireByteCounters,
}

impl<SMInner> InstrumentedStreamMuxer<SMInner> {
    pub(crate) fn new(inner: SMInner, counters: SharedLibp2pWireByteCounters) -> Self {
        Self { inner, counters }
    }
}

impl<SMInner> StreamMuxer for InstrumentedStreamMuxer<SMInner>
where
    SMInner: StreamMuxer,
{
    type Substream = InstrumentedSubstream<SMInner::Substream>;
    type Error = SMInner::Error;

    fn poll(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<Result<StreamMuxerEvent, Self::Error>> {
        let this = self.project();
        this.inner.poll(cx)
    }

    fn poll_inbound(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<Result<Self::Substream, Self::Error>> {
        let this = self.project();
        let inner = ready!(this.inner.poll_inbound(cx)?);
        Poll::Ready(Ok(InstrumentedSubstream {
            inner,
            counters: this.counters.clone(),
            read_wait: BudgetWait::default(),
            write_wait: BudgetWait::default(),
        }))
    }

    fn poll_outbound(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<Result<Self::Substream, Self::Error>> {
        let this = self.project();
        let inner = ready!(this.inner.poll_outbound(cx)?);
        Poll::Ready(Ok(InstrumentedSubstream {
            inner,
            counters: this.counters.clone(),
            read_wait: BudgetWait::default(),
            write_wait: BudgetWait::default(),
        }))
    }

    fn poll_close(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        let this = self.project();
        this.inner.poll_close(cx)
    }
}

#[pin_project::pin_project]
pub(crate) struct InstrumentedSubstream<SMInner> {
    #[pin]
    inner: SMInner,
    counters: SharedLibp2pWireByteCounters,
    read_wait: BudgetWait,
    write_wait: BudgetWait,
}

fn offered_vectored_bytes(lengths: impl Iterator<Item = usize>) -> io::Result<usize> {
    lengths
        .filter(|len| *len != 0)
        .take(MAX_VECTORS_PER_POLL)
        .try_fold(0usize, |total, len| {
            total.checked_add(len).ok_or_else(|| {
                io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "substream iovec length overflow",
                )
            })
        })
}

impl<SMInner: AsyncRead> AsyncRead for InstrumentedSubstream<SMInner> {
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut [u8],
    ) -> Poll<io::Result<usize>> {
        let this = self.project();
        let result = match &this.counters.window {
            Some(window) => {
                this.read_wait
                    .poll(window, cx, Direction::Read, buf.len(), |len, cx| {
                        this.inner.poll_read(cx, &mut buf[..len])
                    })
            }
            None => this.inner.poll_read(cx, buf),
        };
        let num_bytes = ready!(result)?;
        record_inbound_wire_bytes(this.counters, num_bytes);
        Poll::Ready(Ok(num_bytes))
    }

    fn poll_read_vectored(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        bufs: &mut [IoSliceMut<'_>],
    ) -> Poll<io::Result<usize>> {
        let this = self.project();
        let result = match &this.counters.window {
            Some(window) => {
                let requested = offered_vectored_bytes(bufs.iter().map(|buf| buf.len()))?;
                this.read_wait
                    .poll(window, cx, Direction::Read, requested, |mut len, cx| {
                        let mut limited = Vec::with_capacity(bufs.len().min(MAX_VECTORS_PER_POLL));
                        for buf in bufs
                            .iter_mut()
                            .filter(|buf| !buf.is_empty())
                            .take(MAX_VECTORS_PER_POLL)
                        {
                            let count = len.min(buf.len());
                            if count == 0 {
                                break;
                            }
                            limited.push(IoSliceMut::new(&mut buf[..count]));
                            len -= count;
                        }
                        this.inner.poll_read_vectored(cx, &mut limited)
                    })
            }
            None => this.inner.poll_read_vectored(cx, bufs),
        };
        let num_bytes = ready!(result)?;
        record_inbound_wire_bytes(this.counters, num_bytes);
        Poll::Ready(Ok(num_bytes))
    }
}

impl<SMInner: AsyncWrite> AsyncWrite for InstrumentedSubstream<SMInner> {
    fn poll_write(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<io::Result<usize>> {
        let this = self.project();
        let result = match &this.counters.window {
            Some(window) => {
                this.write_wait
                    .poll(window, cx, Direction::Write, buf.len(), |len, cx| {
                        this.inner.poll_write(cx, &buf[..len])
                    })
            }
            None => this.inner.poll_write(cx, buf),
        };
        let num_bytes = ready!(result)?;
        record_outbound_wire_bytes(this.counters, num_bytes);
        Poll::Ready(Ok(num_bytes))
    }

    fn poll_write_vectored(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        bufs: &[IoSlice<'_>],
    ) -> Poll<io::Result<usize>> {
        let this = self.project();
        let result = match &this.counters.window {
            Some(window) => {
                let requested = offered_vectored_bytes(bufs.iter().map(|buf| buf.len()))?;
                this.write_wait
                    .poll(window, cx, Direction::Write, requested, |mut len, cx| {
                        let mut limited = Vec::with_capacity(bufs.len().min(MAX_VECTORS_PER_POLL));
                        for buf in bufs
                            .iter()
                            .filter(|buf| !buf.is_empty())
                            .take(MAX_VECTORS_PER_POLL)
                        {
                            let count = len.min(buf.len());
                            if count == 0 {
                                break;
                            }
                            limited.push(IoSlice::new(&buf[..count]));
                            len -= count;
                        }
                        this.inner.poll_write_vectored(cx, &limited)
                    })
            }
            None => this.inner.poll_write_vectored(cx, bufs),
        };
        let num_bytes = ready!(result)?;
        record_outbound_wire_bytes(this.counters, num_bytes);
        Poll::Ready(Ok(num_bytes))
    }

    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        let this = self.project();
        this.inner.poll_flush(cx)
    }

    fn poll_close(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        let this = self.project();
        this.inner.poll_close(cx)
    }
}
