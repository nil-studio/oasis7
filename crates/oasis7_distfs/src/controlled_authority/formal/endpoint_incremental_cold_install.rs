//! Private actual cold conjunction and atomic in-process cache installation.
use super::*;
use oasis7_formal_protocol::{
    controlled_authority::formal::bounded_context::FormalBoundedContextV1,
    incremental_material::CheckedCommitCore,
};
use oasis7_proto::memory_budget::{MemoryClass, ReservedValue};
use oasis7_world_state::incremental_world_storage_source_cold::AuthenticatedColdWorldStorageSourceV1;
pub(super) struct AuthenticatedColdCandidateV1 {
    core: ReservedValue<CheckedCommitCore>,
    source: ReservedValue<AuthenticatedColdWorldStorageSourceV1>,
    checkpoint: super::cold_checkpoint::AuthenticatedColdCheckpointV1,
    _outer: ReservedValue<()>,
}
impl AuthenticatedColdCandidateV1 {
    pub(super) fn core(&self) -> &ReservedValue<CheckedCommitCore> {
        &self.core
    }
    pub(super) fn source(&self) -> &ReservedValue<AuthenticatedColdWorldStorageSourceV1> {
        &self.source
    }
    pub(super) fn checkpoint(&self) -> &super::cold_checkpoint::AuthenticatedColdCheckpointV1 {
        &self.checkpoint
    }
    pub(super) fn validate_context(
        &self,
        cache: &super::provisional::AcceptedCandidateCache,
        common: &mut FormalBoundedContextV1<'_>,
    ) -> Result<(), FormalError> {
        self.checkpoint.validate_context(common)?;
        self.checkpoint
            .validate_material_binding_v1(common.memory(), &self.core)?;
        if self.core.retention_ledger_artifact_id().is_some() != cache.retention.is_some() {
            return Err(invalid("cold accepted signed retention role selection"));
        }
        if let Some(retention) = cache.retention.as_ref() {
            retention
                .validate_binding_v1(self.checkpoint.head(), &cache.roots, common)
                .map_err(|e| invalid(format!("cold accepted retention binding: {e:?}")))?;
        }
        self.source
            .validate_sources_v1(
                self.checkpoint.head(),
                &self.core,
                &cache.runtime,
                &cache.cognition,
                common,
            )
            .map_err(storage_error)?;
        Ok(())
    }
}
fn storage_error(
    error: oasis7_world_state::incremental_world_storage_source_cold::ColdWorldStorageSourceErrorV1,
) -> FormalError {
    match error {
        oasis7_world_state::incremental_world_storage_source_cold::ColdWorldStorageSourceErrorV1::Formal(e) => e,
        error => invalid(format!("cold full storage: {error:?}")),
    }
}

#[derive(Debug)]
pub enum AuthenticatedColdHostErrorV1 {
    Formal(FormalError),
    Acquire(crate::controlled_authority::fencing::BoundedAcquireErrorV1),
    Native(oasis7_formal_protocol::controlled_authority::formal::PaidNativeCheckpointErrorV1),
}
impl From<FormalError> for AuthenticatedColdHostErrorV1 {
    fn from(e: FormalError) -> Self {
        Self::Formal(e)
    }
}
impl From<crate::controlled_authority::fencing::BoundedAcquireErrorV1>
    for AuthenticatedColdHostErrorV1
{
    fn from(e: crate::controlled_authority::fencing::BoundedAcquireErrorV1) -> Self {
        Self::Acquire(e)
    }
}
impl From<super::cold_checkpoint::ColdCheckpointErrorV1> for AuthenticatedColdHostErrorV1 {
    fn from(e: super::cold_checkpoint::ColdCheckpointErrorV1) -> Self {
        match e {
            super::cold_checkpoint::ColdCheckpointErrorV1::Formal(e) => Self::Formal(e),
            super::cold_checkpoint::ColdCheckpointErrorV1::Acquire(e) => Self::Acquire(e),
            super::cold_checkpoint::ColdCheckpointErrorV1::Native(e) => Self::Native(e),
        }
    }
}
impl std::fmt::Display for AuthenticatedColdHostErrorV1 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Formal(e) => e.fmt(f),
            Self::Acquire(e) => e.fmt(f),
            Self::Native(e) => e.fmt(f),
        }
    }
}
impl std::error::Error for AuthenticatedColdHostErrorV1 {}

