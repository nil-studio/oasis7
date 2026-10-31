//! Bounded provisional page output shared by original-object writes and typed
//! closure transfers. Logical page bytes/IDs are unchanged; finish is required
//! before treating sink delivery as complete. No head/authority is granted.
use super::*;

pub const BUFFERED_PAGE_MAX_BYTES: usize = 256 * 1024;

#[must_use = "finish the buffered page writer; dropping it discards queued pages"]
pub struct BufferedPageWriter<'a> {
    store: &'a IncrementalPageStore,
    io: &'a mut PageIoBudget,
    cancelled: &'a AtomicBool,
    external: Option<&'a mut dyn FnMut() -> bool>,
    pending: Vec<([u8; 32], ReservedValue<Vec<u8>>)>,
    pending_bytes: usize,
    failed: bool,
    _metadata: oasis7_proto::memory_budget::MemoryReservation,
}

impl IncrementalPageStore {
    /// Buffer at most 256 pages / 256KiB of retained page charges in this
    /// same Ordinary pool (plus admitted metadata and one incoming page). Pending
    /// carriers and both metadata vectors remain charged until flush/drop.
    /// No IO runs in Drop. Call finish and propagate its result before using
    /// the transfer outcome; closure/authority qualification is still separate.
    pub fn buffered_page_writer<'a>(
        &'a self,
        io: &'a mut PageIoBudget,
        cancelled: &'a AtomicBool,
        external: &'a mut dyn FnMut() -> bool,
    ) -> Result<BufferedPageWriter<'a>> {
        check_cancelled(cancelled)?;
        if external() {
            return Err(PageStoreError::Cancelled);
        }
        let mut writer = BufferedPageWriter::new(self, io, cancelled)?;
        writer.external = Some(external);
        Ok(writer)
    }
}

impl<'a> BufferedPageWriter<'a> {
    pub(super) fn new(
        store: &'a IncrementalPageStore,
        io: &'a mut PageIoBudget,
        cancelled: &'a AtomicBool,
    ) -> Result<Self> {
        check_cancelled(cancelled)?;
        let metadata = store.memory.reserve(64 * 1024, MemoryClass::Ordinary)?;
        Ok(Self {
            store,
            io,
            cancelled,
            external: None,
            pending: Vec::with_capacity(pack::MAX_ENTRIES),
            pending_bytes: 0,
            failed: false,
            _metadata: metadata,
        })
    }

    fn check(&mut self) -> Result<()> {
        check_cancelled(self.cancelled)?;
        if self.external.as_mut().is_some_and(|cancelled| cancelled()) {
            return Err(PageStoreError::Cancelled);
        }
        Ok(())
    }

    /// Queued page charges, excluding the fixed metadata reservation. This is
    /// telemetry only, not a completion or durability receipt.
    pub fn pending_reserved_bytes(&self) -> usize {
        self.pending_bytes
    }

    fn healthy(&self) -> Result<()> {
        if self.failed {
            Err(PageStoreError::Corrupt("buffered page writer aborted"))
        } else {
            Ok(())
        }
    }

    /// Flush the last bounded pack and observe cancellation afterwards. A
    /// partial physical prefix on failure stays provisional, never rolled back.
    pub fn finish(mut self) -> Result<()> {
        self.flush()
    }

    pub(super) fn flush(&mut self) -> Result<()> {
        self.healthy()?;
        self.failed = true;
        let result = self.flush_inner();
        if result.is_ok() {
            self.failed = false;
        }
        result
    }

    fn flush_inner(&mut self) -> Result<()> {
        self.check()?;
        if !self.pending.is_empty() {
            {
                let refs: Vec<_> = self
                    .pending
                    .iter()
                    .map(|(id, bytes)| (*id, bytes.as_slice()))
                    .collect();
                self.store
                    .put_page_batch(&refs, MemoryClass::Ordinary, self.io, self.cancelled)?;
            }
            self.check()?;
            self.pending.clear();
            self.pending_bytes = 0;
        }
        self.check()
    }

    fn put_inner(
        &mut self,
        id: [u8; 32],
        bytes: ReservedValue<Vec<u8>>,
    ) -> std::result::Result<(), ValueTreeError> {
        self.check().map_err(value_error)?;
        if !bytes.uses_budget(&self.store.memory)
            || bytes.capacity() > bytes.reserved_bytes()
            || bytes.is_empty()
            || bytes.len() > VALUE_MAX_NODE_BYTES
            || blake3::hash(&bytes).as_bytes() != &id
        {
            return Err(ValueTreeError::Invalid(
                "object batch page accounting/hash/bound",
            ));
        }
        let charge = bytes.reserved_bytes();
        if charge > BUFFERED_PAGE_MAX_BYTES {
            return Err(ValueTreeError::ResourceLimited(
                "buffered page charge bound",
            ));
        }
        if let Some((_, old)) = self.pending.iter().find(|(old, _)| old == &id) {
            if old.as_slice() != bytes.as_slice() {
                return Err(ValueTreeError::Invalid("object batch page collision"));
            }
            self.check().map_err(value_error)?;
            return Ok(());
        }
        if self.pending.len() == pack::MAX_ENTRIES
            || self.pending_bytes + charge > BUFFERED_PAGE_MAX_BYTES
        {
            self.flush_inner().map_err(value_error)?;
        }
        self.check().map_err(value_error)?;
        self.pending_bytes += charge;
        self.pending.push((id, bytes));
        self.check().map_err(value_error)
    }
}

impl ValueNodeWriter for BufferedPageWriter<'_> {
    fn put_node(
        &mut self,
        id: [u8; 32],
        bytes: ReservedValue<Vec<u8>>,
    ) -> std::result::Result<(), ValueTreeError> {
        self.healthy().map_err(value_error)?;
        // Catching a callback unwind must not reopen an incomplete writer.
        self.failed = true;
        let result = self.put_inner(id, bytes);
        if result.is_ok() {
            self.failed = false;
        }
        result
    }
}

#[cfg(all(test, unix))]
#[path = "incremental_buffered_page_writer_tests.rs"]
mod tests;
