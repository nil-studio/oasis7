//! Counts actual complete CBOR bytes around the pinned request-response codec.
use std::{
    fmt, io,
    pin::Pin,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    task::{Context, Poll},
    time::Instant,
};

use futures::{AsyncRead, AsyncWrite};

use crate::encoded_exchange::{EncodedExchangeBudget, EncodedIoDirection, EncodedIoReservation};

const MAX_WINDOW: usize = 64 * 1024;

pub(super) struct Exchange {
    budget: Box<dyn EncodedExchangeBudget>,
    deadline: Instant,
    started: AtomicBool,
    finished: AtomicBool,
}

impl fmt::Debug for Exchange {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("EncodedExchange")
            .finish_non_exhaustive()
    }
}

impl Exchange {
    pub(super) fn new(budget: Box<dyn EncodedExchangeBudget>) -> Arc<Self> {
        let deadline = budget.deadline();
        Arc::new(Self {
            budget,
            deadline,
            started: AtomicBool::new(false),
            finished: AtomicBool::new(false),
        })
    }

    pub(super) fn deadline(&self) -> Instant {
        self.deadline
    }

    pub(super) fn check_deadline(&self) -> io::Result<()> {
        if Instant::now() >= self.deadline {
            return Err(super::request_deadline::elapsed());
        }
        Ok(())
    }

    pub(super) fn check_active(&self) -> io::Result<()> {
        self.check_deadline()?;
        if self.finished.load(Ordering::Acquire) {
            return Err(io::Error::other("encoded exchange has stopped"));
        }
        self.budget.check_active()?;
        // A slow owner/cancellation check cannot extend the original deadline.
        self.check_deadline()
    }

    pub(super) fn start(&self, bytes: usize) -> io::Result<()> {
        self.check_active()?;
        if self.finished.load(Ordering::Acquire) || self.started.swap(true, Ordering::AcqRel) {
            return Err(io::Error::other(
                "encoded exchange already admitted or stopped",
            ));
        }
        self.budget.start(bytes)?;
        // A late durable admission does not authorize even the first stream poll.
        self.check_active()
    }

    pub(super) fn finish(&self) -> io::Result<()> {
        if !self.finished.swap(true, Ordering::AcqRel) {
            self.budget.finish()?;
        }
        Ok(())
    }

    fn reserve(&self, direction: EncodedIoDirection, desired: usize) -> io::Result<ReservedWindow> {
        self.check_active()?;
        if !self.started.load(Ordering::Acquire) || self.finished.load(Ordering::Acquire) {
            return Err(io::Error::other("encoded exchange is not active"));
        }
        let requested = desired.min(MAX_WINDOW);
        let reservation = self.budget.reserve(direction, requested)?;
        let capacity = reservation.capacity();
        if capacity == 0 || capacity > requested {
            reservation.settle(0)?;
            return Err(io::Error::other(
                "encoded reservation exceeds requested window",
            ));
        }
        if let Err(error) = self.check_active() {
            // No I/O has been polled for this newly admitted reservation.
            reservation.settle(0)?;
            return Err(error);
        }
        Ok(ReservedWindow {
            reservation,
            capacity,
        })
    }
}

impl Drop for Exchange {
    fn drop(&mut self) {
        // The stream's codec/request owns the last reference. A caller timeout
        // cannot release this guard while the actual codec is still running.
        let _ = self.finish();
    }
}

struct ReservedWindow {
    reservation: Box<dyn EncodedIoReservation>,
    // Capture the admitted capacity once; a pending poll reuses exactly it.
    capacity: usize,
}

pub(super) struct MeteredIo<'a, T> {
    inner: &'a mut T,
    read: Option<ReservedWindow>,
    write: Option<ReservedWindow>,
    exchange: Arc<Exchange>,
    deadline: Instant,
}

impl<'a, T> MeteredIo<'a, T> {
    pub(super) fn new(inner: &'a mut T, exchange: Arc<Exchange>, deadline: Instant) -> Self {
        Self {
            inner,
            exchange,
            read: None,
            write: None,
            deadline,
        }
    }

