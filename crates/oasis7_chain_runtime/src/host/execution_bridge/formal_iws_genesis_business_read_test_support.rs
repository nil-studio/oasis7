//! Paid test readers for originals and business nodes in real endpoint stores.
//! The original IO accounts remain borrowed from the same Common child; these
//! readers confer no acceptance or execution authority.
use super::{IncrementalObjectStore, MemoryClass, ObjectIo, RefCell, ReservedValue};
use oasis7_distfs::incremental_page_store::PageStoreError;
use oasis7_formal_protocol::controlled_authority::formal::{
    FormalError,
    bounded_context::{FormalBoundedContextV1, WorkIdentityV1},
    business_index::{BUSINESS_INDEX_MAX_LEAF_BYTES, PaidBusinessIndexNodeReaderV1},
};
use oasis7_proto::memory_budget::MemoryReservation;
use std::{cell::Cell, mem::size_of};

#[derive(Debug)]
pub(super) enum MeterError {
    Context(FormalError),
    Store(PageStoreError),
}

fn invalid(note: &'static str) -> FormalError {
    FormalError::Invalid(note.into())
}

fn counters(io: &ObjectIo<'_>) -> [u64; 7] {
    [
        io.physical.reads_remaining,
        io.physical.writes_remaining,
        io.logical.nodes_remaining,
        io.physical.read_bytes_remaining,
        io.physical.write_bytes_remaining,
        io.logical.read_bytes_remaining,
        io.logical.write_bytes_remaining,
    ]
}

fn install(io: &mut ObjectIo<'_>, v: [u64; 7]) {
    io.physical.reads_remaining = v[0];
    io.physical.writes_remaining = v[1];
    io.logical.nodes_remaining = v[2];
    io.physical.read_bytes_remaining = v[3];
    io.physical.write_bytes_remaining = v[4];
    io.logical.read_bytes_remaining = v[5];
    io.logical.write_bytes_remaining = v[6];
}

// Loan the ORIGINAL accounts under one exclusive child. Never replace them
// with a fresh allowance, and never refund any lane's actual consumption.
pub(super) fn metered<T>(
    common: &mut FormalBoundedContextV1<'_>,
    io: &mut ObjectIo<'_>,
    run: impl FnOnce(&mut ObjectIo<'_>) -> Result<T, PageStoreError>,
) -> Result<T, MeterError> {
    use MeterError::{Context, Store};
    common.charge(56, 4096).map_err(Context)?;
    let _frame = common
        .memory()
        .reserve(4096, MemoryClass::Ordinary)
        .map_err(|e| Store(PageStoreError::Memory(e)))?;
    let before = counters(io);
    let lease = common.work().begin_child().map_err(Context)?;
    let steps = lease.remaining_steps() / 3;
    let bytes = lease.remaining_read_bytes() / 4;
    let capped = std::array::from_fn(|i| before[i].min(if i < 3 { steps } else { bytes }));
    install(io, capped);
    let result = match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| run(io))) {
        Ok(result) => result,
        Err(payload) => {
            install(io, [0; 7]);
            drop(lease);
            std::panic::resume_unwind(payload)
        }
    };
    let after = counters(io);
    if (0..7).any(|i| after[i] > capped[i]) {
        install(io, [0; 7]);
        drop(lease);
        return Err(Store(PageStoreError::Corrupt(
            "GEN original lane increased",
        )));
    }
    let used: [u64; 7] = std::array::from_fn(|i| capped[i] - after[i]);
    install(io, std::array::from_fn(|i| before[i] - used[i]));
    let debit = lease
        .consume(used[..3].iter().sum(), used[3..].iter().sum())
        .map_err(Context);
    lease.finish();
    match result {
        Err(error) => Err(Store(error)),
        Ok(value) => debit.map(|()| value),
    }
}

pub(super) struct NoPriorNodes;

impl PaidBusinessIndexNodeReaderV1 for NoPriorNodes {
    fn read_node_bounded(
        &self,
        digest: &str,
        common: &mut FormalBoundedContextV1<'_>,
    ) -> Result<Option<ReservedValue<Vec<u8>>>, FormalError> {
        common.charge(1, digest.len() as u64)?;
        Err(invalid(
            "fresh GEN unexpectedly requested a prior business node",
        ))
    }
}

// Read index nodes from the actual endpoint object store, not delta.node_writes.
pub(super) struct ActualNodes<'a, 'pages, 'io> {
    pub(super) objects: &'a IncrementalObjectStore<'pages>,
    pub(super) io: RefCell<ObjectIo<'io>>,
    pub(super) core: WorkIdentityV1,
    pub(super) fee: MemoryReservation,
    pub(super) reads: Cell<u64>,
    pub(super) first: RefCell<Option<PageStoreError>>,
}

impl PaidBusinessIndexNodeReaderV1 for ActualNodes<'_, '_, '_> {
    fn read_node_bounded(
        &self,
        digest: &str,
        common: &mut FormalBoundedContextV1<'_>,
    ) -> Result<Option<ReservedValue<Vec<u8>>>, FormalError> {
        common.charge(1, digest.len() as u64)?;
        if self.core != common.work().identity()
            || !self.fee.uses_budget(common.memory())
            || !self.objects.uses_budget(common.memory())
            || self.first.borrow().is_some()
        {
            return Err(invalid("GEN node reader context or prior failure"));
        }
        if digest.len() != 64
            || !digest
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        {
            return Err(invalid("GEN node digest"));
        }
        let mut id = [0; 32];
        hex::decode_to_slice(digest, &mut id).map_err(|_| invalid("GEN node digest"))?;
        let mut io = self
            .io
            .try_borrow_mut()
            .map_err(|_| invalid("GEN node reader reentry"))?;
        self.reads.set(self.reads.get() + 1);
        let raw = match metered(common, &mut io, |io| {
            self.objects
                .read_object_owned(&id, BUSINESS_INDEX_MAX_LEAF_BYTES + 128, io)
        }) {
            Ok(raw) => raw,
            Err(MeterError::Context(error)) => return Err(error),
            Err(MeterError::Store(PageStoreError::Missing(missing))) if missing == id => {
                return Ok(None);
            }
            Err(MeterError::Store(error)) => {
                *self.first.borrow_mut() = Some(error);
                return Err(invalid("GEN node physical read failed"));
            }
        };
        drop(io);
        if !raw.uses_budget(common.memory()) {
            return Err(invalid("GEN node raw pool"));
        }
        // Keep the Arc's complete original owner until the separately admitted
        // exact-capacity copy is complete.
        let n = raw.len();
        common.charge(1, (n + size_of::<Vec<u8>>()) as u64)?;
        let fee = common
            .memory()
            .reserve(n + size_of::<Vec<u8>>(), MemoryClass::Ordinary)
            .map_err(|_| invalid("GEN node output admission"))?;
        let mut copied = Vec::new();
        copied
            .try_reserve_exact(n)
            .map_err(|_| invalid("GEN node output allocation"))?;
        if copied.capacity() != n {
            return Err(invalid("GEN node output capacity"));
        }
        for chunk in raw.chunks(8192) {
            common.charge(1, chunk.len() as u64)?;
            copied.extend_from_slice(chunk);
        }
        Ok(Some(fee.retain(copied)))
    }
}
