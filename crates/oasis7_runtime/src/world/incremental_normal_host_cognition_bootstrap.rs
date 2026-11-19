//! One full-source GEN/cold admission of the actual accepted ProofLedger and
//! wake evidence. Ordinary warm execution must retain/promote its delta cache;
//! it cannot invoke this bootstrap to rescan accepted history every tick.
use super::*;
use crate::world::{
    incremental_body_event::AgentEventOriginalV1,
    incremental_module_execution_worker::RootModuleWorkerAllocatorV1,
    incremental_module_invocation::RootModuleExecutionErrorV1,
    incremental_normal_cognition_service_evidence::AcceptedCognitionWakeEvidenceV2,
    incremental_normal_host_context::{
        NormalHostContextErrorV1, NormalHostRootReadSourcesV1, with_begun_normal_root_v1,
    },
    incremental_normal_host_driver::NormalHostCognitionParentSourcesV1,
    incremental_normal_host_work::NormalHostWorkV1,
};
use oasis7_formal_protocol::{
    incremental_execution::ExecutionTransaction, incremental_state::StatePageWriter,
    incremental_value::ValueNodeWriter,
};
use oasis7_proto::memory_budget::MemoryReservation;
use oasis7_wasm_executor::bounded_worker::BoundedWorkerSandboxV1;
use oasis7_world_state::incremental_cognition_proof_ledger::AcceptedCognitionProofLedgerV1;

#[derive(Debug)]
pub enum NormalHostCognitionBootstrapErrorV1<OE> {
    Source(BodyMutationError),
    Context(NormalHostContextErrorV1<OE, RootModuleExecutionErrorV1<OE>>),
}
impl<OE> From<BodyMutationError> for NormalHostCognitionBootstrapErrorV1<OE> {
    fn from(error: BodyMutationError) -> Self {
        Self::Source(error)
    }
}

/// Opaque resident DATA, not an execution permit. Both owners derive from this
/// exact accepted parent and remain on the caller's original Core/pool. There
/// is no empty/default constructor, Clone or caller-selected root setter.
pub struct PaidNormalHostCognitionParentSourcesV1 {
    ledger: ReservedValue<AcceptedCognitionProofLedgerV1>,
    evidence: ReservedValue<AcceptedCognitionWakeEvidenceV2>,
    record: [u8; 32],
    fence: ExecutionFence,
    core: WorkIdentityV1,
    fee: MemoryReservation,
}

pub(super) fn validate_ledger(
    parent: &ReservedValue<BoundNormalTickParentV1<'_>>,
    ledger: &ReservedValue<AcceptedCognitionProofLedgerV1>,
    common: &mut FormalBoundedContextV1<'_>,
) -> Result<(), BodyMutationError> {
    parent.validate_context(common).map_err(formal)?;
    common.charge(1, 256).map_err(formal)?;
    let refs = parent.refs();
    let routed = refs.storage.results().proof_ledger().ok_or_else(bad)?;
    let selected = routed.occurrence();
    let occurrence = ledger.occurrence();
    // ReservedValue records the real pool/charge, not an allocation class.
    // The private accepted occurrence carries the source's Ordinary class,
    // as in AcceptedCognitionProofLedgerV1's own field-reader admission.
    if !ledger.uses_budget(common.memory())
        || ledger.reserved_bytes() < std::mem::size_of::<AcceptedCognitionProofLedgerV1>()
        || !occurrence.uses_budget(common.memory())
        || occurrence.allocation_class() != MemoryClass::Ordinary
        || occurrence.head_record_id() != refs.head.verified_record_id()
        || occurrence.head_body_id() != parent.fence().parent_commit
        || occurrence.fact_body_id() != occurrence.head_body_id()
        || occurrence.roots_id() != parent.fence().parent_roots
        || ledger.manifest().roots != occurrence.roots_id()
        || selected.candidate_record_id() != occurrence.head_record_id()
        || selected.location() != occurrence.location()
        || selected.body_id() != occurrence.head_body_id()
        || selected.roots_id() != occurrence.roots_id()
    {
        return Err(bad());
    }
    Ok(())
}

impl PaidNormalHostCognitionParentSourcesV1 {
    /// The parent is used only to validate these retained owners. Returned
    /// borrows do not retain its temporary read view or the mutable Host slot.
    pub fn as_sources_bounded<'a>(
        &'a self,
        parent: &ReservedValue<BoundNormalTickParentV1<'_>>,
        common: &mut FormalBoundedContextV1<'_>,
    ) -> Result<NormalHostCognitionParentSourcesV1<'a>, BodyMutationError> {
        common.charge(1, 128).map_err(formal)?;
        if self.core != common.work().identity()
            || !self.fee.uses_budget(common.memory())
            || self.record != parent.refs().head.verified_record_id()
            || self.fence != parent.fence()
        {
            return Err(bad());
        }
        validate_ledger(parent, &self.ledger, common)?;
        self.evidence
            .validate_parent(parent, &self.ledger, common)?;
        Ok(NormalHostCognitionParentSourcesV1::Present {
            accepted: &self.ledger,
            evidence: &self.evidence,
        })
    }
}