#[allow(clippy::too_many_arguments)]
pub(super) fn authenticate_and_install_cold_parent_bounded_v1<L>(
    endpoint: &mut FileFormalEndpoint,
    lease: &crate::controlled_authority::fencing::PaidRemoteLeaseV1,
    pinned: &fencing::TrustedFencingPolicyV1,
    memory: &MemoryBudget,
    archive: &FormalArchiveReadWorkV1,
    work: &mut IwsColdParentWorkV1,
    source_work: &mut super::work::IwsCandidateSourceWorkV1,
    hints: IwsColdParentHintsV1,
    mailbox_hint: L,
    stop: &mut dyn FnMut() -> bool,
) -> Result<(), AuthenticatedColdHostErrorV1>
where
    L: FnMut(
        &[u8; 32],
    ) -> Result<oasis7_formal_protocol::incremental_material::FactLocationV1, String>,
{
    let checkpoint = super::cold_checkpoint::recover_authenticated_checkpoint_bounded_v1(
        endpoint, lease, pinned, memory, archive, stop,
    )?;
    let mut common = archive.bounded_context(memory, stop);
    let core =
        super::cold_core::recover_ordinary_core_v1(endpoint, &checkpoint, work, &mut common)?;
    let pages = &endpoint
        .iws_page_store
        .as_ref()
        .ok_or_else(|| invalid("cold actual pages"))?
        .pages;
    let recovered = super::cold_recovery_paid::recover_complete_sources_bounded_v1(
        pages,
        &checkpoint,
        &core,
        work,
        hints,
        mailbox_hint,
        &mut common,
    )?;
    let ordinary = super::cold_readmission_paid::readmit_complete_sources_bounded_v1(
        pages,
        &checkpoint,
        &recovered,
        &core,
        work,
        &mut common,
    )?;
    let source = oasis7_world_state::incremental_world_storage_source_cold::qualify_authenticated_cold_world_storage_source_v1(
        checkpoint.head(), &core, ordinary.current, ordinary.results, &ordinary.runtime,
        &ordinary.cognition, &mut common).map_err(storage_error)?;
    let material = super::cold_execution_material::recover_execution_material_bounded_v1(
        endpoint,
        &checkpoint,
        &core,
        work,
        source_work,
        &mut common,
    )?;
    let outer = common
        .memory()
        .reserve(
            std::mem::size_of::<AuthenticatedColdCandidateV1>(),
            MemoryClass::Ordinary,
        )
        .map_err(|_| invalid("cold actual conjunction admission"))?;
    let record = checkpoint.head().verified_record_id();
    let mut cache = super::provisional::AcceptedCandidateCache {
        commit_record: record,
        current_owner: super::provisional::AcceptedCurrentOwner::AuthenticatedCold(
            AuthenticatedColdCandidateV1 {
                core,
                source,
                checkpoint,
                _outer: outer.retain(()),
            },
        ),
        roots: ordinary.roots,
        cognition: ordinary.cognition,
        runtime: ordinary.runtime,
        ledger: ordinary.ledger,
        retention: ordinary.retention,
        complete_data: None,
        local_closure: None,
        recovered_pages: None,
        genesis_quorum: None,
        normal_quorum: None,
        formal_parent_fee: None,
    };
    let actual = cache
        .authenticated_cold()
        .ok_or_else(|| invalid("cold actual conjunction missing"))?;
    actual.validate_context(&cache, &mut common)?;
    let reference = actual
        .checkpoint()
        .native()
        .local_data_closure_reference()
        .ok_or_else(|| {
            invalid("cold local durability attestation absent; full rebuild required")
        })?;
    common.charge(1, reference.len() as u64 + 64)?;
    if endpoint.store.body.data_closure.as_deref() != Some(reference) {
        return Err(invalid("cold local protected checkpoint reference changed").into());
    }
    let raw = {
        let (memory, _, stop) = common.parts();
        endpoint.store.read_cold_local_original_bounded_v1(
            reference,
            super::local_closure::LOCAL_DATA_CLOSURE_WIRE_BYTES,
            memory,
            archive,
            stop,
        )?
    }
    .into_native_bytes_v1(&mut common)?;
    // Real provider metadata is paid before the original fixed crypto/scan
    // kernel, then only the finite local record lanes enter the scalar child.
    let wire = oasis7_formal_protocol::controlled_authority::formal::context_wire::encode_paid(
        &(lease.evidence(), endpoint.authority.policy()),
        64 << 20,
        &mut common,
        "formal encoding exceeds byte bound",
        "cold local provider",
    )?;
    common.charge(wire.len() as u64 * 16, wire.len() as u64 * 16)?;
    let token = super::cold_io::run(work, &mut common, |work, stop| {
        let local = super::local_closure::LocalClosureReadWorkV1 {
            reads_remaining: std::mem::take(&mut work.local_records_remaining),
            original_bytes_remaining: std::mem::take(&mut work.local_original_bytes_remaining),
        };
        // The original work is a loan; Err and unwind must rejoin its actual
        // scalar remainder before the surrounding Common child reconciles.
        struct Loan<'a> {
            records: &'a mut u64,
            bytes: &'a mut u64,
            work: super::local_closure::LocalClosureReadWorkV1,
        }
        impl Drop for Loan<'_> {
            fn drop(&mut self) {
                *self.records = self.work.reads_remaining;
                *self.bytes = self.work.original_bytes_remaining;
            }
        }
        let mut loan = Loan {
            records: &mut work.local_records_remaining,
            bytes: &mut work.local_original_bytes_remaining,
            work: local,
        };
        super::local_closure::recover_authenticated_local_closure_paid_v1(
            endpoint,
            super::local_closure::PaidLocalClosureReadRefsV1 { lease, raw: &raw },
            actual.checkpoint().head(),
            &cache,
            pages,
            &mut loan.work,
            &mut || stop(),
        )
    })?;
    common.consume_steps(1)?;
    let witness = {
        let (_, _, stop) = common.parts();
        super::closure::restore_store_read_witness_v1(token, memory, &mut || stop())?
    };
    let complete = super::closure::qualify_cold_complete_data_v1(
        actual.checkpoint(),
        actual.core(),
        actual.source(),
        material,
        witness,
        &mut common,
    )?;
    // Pay the one additional actual VerifiedHead clone retained by endpoint.
    let head_wire =
        oasis7_formal_protocol::controlled_authority::formal::context_wire::encode_paid(
            actual.checkpoint().head().record(),
            64 << 20,
            &mut common,
            "formal encoding exceeds byte bound",
            "cold retained head",
        )?;
    use oasis7_formal_protocol::controlled_authority::formal::bounded_cbor_data::{
        CborDataLayoutV1, CborDataLimitsV1, reserve_closed_cbor_output_v1,
    };
    let envelope = std::mem::size_of::<
        oasis7_formal_protocol::controlled_authority::formal::CanonicalRecordV1,
    >() + std::mem::size_of::<
        oasis7_formal_protocol::controlled_authority::formal::ProfileHandoffV1,
    >();
    let head_fee = reserve_closed_cbor_output_v1(&head_wire, CborDataLayoutV1 {
        root: envelope, sequence_element: envelope, map_pair: envelope,
        set_element: envelope, box_pointee: envelope, default_heap_per_value: envelope,
        symbols_bytes: 0, visits: 16, missing: None,
    }, &CborDataLimitsV1 { max_wire_bytes: 64 << 20, max_graph_bytes: usize::MAX, max_depth: 128 },
        &mut common).map_err(|e| match e {
            oasis7_formal_protocol::controlled_authority::formal::bounded_cbor_data::BoundedCborDataErrorV1::Common(e) => e,
            e => invalid(format!("cold retained head admission: {e}")),
        })?;
    common.charge(head_wire.len() as u64 * 16, head_wire.len() as u64 * 16)?;
    let head = actual.checkpoint().head().clone();
    cache.complete_data = Some(complete);
    cache.formal_parent_fee = Some(head_fee.retain(()));
    common.consume_steps(0)?;
    endpoint
        .iws_page_store
        .as_ref()
        .ok_or_else(|| invalid("cold page capability removed"))?
        .jobs
        .restore_cold(cache, &head)?;
    // All fallible source reads/checks/admissions precede the final source move.
    endpoint.parent = Some(head);
    Ok(())
}
