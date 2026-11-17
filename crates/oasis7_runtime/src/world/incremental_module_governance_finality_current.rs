//! Actual Current governance finality, local receipts, and epoch-snapshot evidence.
use super::*;

pub(in crate::world) enum CurrentModuleGovernanceFinalityErrorV1 {
    Body(BodyMutationError),
    World {
        error: WorldError,
        fees: ModuleCapabilityAdmissionFeesV1,
    },
}
pub(in crate::world) struct PaidCurrentModuleGovernanceFinalityV1 {
    guard: PreparedGuard,
    core: WorkIdentityV1,
    memory: MemoryBudget,
    now: u64,
    proposal_id: u64,
    approved_hash: String,
    epoch: u64,
    fees: ModuleCapabilityAdmissionFeesV1,
}
impl PaidCurrentModuleGovernanceFinalityV1 {
    pub(in crate::world) fn validate_provenance<M: MerkleNodeReader>(
        &self,
        tx: &oasis7_formal_protocol::incremental_execution::ExecutionTransaction<'_, M>,
        memory: &MemoryBudget,
        core: WorkIdentityV1,
        now: u64,
        id: u64,
        approved_hash: &str,
        epoch: u64,
        live: &ExecutionFence,
    ) -> Result<(), BodyMutationError> {
        tx.validate_transaction_identity(&self.guard)?;
        tx.validate_guard(&tx.guard(), live)?;
        if !self.memory.same_pool(memory)
            || !tx.uses_budget(memory)
            || !self.fees._layout.uses_budget(memory)
            || self.core != core
            || self.now != now
            || self.proposal_id != id
            || self.approved_hash != approved_hash
            || self.epoch != epoch
        {
            return Err(bad());
        }
        Ok(())
    }
}
#[allow(clippy::too_many_arguments)]
pub(in crate::world) fn validate_current_module_governance_finality_v1<
    M: MerkleNodeReader,
    P: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    PE: FnMut() -> Option<ValueTreeError>,
    PC: FnMut() -> bool,
>(
    proposal_id: u64,
    approved_hash: &str,
    epoch: u64,
    certificate: &GovernanceFinalityCertificate,
    sources: &ModuleLifecycleSourcesV1<'_>,
    ctx: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    parent: &mut WorldFieldReadContextV1<'_, P, R, PE, PC>,
    factory: &mut FactoryScheduleWorkV1,
    module: &mut ModuleLifecycleWorkV1<'_>,
    common: &mut FormalBoundedContextV1<'_>,
    live: &ExecutionFence,
) -> Result<PaidCurrentModuleGovernanceFinalityV1, CurrentModuleGovernanceFinalityErrorV1> {
    let result = ctx
        .transaction
        .contribute(|tx| -> Result<_, BodyMutationError> {
            tx.validate_guard(&tx.guard(), live)?;
            if !ctx.values.memory.same_pool(common.memory()) {
                return Err(bad());
            }
            let n = approved_hash
                .len()
                .checked_add(std::mem::size_of::<PaidCurrentModuleGovernanceFinalityV1>() + 16384)
                .ok_or_else(bad)?;
            common
                .charge(1, n as u64)
                .map_err(|e| BodyMutationError::Original(Box::new(e)))?;
            let layout = common
                .memory()
                .reserve(n, MemoryClass::Ordinary)
                .map_err(ValueTreeError::Memory)?;
            let slots = common
                .memory()
                .reserve(
                    32 * std::mem::size_of::<MemoryReservation>(),
                    MemoryClass::Ordinary,
                )
                .map_err(ValueTreeError::Memory)?;
            let mut overlay = WorldFieldOverlayContextV1 {
                transaction: tx,
                values: ValueReadContext {
                    source: ctx.values.source,
                    memory: ctx.values.memory,
                    class: ctx.values.class,
                    io: &mut *ctx.values.io,
                },
                key_work: &mut *ctx.key_work,
                semantic_bytes: ctx.semantic_bytes,
                record_bytes: ctx.record_bytes,
                take_state_error: &mut ctx.take_state_error,
                cancelled: &mut ctx.cancelled,
            };
            let mut current = Current {
                sources,
                ctx: &mut overlay,
                parent,
                factory,
                module,
                common,
                live,
                bootstrap_canonical: None,
                held: ModuleCapabilityAdmissionFeesV1 {
                    fees: Vec::with_capacity(32),
                    slots,
                    _layout: layout,
                },
            };
            let business =
                match current.verify_certificate(proposal_id, approved_hash, epoch, certificate) {
                    Ok(()) => None,
                    Err(Error::World(error)) => Some(error),
                    Err(Error::Body(error)) => return Err(error),
                };
            if let Some(error) = business {
                return Ok(Err((error, current.held)));
            }
            let now = match current.tick() {
                Ok(now) => now,
                Err(Error::Body(error)) => return Err(error),
                Err(Error::World(error)) => return Ok(Err((error, current.held))),
            };
            current
                .ctx
                .transaction
                .validate_guard(&current.ctx.transaction.guard(), live)?;
            Ok(Ok(PaidCurrentModuleGovernanceFinalityV1 {
                guard: current.ctx.transaction.guard(),
                core: current.common.work().identity(),
                memory: current.common.memory().clone(),
                now,
                proposal_id,
                approved_hash: approved_hash.to_owned(),
                epoch,
                fees: current.held,
            }))
        })
        .map_err(CurrentModuleGovernanceFinalityErrorV1::Body)?;
    result.map_err(|(error, fees)| CurrentModuleGovernanceFinalityErrorV1::World { error, fees })
}

