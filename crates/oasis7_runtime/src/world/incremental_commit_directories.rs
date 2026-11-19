//! Actual sole-tx directory closure -> actual seal -> actual History candidate.
//! No public commit ID, acceptance receipt, or caller RootSet can mint this owner.
use super::*;
use crate::world::incremental_directory_bundle::{
    FrozenRuntimeDirectoryBundleV1, RuntimeDirectoryFreezeErrorV1, SealedRuntimeDirectoryBundleV1,
    bind_runtime_directories_to_actual_seal_v1,
};
use oasis7_formal_protocol::incremental_value::ValueTreeError;
use oasis7_world_state::incremental_inventory::{
    InventoryIdentityDirectoryWorkV1, InventoryMaterialScopeWorkV1,
};
use oasis7_world_state::incremental_normal_gameplay_cycles_directory::GameplayDirectoryWorkV1;
use oasis7_world_state::incremental_runtime_business::ThreatKeyDirectoryWorkV1;
use oasis7_world_state::incremental_world_native::IndustrialClaimDirectoryWorkV1;
use oasis7_world_state::{
    incremental_factory_schedule_index::FactoryScheduleIndexWorkV1,
    incremental_factory_schedule_receipts::FactoryScheduleReceiptWorkV1,
};
pub(in crate::world) struct CandidateBoundRuntimeDirectoriesV1 {
    sealed: ReservedValue<SealedRuntimeDirectoryBundleV1>,
    commit: [u8; 32],
    after_roots: [u8; 32],
    parent: ExecutionFence,
}
impl std::fmt::Debug for CandidateBoundRuntimeDirectoriesV1 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CandidateBoundRuntimeDirectoriesV1")
            .field("commit", &self.commit)
            .field("after_roots", &self.after_roots)
            .finish_non_exhaustive()
    }
}
impl CandidateBoundRuntimeDirectoriesV1 {
    pub(in crate::world) fn candidate_commit(&self) -> [u8; 32] {
        self.commit
    }
    pub(in crate::world) fn after_roots_digest(&self) -> [u8; 32] {
        self.after_roots
    }
    pub(in crate::world) fn sealed_directories(
        &self,
    ) -> &ReservedValue<SealedRuntimeDirectoryBundleV1> {
        &self.sealed
    }
    pub(in crate::world) fn validate_candidate(
        &self,
        prepared: &PreparedRuntimeCommit,
        memory: &MemoryBudget,
    ) -> Result<(), ExecutionError> {
        if !self.sealed.uses_budget(memory)
            || !prepared.transition.uses_budget(memory)
            || self.commit != prepared.commit_id()
            || self.parent != prepared.expected_fence()
            || self.after_roots != self.sealed.after_roots_digest()
        {
            return Err(ExecutionError::Invalid("directory candidate binding"));
        }
        let _phase = memory.reserve(4096, MemoryClass::Ordinary)?;
        if self.after_roots != prepared.transition.record.after.digest()?
            || self.after_roots != prepared.history.body.roots
            || self.commit != prepared.history.body.digest()?
        {
            return Err(ExecutionError::Invalid(
                "directory candidate History binding",
            ));
        }
        Ok(())
    }
}
/// Private child constructor: only the parent's real HeaderPreparation path
/// can supply this function. A public PreparedTransition is never an input to
/// the public enabling entrypoint and cannot manufacture a sealed bundle.
pub(super) fn bind_actual_candidate_v1(
    sealed: ReservedValue<SealedRuntimeDirectoryBundleV1>,
    transition: &ReservedValue<PreparedTransition>,
    history: &ReservedValue<PreparedHistoryV1>,
    catalog: &ReservedValue<CatalogDeltaV1>,
    parent: &ExecutionFence,
    memory: &MemoryBudget,
) -> Result<ReservedValue<CandidateBoundRuntimeDirectoriesV1>, ExecutionError> {
    if !sealed.uses_budget(memory)
        || !transition.uses_budget(memory)
        || !history.uses_budget(memory)
        || !catalog.uses_budget(memory)
    {
        return Err(ExecutionError::Invalid("directory candidate memory pool"));
    }
    let fee = memory.reserve(
        std::mem::size_of::<CandidateBoundRuntimeDirectoriesV1>() + 4096,
        MemoryClass::Ordinary,
    )?;
    let commit = history.body.digest()?;
    let after_roots = transition.record.after.digest()?;
    if sealed.after_roots_digest() != after_roots
        || history.body.roots != after_roots
        || catalog.entry.commit != commit
        || transition.record.parent_commit != parent.parent_commit
        || transition.record.before.digest()? != parent.parent_roots
    {
        return Err(ExecutionError::Invalid(
            "directory candidate real History mismatch",
        ));
    }
    Ok(fee.retain(CandidateBoundRuntimeDirectoriesV1 {
        sealed,
        commit,
        after_roots,
        parent: *parent,
    }))
}
/// Basic profile, matching the original basic prepare entrypoint. This owns a
/// genuine directory bundle but cannot substitute for complete Result originals.
#[allow(clippy::too_many_arguments)]
pub(in crate::world) fn prepare_iws_runtime_commit_with_directories_v1<
    'a,
    R: MerkleNodeReader,
    E: From<ExecutionError> + From<RuntimeDirectoryFreezeErrorV1>,
