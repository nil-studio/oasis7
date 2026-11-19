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
    incremental_world_storage_source_cold::AuthenticatedWorldStorageParentSourceV1,
};
/// These are readonly data inputs. Only the Host scoped source supplies the
/// actual namespace/FD lifetime; constructing this view grants no such lease.
pub struct NormalTickParentRefsV1<'a> {
    pub core: &'a ReservedValue<CheckedCommitCore>,
    pub storage: &'a dyn AuthenticatedWorldStorageParentSourceV1,
    pub roots: &'a ReservedValue<RootSetV1>,
    pub runtime: &'a ReservedValue<AcceptedRuntimeHistorySourceV1>,
    pub cognition: &'a ReservedValue<CompleteCognitionHistorySourceV3>,
    pub retention: Option<&'a ReservedValue<oasis7_world_state::incremental_cognition_retention_ledger::AcceptedCognitionRetentionLedgerV1>>,
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
pub(in crate::world) fn root_bytes(a: &RootSetV1, b: &RootSetV1) -> Result<u64, FormalError> {
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
    if refs.core.retention_ledger_artifact_id().is_some() != refs.retention.is_some() {
        return Err(invalid());
    }
    if let Some(retention) = refs.retention {
        retention
            .validate_binding_v1(refs.head, refs.roots, ctx)
            .map_err(|_| invalid())?;
    }
    // Bind the complete Current carrier itself, not only its history siblings.
    // Use its already-qualified digest rather than encoding any supplied roots.
    ctx.charge(1, 32)?;
    if refs.storage.current().roots_id() != refs.core.body().roots {
        return Err(invalid());
    }
    let record = refs.head.verified_record_id();
    let runtime = refs.runtime.occurrence();
    refs.storage.validate_parent_sources_v1(refs.head, refs.core, refs.runtime,
        refs.cognition, ctx).map_err(|error| match error {
            oasis7_world_state::incremental_world_storage_source_cold::ColdWorldStorageSourceErrorV1::Formal(error) => error,
            _ => invalid(),
        })?;
    if refs.core.candidate_record_id() != record
        || runtime.head_record_id() != record
        || refs.cognition.current().head_record_id() != record
        || refs.cognition.selected().occurrence().head_body_id() != runtime.head_body_id()
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
    /// Derive every module family from this one complete accepted parent. The
    /// returned references remain borrowed while the Host namespace/FD lives.
    pub(in crate::world) fn module_execution_families_v1<'borrow>(
        &'borrow self,
        ctx: &mut FormalBoundedContextV1<'_>,
    ) -> Result<
        ReservedValue<
            crate::world::incremental_module_invocation::RootModuleExecutionFamiliesV1<'borrow>,
        >,
        FormalError,
    > {
        self.validate_context(ctx)?;
        ctx.charge(12, 32)?;
        let current = self.refs.storage.current();
        if current.roots_id() != self.refs.core.body().roots
            || current.allocation_class() != MemoryClass::Ordinary
        {
            return Err(invalid());
        }
        let fee = ctx
            .memory()
            .reserve(
                std::mem::size_of::<
                    crate::world::incremental_module_invocation::RootModuleExecutionFamiliesV1<
                        'borrow,
                    >,
                >(),
                MemoryClass::Ordinary,
            )
            .map_err(|_| invalid())?;
        Ok(fee.retain(
            crate::world::incremental_module_invocation::RootModuleExecutionFamiliesV1 {
                roots: self.refs.roots,
                parent: self.refs.runtime,
                head: self.refs.head,
                instances: current.raw().instances(),
                config: current.runtime().config(),
                content: current.runtime().content(),
                agents: current.raw().agents(),
                authorization: current.runtime().module_authorization(),
                effects: current.runtime().effects(),
                state: current.raw().module_states(),
                agent_authority: current.native().agent_authority(),
                marketplace: current.native().market(),
            },
        ))
    }
    pub(in crate::world) fn validate_module_execution_families_v1(
        &self,
        actual: &crate::world::incremental_module_invocation::RootModuleExecutionFamiliesV1<'_>,
        ctx: &mut FormalBoundedContextV1<'_>,
    ) -> Result<(), FormalError> {
        let expected = self.module_execution_families_v1(ctx)?;
        // This is the private fixed normal driver, whose sources must all be
        // borrowed from the same live parent scope. A mixed family cannot be
        // admitted merely because its caller copied matching root bytes.
        if !std::ptr::eq(actual.roots, expected.roots)
            || !std::ptr::eq(actual.parent, expected.parent)
            || !std::ptr::eq(actual.head, expected.head)
            || !std::ptr::eq(actual.instances, expected.instances)
            || !std::ptr::eq(actual.config, expected.config)
            || !std::ptr::eq(actual.content, expected.content)
            || !std::ptr::eq(actual.agents, expected.agents)
            || !std::ptr::eq(actual.authorization, expected.authorization)
            || !std::ptr::eq(actual.effects, expected.effects)
            || !std::ptr::eq(actual.state, expected.state)
            || !std::ptr::eq(actual.agent_authority, expected.agent_authority)
            || !std::ptr::eq(actual.marketplace, expected.marketplace)
        {
            return Err(invalid());
        }
        Ok(())
    }
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
    /// Read-only bootstrap DATA. These bytes grant no execution authority;
    /// the fixed Cold factory independently revalidates this actual parent/TX.
    pub fn bootstrap_fence_data_v1(&self) -> ExecutionFence {
        self.fence
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
