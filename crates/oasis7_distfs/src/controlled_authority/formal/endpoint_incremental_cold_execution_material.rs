//! Full native/artifact material verification on the actual cold checkpoint.
//! The old live-parent wrapper is not used to manufacture a before-cache.
use super::*;
use oasis7_formal_protocol::controlled_authority::formal::bounded_context::FormalBoundedContextV1;
use oasis7_formal_protocol::{
    controlled_authority::formal::{
        IncrementalFrozenMaterialWorkV1, verify_formal_reservation,
        verify_incremental_frozen_material_v1,
    },
    incremental_material::{CheckedCommitCore, OriginalObjectReader},
};
use oasis7_proto::memory_budget::{MemoryClass, ReservedValue};
use std::cell::RefCell;

struct BridgeLoan<'a> {
    occurrences: &'a mut u64,
    bytes: &'a mut u64,
    frozen: &'a mut u64,
    work: super::closure::NativeChunkReadWorkV1,
}
impl<'a> BridgeLoan<'a> {
    fn new(occurrences: &'a mut u64, bytes: &'a mut u64, frozen: &'a mut u64) -> Self {
        let work = super::closure::NativeChunkReadWorkV1 {
            occurrences_remaining: std::mem::take(occurrences),
            bytes_remaining: std::mem::take(bytes),
            frozen_wire_bytes_remaining: std::mem::take(frozen),
        };
        Self {
            occurrences,
            bytes,
            frozen,
            work,
        }
    }
}
impl Drop for BridgeLoan<'_> {
    fn drop(&mut self) {
        *self.occurrences = self.work.occurrences_remaining;
        *self.bytes = self.work.bytes_remaining;
        *self.frozen = self.work.frozen_wire_bytes_remaining;
    }
}
fn verify_cold_frozen_originals(
    endpoint: &FileFormalEndpoint,
    checkpoint: &super::cold_checkpoint::AuthenticatedColdCheckpointV1,
    core: &CheckedCommitCore,
    work: &mut IncrementalFrozenMaterialWorkV1,
    memory: &MemoryBudget,
    cancelled: &mut impl FnMut() -> bool,
) -> Result<(), FormalError> {
    let FormalRequestStateV1::Qualified { candidate, .. } = checkpoint.state() else {
        return Err(invalid("cold Frozen actual Qualified source"));
    };
    let (certificate, material) = match (&candidate.frozen, &candidate.frozen_material) {
        (None, None)
            if checkpoint.predecessor().is_none()
                && core.roots().height == 0
                && core.parent_record_id().is_none() =>
        {
            return Ok(());
        }
        (Some(certificate), Some(material)) => (certificate, material),
        _ => return Err(invalid("IWS Frozen content certificate/material pairing")),
    };
    let parent = checkpoint
        .predecessor()
        .ok_or_else(|| invalid("IWS Frozen content verified parent missing"))?;
    let authority = if candidate.branch == FormalCompletionBranchV1::Normal {
        &endpoint.authority
    } else {
        &endpoint.original
    };
    let reservation = verify_formal_reservation(&certificate.reservation, authority, parent)?;
    let mut originals = ClosedRecordOriginalReader {
        record: material,
        memory,
    };
    let checked = verify_incremental_frozen_material_v1(
        &reservation,
        parent,
        material,
        &mut originals,
        memory,
        work,
        &mut *cancelled,
    )
    .map_err(|error| invalid(format!("IWS original Frozen content: {error}")))?;
    if cancelled() {
        return Err(invalid("IWS Frozen content final cancellation"));
    }
    // The original material/frozen shared kernel below binds this same signed
    // reservation and native ID to the accepted head. Full cold native checks
    // and the authenticated local namespace/generation pin replace no predicate
    // of this original Frozen byte decoder.
    drop(checked);
    Ok(())
}
pub(super) fn recover_execution_material_bounded_v1(
    endpoint: &FileFormalEndpoint,
    checkpoint: &super::cold_checkpoint::AuthenticatedColdCheckpointV1,
    core: &ReservedValue<CheckedCommitCore>,
    work: &mut IwsColdParentWorkV1,
    source: &mut super::work::IwsCandidateSourceWorkV1,
    common: &mut FormalBoundedContextV1<'_>,
) -> Result<ReservedValue<super::closure::CandidateExecutionMaterialV1>, FormalError> {
    checkpoint.validate_context(common)?;
    let capability = endpoint
        .iws_page_store
        .as_ref()
        .ok_or_else(|| invalid("cold execution material actual pages"))?;
    let pages = &capability.pages;
    let memory = pages.memory_budget();
    if !core.uses_budget(common.memory()) || !memory.same_pool(common.memory()) {
        return Err(invalid("cold execution material pool"));
    }
    let FormalRequestStateV1::Qualified { candidate, .. } = checkpoint.state() else {
        return Err(invalid("cold material Qualified source"));
    };
    // The repeated original reservation predicate uses this actual closed
    // body. Admit its serializer/clone phase before entering scalar IO.
    let wire = oasis7_formal_protocol::controlled_authority::formal::context_wire::encode_paid(
        &candidate.frozen,
        64 << 20,
        common,
        "formal encoding exceeds byte bound",
        "cold Frozen reservation",
    )?;
    let n = wire
        .len()
        .checked_mul(16)
        .ok_or_else(|| invalid("cold Frozen phase layout"))?;
    common.charge(
        u64::try_from(n).map_err(|_| invalid("cold Frozen phase visits"))?,
        n as u64,
    )?;
    let _pure_phase = common
        .memory()
        .reserve(
            n.checked_add(128 << 10)
                .ok_or_else(|| invalid("cold Frozen phase layout"))?,
            MemoryClass::Ordinary,
        )
        .map_err(|_| invalid("cold Frozen reservation phase admission"))?;
    super::cold_io::run_material(work, source, common, |work, source, cancelled| {
        let physical = std::sync::atomic::AtomicBool::new(false);
        let mut poll = || {
            let cancelled = cancelled();
            if cancelled {
                physical.store(true, std::sync::atomic::Ordering::Release);
            }
            cancelled
        };
        verify_cold_frozen_originals(
            endpoint,
            checkpoint,
            core,
            &mut source.frozen_material,
            memory,
            &mut poll,
        )?;
        let objects = crate::incremental_page_store::objects::IncrementalObjectStore::open(
            pages,
            MemoryClass::Ordinary,
            &mut work.objects_physical,
            &physical,
        )
        .map_err(page_store_formal_error)?;
        let mut io = crate::incremental_page_store::objects::ObjectIo {
            physical: &mut work.objects_physical,
            logical: &mut work.objects_values,
            class: MemoryClass::Ordinary,
            cancelled: &physical,
        };
        let mut originals = objects.original_reader(&mut io);
        let state = super::cold_page_loans::StateLoan::new(
            pages,
            &mut work.state_ordinary,
            MemoryClass::Ordinary,
            &physical,
            None,
        )
        .map_err(page_store_formal_error)?;
        let reader = super::cold_core::Pages {
            source: state.source(),
            io: RefCell::new(&mut work.read_ordinary),
        };
        let mut bridge = BridgeLoan::new(
            &mut source.native_chunk_occurrences,
            &mut source.native_chunk_bytes,
            &mut source.frozen_wire_bytes,
        );
        super::closure::verify_accepted_cold_execution_material_v1(
            endpoint,
            checkpoint,
            core,
            &mut originals,
            pages,
            &reader,
            &mut work.core,
            &mut source.artifacts,
            &mut bridge.work,
            &mut poll,
        )
    })
}