>(
    request: &CommitPreparationRequest<'_>,
    source: &'a R,
    memory: &'a MemoryBudget,
    live_fence: impl FnMut() -> Result<ExecutionFence, E>,
    apply: impl FnOnce(
        &mut ExecutionTransaction<'a, R>,
        &mut InventoryIdentityDirectoryWorkV1,
        &mut IndustrialClaimDirectoryWorkV1,
        &mut u64,
        &mut usize,
        &mut GameplayDirectoryWorkV1,
        &mut ThreatKeyDirectoryWorkV1,
        &mut FactoryScheduleIndexWorkV1,
        &mut FactoryScheduleReceiptWorkV1,
    ) -> Result<ReservedValue<FrozenRuntimeDirectoryBundleV1>, E>,
    material_work: &mut InventoryMaterialScopeWorkV1,
    identity_work: &mut InventoryIdentityDirectoryWorkV1,
    claim_work: &mut IndustrialClaimDirectoryWorkV1,
    grant_cells: &mut u64,
    grant_pending: &mut usize,
    gameplay_work: &mut GameplayDirectoryWorkV1,
    threat_work: &mut ThreatKeyDirectoryWorkV1,
    build_work: &mut FactoryScheduleIndexWorkV1,
    receipt_work: &mut FactoryScheduleReceiptWorkV1,
    mut cancelled: impl FnMut() -> bool,
) -> Result<ReservedValue<PreparedRuntimeCommit>, E> {
    let identity_work = std::cell::RefCell::new(identity_work);
    let claim_work = std::cell::RefCell::new(claim_work);
    let grant_cells = std::cell::RefCell::new(grant_cells);
    let grant_pending = std::cell::RefCell::new(grant_pending);
    let gameplay_work = std::cell::RefCell::new(gameplay_work);
    let threat_work = std::cell::RefCell::new(threat_work);
    let build_work = std::cell::RefCell::new(build_work);
    let receipt_work = std::cell::RefCell::new(receipt_work);
    let value = prepare_commit(
        request,
        source,
        memory,
        live_fence,
        |tx| {
            apply(
                tx,
                &mut identity_work.borrow_mut(),
                &mut claim_work.borrow_mut(),
                &mut grant_cells.borrow_mut(),
                &mut grant_pending.borrow_mut(),
                &mut gameplay_work.borrow_mut(),
                &mut threat_work.borrow_mut(),
                &mut build_work.borrow_mut(),
                &mut receipt_work.borrow_mut(),
            )
        },
        |mut tx, guard, live, tick, frozen| {
            frozen
                .validate_terminal_guard(&mut tx, memory, live)
                .map_err(E::from)?;
            if cancelled() {
                return Err(
                    ExecutionError::Invalid("directory commit preparation cancelled").into(),
                );
            }
            let sealed = tx.seal_with_binding(guard, live, tick).map_err(E::from)?;
            let (transition, binding) = sealed.into_parts();
            let directories = bind_runtime_directories_to_actual_seal_v1(
                frozen,
                binding,
                &transition.record.after,
                memory,
                material_work,
                &mut identity_work.borrow_mut(),
                &mut claim_work.borrow_mut(),
                &mut grant_cells.borrow_mut(),
                &mut gameplay_work.borrow_mut(),
                &mut threat_work.borrow_mut(),
                &mut build_work.borrow_mut(),
                &mut receipt_work.borrow_mut(),
                &mut cancelled,
            )
            .map_err(E::from)?;
            Ok((transition, Some(directories)))
        },
    )?;
    if cancelled() {
        return Err(ExecutionError::Invalid("directory commit preparation cancelled").into());
    }
    value
        .require_directories_v1()
        .map_err(E::from)?
        .validate_candidate(&value, memory)
        .map_err(E::from)?;
    Ok(value)
}
/// Complete-profile variant: final Result originals are produced only from the
/// actual frozen after roots by the existing real seal producer boundary.
#[allow(clippy::too_many_arguments)]
pub(in crate::world) fn prepare_iws_runtime_commit_with_results_and_directories_v1<
    'a,
    R: MerkleNodeReader,
    E: From<ExecutionError> + From<RuntimeDirectoryFreezeErrorV1>,
