//! Optional session-local positive page memo. It never caches errors/misses,
//! changes logical verification work, or grants any root/head qualification.
use super::*;
use std::collections::BTreeMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StateReadMemoStats {
    pub hits: u64,
    pub misses: u64,
    pub entries: usize,
    pub retained_bytes: usize,
}
pub(super) struct StateReadMemo {
    pages: BTreeMap<[u8; 32], Arc<ReservedValue<Vec<u8>>>>,
    max_entries: usize,
    max_retained_bytes: usize,
    retained_bytes: usize,
    hits: u64,
    misses: u64,
    // Covers at most one BTree node per entry, including its inline keys and
    // Arc values, node split scratch and allocator bookkeeping. Payload/Arc
    // ownership remains with the actual PageStore reservation, not this fee.
    _metadata: MemoryReservation,
}
impl StateReadMemo {
    pub(super) fn new(
        memory: &MemoryBudget,
        class: MemoryClass,
        max_entries: usize,
        max_retained_bytes: usize,
    ) -> Result<Self> {
        let metadata = max_entries
            .checked_mul(2048)
            .and_then(|n| n.checked_add(4096))
            .ok_or(PageStoreError::ResourceLimited("state memo metadata bound"))?;
        let charge = memory.reserve(metadata, class)?;
        Ok(Self {
            pages: BTreeMap::new(),
            max_entries,
            max_retained_bytes,
            retained_bytes: 0,
            hits: 0,
            misses: 0,
            _metadata: charge,
        })
    }
    pub(super) fn get(
        &mut self,
        id: &[u8; 32],
        max_bytes: usize,
    ) -> Option<Arc<ReservedValue<Vec<u8>>>> {
        let found = self
            .pages
            .get(id)
            .filter(|page| page.len() <= max_bytes)
            .cloned();
        if found.is_some() {
            self.hits = self.hits.saturating_add(1);
        } else {
            self.misses = self.misses.saturating_add(1);
        }
        found
    }
    pub(super) fn remember(&mut self, id: [u8; 32], page: &Arc<ReservedValue<Vec<u8>>>) {
        if self.pages.contains_key(&id) || self.pages.len() >= self.max_entries {
            return;
        }
        let Some(total) = self
            .retained_bytes
            .checked_add(page.reserved_bytes())
            .filter(|n| *n <= self.max_retained_bytes)
        else {
            return;
        };
        self.pages.insert(id, Arc::clone(page));
        self.retained_bytes = total;
    }
    pub(super) fn stats(&self) -> StateReadMemoStats {
        StateReadMemoStats {
            hits: self.hits,
            misses: self.misses,
            entries: self.pages.len(),
            retained_bytes: self.retained_bytes,
        }
    }
}
