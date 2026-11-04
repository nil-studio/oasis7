//! Charged page-backed sources for the actual IWS transaction and catalog
//! producers. Content publication here never selects or authorizes a head.
use super::*;
use oasis7_formal_protocol::{
    incremental_execution::PreparedTransition,
    incremental_history::CatalogDeltaV1,
    incremental_state::{MerkleNodeReader, STATE_MAX_PAGE_BYTES},
};

pub struct ChargedMerklePage(Arc<ReservedValue<Vec<u8>>>);
impl AsRef<[u8]> for ChargedMerklePage {
    fn as_ref(&self) -> &[u8] {
        self.0.as_slice()
    }
}

pub struct StatePageSession<'a> {
    store: &'a IncrementalPageStore,
    io: Mutex<PageIoBudget>,
    first_read_error: Mutex<Option<PageStoreError>>,
    class: MemoryClass,
    pub cancelled: AtomicBool,
    caller_cancelled: Option<&'a AtomicBool>,
    _charge: MemoryReservation,
}
impl IncrementalPageStore {
    pub fn state_session(
        &self,
        io: PageIoBudget,
        class: MemoryClass,
    ) -> Result<StatePageSession<'_>> {
        let charge = self.memory.reserve(4096, class)?;
        Ok(StatePageSession {
            store: self,
            io: Mutex::new(io),
            first_read_error: Mutex::new(None),
            class,
            cancelled: AtomicBool::new(false),
            caller_cancelled: None,
            _charge: charge,
        })
    }
    /// Share the caller's live cancellation flag with every physical page read
    /// and publication; the default constructor retains its local flag.
    pub fn state_session_with_cancelled<'a>(
        &'a self,
        io: PageIoBudget,
        class: MemoryClass,
        cancelled: &'a AtomicBool,
    ) -> Result<StatePageSession<'a>> {
        check_cancelled(cancelled)?;
        let charge = self.memory.reserve(4096, class)?;
        Ok(StatePageSession {
            store: self,
            io: Mutex::new(io),
            first_read_error: Mutex::new(None),
            class,
            cancelled: AtomicBool::new(false),
            caller_cancelled: Some(cancelled),
            _charge: charge,
        })
    }
}
impl StatePageSession<'_> {
    fn physical_cancelled(&self) -> &AtomicBool {
        self.caller_cancelled.unwrap_or(&self.cancelled)
    }
    fn check_cancelled(&self) -> Result<()> {
        check_cancelled(&self.cancelled)?;
        check_cancelled(self.physical_cancelled())
    }
    /// Preserve the original typed storage failure across the shared Merkle
    /// interface's legacy String error boundary. It is diagnostic only; a
    /// failed execution transaction remains poisoned and cannot be sealed.
    pub fn take_read_error(&self) -> Option<PageStoreError> {
        self.first_read_error
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .take()
    }
    pub fn remaining_io(&self) -> Result<PageIoBudget> {
        self.io
            .lock()
            .map(|io| io.clone())
            .map_err(|_| PageStoreError::Corrupt("state session poisoned"))
    }
    pub fn persist_transition_pages(
        &mut self,
        prepared: &ReservedValue<PreparedTransition>,
    ) -> Result<()> {
        if !prepared.uses_budget(&self.store.memory) {
            return Err(PageStoreError::Corrupt("transition producer pool"));
        }
        self.persist_nodes(
            prepared
                .new_nodes
                .iter()
                .map(|node| (node.digest.as_str(), node.bytes.as_slice())),
        )
    }
    pub fn persist_catalog_pages(&mut self, delta: &ReservedValue<CatalogDeltaV1>) -> Result<()> {
        if !delta.uses_budget(&self.store.memory) {
            return Err(PageStoreError::Corrupt("catalog producer pool"));
        }
        self.persist_nodes(
            delta
                .new_nodes
                .iter()
                .map(|node| (node.digest.as_str(), node.bytes.as_slice())),
        )
    }
    /// Prepare actual artifact catalog pages in CAS. This does not publish a
    /// root, admit an executable or create a durability/serving qualification.
    pub fn persist_artifact_pages(
        &mut self,
        delta: &ReservedValue<
            oasis7_formal_protocol::incremental_artifacts::ArtifactCatalogDeltaV1,
        >,
    ) -> Result<()> {
        use oasis7_formal_protocol::incremental_artifacts::{ArtifactEntryV1, ArtifactPageWriteV1};
        let bound = || PageStoreError::Corrupt("artifact producer admission");
        let base = 512usize
            .checked_add(
                delta
                    .entries
                    .capacity()
                    .checked_mul(std::mem::size_of::<ArtifactEntryV1>())
                    .ok_or_else(bound)?,
            )
            .and_then(|bytes| {
                bytes.checked_add(
                    delta
                        .new_nodes
                        .capacity()
                        .checked_mul(std::mem::size_of::<ArtifactPageWriteV1>())?,
                )
            })
            .ok_or_else(bound)?;
        let minimum = delta
            .new_nodes
            .iter()
            .try_fold(base, |bytes, node| {
                bytes
                    .checked_add(node.bytes.capacity())?
                    .checked_add(node.digest.capacity())
            })
            .ok_or_else(bound)?;
        if !delta.uses_budget(&self.store.memory) || delta.reserved_bytes() < minimum {
            return Err(bound());
        }
        self.persist_nodes(
            delta
                .new_nodes
                .iter()
                .map(|node| (node.digest.as_str(), node.bytes.as_slice())),
        )
    }
    fn persist_nodes<'a>(
        &mut self,
        nodes: impl Iterator<Item = (&'a str, &'a [u8])>,
    ) -> Result<()> {
        if self.caller_cancelled.is_some() {
            self.check_cancelled()?;
        }
        let cancelled = self.caller_cancelled.unwrap_or(&self.cancelled);
        let io = self
            .io
            .get_mut()
            .map_err(|_| PageStoreError::Corrupt("state session poisoned"))?;
        let result = persist_nodes(self.store, self.class, io, cancelled, nodes);
        if self.caller_cancelled.is_some() {
            self.check_cancelled()?;
        }
        result
    }
    fn failure(&self, error: PageStoreError) -> String {
        let message = error.to_string();
        let mut first = self
            .first_read_error
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        if first.is_none() {
            *first = Some(error);
        }
        message
    }
}
impl MerkleNodeReader for StatePageSession<'_> {
    type Node = ChargedMerklePage;
    fn allocation_class(&self) -> Option<MemoryClass> {
        Some(self.class)
    }
    fn allocation_budget(&self) -> Option<&MemoryBudget> {
        Some(&self.store.memory)
    }
    fn read_node(
        &self,
        digest: &str,
        max_bytes: usize,
    ) -> std::result::Result<Option<Self::Node>, String> {
        let result = (|| {
            if max_bytes > STATE_MAX_PAGE_BYTES {
                return Err(PageStoreError::Corrupt("state source request bound"));
            }
            let id = parse_id(digest)?;
            if self.caller_cancelled.is_some() {
                self.check_cancelled()?;
            }
            let mut io = self
                .io
                .lock()
                .map_err(|_| PageStoreError::Corrupt("state session poisoned"))?;
            let result = self
                .store
                .read_page(
                    &id,
                    max_bytes,
                    self.class,
                    &mut io,
                    self.physical_cancelled(),
                )
                .map(|bytes| Some(ChargedMerklePage(bytes)));
            if self.caller_cancelled.is_some() {
                self.check_cancelled()?;
            }
            result
        })();
        result.map_err(|error| self.failure(error))
    }
}
fn persist_nodes<'a>(
    store: &IncrementalPageStore,
    class: MemoryClass,
    io: &mut PageIoBudget,
    cancelled: &AtomicBool,
    nodes: impl Iterator<Item = (&'a str, &'a [u8])>,
) -> Result<()> {
    persist_nodes_profile(store, class, io, cancelled, nodes, store.compression)
}
fn persist_nodes_profile<'a>(
    store: &IncrementalPageStore,
    class: MemoryClass,
    io: &mut PageIoBudget,
    cancelled: &AtomicBool,
    nodes: impl Iterator<Item = (&'a str, &'a [u8])>,
    compression: PageCompression,
) -> Result<()> {
    persist_nodes_profile_cancelled(store, class, io, cancelled, nodes, compression, &mut || {
        false
    })
}
fn persist_nodes_profile_cancelled<'a>(
    store: &IncrementalPageStore,
    class: MemoryClass,
    io: &mut PageIoBudget,
    cancelled: &AtomicBool,
    nodes: impl Iterator<Item = (&'a str, &'a [u8])>,
    compression: PageCompression,
    external_cancelled: &mut dyn FnMut() -> bool,
) -> Result<()> {
    let mut check = || {
        check_cancelled(cancelled)?;
        if external_cancelled() {
            return Err(PageStoreError::Cancelled);
        }
        Ok(())
    };
    check()?;
    let _metadata = store.memory.reserve(64 * 1024, class)?;
    let mut batch = Vec::with_capacity(pack::MAX_ENTRIES);
    for (id, bytes) in nodes {
        check()?;
        if bytes.len() > STATE_MAX_PAGE_BYTES {
            return Err(PageStoreError::Corrupt("Merkle page bound"));
        }
        let id = parse_id(id)?;
        batch.push((id, bytes));
        if batch.len() == pack::MAX_ENTRIES {
            check()?;
            store.put_page_batch_with_compression(&batch, class, io, cancelled, compression)?;
            check()?;
            batch.clear();
        }
    }
    if !batch.is_empty() {
        check()?;
        store.put_page_batch_with_compression(&batch, class, io, cancelled, compression)?;
        check()?;
    }
    check()
}
fn parse_id(id: &str) -> Result<[u8; 32]> {
    if id.len() != 64
        || !id
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err(PageStoreError::Corrupt("canonical Merkle page ID"));
    }
    hex::decode(id)
        .map_err(|_| PageStoreError::Corrupt("Merkle page ID"))?
        .try_into()
        .map_err(|_| PageStoreError::Corrupt("Merkle page ID length"))
}

#[path = "incremental_producer_page_writers.rs"]
mod producer_writers;

#[cfg(all(test, unix))]
#[path = "incremental_state_store_tests.rs"]
mod tests;