pub(in crate::world) struct PaidCurrentLocalModuleGovernanceFinalityV1 {
    certificate: GovernanceFinalityCertificate,
    fees: ModuleCapabilityAdmissionFeesV1,
    guard: PreparedGuard,
    core: WorkIdentityV1,
    memory: MemoryBudget,
    now: u64,
}
impl PaidCurrentLocalModuleGovernanceFinalityV1 {
    pub(in crate::world) fn certificate(&self) -> &GovernanceFinalityCertificate {
        &self.certificate
    }
    pub(in crate::world) fn validate_provenance<M: MerkleNodeReader>(
        &self,
        tx: &oasis7_formal_protocol::incremental_execution::ExecutionTransaction<'_, M>,
        memory: &MemoryBudget,
        core: WorkIdentityV1,
        now: u64,
        id: u64,
        live: &ExecutionFence,
    ) -> Result<(), BodyMutationError> {
        tx.validate_transaction_identity(&self.guard)?;
        tx.validate_guard(&tx.guard(), live)?;
        if !self.memory.same_pool(memory)
            || !tx.uses_budget(memory)
            || self.core != core
            || self.now != now
            || self.certificate.proposal_id != id
        {
            return Err(bad());
        }
        Ok(())
    }
    pub(in crate::world) fn into_parts(
        self,
    ) -> (
        GovernanceFinalityCertificate,
        ModuleCapabilityAdmissionFeesV1,
    ) {
        (self.certificate, self.fees)
    }
}
#[allow(clippy::too_many_arguments)]
pub(in crate::world) fn build_current_local_module_governance_finality_v1<
    M: MerkleNodeReader,
    P: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    PE: FnMut() -> Option<ValueTreeError>,
    PC: FnMut() -> bool,
