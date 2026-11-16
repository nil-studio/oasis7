//! Prepaid warm structural parents. An actual seal supplies immutable data;
//! only a genuine accepted cache + matching live History source exposes it.
use crate::world::{
    incremental_commit::{AcceptedRuntimeDirectoryCacheV1, RuntimeDirectoryCacheErrorV1},
    incremental_directory_bundle::{
        SealedRuntimeDirectoryBundleV1, SharedFrozenRuntimeDirectoryBundleV1,
    },
    incremental_factory_directory_orders::AcceptedFactoryOrderDirectoryV1,
    incremental_factory_directory_records::AcceptedFactoryRecipeDirectoryV1,
    incremental_module_invocation::AcceptedInvocationDirectoryV1,
};
use oasis7_formal_protocol::{
    controlled_authority::formal::{
        VerifiedFormalCommitV1, bounded_context::FormalBoundedContextV1,
    },
    incremental_execution::{ExecutionError, ExecutionFactKindV1, ExecutionFence, RootSetV1},
    incremental_material::CheckedCommitCore,
    incremental_value::ValueTreeError,
};
use oasis7_proto::memory_budget::{MemoryBudget, MemoryClass, ReservedValue};
use oasis7_world_state::incremental_runtime_history_source::AcceptedRuntimeHistorySourceV1;
use std::sync::Arc;
fn invalid() -> ExecutionError {
    ExecutionError::Invalid("normal warm directory accepted source")
}
/// Only this module creates the temporary seal binding, after validating the
/// actual checked candidate and its sealed after roots. No public constructor.
pub(in crate::world) struct WarmSealedDirectoryBindingV1<'a> {
    shared: &'a Arc<SharedFrozenRuntimeDirectoryBundleV1>,
    memory: &'a MemoryBudget,
    body: [u8; 32],
    roots: [u8; 32],
}
impl WarmSealedDirectoryBindingV1<'_> {
    pub(in crate::world) fn shared(&self) -> &Arc<SharedFrozenRuntimeDirectoryBundleV1> {
        self.shared
    }
    pub(in crate::world) fn memory(&self) -> &MemoryBudget {
        self.memory
    }
    pub(in crate::world) fn body_id(&self) -> [u8; 32] {
        self.body
    }
    pub(in crate::world) fn roots_id(&self) -> [u8; 32] {
        self.roots
    }
}
pub(in crate::world) struct PreparedNormalParentCachesV1 {
    orders: ReservedValue<AcceptedFactoryOrderDirectoryV1>,
    records: ReservedValue<AcceptedFactoryRecipeDirectoryV1>,
    invocations: ReservedValue<AcceptedInvocationDirectoryV1>,
}
/// Shared genuine producer graph. Both the accepted cache and its normal
/// parent view retain this same paid backing; no sealed owner is detached.
pub(in crate::world) struct NormalWarmCacheBackingV1 {
    candidate: crate::world::incremental_commit::PreparedRuntimeCommit,
    warm: PreparedNormalParentCachesV1,
}
impl NormalWarmCacheBackingV1 {
    pub(in crate::world) fn candidate(
        &self,
    ) -> &crate::world::incremental_commit::PreparedRuntimeCommit {
        &self.candidate
    }
    pub(in crate::world) fn warm(&self) -> &PreparedNormalParentCachesV1 {
        &self.warm
    }
    pub(in crate::world) fn prepare(
        candidate: crate::world::incremental_commit::PreparedRuntimeCommit,
        warm: PreparedNormalParentCachesV1,
        memory: &MemoryBudget,
    ) -> Result<Arc<ReservedValue<Self>>, RuntimeDirectoryCacheErrorV1> {
        let directories = candidate.require_directories_v1()?;
        if !directories.uses_budget(memory) || !directories.sealed_directories().uses_budget(memory)
        {
            return Err(invalid().into());
        }
        let fee = memory.reserve(
            std::mem::size_of::<Self>() + 2 * std::mem::size_of::<usize>(),
            MemoryClass::Ordinary,
        )?;
        Ok(Arc::new(fee.retain(Self { candidate, warm })))
    }
    pub(in crate::world) fn sealed(&self) -> &SealedRuntimeDirectoryBundleV1 {
        // This owner is private-minted only after prepare checked mandatory seal.
        self.candidate
            .require_directories_v1()
            .expect("private warm backing has mandatory seal")
            .sealed_directories()
    }
    pub(in crate::world) fn borrow(
        &self,
    ) -> crate::world::incremental_normal_source_action_views::NormalActionParentCachesV1<'_> {
        let sealed = self.sealed();
        crate::world::incremental_normal_source_action_views::NormalActionParentCachesV1 {
            builds: sealed.build_parent(),
            receipts: sealed.receipt_parent(),
            inventory_identity: sealed.inventory_identity_parent(),
            invocations: &self.warm.invocations,
            orders: &self.warm.orders,
        }
    }
}
/// Called before durable acceptance: all fixed headers and Arc ownership are
/// admitted here. Keys and record projections stay in their genuine paid graph.
pub(in crate::world) fn prepare_normal_cache_promotion_v1(
    sealed: &ReservedValue<SealedRuntimeDirectoryBundleV1>,
    roots: &RootSetV1,
    checked: &ReservedValue<CheckedCommitCore>,
    common: &mut FormalBoundedContextV1<'_>,
) -> Result<PreparedNormalParentCachesV1, RuntimeDirectoryCacheErrorV1> {
    common
        .charge(32, 4096)
        .map_err(RuntimeDirectoryCacheErrorV1::Common)?;
    let memory = common.memory();
    if !sealed.uses_budget(memory)
        || !checked.uses_budget(memory)
        || !sealed.shared_frozen().uses_budget(memory)
        || checked.body().roots != sealed.after_roots_digest()
    {
        return Err(invalid().into());
    }
    sealed.validate_after_roots(roots, memory)?;
    let binding = WarmSealedDirectoryBindingV1 {
        shared: sealed.shared_frozen(),
        memory,
        body: checked.catalog().entry.commit,
        roots: checked.body().roots,
    };
    Ok(PreparedNormalParentCachesV1 {
        orders: AcceptedFactoryOrderDirectoryV1::prepare_warm_parent_v1(&binding)
            .map_err(RuntimeDirectoryCacheErrorV1::Value)?,
        records: AcceptedFactoryRecipeDirectoryV1::prepare_warm_parent_v1(&binding)
            .map_err(RuntimeDirectoryCacheErrorV1::Value)?,
        invocations: AcceptedInvocationDirectoryV1::prepare_warm_parent_v1(&binding)
            .map_err(RuntimeDirectoryCacheErrorV1::Value)?,
    })
}
/// Structural parents only; no transaction, worker, Begin or execution permit.
pub struct NormalWarmParentRefsV1<'a> {
    pub(in crate::world) cache: &'a AcceptedRuntimeDirectoryCacheV1,
    warm: &'a PreparedNormalParentCachesV1,
}
impl NormalWarmParentRefsV1<'_> {
    pub fn parent_caches(
        &self,
    ) -> &ReservedValue<crate::world::incremental_normal_parent_caches::NormalParentCachesV1> {
        self.cache.normal_parent_caches_v1()
    }
    pub(in crate::world) fn borrow(
        &self,
    ) -> crate::world::incremental_normal_source_action_views::NormalActionParentCachesV1<'_> {
        self.parent_caches().borrow()
    }
    pub fn orders(&self) -> &ReservedValue<AcceptedFactoryOrderDirectoryV1> {
        &self.warm.orders
    }
    pub fn records(&self) -> &ReservedValue<AcceptedFactoryRecipeDirectoryV1> {
        &self.warm.records
    }
    pub fn invocations(&self) -> &ReservedValue<AcceptedInvocationDirectoryV1> {
        &self.warm.invocations
    }
    pub(in crate::world) fn sealed(
        &self,
    ) -> Result<&ReservedValue<SealedRuntimeDirectoryBundleV1>, ExecutionError> {
        Ok(self.cache.directories()?.sealed_directories())
    }
}
pub(in crate::world) fn bind_normal_warm_cache_v1<'a>(
    cache: &'a AcceptedRuntimeDirectoryCacheV1,
    history: &ReservedValue<AcceptedRuntimeHistorySourceV1>,
    head: &VerifiedFormalCommitV1,
    live: &ExecutionFence,
    common: &mut FormalBoundedContextV1<'_>,
) -> Result<ReservedValue<NormalWarmParentRefsV1<'a>>, RuntimeDirectoryCacheErrorV1> {
    common
        .charge(32, 32 * 16)
        .map_err(RuntimeDirectoryCacheErrorV1::Common)?;
    let memory = common.memory();
    let occurrence = history.occurrence();
    if !cache.uses_budget(memory)
        || !history.uses_budget(memory)
        || history.allocation_class() != MemoryClass::Ordinary
        || head.verified_record_id() != cache.record_id()
        || occurrence.kind() != ExecutionFactKindV1::Result
        || occurrence.head_record_id() != cache.record_id()
        || occurrence.head_body_id() != cache.body_id()
        || occurrence.fact_body_id() != cache.body_id()
        || occurrence.roots_id() != cache.roots_id()
        || history.manifest().roots != cache.roots_id()
        || live.parent_commit != cache.body_id()
        || live.parent_roots != cache.roots_id()
    {
        return Err(invalid().into());
    }
    let warm = cache.normal_warm_parents_v1();
    if !warm.orders.uses_budget(memory)
        || !warm.records.uses_budget(memory)
        || !warm.invocations.uses_budget(memory)
    {
        return Err(invalid().into());
    }
    warm.orders
        .validate_action_source_v1(live, memory)
        .map_err(RuntimeDirectoryCacheErrorV1::Value)?;
    warm.invocations
        .validate_action_source_v1(live, memory)
        .map_err(RuntimeDirectoryCacheErrorV1::Value)?;
    let fee = memory.reserve(
        std::mem::size_of::<NormalWarmParentRefsV1<'_>>(),
        MemoryClass::Ordinary,
    )?;
    Ok(fee.retain(NormalWarmParentRefsV1 { cache, warm }))
}
