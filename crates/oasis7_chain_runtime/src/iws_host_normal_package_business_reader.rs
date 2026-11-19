//! Paid IO adapter over the actual paired Host Original holder. These are
//! content bytes only: fixed business consumers still verify all typed roots,
//! metadata, append-only predicates and the genuine accepted-parent boundary.
use crate::iws_host_runtime::normal::originals::{
    HostNormalOriginalsV1, PairedNormalOriginalMeterErrorV1,
};
use oasis7::world_service::formal_result_iws_material::PaidIwsBusinessOriginalReaderV1;
use oasis7_distfs::incremental_page_store::PageStoreError;
use oasis7_formal_protocol::{
    controlled_authority::formal::{
        FormalError,
        bounded_context::{FormalBoundedContextV1, WorkIdentityV1},
        business_index::{BUSINESS_INDEX_MAX_LEAF_BYTES, PaidBusinessIndexNodeReaderV1},
    },
    incremental_material::OriginalObjectReader,
};
use oasis7_proto::memory_budget::{MemoryBudget, MemoryClass, MemoryReservation, ReservedValue};
use std::{cell::RefCell, mem::size_of, sync::Arc};
const FAILED: &str = "paid Host business original reader failed";
#[derive(Debug)]
pub(in crate::iws_host_runtime) enum HostBusinessOriginalReadErrorV1 {
    Context(FormalError),
    Physical(PageStoreError),
    Invalid(&'static str),
    PriorFailure,
}
impl std::fmt::Display for HostBusinessOriginalReadErrorV1 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for HostBusinessOriginalReadErrorV1 {}
/// The Node trait's original FormalError shape cannot own a physical error.
/// Keep that original first cause and its adapter admission until consumed.
#[derive(Debug)]
pub(in crate::iws_host_runtime) struct PaidHostBusinessOriginalFailureV1 {
    error: HostBusinessOriginalReadErrorV1,
    _fee: MemoryReservation,
}
impl PaidHostBusinessOriginalFailureV1 {
    pub(in crate::iws_host_runtime) fn error(&self) -> &HostBusinessOriginalReadErrorV1 {
        &self.error
    }
}
pub(in crate::iws_host_runtime) struct HostNormalBusinessOriginalReaderV1<
    'a,
    'holder,
    'store,
    'quota,
> {
    originals: &'a HostNormalOriginalsV1<'holder, 'store, 'quota>,
    memory: &'a MemoryBudget,
    core: WorkIdentityV1,
    first: RefCell<Option<HostBusinessOriginalReadErrorV1>>,
    fee: MemoryReservation,
}
impl<'a, 'holder, 'store, 'quota> HostNormalBusinessOriginalReaderV1<'a, 'holder, 'store, 'quota> {
    /// Only the private fixed Host scope can supply the genuine paired holder.
    /// No namespace, permission flag or standalone object DTO enters here.
    pub(in crate::iws_host_runtime) fn new(
        originals: &'a HostNormalOriginalsV1<'holder, 'store, 'quota>,
        memory: &'a MemoryBudget,
        common: &mut FormalBoundedContextV1<'_>,
    ) -> Result<Self, HostBusinessOriginalReadErrorV1> {
        common
            .charge(1, size_of::<Self>() as u64)
            .map_err(HostBusinessOriginalReadErrorV1::Context)?;
        let reader = originals
            .paired_reader()
            .map_err(HostBusinessOriginalReadErrorV1::Physical)?;
        if !memory.same_pool(common.memory())
            || !reader.allocation_budget().same_pool(memory)
            || reader.allocation_class() != MemoryClass::Ordinary
        {
            return Err(HostBusinessOriginalReadErrorV1::Invalid(
                "Host business reader actual pool/class",
            ));
        }
        let fee = memory
            .reserve(
                size_of::<Self>()
                    + size_of::<blake3::Hasher>()
                    + FAILED.len()
                    + size_of::<FormalError>(),
                MemoryClass::Ordinary,
            )
            .map_err(|e| HostBusinessOriginalReadErrorV1::Physical(PageStoreError::Memory(e)))?;
        Ok(Self {
            originals,
            memory,
            core: common.work().identity(),
            first: RefCell::new(None),
            fee,
        })
    }
    /// Fixed callers inspect this before turning the shared kernel sentinel
    /// into an outer error, preserving the physical/resource first cause.
    pub(in crate::iws_host_runtime) fn into_failure(
        self,
    ) -> Option<PaidHostBusinessOriginalFailureV1> {
        self.first
            .into_inner()
            .map(|error| PaidHostBusinessOriginalFailureV1 {
                error,
                _fee: self.fee,
            })
    }
    fn read(
        &self,
        id: &[u8; 32],
        max: usize,
        common: &mut FormalBoundedContextV1<'_>,
    ) -> Result<Arc<ReservedValue<Vec<u8>>>, HostBusinessOriginalReadErrorV1> {
        use HostBusinessOriginalReadErrorV1::{Context, Invalid, Physical};
        common.charge(1, 32).map_err(Context)?;
        if self
            .first
            .try_borrow()
            .map_err(|_| Invalid("Host business reader reentry"))?
            .is_some()
        {
            return Err(HostBusinessOriginalReadErrorV1::PriorFailure);
        }
        if self.core != common.work().identity()
            || !self.memory.same_pool(common.memory())
            || !self.fee.uses_budget(common.memory())
        {
            return Err(Invalid("Host business reader foreign Core/pool"));
        }
        // The exclusive child accounts the real two-endpoint physical/logical
        // lanes. This callback only invokes the existing bounded raw reader;
        // it never borrows/reenters the parent Common account.
        let raw = self
            .originals
            .with_metered_paired_reader(common, |reader| reader.read_original(id, max, self.memory))
            .map_err(|e| match e {
                PairedNormalOriginalMeterErrorV1::Context(e) => Context(e),
                PairedNormalOriginalMeterErrorV1::Store(e) => Physical(e),
            })?;
        if !raw.uses_budget(self.memory) || raw.len() > max {
            return Err(Invalid("Host business original actual pool/length"));
        }
        let mut hash = blake3::Hasher::new();
        for chunk in raw.chunks(8192) {
            common.charge(1, chunk.len() as u64).map_err(Context)?;
            hash.update(chunk);
        }
        common.charge(1, 32).map_err(Context)?;
        if hash.finalize().as_bytes() != id {
            return Err(Invalid("Host business original actual hash"));
        }
        Ok(raw)
    }
    fn node(
        &self,
        digest: &str,
        common: &mut FormalBoundedContextV1<'_>,
    ) -> Result<Option<ReservedValue<Vec<u8>>>, HostBusinessOriginalReadErrorV1> {
        use HostBusinessOriginalReadErrorV1::{Context, Invalid, Physical};
        common.charge(1, digest.len() as u64).map_err(Context)?;
        if digest.len() != 64
            || !digest
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        {
            return Err(Invalid("Host business node digest encoding"));
        }
        let mut id = [0u8; 32];
        hex::decode_to_slice(digest, &mut id)
            .map_err(|_| Invalid("Host business node digest encoding"))?;
        let raw = match self.read(&id, BUSINESS_INDEX_MAX_LEAF_BYTES + 128, common) {
            Ok(raw) => raw,
            // Only a real Missing for this exact requested object is absence.
            // The original fixed SMT kernel decides whether that path is legal.
            Err(Physical(PageStoreError::Missing(missing))) if missing == id => return Ok(None),
            Err(error) => return Err(error),
        };
        let n = raw.len();
        let bytes = n
            .checked_add(size_of::<Vec<u8>>())
            .ok_or(Invalid("Host business node allocation layout"))?;
        common.charge(1, bytes as u64).map_err(Context)?;
        let fee = self
            .memory
            .reserve(bytes, MemoryClass::Ordinary)
            .map_err(|e| Physical(PageStoreError::Memory(e)))?;
        let mut copy = Vec::new();
        copy.try_reserve_exact(n)
            .map_err(|_| Invalid("Host business node allocation"))?;
        if copy.capacity() > n {
            return Err(Invalid("Host business node capacity"));
        }
        for chunk in raw.chunks(8192) {
            common.charge(1, chunk.len() as u64).map_err(Context)?;
            copy.extend_from_slice(chunk);
        }
        // The original Arc and its complete admission stay alive until this
        // independently paid exact-capacity copy is complete.
        Ok(Some(fee.retain(copy)))
    }
}
impl PaidIwsBusinessOriginalReaderV1 for HostNormalBusinessOriginalReaderV1<'_, '_, '_, '_> {
    type Error = HostBusinessOriginalReadErrorV1;
    fn allocation_budget(&self) -> &MemoryBudget {
        self.memory
    }
    fn allocation_class(&self) -> MemoryClass {
        MemoryClass::Ordinary
    }
    fn read_business_original_bounded_v1(
        &mut self,
        id: &[u8; 32],
        max: usize,
        common: &mut FormalBoundedContextV1<'_>,
    ) -> Result<Arc<ReservedValue<Vec<u8>>>, Self::Error> {
        self.read(id, max, common)
    }
}
impl PaidBusinessIndexNodeReaderV1 for HostNormalBusinessOriginalReaderV1<'_, '_, '_, '_> {
    fn read_node_bounded(
        &self,
        digest: &str,
        common: &mut FormalBoundedContextV1<'_>,
    ) -> Result<Option<ReservedValue<Vec<u8>>>, FormalError> {
        match self.node(digest, common) {
            Ok(raw) => Ok(raw),
            Err(error) => {
                if let Ok(mut first) = self.first.try_borrow_mut() {
                    if first.is_none() {
                        *first = Some(error);
                    }
                }
                Err(FormalError::Io(FAILED.into()))
            }
        }
    }
}

impl oasis7_formal_protocol::incremental_projection_sidecar::PaidProjectionOriginalReaderV1
    for HostNormalBusinessOriginalReaderV1<'_, '_, '_, '_>
{
    type Error = HostBusinessOriginalReadErrorV1;
    fn allocation_budget(&self) -> &MemoryBudget {
        self.memory
    }
    fn allocation_class(&self) -> MemoryClass {
        MemoryClass::Ordinary
    }
    fn read_projection_original_bounded_v1(
        &mut self,
        id: &[u8; 32],
        max: usize,
        common: &mut FormalBoundedContextV1<'_>,
    ) -> Result<Arc<ReservedValue<Vec<u8>>>, Self::Error> {
        // Share the real paired physical/logical IO child, exact content hash
        // and owner/context checks; side data receives no history authority.
        self.read(id, max, common)
    }
}