#[allow(clippy::too_many_arguments)]
pub fn bootstrap_normal_host_cognition_parent_v1<'s, 'p, M, R, W, O, E, C, F>(
    parent: &'p ReservedValue<BoundNormalTickParentV1<'p>>,
    transaction: &mut ExecutionTransaction<'s, M>,
    state: &M,
    values: &R,
    member_sink: &mut W,
    fee_sink: &mut W,
    event_sink: &mut W,
    originals: &mut O,
    persist_original: F,
    worker: &BoundedWorkerSandboxV1<'_, RootModuleWorkerAllocatorV1>,
    resident: &'p NormalHostResidentSourcesV1,
    accepted_ledger: ReservedValue<AcceptedCognitionProofLedgerV1>,
    work: &mut NormalHostWorkV1,
    take_state_error: E,
    cancelled: C,
    common: &mut FormalBoundedContextV1<'_>,
    live: &mut dyn FnMut() -> ExecutionFence,
) -> Result<PaidNormalHostCognitionParentSourcesV1, NormalHostCognitionBootstrapErrorV1<O::Error>>
where
    M: MerkleNodeReader,
    R: ValueNodeReader,
    W: ValueNodeReader + ValueNodeWriter + StatePageWriter,
    O: OriginalObjectStreamReader,
    E: FnMut() -> Option<ValueTreeError> + Copy,
    C: FnMut() -> bool + Copy,
    F: FnMut(&mut dyn AgentEventOriginalV1) -> Result<(), BodyMutationError>,
{
    let attempt = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        parent.validate_context(common).map_err(formal)?;
        work.validate_context(common)?;
        resident.validate_live_binding_v1(
            parent.refs().head.verified_record_id(),
            &parent.fence(),
            common,
        )?;
        // A warm resident must use the ordinary terminal-capture promotion.
        // Its retained directory/cache cannot be relabelled as cold admission.
        if !matches!(&resident.storage, ResidentStorage::Built { .. }) {
            return Err(BodyMutationError::Invalid(
                "Cognition full source only at GEN/cold admission",
            )
            .into());
        }
        transaction
            .validate_bootstrap_read_only_v1(&live())
            .map_err(BodyMutationError::from)?;
        validate_ledger(parent, &accepted_ledger, common)?;
        resident
            .storage
            .caches()
            .validate_parent_v1(parent, common)?;
        let layout = std::mem::size_of::<PaidNormalHostCognitionParentSourcesV1>();
        common.charge(1, layout as u64).map_err(formal)?;
        let fee = common
            .memory()
            .reserve(layout, MemoryClass::Ordinary)
            .map_err(ValueTreeError::Memory)
            .map_err(BodyMutationError::from)?;
        let sources = NormalHostRootReadSourcesV1 {
            manifest: &resident.manifest,
            delivery: &resident.delivery,
            retention: &resident.retention,
            materials: resident.storage.materials(),
        };
        let evidence = with_begun_normal_root_v1(
            parent,
            transaction,
            state,
            values,
            member_sink,
            fee_sink,
            event_sink,
            originals,
            persist_original,
            worker,
            &sources,
            work,
            take_state_error,
            cancelled,
            common,
            live,
            |root,
             _pending,
             _revocation,
             _delivery,
             _current,
             _parent_read,
             _admission,
             _factory,
             _recipe,
             _industry,
             _materials,
             driver,
             common,
             live| {
                root.ctx
                    .transaction
                    .validate_bootstrap_read_only_v1(&live())?;
                root.build_accepted_cognition_wake_evidence_v2(
                    parent,
                    &accepted_ledger,
                    &mut driver.tail.cognition,
                    common,
                    live,
                )
            },
        )
        .map_err(NormalHostCognitionBootstrapErrorV1::Context)?;
        transaction
            .validate_bootstrap_read_only_v1(&live())
            .map_err(BodyMutationError::from)?;
        evidence.validate_parent(parent, &accepted_ledger, common)?;
        Ok(PaidNormalHostCognitionParentSourcesV1 {
            ledger: accepted_ledger,
            evidence,
            record: parent.refs().head.verified_record_id(),
            fence: parent.fence(),
            core: common.work().identity(),
            fee,
        })
    }));
    match attempt {
        Ok(Ok(owner)) => Ok(owner),
        Ok(Err(error)) => {
            let _: Result<(), ExecutionError> =
                transaction.contribute(|_| Err(ExecutionError::Aborted));
            Err(error)
        }
        Err(cause) => {
            let _: Result<(), ExecutionError> =
                transaction.contribute(|_| Err(ExecutionError::Aborted));
            std::panic::resume_unwind(cause)
        }
    }
}
