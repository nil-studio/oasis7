//! Per-operation accounting at the complete encoded application message boundary.
//!
//! The Libp2pNetwork consumer measures the pinned CBOR codec's complete
//! `NetworkRequest { protocol, payload }` and `NetworkResponse { payload }`.
//! Any application prefix in the payload is included once. The existing codec
//! uses EOF as its message boundary and does not add a length word.
//!
//! Implementations retain the original operation, owner and attempt identities.
//! This interface does not supply a ledger, read authority, or protected storage.
use std::{io, time::Instant};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EncodedIoDirection {
    Tx,
    Rx,
}

/// A uniquely owned reservation, durably admitted before the corresponding I/O.
/// Dropping an unsettled reservation MUST retain its charge and unknown progress.
/// It must never refund the reservation or manufacture an actual byte count.
pub trait EncodedIoReservation: Send {
    /// Positive permitted bytes, at most the requested window and 64 KiB.
    fn capacity(&self) -> usize;

    /// Settle a confirmed result no larger than capacity, including a short
    /// read/write or a known zero. A pending poll has no confirmed result.
    /// An unsuccessful durable settlement must preserve the unresolved charge.
    fn settle(self: Box<Self>, actual_bytes: usize) -> io::Result<()>;
}

/// One exchange within an already authorized original operation and attempt.
/// A separate exchange may share that operation's persistent cumulative ledger.
pub trait EncodedExchangeBudget: Send + Sync {
    /// The original absolute deadline. Net captures it once and never extends it.
    fn deadline(&self) -> Instant;

    /// Recheck the bound operation and owner's current validity, including any
    /// independent cancellation, without allocating credit or renewing identity.
    /// Net calls this before every actual stream poll, even when that poll reuses
    /// a pending reservation, and after durable callbacks. Implementations that
    /// support cancellation must make this read observe it. The original fixed
    /// deadline remains the bound on a pending stream that has not been woken.
    fn check_active(&self) -> io::Result<()>;

    /// Admit exactly one exchange using the actual codec's complete request size.
    /// This may reserve response credit, but cannot create a new operation or
    /// renew its attempt. Failure must preserve any ambiguous durable admission.
    fn start(&self, encoded_request_bytes: usize) -> io::Result<()>;

    /// Durably reserve a positive window before polling the actual stream.
    fn reserve(
        &self,
        direction: EncodedIoDirection,
        max_bytes: usize,
    ) -> io::Result<Box<dyn EncodedIoReservation>>;

    /// The codec has stopped using this exchange, including on error or cancel.
    /// This can precede `start` if dispatch failed. Unresolved reservations stay
    /// charged; they cannot be refunded merely because the stream has stopped.
    /// A failed finish must leave the original ledger blocked or poisoned.
    fn finish(&self) -> io::Result<()>;
}
