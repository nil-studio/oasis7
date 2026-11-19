//! Genuine normal tick -> terminal directory snapshots -> pre-seal DATA.
//! Reading closure directories may extend the read set. It cannot change the
//! already-validated tick's writes or ordered facts, or refresh that old owner.
use super::{
    incremental_body::BodyMutationError,
    incremental_body_event::AgentEventOriginalV1,
    incremental_directory_bundle::{FrozenRuntimeDirectoryBundleV1, RuntimeDirectoryFreezeErrorV1},
    incremental_factory_phase::{
        FactoryPhaseAcceptedCachesV1, FactoryPhaseFamiliesV1,
        io::{
            FactoryPhaseMeterLaneV1, FactoryPhaseMeterLanesV1, visit_factory_phase_work_lanes_v1,
        },
    },
    incremental_module_invocation::*,
    incremental_normal_host_work::{NormalHostDirectoryWorkLoanV1, NormalHostTickDataV1},
    incremental_normal_parent_caches::NormalParentCachesV1,
    incremental_normal_sources::BoundNormalTickParentV1,
    incremental_normal_tick_driver::NormalTickTailSourcesV1,
};
use oasis7_consensus::normal_tick_input::PaidBoundNormalInputDataV1;
use oasis7_formal_protocol::{
    controlled_authority::formal::bounded_context::{FormalBoundedContextV1, WorkIdentityV1},
    incremental_execution::{ExecutionError, ExecutionFence, ExecutionTransaction, PreparedGuard},
    incremental_material::OriginalObjectStreamReader,
    incremental_state::{MerkleNodeReader, StatePageWriter},
    incremental_value::{ValueNodeReader, ValueNodeWriter, ValueTreeError},
};
use oasis7_proto::memory_budget::{MemoryClass, MemoryReservation, ReservedValue};
use oasis7_world_state::{
    incremental_factory_records::FactoryTransitionReadContextV1,
    incremental_industry_jobs::IndustryTransitionReadContextV1,
    incremental_inventory::ParentInventoryMaterialScopeV1,
};
use std::{cell::RefCell, sync::atomic::Ordering};
#[path = "incremental_normal_host_finish_cut.rs"]
mod cut;
use cut::capture;
pub(in crate::world) use cut::{
    NormalFinishWriteFactCutV1, capture as capture_normal_write_fact_cut_v1,
};
/// Continuous inspection quota supplied by Host, shared by both cuts. Bytes
/// and CPU are charged directly to the same Common, outside its IO child.
pub(in crate::world) struct NormalHostFinishCutWorkV1 {
    pub pending_entries_remaining: usize,
}
#[derive(Debug)]
pub(in crate::world) enum NormalHostFinishErrorV1<OE> {
    Root(RootModuleExecutionErrorV1<OE>),
    Directories(RuntimeDirectoryFreezeErrorV1),
}
/// Owns the original tick/input source and every real frozen snapshot until the
/// actual seal consumer takes them. No acceptance or completion is minted.
pub(in crate::world) struct NormalResidentTerminalSourceV1 {
    sources: std::sync::Arc<
        ReservedValue<super::incremental_normal_final_sources::CapturedNormalFinalSourcesV1>,
    >,
    guard: oasis7_formal_protocol::incremental_execution::PreparedGuard,
    core: oasis7_formal_protocol::controlled_authority::formal::bounded_context::WorkIdentityV1,
    _fee: MemoryReservation,
}
pub(in crate::world) struct SealedNormalResidentSourceV1 {
    terminal: NormalResidentTerminalSourceV1,
    after_roots: [u8; 32],
}
impl NormalResidentTerminalSourceV1 {
    pub(in crate::world) fn bind_actual_seal_v1(
        mut self,
        binding: &oasis7_formal_protocol::incremental_execution::SealedExecutionBindingV1,
        roots: &oasis7_formal_protocol::incremental_execution::RootSetV1,
        common: &mut FormalBoundedContextV1<'_>,
    ) -> Result<SealedNormalResidentSourceV1, BodyMutationError> {
        common
            .charge(1, 8192)
            .map_err(|e| BodyMutationError::Original(Box::new(e)))?;
        let mut fee = common
            .memory()
            .reserve(
                std::mem::size_of::<SealedNormalResidentSourceV1>(),
                MemoryClass::Ordinary,
            )
            .map_err(ValueTreeError::Memory)?;
        self._fee
            .merge(&mut fee)
            .map_err(BodyMutationError::Invalid)?;
        let memory = common.memory().clone();
        self.sources
            .projection_capture()
            .validate_sealed_v1(binding, roots, &memory, common)?;
        binding.validate_source_guard(&self.guard, common.memory())?;
        binding.validate_after_roots(roots, common.memory())?;
        if self.core != common.work().identity() || !self._fee.uses_budget(common.memory()) {
            return Err(BodyMutationError::Invalid("normal resident actual seal"));
        }
        Ok(SealedNormalResidentSourceV1 {
            terminal: self,
            after_roots: binding.after_roots_digest(),
        })
    }
}
impl SealedNormalResidentSourceV1 {
    pub(in crate::world) fn validate_cognition_evidence_source_v1(
        &self,
        source: &super::incremental_normal_cognition_service_evidence::FrozenCognitionWakeEvidenceV2,
        common: &mut FormalBoundedContextV1<'_>,
    ) -> Result<(), BodyMutationError> {
        common
            .consume_steps(1)
            .map_err(|error| BodyMutationError::Original(Box::new(error)))?;
        if !self
            .terminal
            .sources
            .owns_cognition_evidence_capture(source)
            || self.terminal.core != common.work().identity()
            || !self.terminal._fee.uses_budget(common.memory())
        {
            return Err(BodyMutationError::Invalid(
                "normal actual sealed Cognition evidence",
            ));
        }
        Ok(())
    }