>(
    proposal_id: u64,
    counter: &crate::world::incremental_world_event_counter::WorldEventCounterV1,
    sources: &ModuleLifecycleSourcesV1<'_>,
    ctx: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    parent: &mut WorldFieldReadContextV1<'_, P, R, PE, PC>,
    factory: &mut FactoryScheduleWorkV1,
    module: &mut ModuleLifecycleWorkV1<'_>,
    common: &mut FormalBoundedContextV1<'_>,
    live: &ExecutionFence,
) -> Result<PaidCurrentLocalModuleGovernanceFinalityV1, CurrentModuleGovernanceFinalityErrorV1> {
    let result = ctx
        .transaction
        .contribute(|tx| -> Result<_, BodyMutationError> {
            tx.validate_guard(&tx.guard(), live)?;
            if !ctx.values.memory.same_pool(common.memory()) {
                return Err(bad());
            }
            let n = std::mem::size_of::<PaidCurrentLocalModuleGovernanceFinalityV1>() + 16384;
            common
                .charge(1, n as u64)
                .map_err(|e| BodyMutationError::Original(Box::new(e)))?;
            let layout = common
                .memory()
                .reserve(n, MemoryClass::Ordinary)
                .map_err(ValueTreeError::Memory)?;
            let slots = common
                .memory()
                .reserve(
                    32 * std::mem::size_of::<MemoryReservation>(),
                    MemoryClass::Ordinary,
                )
                .map_err(ValueTreeError::Memory)?;
            let mut overlay = WorldFieldOverlayContextV1 {
                transaction: tx,
                values: ValueReadContext {
                    source: ctx.values.source,
                    memory: ctx.values.memory,
                    class: ctx.values.class,
                    io: &mut *ctx.values.io,
                },
                key_work: &mut *ctx.key_work,
                semantic_bytes: ctx.semantic_bytes,
                record_bytes: ctx.record_bytes,
                take_state_error: &mut ctx.take_state_error,
                cancelled: &mut ctx.cancelled,
            };
            let mut current = Current {
                sources,
                ctx: &mut overlay,
                parent,
                factory,
                module,
                common,
                live,
                bootstrap_canonical: None,
                held: ModuleCapabilityAdmissionFeesV1 {
                    fees: Vec::with_capacity(32),
                    slots,
                    _layout: layout,
                },
            };
            let certificate = match current.local_certificate(proposal_id, counter) {
                Ok(value) => value,
                Err(Error::World(error)) => return Ok(Err((error, current.held))),
                Err(Error::Body(error)) => return Err(error),
            };
            let now = match current.tick() {
                Ok(now) => now,
                Err(Error::Body(error)) => return Err(error),
                Err(Error::World(error)) => return Ok(Err((error, current.held))),
            };
            current
                .ctx
                .transaction
                .validate_guard(&current.ctx.transaction.guard(), live)?;
            Ok(Ok(PaidCurrentLocalModuleGovernanceFinalityV1 {
                certificate,
                guard: current.ctx.transaction.guard(),
                core: current.common.work().identity(),
                memory: current.common.memory().clone(),
                now,
                fees: current.held,
            }))
        })
        .map_err(CurrentModuleGovernanceFinalityErrorV1::Body)?;
    result.map_err(|(error, fees)| CurrentModuleGovernanceFinalityErrorV1::World { error, fees })
}

/// Actual Current epoch authority data for the release attestation threshold.
/// This is neither an accepted release nor a finality certificate.
pub(in crate::world) struct PaidCurrentModuleEpochSnapshotV1 {
    snapshot: GovernanceFinalityEpochSnapshot,
    fees: ModuleCapabilityAdmissionFeesV1,
    guard: PreparedGuard,
    core: WorkIdentityV1,
    memory: MemoryBudget,
    now: u64,
    actual_epoch: u64,
}
impl PaidCurrentModuleEpochSnapshotV1 {
    pub(in crate::world) fn snapshot(&self) -> &GovernanceFinalityEpochSnapshot {
        &self.snapshot
    }
    pub(in crate::world) fn now(&self) -> u64 {
        self.now
    }
    pub(in crate::world) fn epoch_id(&self) -> u64 {
        self.actual_epoch
    }
    pub(in crate::world) fn validate<M: MerkleNodeReader>(
        &self,
        tx: &oasis7_formal_protocol::incremental_execution::ExecutionTransaction<'_, M>,
        memory: &MemoryBudget,
        core: WorkIdentityV1,
        now: u64,
        live: &ExecutionFence,
    ) -> Result<(), BodyMutationError> {
        tx.validate_transaction_identity(&self.guard)?;
        tx.validate_guard(&tx.guard(), live)?;
        if !self.memory.same_pool(memory)
            || !tx.uses_budget(memory)
            || self.core != core
            || self.now != now
            || !self.fees._layout.uses_budget(memory)
        {
            return Err(bad());
        }
        Ok(())
    }
}
#[allow(clippy::too_many_arguments)]
pub(in crate::world) fn read_current_module_epoch_snapshot_v1<
    M: MerkleNodeReader,
    P: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    PE: FnMut() -> Option<ValueTreeError>,
    PC: FnMut() -> bool,
