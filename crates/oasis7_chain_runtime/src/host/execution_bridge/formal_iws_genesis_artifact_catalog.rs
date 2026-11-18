//! Same-producer Wasm/Rules inventory DATA. No accepted-head authority.
use super::super::originals::Originals;
use super::*;
use oasis7_formal_protocol::{
    incremental_artifacts::{
        ARTIFACT_MAX_PAGE_BYTES, ArtifactCatalogV1, ArtifactEntryV1, ArtifactKindV1,
    },
    incremental_state::MerkleNodeReader,
};
use oasis7_proto::memory_budget::MemoryBudget;
use std::{cell::RefCell, sync::Arc};

// A single 32-byte key changes at most one leaf and 256 branches. This is a
// per-append bound, never a limit on the world's total number of originals.
const PENDING_BYTES: usize = (32 * 8 + 1) * ARTIFACT_MAX_PAGE_BYTES;
// The frozen empty table has 257 String headers and 257 64-byte hashes;
// construction also needs bounded codec/hash temporaries. Its cache remains
// the protocol's original fixed format cache, not candidate-owned state.
const EMPTY_WORKSPACE: usize = 32 << 10;

pub(in super::super) struct CatalogBuilder {
    catalog: ArtifactCatalogV1,
    allocation: MemoryReservation,
}

pub(in super::super) struct PaidArtifactCatalog {
    state: CatalogBuilder,
    wire: ReservedValue<Vec<u8>>,
    reference: OriginalArtifactRefV1,
}

pub(super) fn execution_error(error: ExecutionError) -> Error {
    Error::Body(BodyMutationError::Original(Box::new(error)))
}

pub(super) fn protocol<T>(
    common: &mut FormalBoundedContextV1<'_>,
    run: impl FnOnce(
        &mut dyn FnMut(u64, u64) -> std::result::Result<(), ExecutionError>,
    ) -> std::result::Result<T, ExecutionError>,
) -> Result<T> {
    let mut first = None;
    let result = run(&mut |steps, bytes| {
        common.charge(steps, bytes).map_err(|error| {
            if first.is_none() {
                first = Some(error);
            }
            ExecutionError::Aborted
        })
    });
    if let Some(error) = first {
        return Err(context_error(error));
    }
    result.map_err(execution_error)
}

struct Page(Arc<ReservedValue<Vec<u8>>>);
impl AsRef<[u8]> for Page {
    fn as_ref(&self) -> &[u8] {
        self.0.as_slice()
    }
}

struct Nodes<'a, F> {
    memory: &'a MemoryBudget,
    read: F,
}
impl<F: Fn(&str, usize) -> std::result::Result<Page, String>> MerkleNodeReader for Nodes<'_, F> {
    type Node = Page;
    fn allocation_class(&self) -> Option<MemoryClass> {
        Some(MemoryClass::Ordinary)
    }
    fn allocation_budget(&self) -> Option<&MemoryBudget> {
        Some(self.memory)
    }
    fn read_node(&self, digest: &str, max: usize) -> std::result::Result<Option<Page>, String> {
        (self.read)(digest, max).map(Some)
    }
}

fn read_page(
    originals: &mut Originals<'_, '_, '_>,
    digest: &str,
    max: usize,
    common: &mut FormalBoundedContextV1<'_>,
) -> Result<Page> {
    common
        .charge(2, digest.len() as u64)
        .map_err(context_error)?;
    if digest.len() != 64 || max > ARTIFACT_MAX_PAGE_BYTES {
        return Err(PageStoreError::Corrupt("GEN artifact catalog page request").into());
    }
    let mut id = [0; 32];
    hex::decode_to_slice(digest, &mut id)
        .map_err(|_| PageStoreError::Corrupt("GEN artifact catalog page ID"))?;
    let mut io = ObjectIo {
        physical: &mut originals.physical[0],
        logical: &mut originals.logical[0],
        class: MemoryClass::Ordinary,
        cancelled: originals.cancelled,
    };
    // This is the real source namespace and the original finite IO lanes.
    // The paid Arc remains live through the protocol's hash/decode checks.
    metered(common, &mut io, |io| {
        originals.stores[0].read_object_owned(&id, max, io)
    })
    .map(Page)
}

impl CatalogBuilder {
    pub(in super::super) fn new(
        kind: ArtifactKindV1,
        common: &mut FormalBoundedContextV1<'_>,
    ) -> Result<Self> {
        let bytes = size_of::<Self>() + 64;
        common
            .charge(2, (bytes + EMPTY_WORKSPACE) as u64)
            .map_err(context_error)?;
        let allocation = common
            .memory()
            .reserve(bytes, MemoryClass::Ordinary)
            .map_err(PageStoreError::Memory)?;
        let _empty = common
            .memory()
            .reserve(EMPTY_WORKSPACE, MemoryClass::Ordinary)
            .map_err(PageStoreError::Memory)?;
        let catalog = protocol(common, |work| {
            ArtifactCatalogV1::empty_with_work(kind, work)
        })?;
        if catalog.node_root.capacity() > 64 {
            return Err(PageStoreError::Corrupt("GEN artifact catalog root allocation").into());
        }
        Ok(Self {
            catalog,
            allocation,
        })
    }

