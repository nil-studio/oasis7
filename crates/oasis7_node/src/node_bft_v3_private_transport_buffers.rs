//! Existing private transport buffer limits, shared by candidate and admission
//! input consumers. Backend retained material has a separate protected ledger.
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};

const PACKAGE_BYTES: usize = 66 * 1024 * 1024;
const DIRECTION_BYTES: usize = 2 * PACKAGE_BYTES;

/// One private transport scope. Outbound and inbound are separate existing
/// 132MiB responsibilities; neither protocol receives an additional pool.
#[derive(Default)]
pub struct NodeBftPrivateTransportBuffersV1 {
    outbound: Arc<AtomicUsize>,
    inbound: Arc<AtomicUsize>,
}
impl NodeBftPrivateTransportBuffersV1 {
    pub(crate) fn outbound(&self, bytes: usize) -> Result<BufferHold, String> {
        BufferHold::reserve(self.outbound.clone(), bytes)
    }
    pub(crate) fn inbound(&self, bytes: usize) -> Result<BufferHold, String> {
        BufferHold::reserve(self.inbound.clone(), bytes)
    }
}
/// The owned reservation outlives every borrowed or shared encoded buffer.
pub(crate) struct BufferHold {
    pool: Arc<AtomicUsize>,
    bytes: usize,
}
impl BufferHold {
    fn reserve(pool: Arc<AtomicUsize>, bytes: usize) -> Result<Self, String> {
        if bytes == 0 || bytes > DIRECTION_BYTES {
            return Err("private transport buffer reservation bound".into());
        }
        pool.fetch_update(Ordering::AcqRel, Ordering::Acquire, |used| {
            used.checked_add(bytes)
                .filter(|total| *total <= DIRECTION_BYTES)
        })
        .map_err(|_| "private transport shared buffer capacity")?;
        Ok(Self { pool, bytes })
    }
    pub(crate) fn shrink(&mut self, bytes: usize) -> Result<(), String> {
        if bytes == 0 || bytes > self.bytes {
            return Err("private transport buffer cannot grow after allocation".into());
        }
        self.pool.fetch_sub(self.bytes - bytes, Ordering::AcqRel);
        self.bytes = bytes;
        Ok(())
    }
}
impl Drop for BufferHold {
    fn drop(&mut self) {
        self.pool.fetch_sub(self.bytes, Ordering::AcqRel);
    }
}

pub(crate) struct BufferedBytes {
    bytes: Vec<u8>,
    _hold: BufferHold,
}
impl BufferedBytes {
    pub(crate) fn belongs_to(&self, buffers: &NodeBftPrivateTransportBuffersV1) -> bool {
        Arc::ptr_eq(&self._hold.pool, &buffers.outbound)
    }
    pub(crate) fn new(bytes: Vec<u8>, mut hold: BufferHold) -> Result<Self, String> {
        hold.shrink(bytes.len())?;
        Ok(Self { bytes, _hold: hold })
    }
    #[cfg(test)]
    pub(crate) fn fixture(bytes: Vec<u8>) -> Self {
        let buffers = NodeBftPrivateTransportBuffersV1::default();
        let hold = buffers.outbound(bytes.len()).unwrap();
        Self::new(bytes, hold).unwrap()
    }
}
impl std::ops::Deref for BufferedBytes {
    type Target = Vec<u8>;
    fn deref(&self) -> &Self::Target {
        &self.bytes
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn private_protocols_share_outbound_ownership_until_last_buffer_drops() {
        let pool = NodeBftPrivateTransportBuffersV1::default();
        let candidate =
            Arc::new(BufferedBytes::new(vec![1], pool.outbound(DIRECTION_BYTES).unwrap()).unwrap());
        let serving = candidate.clone();
        let other = pool.outbound(DIRECTION_BYTES - 1).unwrap();
        assert!(pool.outbound(1).is_err());
        drop(candidate);
        assert!(pool.outbound(1).is_err());
        drop(serving);
        let admission = pool.outbound(1).unwrap();
        assert!(pool.outbound(1).is_err());
        drop(admission);
        drop(other);
        assert!(pool.outbound(DIRECTION_BYTES).is_ok());
    }
    #[test]
    fn inbound_decode_peak_and_outbound_are_distinct_existing_bounds() {
        let pool = NodeBftPrivateTransportBuffersV1::default();
        let inbound = pool.inbound(DIRECTION_BYTES).unwrap();
        let outbound = pool.outbound(DIRECTION_BYTES).unwrap();
        assert!(pool.inbound(1).is_err());
        assert!(pool.outbound(1).is_err());
        drop(inbound);
        assert!(pool.inbound(DIRECTION_BYTES).is_ok());
        drop(outbound);
    }
}
