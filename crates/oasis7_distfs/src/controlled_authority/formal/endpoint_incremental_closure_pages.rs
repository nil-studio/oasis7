//! Real PageStore readers whose successful reads also synchronize the winning
//! representation. No schema is inferred from a generic RecordRoot payload.
use super::*;
use crate::incremental_page_store::{
    IncrementalPageStore, PageCompression, PageIoBudget, PageStoreError, StatePageSession,
    ValuePageSession,
};
use oasis7_formal_protocol::{
    incremental_state::MerkleNodeReader,
    incremental_value::{ValueNodeReader, ValueTreeError},
};
use oasis7_proto::memory_budget::{MemoryBudget, MemoryClass, ReservedValue};
use std::sync::{Arc, Mutex, atomic::AtomicBool};
struct Publisher<'a> {
    pages: &'a IncrementalPageStore,
    io: Mutex<PageIoBudget>,
    cancelled: &'a AtomicBool,
    error: Mutex<Option<PageStoreError>>,
    failed: AtomicBool,
    synced: Mutex<std::collections::BTreeSet<[u8; 32]>>,
    memo_steps: Mutex<u64>,
    max_synced: usize,
    _memo_charge: oasis7_proto::memory_budget::MemoryReservation,
}
impl Publisher<'_> {
    fn already_synced(&self, id: &[u8; 32]) -> Result<bool, ValueTreeError> {
        if self.failed.load(std::sync::atomic::Ordering::Acquire) {
            return Err(ValueTreeError::Invalid("closure reader poisoned"));
        }
        let height = usize::BITS - self.max_synced.leading_zeros();
        let operation = u64::from(height.max(1) + 1) * 16;
        let mut steps = self
            .memo_steps
            .lock()
            .map_err(|_| ValueTreeError::Invalid("closure memo work lock"))?;
        *steps = steps
            .checked_sub(operation)
            .ok_or(ValueTreeError::ResourceLimited("closure memo work"))?;
        drop(steps);
        Ok(self
            .synced
            .lock()
            .map_err(|_| ValueTreeError::Invalid("closure memo lock"))?
            .contains(id))
    }
    fn put(&self, id: [u8; 32], bytes: &ReservedValue<Vec<u8>>) -> Result<(), ValueTreeError> {
        if self.failed.load(std::sync::atomic::Ordering::Acquire) {
            return Err(ValueTreeError::Invalid("closure reader poisoned"));
        }
        let result = self.put_inner(id, bytes);
        if result.is_err() {
            self.failed
                .store(true, std::sync::atomic::Ordering::Release);
        }
        result
    }
    fn put_inner(
        &self,
        id: [u8; 32],
        bytes: &ReservedValue<Vec<u8>>,
    ) -> Result<(), ValueTreeError> {
        let mut synced = self
            .synced
            .lock()
            .map_err(|_| ValueTreeError::Invalid("closure memo lock"))?;
        // Same conservative node bound as the existing StateReadMemo (one
        // 2048B node/entry plus split scratch), with a continuous operation
        // allowance. A BTree node has fewer than 16 key slots; its height is
        // below ceil(log2(max_entries+1))+1. Charge search and insertion/split.
        let height = usize::BITS - self.max_synced.leading_zeros();
        let operation = u64::from(height.max(1) + 1) * 16;
        let mut steps = self
            .memo_steps
            .lock()
            .map_err(|_| ValueTreeError::Invalid("closure memo work lock"))?;
        *steps = steps
            .checked_sub(operation)
            .ok_or(ValueTreeError::ResourceLimited("closure memo work"))?;
        if synced.contains(&id) {
            return Ok(());
        }
        *steps = steps
            .checked_sub(
                operation
                    .checked_mul(2)
                    .ok_or(ValueTreeError::ResourceLimited(
                        "closure memo insertion work",
                    ))?,
            )
            .ok_or(ValueTreeError::ResourceLimited(
                "closure memo insertion work",
            ))?;
        if synced.len() >= self.max_synced {
            return Err(ValueTreeError::ResourceLimited(
                "closure unique synchronized pages",
            ));
        }
        let result = self
            .io
            .lock()
            .map_err(|_| PageStoreError::Corrupt("closure publish lock"))
            .and_then(|mut io| self.pages.put_page(id, bytes, &mut io, self.cancelled));
        result.map_err(|error| {
            if let Ok(mut slot) = self.error.lock()
                && slot.is_none()
            {
                *slot = Some(error);
            }
            ValueTreeError::Invalid("physical closure page synchronization failed")
        })?;
        synced.insert(id);
        Ok(())
    }
}
pub(crate) struct DurableState<'a> {
    inner: StatePageSession<'a>,
    publisher: Arc<ReservedValue<Publisher<'a>>>,
}
impl DurableState<'_> {
    pub(crate) fn take_read_error(&self) -> Option<PageStoreError> {
        self.publisher
            .error
            .lock()
            .ok()
            .and_then(|mut e| e.take())
            .or_else(|| self.inner.take_read_error())
    }
}
impl MerkleNodeReader for DurableState<'_> {
    type Node = <StatePageSession<'static> as MerkleNodeReader>::Node;
    fn allocation_class(&self) -> Option<MemoryClass> {
        Some(MemoryClass::Ordinary)
    }
    fn allocation_budget(&self) -> Option<&MemoryBudget> {
        Some(self.publisher.pages.memory_budget())
    }
    fn read_node(&self, digest: &str, max_bytes: usize) -> Result<Option<Self::Node>, String> {
        let result = (|| {
            let page =
                MerkleNodeReader::read_node(&self.inner, digest, max_bytes).inspect_err(|_| {
                    self.publisher
                        .failed
                        .store(true, std::sync::atomic::Ordering::Release);
                })?;
            if let Some(page) = &page {
                let mut id = [0; 32];
                hex::decode_to_slice(digest, &mut id).map_err(|_| "closure state ID")?;
                if !self
                    .publisher
                    .already_synced(&id)
                    .map_err(|e| e.to_string())?
                {
                    let raw: &[u8] = page.as_ref();
                    let memory = self.publisher.pages.memory_budget();
                    let fee = raw
                        .len()
                        .checked_add(128)
                        .ok_or("closure page admission overflow")?;
                    let charge = memory
                        .reserve(fee, MemoryClass::Ordinary)
                        .map_err(|e| e.to_string())?;
                    let bytes = charge.retain(raw.to_vec());
                    self.publisher.put(id, &bytes).map_err(|e| e.to_string())?;
                }
            }
            Ok(page)
        })();
        if result.is_err() {
            self.publisher
                .failed
                .store(true, std::sync::atomic::Ordering::Release);
        }
        result
    }
}
pub(crate) struct DurableValues<'a> {
    inner: ValuePageSession<'a>,
    publisher: Arc<ReservedValue<Publisher<'a>>>,
}
impl ValueNodeReader for DurableValues<'_> {
    fn read_node(
        &self,
        id: &[u8; 32],
        max_bytes: usize,
        memory: &MemoryBudget,
        class: MemoryClass,
    ) -> Result<Arc<ReservedValue<Vec<u8>>>, ValueTreeError> {
        let result = (|| {
            if class != MemoryClass::Ordinary
                || !memory.same_pool(self.publisher.pages.memory_budget())
            {
                return Err(ValueTreeError::Invalid("closure value class/pool"));
            }
            let bytes = ValueNodeReader::read_node(&self.inner, id, max_bytes, memory, class)
                .inspect_err(|_| {
                    self.publisher
                        .failed
                        .store(true, std::sync::atomic::Ordering::Release);
                })?;
            self.publisher.put(*id, &bytes)?;
            Ok(bytes)
        })();
        if result.is_err() {
            self.publisher
                .failed
                .store(true, std::sync::atomic::Ordering::Release);
        }
        result
    }
}
struct StoreLease {
    _pages: IncrementalPageStore,
    identity: (u64, u64),
}
/// Owns real sessions from this one store. Its private fields cannot be filled
/// with another reader or a caller-reported store identifier.
pub(crate) struct StoreSessions<'a> {
    pub(crate) state: DurableState<'a>,
    pub(crate) values: DurableValues<'a>,
    pin: Arc<ReservedValue<StoreLease>>,
}
impl<'a> StoreSessions<'a> {
    pub(crate) fn new(
        pages: &'a IncrementalPageStore,
        state_io: PageIoBudget,
        value_io: PageIoBudget,
        publish_io: PageIoBudget,
        max_synced_pages: usize,
        memo_steps: u64,
        cancelled: &'a AtomicBool,
    ) -> Result<Self, FormalError> {
        let memory = pages.memory_budget();
        let memo_bytes = max_synced_pages
            .checked_mul(2048)
            .and_then(|n| n.checked_add(4096))
            .ok_or_else(|| invalid("closure memo capacity overflow"))?;
        if max_synced_pages == 0 {
            return Err(invalid("closure memo capacity is zero"));
        }
        let memo_charge = memory
            .reserve(memo_bytes, MemoryClass::Ordinary)
            .map_err(|_| invalid("closure memo admission"))?;
        let synced = std::collections::BTreeSet::new();
        let publisher_charge = memory
            .reserve(
                std::mem::size_of::<Publisher>() + 128,
                MemoryClass::Ordinary,
            )
            .map_err(|_| invalid("closure publisher admission"))?;
        let pin_charge = memory
            .reserve(
                std::mem::size_of::<StoreLease>() + 128,
                MemoryClass::Ordinary,
            )
            .map_err(|_| invalid("closure live lease admission"))?;
        let owned = IncrementalPageStore::open(
            pages.root(),
            memory.clone(),
            PageCompression::Raw,
            MemoryClass::Ordinary,
        )
        .map_err(page_store_formal_error)?;
        let identity = owned
            .opened_lease_identity()
            .map_err(page_store_formal_error)?;
        let pin = Arc::new(pin_charge.retain(StoreLease {
            _pages: owned,
            identity,
        }));
        let publisher = Arc::new(publisher_charge.retain(Publisher {
            pages,
            io: Mutex::new(publish_io),
            cancelled,
            error: Mutex::new(None),
            failed: AtomicBool::new(false),
            synced: Mutex::new(synced),
            memo_steps: Mutex::new(memo_steps),
            max_synced: max_synced_pages,
            _memo_charge: memo_charge,
        }));
        let state = DurableState {
            inner: pages
                .state_session_with_cancelled(state_io, MemoryClass::Ordinary, cancelled)
                .map_err(page_store_formal_error)?,
            publisher: publisher.clone(),
        };
        let values = DurableValues {
            inner: pages.value_session_with_cancelled(value_io, cancelled),
            publisher,
        };
        Ok(Self { state, values, pin })
    }
    /// Optional existing bounded positive read memo. Its source remains this
    /// exact store, and publisher still checks its own finite durability memo.
    pub(crate) fn with_state_read_memo(
        mut self,
        max_entries: usize,
        max_retained_bytes: usize,
    ) -> Result<Self, FormalError> {
        self.state.inner = self
            .state
            .inner
            .with_read_memo(max_entries, max_retained_bytes)
            .map_err(page_store_formal_error)?;
        Ok(self)
    }
    /// Must be called only by the private complete consumer after its real
    /// Current and both History passes used these exact state/value sessions.
    /// It seals read/sync provenance, not a business/ABI execution permission.
    pub(crate) fn seal(
        &self,
        core: &ReservedValue<CheckedCommitCore>,
        source: &ReservedValue<
            oasis7_world_state::incremental_world_storage_source::CheckedWorldStorageSourceV1,
        >,
        parent: Option<&ReservedValue<StoreReadWitness>>,
        cancelled: &mut impl FnMut() -> bool,
    ) -> Result<ReservedValue<StoreReadWitness>, FormalError> {
        if cancelled() {
            return Err(invalid("closure final cancellation"));
        }
        if let Some(error) = self.state.take_read_error() {
            return Err(page_store_formal_error(error));
        }
        if self
            .state
            .publisher
            .failed
            .load(std::sync::atomic::Ordering::Acquire)
        {
            return Err(invalid("closure failed reader cannot seal"));
        }
        let memory = self.pin._pages.memory_budget();
        if !core.uses_budget(memory)
            || core.reserved_bytes() < std::mem::size_of::<CheckedCommitCore>()
            || !source.uses_budget(memory)
            || source.reserved_bytes() < std::mem::size_of_val(&**source)
            || source.current().allocation_class() != MemoryClass::Ordinary
        {
            return Err(invalid("closure source pool"));
        }
        let record = core.candidate_record_id();
        // These fixed fields were authenticated together by verify_commit_core;
        // do not re-encode RootSet/CommitBody merely to attach the reader.
        let body = core.catalog().entry.commit;
        let roots = core.body().roots;
        if source.cognition().candidate_record_id() != record
            || source.cognition().body_id() != body
            || source.cognition().roots_id() != roots
        {
            return Err(invalid("closure exact checked source"));
        }
        match (core.parent_record_id(), parent) {
            (None, None) if core.roots().height == 0 => {}
            (Some(id), Some(parent))
                if parent.uses_budget(memory)
                    && parent.reserved_bytes() >= std::mem::size_of::<StoreReadWitness>() + 128
                    && parent.record == id
                    && parent.pin.identity() == self.pin.identity
                    && core.body().parent_commit == Some(parent.body)
                    && core.parent_body().map(|body| body.roots) == Some(parent.roots) => {}
            _ => return Err(invalid("closure exact parent live store")),
        }
        let charge = memory
            .reserve(
                std::mem::size_of::<StoreReadWitness>() + 128,
                MemoryClass::Ordinary,
            )
            .map_err(|_| invalid("closure witness admission"))?;
        let pin = self.pin.clone();
        if cancelled() {
            return Err(invalid("closure witness final cancellation"));
        }
        Ok(charge.retain(StoreReadWitness {
            record,
            body,
            roots,
            pin: StoreWitnessLease::Live(pin),
        }))
    }
}
/// Private receipt-path attachment. Its live lease pins all inherited page
/// recipes; fixed fingerprints are genuine checked Core data, not public hints.
type AuthenticatedLocalClosureV1 = super::super::local_closure::AuthenticatedLocalClosureV1;
enum StoreWitnessLease {
    Live(Arc<ReservedValue<StoreLease>>),
    Cold(ReservedValue<AuthenticatedLocalClosureV1>),
}
impl StoreWitnessLease {
    fn identity(&self) -> (u64, u64) {
        match self {
            Self::Live(pin) => pin.identity,
            Self::Cold(token) => token.page_identity(),
        }
    }
}
pub(crate) struct StoreReadWitness {
    record: [u8; 32],
    body: [u8; 32],
    roots: [u8; 32],
    pin: StoreWitnessLease,
}