    fn check_active(&self) -> io::Result<()> {
        if Instant::now() >= self.deadline {
            return Err(super::request_deadline::elapsed());
        }
        self.exchange.check_active()?;
        if Instant::now() >= self.deadline {
            return Err(super::request_deadline::elapsed());
        }
        Ok(())
    }

    fn window(&mut self, direction: EncodedIoDirection, desired: usize) -> io::Result<usize> {
        self.check_active()?;
        let absent = match direction {
            EncodedIoDirection::Rx => self.read.is_none(),
            EncodedIoDirection::Tx => self.write.is_none(),
        };
        if absent {
            let reservation = self.exchange.reserve(direction, desired)?;
            if let Err(error) = self.check_active() {
                // This newly returned reservation has never polled I/O.
                reservation.reservation.settle(0)?;
                return Err(error);
            }
            let slot = match direction {
                EncodedIoDirection::Rx => &mut self.read,
                EncodedIoDirection::Tx => &mut self.write,
            };
            *slot = Some(reservation);
        }
        let slot = match direction {
            EncodedIoDirection::Rx => &self.read,
            EncodedIoDirection::Tx => &self.write,
        };
        Ok(slot
            .as_ref()
            .expect("reserved window")
            .capacity
            .min(desired))
    }

    fn settled(
        &mut self,
        direction: EncodedIoDirection,
        actual: usize,
        offered: usize,
    ) -> io::Result<usize> {
        if actual > offered {
            return Err(io::Error::other("stream exceeded encoded reservation"));
        }
        let slot = match direction {
            EncodedIoDirection::Rx => &mut self.read,
            EncodedIoDirection::Tx => &mut self.write,
        };
        slot.take()
            .expect("reserved I/O result")
            .reservation
            .settle(actual)?;
        // Settlement is allowed after expiry; its late completion cannot make
        // this read/write successful or authorize the next I/O operation.
        self.check_active()?;
        Ok(actual)
    }
}

impl<T: AsyncRead + Unpin> AsyncRead for MeteredIo<'_, T> {
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buffer: &mut [u8],
    ) -> Poll<io::Result<usize>> {
        let this = self.get_mut();
        if buffer.is_empty() {
            return Poll::Ready(Ok(0));
        }
        let count = match this.window(EncodedIoDirection::Rx, buffer.len()) {
            Ok(count) => count,
            Err(error) => return Poll::Ready(Err(error)),
        };
        match Pin::new(&mut *this.inner).poll_read(cx, &mut buffer[..count]) {
            Poll::Ready(Ok(actual)) => {
                Poll::Ready(this.settled(EncodedIoDirection::Rx, actual, count))
            }
            result => result,
        }
    }
}

impl<T: AsyncWrite + Unpin> AsyncWrite for MeteredIo<'_, T> {
    fn poll_write(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buffer: &[u8],
    ) -> Poll<io::Result<usize>> {
        let this = self.get_mut();
        if buffer.is_empty() {
            return Poll::Ready(Ok(0));
        }
        let count = match this.window(EncodedIoDirection::Tx, buffer.len()) {
            Ok(count) => count,
            Err(error) => return Poll::Ready(Err(error)),
        };
        match Pin::new(&mut *this.inner).poll_write(cx, &buffer[..count]) {
            Poll::Ready(Ok(actual)) => {
                Poll::Ready(this.settled(EncodedIoDirection::Tx, actual, count))
            }
            result => result,
        }
    }

    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        let this = self.get_mut();
        if let Err(error) = this.check_active() {
            return Poll::Ready(Err(error));
        }
        match Pin::new(&mut *this.inner).poll_flush(cx) {
            Poll::Ready(Ok(())) => Poll::Ready(this.check_active()),
            result => result,
        }
    }

    fn poll_close(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        let this = self.get_mut();
        if let Err(error) = this.check_active() {
            return Poll::Ready(Err(error));
        }
        match Pin::new(&mut *this.inner).poll_close(cx) {
            Poll::Ready(Ok(())) => Poll::Ready(this.check_active()),
            result => result,
        }
    }
}

#[cfg(test)]
#[path = "encoded_exchange_tests.rs"]
mod tests;