    fn validate_context(&self, common: &mut FormalBoundedContextV1<'_>) -> Result<()> {
        common.consume_steps(1).map_err(context_error)?;
        if !self.allocation.uses_budget(common.memory())
            || self.catalog.node_root.len() != 64
            || self.catalog.node_root.capacity() > 64
        {
            return Err(PageStoreError::Corrupt("GEN artifact catalog owner").into());
        }
        Ok(())
    }

    /// Called only after the original P callback has completed all three writes.
    /// Each one-entry delta is persisted before promoting its private root.
    pub(in super::super) fn append_original(
        &mut self,
        original: OriginalArtifactRefV1,
        originals: &RefCell<Originals<'_, '_, '_>>,
        common: &mut FormalBoundedContextV1<'_>,
    ) -> Result<()> {
        self.validate_context(common)?;
        common.charge(1, 4096).map_err(context_error)?;
        let _adapter = common
            .memory()
            .reserve(4096, MemoryClass::Ordinary)
            .map_err(PageStoreError::Memory)?;
        let memory = common.memory().clone();
        if !originals.borrow().stores[0].uses_budget(&memory) {
            return Err(PageStoreError::Corrupt("GEN artifact catalog source pool").into());
        }
        let delta = {
            let shared = RefCell::new(&mut *common);
            let first = RefCell::new(None);
            let nodes = Nodes {
                memory: &memory,
                read: |digest: &str, max| {
                    let result = read_page(
                        &mut originals.borrow_mut(),
                        digest,
                        max,
                        &mut shared.borrow_mut(),
                    );
                    result.map_err(|error| {
                        let mut slot = first.borrow_mut();
                        if slot.is_none() {
                            *slot = Some(error);
                        }
                        "GEN artifact catalog source failed".to_owned()
                    })
                },
            };
            let result = self.catalog.append_with_work(
                &[ArtifactEntryV1 {
                    kind: self.catalog.kind,
                    original,
                }],
                &nodes,
                &memory,
                PENDING_BYTES,
                &mut |steps, bytes| {
                    if first.borrow().is_some() {
                        return Err(ExecutionError::Aborted);
                    }
                    shared.borrow_mut().charge(steps, bytes).map_err(|error| {
                        *first.borrow_mut() = Some(context_error(error));
                        ExecutionError::Aborted
                    })
                },
            );
            if let Some(error) = first.into_inner() {
                return Err(error);
            }
            result.map_err(execution_error)?
        };
        for node in &delta.new_nodes {
            common
                .charge(2, node.digest.len() as u64)
                .map_err(context_error)?;
            let mut id = [0; 32];
            hex::decode_to_slice(&node.digest, &mut id)
                .map_err(|_| PageStoreError::Corrupt("GEN artifact catalog new page ID"))?;
            originals
                .borrow_mut()
                .raw_bounded(
                    OriginalArtifactRefV1 {
                        id,
                        byte_len: node.bytes.len() as u64,
                    },
                    &node.bytes,
                    common,
                )
                .map_err(Error::Body)?;
        }
        // The persistent owner pays for its root. Admit the brief extra clone
        // while the previous root and the delta's after root are still alive.
        common
            .charge(1, delta.after.node_root.len() as u64)
            .map_err(context_error)?;
        if delta.after.node_root.capacity() > 64 {
            return Err(PageStoreError::Corrupt("GEN artifact catalog after allocation").into());
        }
        let _copy = memory
            .reserve(64, MemoryClass::Ordinary)
            .map_err(PageStoreError::Memory)?;
        self.catalog = delta.after.clone();
        Ok(())
    }

    /// The caller invokes this only after the fixed projected GEN and original
    /// business output succeed. No earlier per-item delta is a full-GEN delta.
    pub(super) fn finish(
        self,
        objects: &[IncrementalObjectStore<'_>; 3],
        work: &mut IwsGenesisTransferWorkV1,
        common: &mut FormalBoundedContextV1<'_>,
        stop: &AtomicBool,
    ) -> Result<PaidArtifactCatalog> {
        self.validate_context(common)?;
        // Frozen descriptor is <=128B; retain this encoding admission with the
        // wire, including temporary codec buffers and its carrier metadata.
        common.charge(1, 1024).map_err(context_error)?;
        let fee = common
            .memory()
            .reserve(1024, MemoryClass::Ordinary)
            .map_err(PageStoreError::Memory)?;
        let bytes = protocol(common, |pay| self.catalog.canonical_bytes_with_work(pay))?;
        if bytes.len() > 128 || bytes.capacity() > 128 {
            return Err(
                PageStoreError::Corrupt("GEN artifact catalog descriptor allocation").into(),
            );
        }
        let wire = fee.retain(bytes);
        let reference = write_original(objects, &wire, None, work, common, stop)?;
        Ok(PaidArtifactCatalog {
            state: self,
            wire,
            reference,
        })
    }
}

impl PaidArtifactCatalog {
    pub(super) fn validate_context(&self, common: &mut FormalBoundedContextV1<'_>) -> Result<()> {
        self.state.validate_context(common)?;
        if !self.wire.uses_budget(common.memory())
            || self.reference.byte_len != self.wire.len() as u64
        {
            return Err(PageStoreError::Corrupt("GEN artifact catalog wire binding").into());
        }
        Ok(())
    }
    pub(super) fn catalog(&self) -> &ArtifactCatalogV1 {
        &self.state.catalog
    }
    pub(super) fn original(&self) -> (OriginalArtifactRefV1, &ReservedValue<Vec<u8>>) {
        (self.reference, &self.wire)
    }
}