>(
    request: &CommitPreparationRequest<'_>,
    source: &'a R,
    memory: &'a MemoryBudget,
    live_fence: impl FnMut() -> Result<ExecutionFence, E>,
    apply: impl FnOnce(
        &mut ExecutionTransaction<'a, R>,
        &mut InventoryIdentityDirectoryWorkV1,
        &mut IndustrialClaimDirectoryWorkV1,
        &mut u64,
        &mut usize,
        &mut GameplayDirectoryWorkV1,
        &mut ThreatKeyDirectoryWorkV1,
        &mut FactoryScheduleIndexWorkV1,
        &mut FactoryScheduleReceiptWorkV1,
    ) -> Result<ReservedValue<FrozenRuntimeDirectoryBundleV1>, E>,
    material_work: &mut InventoryMaterialScopeWorkV1,
    identity_work: &mut InventoryIdentityDirectoryWorkV1,
    claim_work: &mut IndustrialClaimDirectoryWorkV1,
    grant_cells: &mut u64,
    grant_pending: &mut usize,
    gameplay_work: &mut GameplayDirectoryWorkV1,
    threat_work: &mut ThreatKeyDirectoryWorkV1,
    build_work: &mut FactoryScheduleIndexWorkV1,
    receipt_work: &mut FactoryScheduleReceiptWorkV1,
    mut cancelled: impl FnMut() -> bool,
    produce: impl FnOnce(&RootSetV1) -> Result<[ExecutionFactRefV1; 2], E>,
) -> Result<ReservedValue<PreparedRuntimeCommit>, E> {
    let identity_work = std::cell::RefCell::new(identity_work);
    let claim_work = std::cell::RefCell::new(claim_work);
    let grant_cells = std::cell::RefCell::new(grant_cells);
    let grant_pending = std::cell::RefCell::new(grant_pending);
    let gameplay_work = std::cell::RefCell::new(gameplay_work);
    let threat_work = std::cell::RefCell::new(threat_work);
    let build_work = std::cell::RefCell::new(build_work);
    let receipt_work = std::cell::RefCell::new(receipt_work);
    let value = prepare_commit(
        request,
        source,
        memory,
        live_fence,
        |tx| {
            apply(
                tx,
                &mut identity_work.borrow_mut(),
                &mut claim_work.borrow_mut(),
                &mut grant_cells.borrow_mut(),
                &mut grant_pending.borrow_mut(),
                &mut gameplay_work.borrow_mut(),
                &mut threat_work.borrow_mut(),
                &mut build_work.borrow_mut(),
                &mut receipt_work.borrow_mut(),
            )
        },
        |mut tx, guard, live, tick, frozen| {
            frozen
                .validate_terminal_guard(&mut tx, memory, live)
                .map_err(E::from)?;
            let sealed = tx
                .seal_with_final_results_and_binding(guard, live, tick, &mut cancelled, produce)
                .map_err(|error| match error {
                    SealWithFinalFactError::Execution(error) => E::from(error),
                    SealWithFinalFactError::Producer(error) => error,
                    SealWithFinalFactError::Cancelled => E::from(ExecutionError::Invalid(
                        "directory commit preparation cancelled",
                    )),
                })?;
            let (transition, binding) = sealed.into_parts();
            let directories = bind_runtime_directories_to_actual_seal_v1(
                frozen,
                binding,
                &transition.record.after,
                memory,
                material_work,
                &mut identity_work.borrow_mut(),
                &mut claim_work.borrow_mut(),
                &mut grant_cells.borrow_mut(),
                &mut gameplay_work.borrow_mut(),
                &mut threat_work.borrow_mut(),
                &mut build_work.borrow_mut(),
                &mut receipt_work.borrow_mut(),
                &mut cancelled,
            )
            .map_err(E::from)?;
            Ok((transition, Some(directories)))
        },
    )?;
    if cancelled() {
        return Err(ExecutionError::Invalid("directory commit preparation cancelled").into());
    }
    value
        .require_directories_v1()
        .map_err(E::from)?
        .validate_candidate(&value, memory)
        .map_err(E::from)?;
    Ok(value)
}