    pub(in crate::world) fn validate_retention_source_v1(
        &self,
        source: &super::incremental_normal_retention::FrozenNormalRetentionV1,
        common: &mut FormalBoundedContextV1<'_>,
    ) -> Result<(), BodyMutationError> {
        common
            .consume_steps(1)
            .map_err(|e| BodyMutationError::Original(Box::new(e)))?;
        if !self.terminal.sources.owns_retention_capture(source)
            || self.terminal.core != common.work().identity()
            || !self.terminal._fee.uses_budget(common.memory())
        {
            return Err(BodyMutationError::Invalid(
                "normal actual sealed retention source",
            ));
        }
        Ok(())
    }
    pub(in crate::world) fn validate_projection_source_v1(
        &self,
        source: &super::incremental_normal_projection_commitment::FrozenNormalProjectionCommitmentV1,
        common: &mut FormalBoundedContextV1<'_>,
    ) -> Result<(), BodyMutationError> {
        common
            .consume_steps(1)
            .map_err(|e| BodyMutationError::Original(Box::new(e)))?;
        if !self.terminal.sources.owns_projection_capture(source)
            || self.terminal.core != common.work().identity()
            || !self.terminal._fee.uses_budget(common.memory())
        {
            return Err(BodyMutationError::Invalid(
                "normal actual sealed projection source",
            ));
        }
        Ok(())
    }
    pub(in crate::world) fn validate_projection_raw_hash_v1(
        &self,
        raw: &str,
        common: &mut FormalBoundedContextV1<'_>,
    ) -> Result<(), BodyMutationError> {
        let source = self.terminal.sources.projection_capture();
        self.validate_projection_source_v1(source, common)?;
        if !std::ptr::eq(raw, source.raw_state_hash()) {
            return Err(BodyMutationError::Invalid(
                "normal actual raw state hash owner",
            ));
        }
        Ok(())
    }

