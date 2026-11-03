//! Pull access to a fully flat-hash-verified immutable original. The locator is
//! captured only after a complete verification, never trusted as range authority.
use super::*;

pub struct VerifiedOriginalCursor<'s, 'pool, 'io, 'quota> {
    store: &'s IncrementalObjectStore<'pool>,
    locator: ObjectLocator,
    position: u64,
    io: &'io mut ObjectIo<'quota>,
    _charge: MemoryReservation,
    failed: bool,
    error: Option<PageStoreError>,
}
impl<'pool> IncrementalObjectStore<'pool> {
    /// Verify the entire flat ID first, then expose bounded authenticated
    /// ranges using the same continuous IO allowances and memory class.
    /// This proves original integrity only, not an accepted occurrence.
    pub fn verified_original_cursor<'s, 'io, 'quota>(
        &'s self,
        id: &[u8; 32],
        max_length: u64,
        io: &'io mut ObjectIo<'quota>,
    ) -> Result<VerifiedOriginalCursor<'s, 'pool, 'io, 'quota>> {
        let charge = self.pages.memory.reserve(WORKSPACE, io.class)?;
        self.validate(io)?;
        let locator = self.locator(id, max_length, io)?;
        self.read_locator(id, &locator, &mut io::sink(), io)?;
        check_cancelled(io.cancelled)?;
        Ok(VerifiedOriginalCursor {
            store: self,
            locator,
            position: 0,
            io,
            _charge: charge,
            failed: false,
            error: None,
        })
    }
}
impl VerifiedOriginalCursor<'_, '_, '_, '_> {
    pub fn length(&self) -> u64 {
        self.locator.length
    }
    /// Retrieve the precise underlying failure. Taking it never clears poison.
    pub fn take_error(&mut self) -> Option<PageStoreError> {
        self.error.take()
    }
    pub fn finish(self) -> Result<()> {
        if let Some(error) = self.error {
            return Err(error);
        }
        if self.failed || self.position != self.locator.length {
            return Err(PageStoreError::Corrupt("incomplete original cursor"));
        }
        check_cancelled(self.io.cancelled)
    }
    fn read_part(&mut self, output: &mut [u8]) -> Result<usize> {
        check_cancelled(self.io.cancelled)?;
        let length = (self.locator.length - self.position)
            .min(output.len() as u64)
            .min(WORKSPACE as u64);
        if length == 0 {
            return Ok(0);
        }
        let source = ReadPageSession {
            store: self.store.pages,
            io: Mutex::new(&mut *self.io.physical),
            cancelled: self.io.cancelled,
        };
        let mut context = ValueReadContext {
            source: &source,
            memory: &self.store.pages.memory,
            class: self.io.class,
            io: self.io.logical,
        };
        let root = context
            .load_root(&self.locator.value_root)
            .map_err(from_value)?;
        if root.0.logical_len != self.locator.length {
            return Err(PageStoreError::Corrupt("original cursor root length"));
        }
        let mut target = &mut output[..length as usize];
        context
            .read_range(&root, self.position, length, &mut target, || {
                self.io.cancelled.load(Ordering::Acquire)
            })
            .map_err(from_value)?;
        if !target.is_empty() {
            return Err(PageStoreError::Corrupt("original cursor short range"));
        }
        check_cancelled(self.io.cancelled)?;
        self.position += length;
        Ok(length as usize)
    }
}
impl Read for VerifiedOriginalCursor<'_, '_, '_, '_> {
    fn read(&mut self, output: &mut [u8]) -> io::Result<usize> {
        if self.failed {
            return Err(io::Error::other("poisoned original cursor"));
        }
        // A panic during physical/source work cannot permit a later retry.
        self.failed = true;
        match self.read_part(output) {
            Ok(length) => {
                self.failed = false;
                Ok(length)
            }
            Err(error) => {
                self.error = Some(error);
                Err(io::Error::other("original cursor source failure"))
            }
        }
    }
}
