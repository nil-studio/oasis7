//! Concurrent transport ownership, separate from non-refundable wire spending.
use super::{Libp2pNetwork, WorldError, authenticated_requests::rejected};
use std::sync::Arc;
use tokio::sync::{OwnedSemaphorePermit, Semaphore};

/// Probe one byte beyond the encoded cap; reject before the codec decodes it.
pub(super) struct ResponseReader<'a, T> {
    pub(super) inner: &'a mut T,
    pub(super) remaining: Option<usize>,
}
impl<T: futures::AsyncRead + Unpin> futures::AsyncRead for ResponseReader<'_, T> {
    fn poll_read(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        buf: &mut [u8],
    ) -> std::task::Poll<std::io::Result<usize>> {
        let this = self.get_mut();
        let size = this
            .remaining
            .map_or(buf.len(), |left| buf.len().min(left + 1));
        let count =
            futures::ready!(std::pin::Pin::new(&mut *this.inner).poll_read(cx, &mut buf[..size]))?;
        if let Some(left) = &mut this.remaining {
            if count > *left {
                return std::task::Poll::Ready(Err(std::io::Error::other(
                    "held response encoded bound",
                )));
            }
            *left -= count;
        }
        std::task::Poll::Ready(Ok(count))
    }
}

pub const TRANSPORT_MEMORY_DIRECTION_BYTES: usize = 132 * 1024 * 1024;
pub(super) const RPC_RESPONSE_ENCODED_MAX: usize = 2 * 64 * 1024 + 128;
pub(super) const RPC_INBOUND_BYTES: usize = 2 * 1024 * 1024;
pub(super) const RPC_OUTBOUND_BYTES: usize = 1024 * 1024;

#[derive(Clone, Copy, Debug)]
pub enum TransportMemoryDirection {
    Inbound,
    Outbound,
}

/// Opaque ownership only: no read permission, byte spend or reset operation.
#[derive(Debug)]
pub struct TransportMemoryHold {
    _permit: OwnedSemaphorePermit,
}
pub(super) type RpcHold = [TransportMemoryHold; 2];

#[derive(Debug)]
pub(super) struct MemoryPool([Arc<Semaphore>; 2]);
impl Default for MemoryPool {
    fn default() -> Self {
        Self(std::array::from_fn(|_| {
            Arc::new(Semaphore::new(TRANSPORT_MEMORY_DIRECTION_BYTES))
        }))
    }
}
impl MemoryPool {
    fn reserve(
        &self,
        direction: TransportMemoryDirection,
        bytes: usize,
    ) -> Result<TransportMemoryHold, WorldError> {
        let bytes = u32::try_from(bytes).map_err(|_| rejected("transport memory size overflow"))?;
        let permit = Arc::clone(&self.0[direction as usize])
            .try_acquire_many_owned(bytes)
            .map_err(|_| WorldError::NetworkRequestFailed {
                code: oasis7_proto::distributed::DistributedErrorCode::ErrBusy,
                message: "transport memory capacity exhausted".into(),
                retryable: true,
            })?;
        Ok(TransportMemoryHold { _permit: permit })
    }
    pub(super) fn rpc(&self) -> Result<Arc<RpcHold>, WorldError> {
        Ok(Arc::new([
            self.reserve(TransportMemoryDirection::Inbound, RPC_INBOUND_BYTES)?,
            self.reserve(TransportMemoryDirection::Outbound, RPC_OUTBOUND_BYTES)?,
        ]))
    }
    pub(super) fn used(&self) -> [usize; 2] {
        std::array::from_fn(|i| TRANSPORT_MEMORY_DIRECTION_BYTES - self.0[i].available_permits())
    }
}

/// Borrowed access keeps the same RPC reservation through caller consumption.
pub struct HeldBytes {
    // Declaration order ensures payload storage is freed before its permits.
    pub(super) payload: Vec<u8>,
    pub(super) memory: Option<Arc<RpcHold>>,
}
impl HeldBytes {
    pub fn as_slice(&self) -> &[u8] {
        &self.payload
    }
    pub(super) fn unheld(payload: Vec<u8>) -> Self {
        Self {
            payload,
            memory: None,
        }
    }
    pub(super) fn into_unheld(self) -> Result<Vec<u8>, WorldError> {
        if self.memory.is_some() {
            return Err(rejected("held response requires its resource owner"));
        }
        Ok(self.payload)
    }
}
impl std::fmt::Debug for HeldBytes {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("HeldBytes")
            .field("len", &self.payload.len())
            .finish_non_exhaustive()
    }
}

impl Libp2pNetwork {
    pub fn retain_transport_memory(
        &self,
        direction: TransportMemoryDirection,
        bytes: usize,
    ) -> Result<TransportMemoryHold, WorldError> {
        self.memory_pool.reserve(direction, bytes)
    }
    /// Current reserved bytes, [inbound, outbound]; directions are independent.
    pub fn transport_memory_usage(&self) -> [usize; 2] {
        self.memory_pool.used()
    }
}