#[derive(Debug)]
pub enum RuntimeDirectoryCacheErrorV1 {
    Common(oasis7_formal_protocol::controlled_authority::formal::FormalError),
    Execution(ExecutionError),
    Value(ValueTreeError),
    Directory(RuntimeDirectoryFreezeErrorV1),
}
impl From<RuntimeDirectoryFreezeErrorV1> for RuntimeDirectoryCacheErrorV1 {
    fn from(e: RuntimeDirectoryFreezeErrorV1) -> Self {
        Self::Directory(e)
    }
}
impl std::fmt::Display for RuntimeDirectoryCacheErrorV1 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Common(e) => e.fmt(f),
            Self::Execution(e) => e.fmt(f),
            Self::Value(e) => e.fmt(f),
            Self::Directory(e) => write!(f, "{e:?}"),
        }
    }
}
impl std::error::Error for RuntimeDirectoryCacheErrorV1 {}
impl From<ExecutionError> for RuntimeDirectoryCacheErrorV1 {
    fn from(e: ExecutionError) -> Self {
        Self::Execution(e)
    }
}
impl From<oasis7_proto::memory_budget::MemoryLimitExceeded> for RuntimeDirectoryCacheErrorV1 {
    fn from(e: oasis7_proto::memory_budget::MemoryLimitExceeded) -> Self {
        Self::Execution(ExecutionError::Memory(e))
    }
}

