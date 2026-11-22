//! Idempotent durability for a real winning original, not a CAS-exists proof.
use super::*;
impl IncrementalObjectStore<'_> {
    /// Stream-verify the complete actual flat ID/length, then synchronize its
    /// winning immutable locator and containing directory through the existing
    /// publication path, and re-read the winner before returning. No whole
    /// original Vec is created; all read work uses this caller's actual IO.
    ///
    /// Object writers flush every winning ValueTree/page barrier before making
    /// a locator visible (write_object and write_object_from_stream). Thus the
    /// alias completion barrier closes a writer paused after publication but
    /// before directory fsync; it does not infer durability from readable bytes.
    /// This relies on the PageStore's existing immutable/cooperative-writer and
    /// held-kernel-lease contract. It grants no occurrence or world authority.
    pub fn sync_original_durable(
        &self,
        id: &[u8; 32],
        exact_length: u64,
        io: &mut ObjectIo<'_>,
    ) -> Result<()> {
        check_cancelled(io.cancelled)?;
        if io.class != MemoryClass::Ordinary {
            return Err(PageStoreError::Corrupt(
                "original durability requires Ordinary",
            ));
        }
        let _phase = self
            .pages
            .memory
            .reserve(WORKSPACE, MemoryClass::Ordinary)?;
        self.validate(io)?;
        let locator = self.locator(id, exact_length, io)?;
        if locator.length != exact_length {
            return Err(PageStoreError::Corrupt("original durability exact length"));
        }
        self.read_locator(id, &locator, &mut std::io::sink(), io)?;
        check_cancelled(io.cancelled)?;
        // publish_original's existing-winner branch fsyncs the actual target
        // and objects directory, then verifies the full winning original again.
        self.publish_original(*id, exact_length, locator.value_root, io)?;
        check_cancelled(io.cancelled)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::incremental_page_store::tests::{Fixture, logical_io, physical_io};
    #[test]
    fn winning_original_durability_requires_exact_length_and_remaining_io() {
        let fixture = Fixture::new();
        let memory = MemoryBudget::new(12 << 20, 3 << 20).unwrap();
        let pages =
            IncrementalPageStore::create(&fixture.store(), memory, PageCompression::Raw).unwrap();
        let cancelled = AtomicBool::new(false);
        let mut physical = physical_io();
        let mut logical = logical_io();
        let objects = IncrementalObjectStore::create(&pages, &mut physical, &cancelled).unwrap();
        let bytes = b"actual retained original durability";
        let mut io = ObjectIo {
            physical: &mut physical,
            logical: &mut logical,
            class: MemoryClass::Ordinary,
            cancelled: &cancelled,
        };
        let id = objects
            .write_object(&mut bytes.as_slice(), bytes.len() as u64, None, &mut io)
            .unwrap();
        objects
            .sync_original_durable(&id, bytes.len() as u64, &mut io)
            .unwrap();
        assert!(
            objects
                .sync_original_durable(&id, bytes.len() as u64 + 1, &mut io)
                .is_err()
        );
        io.physical.reads_remaining = 0;
        assert!(
            objects
                .sync_original_durable(&id, bytes.len() as u64, &mut io)
                .is_err()
        );
        io.class = MemoryClass::Recovery;
        assert!(
            objects
                .sync_original_durable(&id, bytes.len() as u64, &mut io)
                .is_err()
        );
        cancelled.store(true, Ordering::Release);
        io.class = MemoryClass::Ordinary;
        assert!(
            objects
                .sync_original_durable(&id, bytes.len() as u64, &mut io)
                .is_err()
        );
    }
}