>(
    sources: &ModuleLifecycleSourcesV1<'_>,
    ctx: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    parent: &mut WorldFieldReadContextV1<'_, P, R, PE, PC>,
    factory: &mut FactoryScheduleWorkV1,
    module: &mut ModuleLifecycleWorkV1<'_>,
    common: &mut FormalBoundedContextV1<'_>,
    live: &ExecutionFence,
) -> Result<PaidCurrentModuleEpochSnapshotV1, CurrentModuleGovernanceFinalityErrorV1> {
    let result=ctx.transaction.contribute(|tx|->Result<_,BodyMutationError>{
        tx.validate_guard(&tx.guard(),live)?;
        if !ctx.values.memory.same_pool(common.memory()){return Err(bad())}
        let n=std::mem::size_of::<PaidCurrentModuleEpochSnapshotV1>()+16384;
        common.charge(1,n as u64).map_err(|error|BodyMutationError::Original(Box::new(error)))?;
        let layout=common.memory().reserve(n,MemoryClass::Ordinary).map_err(ValueTreeError::Memory)?;
        let slots=common.memory().reserve(32*std::mem::size_of::<MemoryReservation>(),MemoryClass::Ordinary).map_err(ValueTreeError::Memory)?;
        let mut overlay=WorldFieldOverlayContextV1{transaction:tx,values:ValueReadContext{source:ctx.values.source,memory:ctx.values.memory,class:ctx.values.class,io:&mut*ctx.values.io},key_work:&mut*ctx.key_work,semantic_bytes:ctx.semantic_bytes,record_bytes:ctx.record_bytes,take_state_error:&mut ctx.take_state_error,cancelled:&mut ctx.cancelled};
        let mut current=Current{sources,ctx:&mut overlay,parent,factory,module,common,live,bootstrap_canonical:None,held:ModuleCapabilityAdmissionFeesV1{fees:Vec::with_capacity(32),slots,_layout:layout}};
        let make=|current:&mut Current<'_,'_,'_,'_,'_,'_,'_,'_,M,P,R,_,_,PE,PC>|->Result<_,Error>{
            let now=current.tick()?;
            let cap=current.sources.capability;
            let length=with_module_lifecycle_read_io_v1(cap.roots,current.ctx,current.parent,current.factory,current.common,current.module,|ctx,parent,factory,module|{
                let guard=ctx.transaction.guard();Ok(oasis7_world_state::incremental_runtime_governance::read_current_governance_epoch_length_v1(cap.governance,cap.roots,parent.state,ctx,&mut factory.lifecycle.native_scan,module.max_selected_heap,&guard,live).map_err(super::overlay)?)
            }).map_err(read_error)?;
            let epoch=now/length.max(1);
            if let Some(selected)=current.governance(RuntimeGovernanceKeyV1::Epoch(epoch))? {
                let RecoveredRuntimeGovernanceRecordV1::EpochSnapshot(snapshot)=selected.value()else{return Err(bad().into())};
                current.record(snapshot)?;
                return Ok((snapshot.clone(),now,epoch));
            }
            let (records,length)=current.finality_records(0)?;
            // The original registry projection includes retained rotations and
            // the explicit legacy local fallback. Pay before its map copies.
            current.record(&(&records.registry,&records.admissions,&records.rotations,&records.bootstrap_rotations))?;
            let view=crate::world::governance::module_admission_finality::ModuleFinalityRecordsV1{registry:&records.registry,admissions:&records.admissions,rotations:&records.rotations,bootstrap_rotations:&records.bootstrap_rotations,epoch_length:length};
            let snapshot=view.fallback_epoch_snapshot_v1(epoch,epoch);
            current.record(&snapshot)?;
            Ok((snapshot,now,epoch))
        };
        let (snapshot,now,actual_epoch)=match make(&mut current){Ok(value)=>value,Err(Error::World(error))=>return Ok(Err((error,current.held))),Err(Error::Body(error))=>return Err(error)};
        Ok(Ok(PaidCurrentModuleEpochSnapshotV1{snapshot,fees:current.held,guard:current.ctx.transaction.guard(),core:current.common.work().identity(),memory:current.common.memory().clone(),now,actual_epoch}))
    }).map_err(CurrentModuleGovernanceFinalityErrorV1::Body)?;
    result.map_err(|(error, fees)| CurrentModuleGovernanceFinalityErrorV1::World { error, fees })
}