    pub(in crate::world) fn after_roots(&self) -> [u8; 32] {
        self.after_roots
    }
    pub(in crate::world) fn validate_business_source_v1(
        &self,
        source: &super::incremental_normal_business_commitment::ActualCurrentBusinessDeltaV1,
        common: &mut FormalBoundedContextV1<'_>,
    ) -> Result<(), BodyMutationError> {
        source.validate_context(common)?;
        if !self.terminal.sources.owns_business_capture(source)
            || self.terminal.core != common.work().identity()
            || !self.terminal._fee.uses_budget(common.memory())
        {
            return Err(BodyMutationError::Invalid(
                "normal resident terminal business capture",
            ));
        }
        Ok(())
    }
    pub(in crate::world) fn validate_business_metadata_source_v1(
        &self,
        source: &super::incremental_normal_business_commitment::NormalTerminalReadMetadataV1,
        common: &mut FormalBoundedContextV1<'_>,
    ) -> Result<(), BodyMutationError> {
        source.validate_context(common)?;
        if !self.terminal.sources.owns_terminal_metadata_capture(source)
            || self.terminal.core != common.work().identity()
            || !self.terminal._fee.uses_budget(common.memory())
        {
            return Err(BodyMutationError::Invalid(
                "normal resident terminal business metadata",
            ));
        }
        Ok(())
    }
    pub(in crate::world) fn validate_manifest_source_v1(
        &self,
        source: &super::incremental_module_execution_manifest_hash::CurrentWorldManifestHashV1,
        common: &mut FormalBoundedContextV1<'_>,
    ) -> Result<(), BodyMutationError> {
        if !self.terminal.sources.owns_manifest_capture(source)
            || self.terminal.core != common.work().identity()
            || !self.terminal._fee.uses_budget(common.memory())
        {
            return Err(BodyMutationError::Invalid(
                "normal resident terminal Manifest capture",
            ));
        }
        Ok(())
    }
    pub(in crate::world) fn validate_event_source_v1(
        &self,
        source: &super::incremental_normal_event_commitment::NormalEventCommitmentV1,
        common: &mut FormalBoundedContextV1<'_>,
    ) -> Result<(), BodyMutationError> {
        common
            .charge(1, 32)
            .map_err(|e| BodyMutationError::Original(Box::new(e)))?;
        if !self.terminal.sources.owns_event_capture(source)
            || self.terminal.core != common.work().identity()
            || !self.terminal._fee.uses_budget(common.memory())
        {
            return Err(BodyMutationError::Invalid(
                "normal resident terminal Event capture",
            ));
        }
        Ok(())
    }
    pub(in crate::world) fn validate_delivery_source_v1(
        &self,
        source:&super::incremental_normal_industry_due::delivery::FrozenProductValidationDeliveryIndexV1,
        common: &mut FormalBoundedContextV1<'_>,
    ) -> Result<(), BodyMutationError> {
        if !self.terminal.sources.owns_delivery_capture(source)
            || self.terminal.core != common.work().identity()
            || !self.terminal._fee.uses_budget(common.memory())
        {
            return Err(BodyMutationError::Invalid(
                "normal resident terminal delivery capture",
            ));
        }
        Ok(())
    }
    pub(in crate::world) fn validate_authorization_source_v1(
        &self,
        source:&super::incremental_authorization_root_current::FrozenNormalAuthorizationProjectionV1,
        common: &mut FormalBoundedContextV1<'_>,
    ) -> Result<(), BodyMutationError> {
        if !self.terminal.sources.owns_authorization_capture(source)
            || self.terminal.core != common.work().identity()
            || !self.terminal._fee.uses_budget(common.memory())
        {
            return Err(BodyMutationError::Invalid(
                "normal resident terminal capture",
            ));
        }
        Ok(())
    }
}

