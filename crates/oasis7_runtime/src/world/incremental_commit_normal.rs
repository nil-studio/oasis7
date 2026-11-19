//! Sole consuming normal-tick seal and History candidate assembly. The real
//! parent/input and terminal directory owners remain live through every phase.
use super::*;
use crate::world::{
    incremental_directory_bundle::{
        RuntimeDirectoryFreezeErrorV1, bind_runtime_directories_to_actual_seal_v1,
    },
    incremental_normal_host_finish::{FinishedNormalTickDirectoriesV1, NormalHostFinishErrorV1},
    incremental_normal_host_work::{NormalHostDirectoryWorkLoanV1, NormalHostTickDataV1},
    incremental_normal_sources::BoundNormalTickParentV1,
};
use oasis7_consensus::normal_tick_input::PaidBoundNormalInputDataV1;
use oasis7_formal_protocol::controlled_authority::formal::{
    FormalError, bounded_context::FormalBoundedContextV1,
};
use oasis7_proto::memory_budget::MemoryReservation;
use oasis7_world_state::incremental_inventory::InventoryMaterialScopeWorkV1;
use std::cell::{Cell, RefCell};
#[path = "incremental_commit_normal_rebind.rs"]
mod rebind;

/// Both producers persist their real final originals from the actual frozen
/// after roots. The genuine parent's full router, never a caller bool, selects
/// the two-Result or independent ProofLedger three-Result profile.
pub(in crate::world) enum NormalFinalResultProducerV1<P2, P3> {
    Pair(P2),
    ProofLedger(P3),
}
#[derive(Debug)]
pub(in crate::world) enum NormalCommitPreparationErrorV1<OE, PE> {
    Execution(ExecutionError),
    Context(FormalError),
    Finish(NormalHostFinishErrorV1<OE>),
    Directory(RuntimeDirectoryFreezeErrorV1),
    Producer(PE),
}
impl<OE, PE> From<ExecutionError> for NormalCommitPreparationErrorV1<OE, PE> {
    fn from(e: ExecutionError) -> Self {
        Self::Execution(e)
    }
}
impl<OE, PE> From<FormalError> for NormalCommitPreparationErrorV1<OE, PE> {
    fn from(e: FormalError) -> Self {
        Self::Context(e)
    }
}
/// Candidate content only. In particular this keeps the genuine pending
/// cognition continuation and its original tick evidence alive for the later
/// fixed Host consumer; preparing History must not silently drop that owner.
pub(in crate::world) struct PreparedNormalCommitV1<'p> {
    candidate: PreparedRuntimeCommit,
    tick: NormalHostTickDataV1<'p>,
    resident: crate::world::incremental_normal_host_finish::SealedNormalResidentSourceV1,
    _fee: MemoryReservation,
}
impl<'p> PreparedNormalCommitV1<'p> {
    pub(in crate::world) fn detach_after_actual_capture(
        self,
    ) -> Result<PreparedNormalCommitV1<'static>, crate::world::incremental_body::BodyMutationError>
    {
        let tick = self.tick.tick.into_detached_after_capture()?;
        Ok(PreparedNormalCommitV1 {
            candidate: self.candidate,
            tick: NormalHostTickDataV1 {
                input: self.tick.input,
                tick,
                final_sources: self.tick.final_sources,
            },
            resident: self.resident,
            _fee: self._fee,
        })
    }

