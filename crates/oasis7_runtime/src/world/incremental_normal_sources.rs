//! Borrowed immutable parent carriers. This data view is not acceptance.
use oasis7_formal_protocol::{
    controlled_authority::formal::{
        FormalError, VerifiedAuthorityV1, VerifiedFormalCommitV1,
        bounded_context::{FormalBoundedContextV1, WorkIdentityV1},
        incremental::authority_binding_bounded,
    },
    incremental_execution::{ExecutionFence, RootSetV1},
    incremental_material::CheckedCommitCore,
};
use oasis7_proto::memory_budget::{MemoryBudget, MemoryClass, ReservedValue};
use oasis7_world_state::{
    incremental_cognition_history::CompleteCognitionHistorySourceV3,
    incremental_runtime_history_source::AcceptedRuntimeHistorySourceV1,
    incremental_world_storage_source::CheckedWorldStorageSourceV1,
};
/// These are readonly data inputs. Only the Host scoped source supplies the
/// actual namespace/FD lifetime; constructing this view grants no such lease.
pub struct NormalTickParentRefsV1<'a> {
    pub core: &'a ReservedValue<CheckedCommitCore>,
    pub storage: &'a ReservedValue<CheckedWorldStorageSourceV1>,
    pub roots: &'a ReservedValue<RootSetV1>,
    pub runtime: &'a ReservedValue<AcceptedRuntimeHistorySourceV1>,
    pub cognition: &'a ReservedValue<CompleteCognitionHistorySourceV3>,
    pub head: &'a VerifiedFormalCommitV1,
    pub authority: &'a VerifiedAuthorityV1,
}
pub struct BoundNormalTickParentV1<'a> {
    refs: NormalTickParentRefsV1<'a>,
    identity: WorkIdentityV1,
    fence: ExecutionFence,
}
fn invalid() -> FormalError {
    FormalError::Invalid("normal tick exact parent carriers".into())
}
fn root_bytes(a: &RootSetV1, b: &RootSetV1) -> Result<u64, FormalError> {
    let fixed = 2
        * (32
            + 2 * 8
            + 6 * (std::mem::size_of::<oasis7_formal_protocol::incremental_state::StateNamespaceV1>(
            ) + 8));
    let mut n = a
        .world_id
        .len()
        .checked_add(b.world_id.len())
        .and_then(|n| n.checked_add(fixed))
        .ok_or_else(invalid)?;
    for (left, right) in a.state.iter().zip(&b.state) {
        n = n
            .checked_add(left.node_root.len())
            .and_then(|n| n.checked_add(right.node_root.len()))
            .ok_or_else(invalid)?;
    }
    u64::try_from(n).map_err(|_| invalid())
}
/// Bind originals and immutable private qualifications, never supplied ids or
/// a root alone. The returned owner remains a borrowed data qualification.
pub fn bind_normal_tick_parent_v1<'a>(
    refs: NormalTickParentRefsV1<'a>,
    ctx: &mut FormalBoundedContextV1<'_>,
) -> Result<ReservedValue<BoundNormalTickParentV1<'a>>, FormalError> {
    ctx.charge(1, 32 * 24 + 8 * 8)?;
    let memory = ctx.memory();
    if !refs.core.uses_budget(memory)
        || !refs.storage.uses_budget(memory)
        || !refs.roots.uses_budget(memory)
        || !refs.runtime.uses_budget(memory)
        || !refs.cognition.uses_budget(memory)
        || !refs.storage.current().uses_budget(memory)
        || refs.storage.current().allocation_class() != MemoryClass::Ordinary
        || refs.runtime.allocation_class() != MemoryClass::Ordinary
    {
        return Err(invalid());
    }
    let record = refs.head.verified_record_id();
    let runtime = refs.runtime.occurrence();
    let checked = refs.storage.runtime().occurrence();
    if refs.core.candidate_record_id() != record
        || checked.candidate_record_id() != record
        || runtime.head_record_id() != record
        || refs.cognition.current().head_record_id() != record
        || checked.body_id() != runtime.head_body_id()
        || refs.storage.cognition().candidate_record_id() != record
        || refs.storage.cognition().body_id() != runtime.head_body_id()
        || refs.cognition.selected().occurrence().head_body_id() != runtime.head_body_id()
        || refs.storage.cognition().roots_id() != refs.core.body().roots
        || refs.cognition.current().roots_id() != refs.core.body().roots
        || refs.core.catalog().after.last_commit != Some(runtime.head_body_id())
    {
        return Err(invalid());
    }
    ctx.charge(1, root_bytes(refs.roots, refs.core.roots())?)?;
    if &**refs.roots != refs.core.roots() {
        return Err(invalid());
    }
    let authority = authority_binding_bounded(refs.authority, ctx)?;
    ctx.charge(1, 40)?;
    if authority != refs.core.body().authority {
        return Err(invalid());
    }
    let fence = ExecutionFence {
        parent_commit: runtime.head_body_id(),
        parent_roots: refs.core.body().roots,
        authority,
    };
    let fee = ctx
        .memory()
        .reserve(
            std::mem::size_of::<BoundNormalTickParentV1<'a>>(),
            MemoryClass::Ordinary,
        )
        .map_err(|_| FormalError::Invalid("normal parent owner admission".into()))?;
    ctx.consume_steps(1)?;
    let identity = ctx.work().identity();
    Ok(fee.retain(BoundNormalTickParentV1 {
        refs,
        identity,
        fence,
    }))
}
impl BoundNormalTickParentV1<'_> {
    pub fn validate_context(
        &self,
        ctx: &mut FormalBoundedContextV1<'_>,
    ) -> Result<(), FormalError> {
        ctx.consume_steps(1)?;
        self.validate_work_binding(ctx.memory(), ctx.work().identity())
    }
    pub(in crate::world) fn validate_work_binding(
        &self,
        memory: &MemoryBudget,
        identity: oasis7_formal_protocol::controlled_authority::formal::bounded_context::WorkIdentityV1,
    ) -> Result<(), FormalError> {
        if self.identity != identity
            || !self.refs.core.uses_budget(memory)
            || !self.refs.storage.uses_budget(memory)
            || !self.refs.runtime.uses_budget(memory)
            || !self.refs.cognition.uses_budget(memory)
            || !self.refs.roots.uses_budget(memory)
        {
            return Err(invalid());
        }
        Ok(())
    }
    pub(in crate::world) fn fence(&self) -> ExecutionFence {
        self.fence
    }
    pub(in crate::world) fn refs(&self) -> &NormalTickParentRefsV1<'_> {
        &self.refs
    }
    pub(in crate::world) fn memory_matches(&self, memory: &MemoryBudget) -> bool {
        self.refs.core.uses_budget(memory)
    }
}