pub(in crate::world) struct FinishedNormalTickDirectoriesV1<'p> {
    tick: NormalHostTickDataV1<'p>,
    resident: NormalResidentTerminalSourceV1,
    directories: ReservedValue<FrozenRuntimeDirectoryBundleV1>,
    _fee: MemoryReservation,
    cut: NormalFinishWriteFactCutV1,
    guard: PreparedGuard,
    core: WorkIdentityV1,
    parent_record: [u8; 32],
    input_material: [u8; 32],
    now: u64,
}
impl<'p> FinishedNormalTickDirectoriesV1<'p> {
    pub(in crate::world) fn validate_terminal_v1<M: MerkleNodeReader, OE>(
        &self,
        tx: &mut ExecutionTransaction<'_, M>,
        parent: &BoundNormalTickParentV1<'_>,
        input: &PaidBoundNormalInputDataV1<'_>,
        common: &mut FormalBoundedContextV1<'_>,
        live: &ExecutionFence,
    ) -> Result<(), NormalHostFinishErrorV1<OE>> {
        let result = (|| {
            parent.validate_context(common).map_err(|e| {
                NormalHostFinishErrorV1::Root(BodyMutationError::Original(Box::new(e)).into())
            })?;
            input.validate_context(common).map_err(|e| {
                NormalHostFinishErrorV1::Root(BodyMutationError::Original(Box::new(e)).into())
            })?;
            self.tick
                .input
                .validate(tx, input, common)
                .map_err(|e| NormalHostFinishErrorV1::Root(e.into()))?;
            tx.validate_guard(&self.guard, live)
                .map_err(|e| NormalHostFinishErrorV1::Root(e.into()))?;
            common.charge(1, 96).map_err(|e| {
                NormalHostFinishErrorV1::Root(BodyMutationError::Original(Box::new(e)).into())
            })?;
            if self.core != common.work().identity()
                || self.parent_record != parent.refs().core.candidate_record_id()
                || self.input_material != input.checked().material_id()
                || self.now != input.context().height
                || tx.parent_fence() != parent.fence()
                || !self._fee.uses_budget(common.memory())
                || !self.directories.uses_budget(common.memory())
                || self.cut.fact_count() != tx.ordered_fact_count()
            {
                return Err(NormalHostFinishErrorV1::Root(
                    BodyMutationError::Invalid("normal terminal directory handoff scope").into(),
                ));
            }
            self.directories
                .validate_terminal_guard(tx, common.memory(), live)
                .map_err(NormalHostFinishErrorV1::Directories)?;
            Ok(())
        })();
        if result.is_err() {
            let _: Result<(), ExecutionError> = tx.contribute(|_| Err(ExecutionError::Aborted));
        }
        result
    }
    pub(in crate::world) fn final_sources(
        &self,
    ) -> &std::sync::Arc<
        ReservedValue<super::incremental_normal_final_sources::CapturedNormalFinalSourcesV1>,
    > {
        &self.tick.final_sources
    }
    /// Only the private actual-seal composition consumes these genuine owners.
    /// The old tick strict guard is deliberately never rewritten.
    pub(in crate::world) fn into_parts(
        self,
    ) -> (
        NormalHostTickDataV1<'p>,
        ReservedValue<FrozenRuntimeDirectoryBundleV1>,
        MemoryReservation,
        NormalResidentTerminalSourceV1,
    ) {
        (self.tick, self.directories, self._fee, self.resident)
    }
}
struct External<'a, 'f, 'r, FR, FC, RR, RC> {
    work: NormalHostDirectoryWorkLoanV1<'a>,
    factory: &'a mut FactoryTransitionReadContextV1<'f, FR, FC>,
    recipe: &'a mut IndustryTransitionReadContextV1<'r, RR, RC>,
}
impl<FR, FC, RR, RC> FactoryPhaseMeterLanesV1 for External<'_, '_, '_, FR, FC, RR, RC> {
    fn visit_lanes(&mut self, v: &mut dyn for<'a> FnMut(FactoryPhaseMeterLaneV1<'a>)) {
        use FactoryPhaseMeterLaneV1::{Bytes as B, Entries as N, Steps as S};
        for io in [&mut *self.factory.values.io, &mut *self.recipe.values.io] {
            v(S(&mut io.nodes_remaining));
            v(B(&mut io.read_bytes_remaining));
            v(B(&mut io.write_bytes_remaining));
        }
        v(B(&mut self.factory.key_work.bytes_remaining));
        v(B(&mut self.recipe.key_work.bytes_remaining));
        let w = &mut self.work;
        visit_factory_phase_work_lanes_v1(w.factory, v);
        v(S(&mut w.inventory.visits_remaining));
        v(B(&mut w.inventory.comparison_bytes_remaining));
        v(N(&mut w.inventory.pending_entries_remaining));
        v(S(&mut w.claims.visits_remaining));
        v(B(&mut w.claims.comparison_bytes_remaining));
        v(N(&mut w.claims.pending_entries_remaining));
        v(S(w.grant_cells));
        v(N(w.grant_pending));
        v(S(&mut w.gameplay.visits_remaining));
        v(B(&mut w.gameplay.comparison_bytes_remaining));
        v(N(&mut w.gameplay.pending_entries_remaining));
        v(S(&mut w.threats.visits_remaining));
        v(B(&mut w.threats.comparison_bytes_remaining));
        v(N(&mut w.threats.pending_entries_remaining));
        v(S(&mut w.builds.cells_remaining));
        v(B(&mut w.builds.comparison_bytes_remaining));
        v(S(&mut w.receipts.cells_remaining));
        v(B(&mut w.receipts.bytes_remaining));
    }
}
impl<'a, 't, 's, 'v, M, R, E, C, W, F, O>
    RootModuleExecutionContextV1<'a, 't, 's, 'v, M, R, E, C, W, F, O>