    pub(in crate::world) fn candidate(&self) -> &PreparedRuntimeCommit {
        &self.candidate
    }
    pub(in crate::world) fn validate_context_v1(
        &self,
        common: &mut FormalBoundedContextV1<'_>,
    ) -> Result<(), crate::world::incremental_body::BodyMutationError> {
        use crate::world::incremental_body::BodyMutationError;
        self.resident
            .validate_event_source_v1(self.tick.final_sources.event_capture(), common)?;
        self.resident
            .validate_manifest_source_v1(self.tick.final_sources.manifest_capture(), common)?;
        self.resident
            .validate_projection_source_v1(self.tick.final_sources.projection_capture(), common)?;
        self.resident
            .validate_retention_source_v1(self.tick.final_sources.retention_capture(), common)?;
        common
            .consume_steps(1)
            .map_err(|e| BodyMutationError::Original(Box::new(e)))?;
        let memory = common.memory();
        if !self._fee.uses_budget(memory)
            || !self.tick.final_sources.uses_budget(memory)
            || !self.candidate.transition().uses_budget(memory)
            || !self.candidate.history().uses_budget(memory)
            || !self.candidate.catalog().uses_budget(memory)
            || !self.candidate.root_set_bytes().uses_budget(memory)
            || !self.candidate.catalog_bytes().uses_budget(memory)
            || !self.candidate.require_directories_v1()?.uses_budget(memory)
        {
            return Err(BodyMutationError::Invalid("normal sealed preparation pool"));
        }
        Ok(())
    }
    pub(in crate::world) fn formal_projection_root_v1<'a>(
        &'a self,
        common: &mut FormalBoundedContextV1<'_>,
    ) -> Result<&'a str, crate::world::incremental_body::BodyMutationError> {
        self.validate_context_v1(common)?;
        Ok(self
            .tick
            .final_sources
            .projection_capture()
            .projection_root())
    }
    pub(in crate::world) fn retained_history_root_json_v1(
        &self,
        common: &mut FormalBoundedContextV1<'_>,
    ) -> Result<ReservedValue<Vec<u8>>, crate::world::incremental_body::BodyMutationError> {
        self.validate_context_v1(common)?;
        self.tick
            .final_sources
            .projection_capture()
            .retained_history_root_json_bounded_v1(common)
    }
    pub(in crate::world) fn business_source_v1<'a>(
        &'a self,
        parent: &'a BoundNormalTickParentV1<'_>,
        common: &mut FormalBoundedContextV1<'_>,
    ) -> Result<
        crate::world::incremental_normal_business_commitment::PaidBorrowedNormalBusinessSourceV1<
            'a,
        >,
        crate::world::incremental_body::BodyMutationError,
    > {
        use crate::world::incremental_body::BodyMutationError;
        use crate::world::incremental_normal_business_commitment::PaidBorrowedNormalBusinessSourceV1 as Source;
        self.validate_context_v1(common)?;
        parent
            .validate_context(common)
            .map_err(|e| BodyMutationError::Original(Box::new(e)))?;
        let delta = self.tick.final_sources.business_capture();
        let metadata = self.tick.final_sources.terminal_metadata_capture();
        let raw = self
            .tick
            .final_sources
            .projection_capture()
            .raw_state_hash();
        self.resident.validate_projection_raw_hash_v1(raw, common)?;
        common
            .charge(1, std::mem::size_of::<Source<'_>>() as u64)
            .map_err(|e| BodyMutationError::Original(Box::new(e)))?;
        let fee = common
            .memory()
            .reserve(
                std::mem::size_of::<Source<'_>>(),
                oasis7_proto::memory_budget::MemoryClass::Ordinary,
            )
            .map_err(|e| BodyMutationError::Original(Box::new(e)))?;
        let source = Source {
            delta,
            metadata,
            parent: parent.refs().head,
            after_roots: &self.candidate.transition().record.after,
            seal: &self.resident,
            raw_state_hash: raw,
            core: common.work().identity(),
            memory: common.memory().clone(),
            fee,
        };
        source.validate_context(common)?;
        Ok(source)
    }
    pub(in crate::world) fn validate_input_source_v1(
        &self,
        input: &PaidBoundNormalInputDataV1<'_>,
        common: &mut FormalBoundedContextV1<'_>,
    ) -> Result<(), crate::world::incremental_body::BodyMutationError> {
        use crate::world::incremental_body::BodyMutationError;
        self.validate_context_v1(common)?;
        input
            .validate_context(common)
            .map_err(|e| BodyMutationError::Original(Box::new(e)))?;
        common
            .charge(1, 64)
            .map_err(|e| BodyMutationError::Original(Box::new(e)))?;
        if self.tick.input.material() != input.checked().material_id() {
            return Err(BodyMutationError::Invalid(
                "normal sealed preparation actual Frozen input",
            ));
        }
        Ok(())
    }
    pub(in crate::world) fn validate_business_source_v1(
        &self,
        source: &crate::world::incremental_normal_business_commitment::ActualCurrentBusinessDeltaV1,
        common: &mut FormalBoundedContextV1<'_>,
    ) -> Result<(), crate::world::incremental_body::BodyMutationError> {
        self.validate_context_v1(common)?;
        self.resident.validate_business_source_v1(source, common)
    }
    pub(in crate::world) fn retention_manifest_v1(
        &self,
        common: &mut FormalBoundedContextV1<'_>,
    ) -> Result<
        ReservedValue<oasis7_formal_protocol::incremental_retention::RetentionLedgerManifestV1>,
        crate::world::incremental_body::BodyMutationError,
    > {
        self.validate_context_v1(common)?;
        let source = self.tick.final_sources.retention_capture();
        self.resident.validate_retention_source_v1(source, common)?;
        source.manifest(&self.candidate.transition().record.after, common)
    }
    pub(in crate::world) fn formal_event_commitment_v1(
        &self,
        common: &mut FormalBoundedContextV1<'_>,
    ) -> Result<([u8; 32], usize), crate::world::incremental_body::BodyMutationError> {
        let source = self.tick.final_sources.event_capture();
        self.resident.validate_event_source_v1(source, common)?;
        common.charge(1, 40).map_err(|e| {
            crate::world::incremental_body::BodyMutationError::Original(Box::new(e))
        })?;
        Ok((*source.digest(), source.count()))
    }
    pub(in crate::world) fn formal_governing_manifest_digest_v1(
        &self,
        common: &mut FormalBoundedContextV1<'_>,
        canonical: &mut oasis7_wasm_abi::CanonicalCborWorkV1,
    ) -> Result<ReservedValue<String>, crate::world::incremental_body::BodyMutationError> {
        use crate::world::incremental_body::BodyMutationError;
        let source = self.tick.final_sources.manifest_capture();
        self.resident.validate_manifest_source_v1(source, common)?;
        let digest = crate::world::normal_bootstrap_current::hash::with_current_meter_v1(
            common,
            canonical,
            |meter| {
                crate::world::cognition_persistence::canonical_runtime_binding_digest_with_work_v1(
                    source.hash(),
                    meter,
                )
            },
        )?
        .map_err(|e| BodyMutationError::Original(Box::new(e)))?;
        let text = digest
            .as_str()
            .strip_prefix("blake3:")
            .filter(|s| s.len() == 64)
            .ok_or(BodyMutationError::Invalid(
                "normal governing Manifest digest encoding",
            ))?;
        common
            .charge(1, 64)
            .map_err(|e| BodyMutationError::Original(Box::new(e)))?;
        let fee = common
            .memory()
            .reserve(
                std::mem::size_of::<String>() + 64,
                oasis7_proto::memory_budget::MemoryClass::Ordinary,
            )
            .map_err(oasis7_formal_protocol::incremental_value::ValueTreeError::Memory)?;
        let mut plain = String::new();
        plain
            .try_reserve_exact(64)
            .map_err(|_| BodyMutationError::Invalid("normal governing Manifest allocation"))?;
        if plain.capacity() > 64 {
            return Err(BodyMutationError::Invalid(
                "normal governing Manifest capacity",
            ));
        }
        plain.push_str(text);
        Ok(fee.retain(plain))
    }
    pub(in crate::world) fn into_parts(
        self,
    ) -> (
        PreparedRuntimeCommit,
        NormalHostTickDataV1<'p>,
        MemoryReservation,
        crate::world::incremental_normal_host_finish::SealedNormalResidentSourceV1,
    ) {
        (self.candidate, self.tick, self._fee, self.resident)
    }
}
/// These are caller-owned resource limits, not source roots or permissions.
/// They are used only for the original History/catalog preparation policy.
pub struct NormalCommitHeaderLimitsV1 {
    pub transaction: TransactionLimits,
    pub checkpoint: bool,
}
/// The transaction must be the one that actually executed and froze this tick.
/// No public PreparedTransition or caller-supplied record/body/root enters.
#[allow(clippy::too_many_arguments)]
pub(in crate::world) fn prepare_finished_normal_commit_v1<'p, 'r, R, OE, PE, P2, P3>(
    mut tx: ExecutionTransaction<'r, R>,
    finished: FinishedNormalTickDirectoriesV1<'p>,
    parent: &ReservedValue<BoundNormalTickParentV1<'_>>,
    input: &PaidBoundNormalInputDataV1<'_>,
    source: &R,
    policy: NormalCommitHeaderLimitsV1,
    material_work: &mut InventoryMaterialScopeWorkV1,
    directory_work: NormalHostDirectoryWorkLoanV1<'_>,
    producer: NormalFinalResultProducerV1<P2, P3>,
    common: &mut FormalBoundedContextV1<'_>,
    live_owner: &mut dyn FnMut() -> ExecutionFence,
) -> Result<PreparedNormalCommitV1<'p>, NormalCommitPreparationErrorV1<OE, PE>>
where
    R: MerkleNodeReader,
    P2: FnOnce(&RootSetV1, &mut FormalBoundedContextV1<'_>) -> Result<[ExecutionFactRefV1; 2], PE>,
    P3: FnOnce(&RootSetV1, &mut FormalBoundedContextV1<'_>) -> Result<[ExecutionFactRefV1; 3], PE>,
{
    type Error<OE, PE> = NormalCommitPreparationErrorV1<OE, PE>;
    let live = live_owner();
    finished
        .validate_terminal_v1::<R, OE>(&mut tx, parent, input, common, &live)
        .map_err(Error::Finish)?;
    let ledger = parent.refs().storage.results().proof_ledger().is_some();
    if ledger != matches!(&producer, NormalFinalResultProducerV1::ProofLedger(_)) {
        return Err(ExecutionError::Invalid("normal final Result full-router profile").into());
    }
    if source
        .allocation_budget()
        .is_some_and(|p| !p.same_pool(common.memory()))
        || source
            .allocation_class()
            .is_some_and(|c| c != MemoryClass::Ordinary)
    {
        return Err(ExecutionError::Invalid("normal History source pool/class").into());
    }
    let memory = common.memory().clone();
    // Fixed final carrier, sealed-directory candidate binding and the two
    // bounded RootSet/catalog header encodings in the sole assembly kernel.
    // Variable transition/fact/catalog work uses the original callbacks below.
    common.charge(64, 64 * 1024)?;
    let workspace = memory
        .reserve(64 * 1024, MemoryClass::Ordinary)
        .map_err(ExecutionError::Memory)?;
    // Same original request digest that the authenticated Frozen reservation
    // binds. A caller cannot select another operation or execution environment.
    let digest = &input.checked().reservation().body().request_digest;
    common.charge(1, digest.len() as u64)?;
    let mut operation = [0u8; 32];
    hex::decode_to_slice(digest, &mut operation)
        .map_err(|_| ExecutionError::Invalid("normal operation digest"))?;
    let refs = parent.refs();
    let core = refs.core;
    let request = CommitPreparationRequest {
        parent: ParentCommitView {
            config: core.config(),
            roots: refs.roots,
            body: core.body(),
            history: core.history(),
            catalog: &core.catalog().after,
            authority: parent.fence().authority,
        },
        operation,
        next_tick: input.context().height,
        execution_environment: core.config().execution_environment,
        checkpoint: policy.checkpoint,
        limits: policy.transaction,
    };
    let fence = tx.parent_fence();
    let guard = tx.guard();
    let (tick, frozen, mut outer, resident) = finished.into_parts();
    let extra = std::mem::size_of::<PreparedNormalCommitV1<'p>>()
        .saturating_sub(std::mem::size_of::<FinishedNormalTickDirectoriesV1<'p>>());
    common.charge(1, extra as u64)?;
    let mut extra = memory
        .reserve(extra, MemoryClass::Ordinary)
        .map_err(ExecutionError::Memory)?;
    outer.merge(&mut extra).map_err(ExecutionError::Invalid)?;
    // The source guard captured by the real seal remains the exact terminal
    // guard of the frozen bundle. Final Result production cannot mutate state.
    let common = RefCell::new(common);
    let live = RefCell::new(live_owner);
    let first = RefCell::new(None);
    let poll = || match common.borrow_mut().consume_steps(0) {
        Ok(()) => false,
        Err(error) => {
            if first.borrow().is_none() {
                *first.borrow_mut() = Some(error)
            }
            true
        }
    };
    let metered = |steps, bytes| {
        let result = common.borrow_mut().charge(steps, bytes);
        result.map_err(|error| {
            if first.borrow().is_none() {
                *first.borrow_mut() = Some(error)
            }
            ExecutionError::Invalid("normal candidate Common work")
        })
    };
    let sealed = match producer {
        NormalFinalResultProducerV1::Pair(produce) => tx
            .seal_with_final_results_and_binding_with_work(
                &guard,
                &(live.borrow_mut())(),
                request.next_tick,
                &poll,
                |roots| produce(roots, &mut common.borrow_mut()),
                &mut |steps, bytes| metered(steps, bytes),
            ),
        NormalFinalResultProducerV1::ProofLedger(produce) => tx
            .seal_with_proof_ledger_results_and_binding_with_work(
                &guard,
                &(live.borrow_mut())(),
                request.next_tick,
                poll,
                |roots| produce(roots, &mut common.borrow_mut()),
                &mut |steps, bytes| metered(steps, bytes),
            ),
    };
    if let Some(error) = first.borrow_mut().take() {
        return Err(Error::Context(error));
    }
    let sealed = sealed.map_err(|e| match e {
        SealWithFinalFactError::Execution(e) => Error::Execution(e),
        SealWithFinalFactError::Producer(e) => Error::Producer(e),
        SealWithFinalFactError::Cancelled => {
            Error::Execution(ExecutionError::Invalid("normal seal cancelled"))
        }
    })?;
    let (transition, binding) = sealed.into_parts();
    let resident = resident
        .bind_actual_seal_v1(&binding, &transition.record.after, &mut common.borrow_mut())
        .map_err(|e| {
            Error::Finish(
                crate::world::incremental_normal_host_finish::NormalHostFinishErrorV1::Root(
                    e.into(),
                ),
            )
        })?;
    // Rebind only the original genuine snapshots to that one actual seal;
    // all logical counters below are the remaining tick-owned loans.
    let mut rebind_work = rebind::RebindLanes {
        material: material_work,
        directories: directory_work,
    };
    let fence_lost = Cell::new(false);
    let directories = rebind::run(&mut common.borrow_mut(), &mut rebind_work, |work, stop| {
        bind_runtime_directories_to_actual_seal_v1(
            frozen,
            binding,
            &transition.record.after,
            &memory,
            work.material,
            work.directories.inventory,
            work.directories.claims,
            work.directories.grant_cells,
            work.directories.gameplay,
            work.directories.threats,
            work.directories.builds,
            work.directories.receipts,
            || {
                if stop() {
                    true
                } else if (live.borrow_mut())() != fence {
                    fence_lost.set(true);
                    true
                } else {
                    false
                }
            },
        )
    });
    if fence_lost.get() {
        return Err(ExecutionError::ParentConflict.into());
    }
    let directories = directories
        .map_err(Error::Context)?
        .map_err(Error::Directory)?;
    // Preserve the typed first Common error across the original canonical and
    // Merkle string-error bridges. Legacy wrappers use this same assembly.
    let candidate = assemble_prepared_commit_owned_v1(
        &request,
        source,
        &memory,
        || {
            common.borrow_mut().consume_steps(0)?;
            Ok((live.borrow_mut())())
        },
        transition,
        Some(directories),
        fence,
        workspace,
        |header| {
            header
                .prepare_with_work(&memory, &mut |s, b| metered(s, b))
                .map_err(Error::Execution)
        },
        |catalog, body, reader, memory, max| {
            catalog
                .extend_with_work(body, reader, memory, max, &mut |s, b| metered(s, b))
                .map_err(Error::Execution)
        },
    );
    if let Some(error) = first.borrow_mut().take() {
        return Err(Error::Context(error));
    }
    let (candidate, mut candidate_fee) = candidate?;
    outer
        .merge(&mut candidate_fee)
        .map_err(ExecutionError::Invalid)?;
    common.borrow_mut().consume_steps(0)?;
    Ok(PreparedNormalCommitV1 {
        candidate,
        tick,
        resident,
        _fee: outer,
    })
}
