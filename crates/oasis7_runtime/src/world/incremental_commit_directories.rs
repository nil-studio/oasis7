//! Actual sole-tx directory closure -> actual seal -> actual History candidate.
//! No public commit ID, acceptance receipt, or caller RootSet can mint this owner.
use super::*;
use crate::world::incremental_directory_bundle::{
    FrozenRuntimeDirectoryBundleV1, RuntimeDirectoryFreezeErrorV1, SealedRuntimeDirectoryBundleV1,
    bind_runtime_directories_to_actual_seal_v1,
};
use oasis7_world_state::incremental_inventory::{
    InventoryIdentityDirectoryWorkV1, InventoryMaterialScopeWorkV1,
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
    ) -> Result<ReservedValue<FrozenRuntimeDirectoryBundleV1>, E>,
    material_work: &mut InventoryMaterialScopeWorkV1,
    identity_work: &mut InventoryIdentityDirectoryWorkV1,
    mut cancelled: impl FnMut() -> bool,
) -> Result<ReservedValue<PreparedRuntimeCommit>, E> {
    let identity_work = std::cell::RefCell::new(identity_work);
    let value = prepare_commit(
        request,
        source,
        memory,
        live_fence,
        |tx| apply(tx, &mut **identity_work.borrow_mut()),
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
                &mut **identity_work.borrow_mut(),
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
    ) -> Result<ReservedValue<FrozenRuntimeDirectoryBundleV1>, E>,
    material_work: &mut InventoryMaterialScopeWorkV1,
    identity_work: &mut InventoryIdentityDirectoryWorkV1,
    mut cancelled: impl FnMut() -> bool,
    produce: impl FnOnce(&RootSetV1) -> Result<[ExecutionFactRefV1; 2], E>,
) -> Result<ReservedValue<PreparedRuntimeCommit>, E> {
    let identity_work = std::cell::RefCell::new(identity_work);
    let value = prepare_commit(
        request,
        source,
        memory,
        live_fence,
        |tx| apply(tx, &mut **identity_work.borrow_mut()),
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
                &mut **identity_work.borrow_mut(),
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