where
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    W: ValueNodeReader + ValueNodeWriter + StatePageWriter,
    F: FnMut(&mut dyn AgentEventOriginalV1) -> Result<(), BodyMutationError>,
    O: OriginalObjectStreamReader,
{
    #[allow(clippy::too_many_arguments)]
    pub(in crate::world) fn finish_normal_tick_directories_v1<'p, FR, FC, RR, RC>(
        &mut self,
        tick: NormalHostTickDataV1<'p>,
        parent: &ReservedValue<BoundNormalTickParentV1<'_>>,
        input: &PaidBoundNormalInputDataV1<'_>,
        caches: &ReservedValue<NormalParentCachesV1>,
        tail: &NormalTickTailSourcesV1<'_>,
        material_parent: &ReservedValue<ParentInventoryMaterialScopeV1>,
        factory: &mut FactoryTransitionReadContextV1<'_, FR, FC>,
        recipe: &mut IndustryTransitionReadContextV1<'_, RR, RC>,
        work: NormalHostDirectoryWorkLoanV1<'_>,
        max_grant_payload: u64,
        common: &mut FormalBoundedContextV1<'_>,
        live_owner: &mut dyn FnMut() -> ExecutionFence,
    ) -> Result<FinishedNormalTickDirectoriesV1<'p>, NormalHostFinishErrorV1<O::Error>>
    where
        FR: ValueNodeReader,
        FC: FnMut() -> bool,
        RR: ValueNodeReader,
        RC: FnMut() -> bool,
    {
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let live = live_owner();
            tick.input
                .validate(self.ctx.transaction, input, common)
                .map_err(|e| NormalHostFinishErrorV1::Root(e.into()))?;
            tick.tick
                .validate_with_final_sources(
                    &tick.final_sources,
                    self.ctx.transaction,
                    parent,
                    input,
                    common,
                    &live,
                )
                .map_err(|e| NormalHostFinishErrorV1::Root(e.into()))?;
            parent
                .validate_module_execution_families_v1(&self.families, common)
                .map_err(|e| {
                    NormalHostFinishErrorV1::Root(BodyMutationError::Original(Box::new(e)).into())
                })?;
            caches
                .validate_parent_v1(parent, common)
                .map_err(|e| NormalHostFinishErrorV1::Root(e.into()))?;
            tail.recipes
                .validate_tail_source_v1(&parent.fence(), common.memory())
                .map_err(|e| NormalHostFinishErrorV1::Root(e.into()))?;
            if factory.values.class != MemoryClass::Ordinary
                || recipe.values.class != MemoryClass::Ordinary
                || !factory.values.memory.same_pool(common.memory())
                || !recipe.values.memory.same_pool(common.memory())
                || !caches.uses_budget(common.memory())
            {
                return Err(NormalHostFinishErrorV1::Root(
                    BodyMutationError::Invalid("normal directory external source pool/class")
                        .into(),
                ));
            }
            common
                .charge(
                    1,
                    std::mem::size_of::<FinishedNormalTickDirectoriesV1<'p>>() as u64,
                )
                .map_err(|e| {
                    NormalHostFinishErrorV1::Root(BodyMutationError::Original(Box::new(e)).into())
                })?;
            let fee = common
                .memory()
                .reserve(
                    std::mem::size_of::<FinishedNormalTickDirectoriesV1<'p>>(),
                    MemoryClass::Ordinary,
                )
                .map_err(|e| NormalHostFinishErrorV1::Root(ValueTreeError::Memory(e).into()))?;
            let before = capture(self.ctx.transaction, work.finish_cut, common, live_owner)
                .map_err(|e| NormalHostFinishErrorV1::Root(e.into()))?;
            let q = parent.refs().storage.current();
            let native = q.native();
            let runtime = q.runtime();
            let accepted = caches.borrow();
            let factory_caches = FactoryPhaseAcceptedCachesV1 {
                orders: accepted.orders,
                records: tail.recipes,
                materials: material_parent,
                invocations: accepted.invocations,
            };
            let families = FactoryPhaseFamiliesV1 {
                orders: native.rules().orders(),
                native: native.native(),
                factories: native.factory(),
                recipes: native.industry(),
                sites: native.factory_authority(),
                anchors: native.agent_authority(),
                revocation: runtime.revocation(),
                artifacts: runtime.resources(),
            };
            let mut external = External {
                work,
                factory,
                recipe,
            };
            let mut first = None;
            let mapped = self.with_factory_prologue_external_meter_v1(
                common,
                live_owner,
                &mut external,
                |root, live, ext| {
                    let live = RefCell::new(live);
                    let stop = root.worker.cancelled;
                    let mut fc = || {
                        let _ = (live.borrow_mut())();
                        (ext.factory.cancelled)() | stop.load(Ordering::Relaxed)
                    };
                    let mut rc = || {
                        let _ = (live.borrow_mut())();
                        (ext.recipe.cancelled)() | stop.load(Ordering::Relaxed)
                    };
                    let mut factory = FactoryTransitionReadContextV1 {
                        values: oasis7_formal_protocol::incremental_value::ValueReadContext {
                            source: ext.factory.values.source,
                            memory: ext.factory.values.memory,
                            class: ext.factory.values.class,
                            io: &mut *ext.factory.values.io,
                        },
                        key_work: &mut *ext.factory.key_work,
                        semantic_bytes: ext.factory.semantic_bytes,
                        record_bytes: ext.factory.record_bytes,
                        max_payload_bytes: ext.factory.max_payload_bytes,
                        cancelled: &mut fc,
                    };
                    let mut recipe = IndustryTransitionReadContextV1 {
                        values: oasis7_formal_protocol::incremental_value::ValueReadContext {
                            source: ext.recipe.values.source,
                            memory: ext.recipe.values.memory,
                            class: ext.recipe.values.class,
                            io: &mut *ext.recipe.values.io,
                        },
                        key_work: &mut *ext.recipe.key_work,
                        semantic_bytes: ext.recipe.semantic_bytes,
                        record_bytes: ext.recipe.record_bytes,
                        max_payload_bytes: ext.recipe.max_payload_bytes,
                        cancelled: &mut rc,
                    };
                    let mut live_owner = || (live.borrow_mut())();
                    match root.freeze_current_runtime_directories_v1(
                        &factory_caches,
                        &families,
                        &mut factory,
                        &mut recipe,
                        ext.work.factory,
                        accepted.inventory_identity,
                        native.inventory(),
                        ext.work.inventory,
                        tail.economic.claims,
                        native.native(),
                        ext.work.claims,
                        tail.grants,
                        native.main_policy(),
                        ext.work.grant_cells,
                        ext.work.grant_pending,
                        max_grant_payload,
                        tail.gameplay,
                        native.diplomacy(),
                        native.governance(),
                        ext.work.gameplay,
                        tail.threats,
                        runtime.business(),
                        ext.work.threats,
                        accepted.builds,
                        accepted.receipts,
                        ext.work.builds,
                        ext.work.receipts,
                        &mut live_owner,
                    ) {
                        Ok(value) => Ok(value),
                        Err(error) => {
                            first = Some(error);
                            Err(RootModuleExecutionErrorV1::Body(
                                BodyMutationError::Invalid(
                                    "directory freeze typed first error retained",
                                ),
                            ))
                        }
                    }
                },
            );
            let directories = if let Some(error) = first {
                return Err(NormalHostFinishErrorV1::Directories(error));
            } else {
                mapped.map_err(NormalHostFinishErrorV1::Root)?
            };
            let after = capture(
                self.ctx.transaction,
                external.work.finish_cut,
                common,
                live_owner,
            )
            .map_err(|e| NormalHostFinishErrorV1::Root(e.into()))?;
            if before != after {
                return Err(NormalHostFinishErrorV1::Root(
                    BodyMutationError::Invalid("directory freeze changed normal writes/facts")
                        .into(),
                ));
            }
            let live = live_owner();
            directories
                .validate_terminal_guard(self.ctx.transaction, common.memory(), &live)
                .map_err(NormalHostFinishErrorV1::Directories)?;
            tick.final_sources
                .validate_cognition_evidence_current_v1(self.ctx.transaction, common, &live)
                .map_err(|error| NormalHostFinishErrorV1::Root(error.into()))?;
            common
                .charge(
                    1,
                    std::mem::size_of::<NormalResidentTerminalSourceV1>() as u64,
                )
                .map_err(|e| {
                    NormalHostFinishErrorV1::Root(BodyMutationError::Original(Box::new(e)).into())
                })?;
            let resident_fee = common
                .memory()
                .reserve(
                    std::mem::size_of::<NormalResidentTerminalSourceV1>(),
                    MemoryClass::Ordinary,
                )
                .map_err(|e| NormalHostFinishErrorV1::Root(ValueTreeError::Memory(e).into()))?;
            let resident = NormalResidentTerminalSourceV1 {
                sources: std::sync::Arc::clone(&tick.final_sources),
                guard: self.ctx.transaction.guard(),
                core: common.work().identity(),
                _fee: resident_fee,
            };
            let output = FinishedNormalTickDirectoriesV1 {
                tick,
                resident,
                directories,
                _fee: fee,
                cut: after,
                guard: self.ctx.transaction.guard(),
                core: common.work().identity(),
                parent_record: parent.refs().core.candidate_record_id(),
                input_material: input.checked().material_id(),
                now: self.now,
            };
            Ok(output)
        }));
        match result {
            Ok(result) => {
                if result.is_err() {
                    let _: Result<(), ExecutionError> = self
                        .ctx
                        .transaction
                        .contribute(|_| Err(ExecutionError::Aborted));
                }
                result
            }
            Err(payload) => {
                let _: Result<(), ExecutionError> = self
                    .ctx
                    .transaction
                    .contribute(|_| Err(ExecutionError::Aborted));
                std::panic::resume_unwind(payload)
            }
        }
    }
}
