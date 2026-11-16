//! The fixed Root-context staging boundary. The caller runs the actual module
//! and post-module Build preflight inside this closure, then selects a storage
//! disposition. Neither choice is business permission or a completed tick.
use super::*;
use oasis7_formal_protocol::controlled_authority::formal::bounded_context::FormalBoundedContextV1;

pub(in crate::world) enum StagedRootModuleDispositionV1<T> {
    Commit(T),
    Discard(T),
}
impl<'a, 't, 's, 'v, M, R, E, C, W, F, O>
    RootModuleExecutionContextV1<'a, 't, 's, 'v, M, R, E, C, W, F, O>
where
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    W: ValueNodeReader
        + ValueNodeWriter
        + oasis7_formal_protocol::incremental_state::StatePageWriter,
    F: FnMut(
        &mut dyn crate::world::incremental_body_event::AgentEventOriginalV1,
    ) -> Result<(), BodyMutationError>,
    O: OriginalObjectStreamReader,
{
    /// Use outside an active Common IO child. All body VM/storage calls borrow
    /// the same continuing caller work/IO/worker and open their original children.
    /// Original/page sinks may retain unreferenced CAS data on discard; no branch
    /// Fact, typed receipt, counter note or child guard is adopted by the parent.
    pub(in crate::world) fn with_staged_module_branch_v1<T>(
        &mut self,
        common: &mut FormalBoundedContextV1<'_>,
        live_owner: &mut dyn FnMut()
            -> oasis7_formal_protocol::incremental_execution::ExecutionFence,
        action: impl FnOnce(
            &mut Self,
            &mut FormalBoundedContextV1<'_>,
            &mut dyn FnMut() -> oasis7_formal_protocol::incremental_execution::ExecutionFence,
        ) -> Result<
            StagedRootModuleDispositionV1<T>,
            RootModuleExecutionErrorV1<O::Error>,
        >,
    ) -> Result<
        (
            T,
            oasis7_formal_protocol::incremental_execution::StagedBranchRejoinEvidenceV1,
        ),
        RootModuleExecutionErrorV1<O::Error>,
    > {
        #[cfg(any(test, feature = "test-support"))]
        let old_observation = self.input_observation.as_deref().copied();
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let live = live_owner();
            self.events
                .world_events
                .validate(self.ctx.transaction)
                .map_err(RootModuleExecutionErrorV1::Body)?;
            if !self.ctx.transaction.uses_budget(common.memory())
                || !self.members.memory.same_pool(common.memory())
                || !self.events.mutation.memory.same_pool(common.memory())
                || !self.fees.values.memory.same_pool(common.memory())
            {
                return Err(RootModuleExecutionErrorV1::Body(
                    BodyMutationError::Invalid("staged Root pool"),
                ));
            }
            let guard = self.ctx.transaction.guard();
            let branch = self
                .ctx
                .transaction
                .begin_staged_branch_v1(common, &guard, &live)
                .map_err(|e| {
                    RootModuleExecutionErrorV1::Body(BodyMutationError::Original(Box::new(e)))
                })?;
            let counter = self.events.world_events;
            let counted = counter
                .begin_staged_branch_v1(&branch, self.ctx.transaction, common, &live)
                .map_err(RootModuleExecutionErrorV1::Body)?;
            let outcome = action(self, common, live_owner)?;
            let live = live_owner();
            let prepared = counted
                .prepare_rejoin(&branch, self.ctx.transaction, common, &live)
                .map_err(RootModuleExecutionErrorV1::Body)?;
            match outcome {
                StagedRootModuleDispositionV1::Commit(value) => {
                    let evidence =
                        branch
                            .commit(self.ctx.transaction, common, &live)
                            .map_err(|e| {
                                RootModuleExecutionErrorV1::Body(BodyMutationError::Original(
                                    Box::new(e),
                                ))
                            })?;
                    prepared.commit();
                    Ok((value, evidence))
                }
                StagedRootModuleDispositionV1::Discard(value) => {
                    let evidence = branch
                        .discard(self.ctx.transaction, common, &live)
                        .map_err(|e| {
                            RootModuleExecutionErrorV1::Body(BodyMutationError::Original(Box::new(
                                e,
                            )))
                        })?;
                    prepared.discard();
                    #[cfg(any(test, feature = "test-support"))]
                    if let (Some(slot), Some(old)) =
                        (self.input_observation.as_deref_mut(), old_observation)
                    {
                        *slot = old;
                    }
                    Ok((value, evidence))
                }
            }
        }));
        match result {
            Ok(Ok(value)) => Ok(value),
            Ok(Err(error)) => {
                #[cfg(any(test, feature = "test-support"))]
                if let (Some(slot), Some(old)) =
                    (self.input_observation.as_deref_mut(), old_observation)
                {
                    *slot = old;
                }
                let _: Result<(), oasis7_formal_protocol::incremental_execution::ExecutionError> =
                    self.ctx.transaction.contribute(|_| {
                        Err(oasis7_formal_protocol::incremental_execution::ExecutionError::Aborted)
                    });
                Err(error)
            }
            Err(panic) => {
                #[cfg(any(test, feature = "test-support"))]
                if let (Some(slot), Some(old)) =
                    (self.input_observation.as_deref_mut(), old_observation)
                {
                    *slot = old;
                }
                let _: Result<(), oasis7_formal_protocol::incremental_execution::ExecutionError> =
                    self.ctx.transaction.contribute(|_| {
                        Err(oasis7_formal_protocol::incremental_execution::ExecutionError::Aborted)
                    });
                std::panic::resume_unwind(panic)
            }
        }
    }
}
