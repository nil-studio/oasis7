//! Retained local data only; no index root or checkpoint is published.
use super::super::FormalArchiveReadWorkV1;
use super::*;
use crate::controlled_authority::formal::physical;
use oasis7_proto::memory_budget::MemoryBudget;

pub(in crate::controlled_authority::formal) struct RetainedIndexBlobV1 {
    staged: StagedIndexBlobV1,
    retained: physical::roots::RetainedAppend,
}
impl RetainedIndexBlobV1 {
    pub(in crate::controlled_authority::formal) fn id(&self) -> &str {
        self.staged.id()
    }
    pub(in crate::controlled_authority::formal) fn bytes(&self) -> &[u8] {
        self.staged.bytes()
    }
    pub(in crate::controlled_authority::formal) fn uses_budget(
        &self,
        memory: &MemoryBudget,
    ) -> bool {
        self.staged.uses_budget(memory) && self.retained.uses_budget(memory)
    }
    pub(in crate::controlled_authority::formal) fn reserved_bytes(&self) -> usize {
        self.staged.reserved_bytes() + self.retained.reserved_bytes()
    }
}
pub(super) fn stage_retained(
    store: &Store,
    request: &str,
    entry: &str,
    memory: &MemoryBudget,
    work: &FormalArchiveReadWorkV1,
    stop: &mut dyn FnMut() -> bool,
) -> Result<RetainedIndexBlobV1, FormalError> {
    let staged = store.stage_index_leaf_bounded(request, entry, memory, work, stop)?;
    let retained = physical::roots::append_one_bounded_v1(&staged, memory, work, stop)?;
    Ok(RetainedIndexBlobV1 { staged, retained })
}

// This fresh-fixture adapter is test-only and exposes no mutable Store/Hot
// state, namespace constructor, qualification setter or production authority.
#[cfg(test)]
pub(in crate::controlled_authority::formal) struct BoundedRetentionStoreFixture<'a> {
    store: &'a mut Store,
}
#[cfg(test)]
impl BoundedRetentionStoreFixture<'_> {
    pub(in crate::controlled_authority::formal) fn stage(
        &self,
        request: &str,
        entry: &str,
        memory: &MemoryBudget,
        work: &FormalArchiveReadWorkV1,
        stop: &mut dyn FnMut() -> bool,
    ) -> Result<StagedIndexBlobV1, FormalError> {
        self.store
            .stage_index_leaf_bounded(request, entry, memory, work, stop)
    }
    pub(in crate::controlled_authority::formal) fn stage_retained(
        &self,
        request: &str,
        entry: &str,
        memory: &MemoryBudget,
        work: &FormalArchiveReadWorkV1,
        stop: &mut dyn FnMut() -> bool,
    ) -> Result<RetainedIndexBlobV1, FormalError> {
        self.store
            .stage_index_leaf_retained_bounded(request, entry, memory, work, stop)
    }
    pub(in crate::controlled_authority::formal) fn lookup(
        &self,
        request: &str,
    ) -> Result<Option<String>, FormalError> {
        self.store.lookup(request)
    }
    pub(in crate::controlled_authority::formal) fn checkpoint_bytes(
        &self,
    ) -> Result<Vec<u8>, FormalError> {
        self.store.checkpoint_bytes()
    }
    pub(in crate::controlled_authority::formal) fn object_root(&self) -> PathBuf {
        self.store.path.join("formal-objects-v1")
    }
}
#[cfg(test)]
pub(in crate::controlled_authority::formal) fn with_store_for_bounded_retention_fixture<T>(
    run: impl FnOnce(&mut BoundedRetentionStoreFixture<'_>, &Path) -> T,
) -> T {
    super::super::tests::with_store_for_bounded_index_fixture(|store, parent| {
        let mut fixture = BoundedRetentionStoreFixture { store };
        run(&mut fixture, parent)
    })
}