/// Prepaid structural cache tied to a genuinely checked candidate. Retaining
/// the whole immutable producer avoids cloning or detaching its sealed owners.
/// This is not accepted until the exact formal commit is supplied to finish.
pub struct PreparedRuntimeDirectoryCacheV1 {
    candidate: std::sync::Arc<
        ReservedValue<crate::world::incremental_normal_cache_promotion::NormalWarmCacheBackingV1>,
    >,
    record: [u8; 32],
    body: [u8; 32],
    roots: [u8; 32],
    fee: ReservedValue<()>,
    parents: ReservedValue<crate::world::incremental_normal_parent_caches::NormalParentCachesV1>,
}
/// A real accepted directory holder. Its identifiers are DATA, never a lease
/// or execution permission; the Host must still bind its live source/fence.
pub struct AcceptedRuntimeDirectoryCacheV1 {
    candidate: std::sync::Arc<
        ReservedValue<crate::world::incremental_normal_cache_promotion::NormalWarmCacheBackingV1>,
    >,
    record: [u8; 32],
    body: [u8; 32],
    roots: [u8; 32],
    fee: ReservedValue<()>,
    parents: ReservedValue<crate::world::incremental_normal_parent_caches::NormalParentCachesV1>,
}
impl PreparedRuntimeCommit {
    /// All resource admission precedes durable acceptance. Compatibility
    /// candidates lacking the mandatory actual-seal directory bundle fail.
    pub fn prepare_directory_cache_bounded_v1(
        self,
        checked: &ReservedValue<oasis7_formal_protocol::incremental_material::CheckedCommitCore>,
        common: &mut oasis7_formal_protocol::controlled_authority::formal::bounded_context::FormalBoundedContextV1<'_>,
    ) -> Result<PreparedRuntimeDirectoryCacheV1, RuntimeDirectoryCacheErrorV1> {
        common
            .charge(32, 32 * 16)
            .map_err(RuntimeDirectoryCacheErrorV1::Common)?;
        let directories = self.require_directories_v1()?;
        let memory = common.memory();
        if !checked.uses_budget(memory) || !directories.uses_budget(memory)
            || !directories.sealed.uses_budget(memory) || !self.transition.uses_budget(memory)
            || !self.history.uses_budget(memory) || !self.catalog.uses_budget(memory)
            || !self.root_set_bytes.uses_budget(memory) || !self.catalog_bytes.uses_budget(memory)
            || directories.commit!=self.commit_id()
            || directories.parent!=self.fence
            || directories.after_roots!=directories.sealed.after_roots_digest()
            // Checked Core authenticated both the body and its catalog entry.
            // Comparing that actual body id avoids another codec/hash/clone.
            || checked.catalog().entry.commit!=self.commit_id()
            || checked.body().roots!=directories.after_roots
            || checked.body().parent_commit!=Some(self.fence.parent_commit)
            || checked.body().authority!=self.fence.authority
            || checked.parent_body().is_none_or(|body|body.roots!=self.fence.parent_roots)
            || checked.transition().is_none()
        {
            return Err(ExecutionError::Invalid(
                "Runtime directory checked candidate cache binding",
            )
            .into());
        }
        let warm =
            crate::world::incremental_normal_cache_promotion::prepare_normal_cache_promotion_v1(
                &directories.sealed,
                &self.transition.record.after,
                checked,
                common,
            )?;
        let memory = common.memory();
        let record = checked.candidate_record_id();
        let body = directories.commit;
        let roots = directories.after_roots;
        let bytes = std::mem::size_of::<PreparedRuntimeDirectoryCacheV1>()
            .max(std::mem::size_of::<AcceptedRuntimeDirectoryCacheV1>());
        let fee = memory.reserve(bytes, MemoryClass::Ordinary)?.retain(());
        let candidate =
            crate::world::incremental_normal_cache_promotion::NormalWarmCacheBackingV1::prepare(
                self, warm, memory,
            )?;
        let parents=crate::world::incremental_normal_parent_caches::NormalParentCachesV1::from_warm_backing_v1(std::sync::Arc::clone(&candidate),memory).map_err(RuntimeDirectoryCacheErrorV1::Value)?;
        Ok(PreparedRuntimeDirectoryCacheV1 {
            candidate,
            record,
            body,
            roots,
            fee,
            parents,
        })
    }
}
impl PreparedRuntimeDirectoryCacheV1 {
    /// Only exact formal identity comparison and prepaid owner movement happen
    /// after acceptance. No allocation, hash, codec, IO or cancellation point.
    pub fn finish(
        self,
        accepted: &oasis7_formal_protocol::controlled_authority::formal::VerifiedFormalCommitV1,
    ) -> Result<AcceptedRuntimeDirectoryCacheV1, ExecutionError> {
        if accepted.verified_record_id() != self.record {
            return Err(ExecutionError::Invalid(
                "Runtime directory accepted cache record",
            ));
        }
        Ok(AcceptedRuntimeDirectoryCacheV1 {
            candidate: self.candidate,
            record: self.record,
            body: self.body,
            roots: self.roots,
            fee: self.fee,
            parents: self.parents,
        })
    }
}
impl AcceptedRuntimeDirectoryCacheV1 {
    pub(in crate::world) fn normal_material_scope_v1(&self)->&ReservedValue<oasis7_world_state::incremental_inventory::ParentInventoryMaterialScopeV1>{self.candidate.sealed().material_parent()}
    pub(in crate::world) fn normal_parent_caches_v1(
        &self,
    ) -> &ReservedValue<crate::world::incremental_normal_parent_caches::NormalParentCachesV1> {
        &self.parents
    }
    pub(in crate::world) fn normal_warm_parents_v1(
        &self,
    ) -> &crate::world::incremental_normal_cache_promotion::PreparedNormalParentCachesV1 {
        self.candidate.warm()
    }
    pub fn bind_normal_warm_cache_v1<'a>(
        &'a self,
        history: &ReservedValue<
            oasis7_world_state::incremental_runtime_history_source::AcceptedRuntimeHistorySourceV1,
        >,
        head: &oasis7_formal_protocol::controlled_authority::formal::VerifiedFormalCommitV1,
        live: &ExecutionFence,
        common:&mut oasis7_formal_protocol::controlled_authority::formal::bounded_context::FormalBoundedContextV1<'_>,
    ) -> Result<
        ReservedValue<crate::world::incremental_normal_cache_promotion::NormalWarmParentRefsV1<'a>>,
        RuntimeDirectoryCacheErrorV1,
    > {
        common
            .charge(1, 32)
            .map_err(RuntimeDirectoryCacheErrorV1::Common)?;
        if live.authority != self.candidate.candidate().expected_fence().authority {
            return Err(ExecutionError::Invalid("normal warm directory live authority").into());
        }
        crate::world::incremental_normal_cache_promotion::bind_normal_warm_cache_v1(
            self, history, head, live, common,
        )
    }
    pub fn record_id(&self) -> [u8; 32] {
        self.record
    }
    pub fn body_id(&self) -> [u8; 32] {
        self.body
    }
    pub fn roots_id(&self) -> [u8; 32] {
        self.roots
    }
    pub fn uses_budget(&self, memory: &MemoryBudget) -> bool {
        self.fee.uses_budget(memory)
            && self.candidate.uses_budget(memory)
            && self.parents.uses_budget(memory)
            && self
                .candidate
                .candidate()
                .directories
                .as_ref()
                .is_some_and(|d| d.uses_budget(memory) && d.sealed.uses_budget(memory))
    }
    pub(in crate::world) fn directories(
        &self,
    ) -> Result<&ReservedValue<CandidateBoundRuntimeDirectoriesV1>, ExecutionError> {
        self.candidate.candidate().require_directories_v1()
    }
}

