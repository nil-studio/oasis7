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
            _charge: charge,
        })
    }
}
impl StatePageSession<'_> {
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
    pub fn persist_artifact_pages(
        &mut self,
        delta: &ReservedValue<
            oasis7_formal_protocol::incremental_artifacts::ArtifactCatalogDeltaV1,
        >,
    ) -> Result<()> {
        if !delta.uses_budget(&self.store.memory) {
            return Err(PageStoreError::Corrupt("artifact producer pool"));
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
        let io = self
            .io
            .get_mut()
            .map_err(|_| PageStoreError::Corrupt("state session poisoned"))?;
        let _metadata = self.store.memory.reserve(64 * 1024, self.class)?;
        let mut batch = Vec::with_capacity(pack::MAX_ENTRIES);
        for (id, bytes) in nodes {
            check_cancelled(&self.cancelled)?;
            if bytes.len() > STATE_MAX_PAGE_BYTES {
                return Err(PageStoreError::Corrupt("Merkle page bound"));
            }
            let id = parse_id(id)?;
            batch.push((id, bytes));
            if batch.len() == pack::MAX_ENTRIES {
                self.store
                    .put_small_page_batch(&batch, self.class, io, &self.cancelled)?;
                batch.clear();
            }
        }
        if !batch.is_empty() {
            self.store
                .put_small_page_batch(&batch, self.class, io, &self.cancelled)?;
        }
        Ok(())
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
            let mut io = self
                .io
                .lock()
                .map_err(|_| PageStoreError::Corrupt("state session poisoned"))?;
            self.store
                .read_page(&id, max_bytes, self.class, &mut io, &self.cancelled)
                .map(|bytes| Some(ChargedMerklePage(bytes)))
        })();
        result.map_err(|error| self.failure(error))
    }
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

#[cfg(test)]
#[path = "incremental_state_store_tests.rs"]
mod tests;