impl StoreReadWitness {
    pub(crate) fn candidate_record_id(&self) -> [u8; 32] {
        self.record
    }
    pub(crate) fn body_id(&self) -> [u8; 32] {
        self.body
    }
    pub(crate) fn roots_id(&self) -> [u8; 32] {
        self.roots
    }
    pub(crate) fn page_namespace_identity(&self) -> (u64, u64) {
        self.pin.identity()
    }
}

/// The source is minted only by the actual protected cold reader after its
/// fixed record, exact sources, opened namespaces and maintenance check. Move
/// it intact so its page/native leases and Ordinary fees remain live.
pub(crate) fn restore_store_read_witness_v1(
    token: ReservedValue<AuthenticatedLocalClosureV1>,
    memory: &MemoryBudget,
    cancelled: &mut impl FnMut() -> bool,
) -> Result<ReservedValue<StoreReadWitness>, FormalError> {
    if cancelled() {
        return Err(invalid("cold store witness cancelled"));
    }
    if !token.uses_budget(memory)
        || token.reserved_bytes() < std::mem::size_of::<AuthenticatedLocalClosureV1>() + 512
        || token.allocation_class() != MemoryClass::Ordinary
        || !token.pages().memory_budget().same_pool(memory)
    {
        return Err(invalid("cold store witness exact Ordinary admission"));
    }
    let charge = memory
        .reserve(
            std::mem::size_of::<StoreReadWitness>() + 128,
            MemoryClass::Ordinary,
        )
        .map_err(|_| invalid("cold store witness admission"))?;
    if token
        .pages()
        .opened_lease_identity()
        .map_err(page_store_formal_error)?
        != token.page_identity()
    {
        return Err(invalid("cold store witness actual namespace identity"));
    }
    let (record, body, roots) = (token.record_id(), token.body_id(), token.roots_id());
    if cancelled() {
        return Err(invalid("cold store witness final cancellation"));
    }
    Ok(charge.retain(StoreReadWitness {
        record,
        body,
        roots,
        pin: StoreWitnessLease::Cold(token),
    }))
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    struct Directory(std::path::PathBuf);
    impl Drop for Directory {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
    fn io() -> PageIoBudget {
        PageIoBudget {
            reads_remaining: 10000,
            read_bytes_remaining: 64 << 20,
            writes_remaining: 10000,
            write_bytes_remaining: 64 << 20,
        }
    }
    #[test]
    fn real_store_sync_memo_does_not_skip_reads_and_resource_failure_poison_is_sticky() {
        let directory = Directory(std::env::temp_dir().join(format!(
            "oasis7-closure-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        )));
        let memory = MemoryBudget::new(128 << 20, 8 << 20).unwrap();
        let pages =
            IncrementalPageStore::create(&directory.0, memory.clone(), PageCompression::Raw)
                .unwrap();
        let stop = AtomicBool::new(false);
        let bytes = memory
            .reserve(256, MemoryClass::Ordinary)
            .unwrap()
            .retain(b"actual durable closure page".to_vec());
        let id = *blake3::hash(&bytes).as_bytes();
        pages.put_page(id, &bytes, &mut io(), &stop).unwrap();
        let sessions = StoreSessions::new(&pages, io(), io(), io(), 8, 10000, &stop).unwrap();
        ValueNodeReader::read_node(&sessions.values, &id, 1024, &memory, MemoryClass::Ordinary)
            .unwrap();
        let first = sessions.values.publisher.io.lock().unwrap().reads_remaining;
        ValueNodeReader::read_node(&sessions.values, &id, 1024, &memory, MemoryClass::Ordinary)
            .unwrap();
        assert_eq!(
            sessions.values.publisher.io.lock().unwrap().reads_remaining,
            first,
            "memo skips repeat durability work only"
        );
        assert_eq!(sessions.values.publisher.synced.lock().unwrap().len(), 1);
        let limited = StoreSessions::new(&pages, io(), io(), io(), 8, 0, &stop).unwrap();
        assert!(matches!(
            ValueNodeReader::read_node(&limited.values, &id, 1024, &memory, MemoryClass::Ordinary),
            Err(ValueTreeError::ResourceLimited(_))
        ));
        *limited.values.publisher.memo_steps.lock().unwrap() = 10000;
        assert!(
            ValueNodeReader::read_node(&limited.values, &id, 1024, &memory, MemoryClass::Ordinary)
                .is_err(),
            "refilling work cannot clear the poisoned attempt"
        );
        // A live positive durability memo cannot replace actual source reads.
        std::fs::rename(directory.0.join("lease"), directory.0.join("lease-old")).unwrap();
        std::fs::File::create(directory.0.join("lease")).unwrap();
        assert!(
            ValueNodeReader::read_node(&sessions.values, &id, 1024, &memory, MemoryClass::Ordinary)
                .is_err()
        );
        assert!(
            sessions
                .values
                .publisher
                .failed
                .load(std::sync::atomic::Ordering::Acquire)
        );
    }
}