impl PreparedRuntimeDirectoryCacheV1 {
    pub(in crate::world) fn share_normal_candidate_v1(&self,checked:&ReservedValue<oasis7_formal_protocol::incremental_material::CheckedCommitCore>,common:&mut oasis7_formal_protocol::controlled_authority::formal::bounded_context::FormalBoundedContextV1<'_>)->Result<Self,RuntimeDirectoryCacheErrorV1>{
        common.charge(16,1024).map_err(RuntimeDirectoryCacheErrorV1::Common)?;
        if !checked.uses_budget(common.memory())||!self.fee.uses_budget(common.memory())||checked.candidate_record_id()!=self.record||checked.catalog().entry.commit!=self.body||checked.body().roots!=self.roots{return Err(ExecutionError::Invalid("normal secondary candidate cache binding").into());}
        let fee=common.memory().reserve(std::mem::size_of::<Self>().max(std::mem::size_of::<AcceptedRuntimeDirectoryCacheV1>()),MemoryClass::Ordinary)?.retain(());
        let parents=crate::world::incremental_normal_parent_caches::NormalParentCachesV1::from_warm_backing_v1(std::sync::Arc::clone(&self.candidate),common.memory()).map_err(RuntimeDirectoryCacheErrorV1::Value)?;
        Ok(Self{candidate:std::sync::Arc::clone(&self.candidate),record:self.record,body:self.body,roots:self.roots,fee,parents})
    }
}
