//! Fixed whole-batch composition. Only successful actual publications may
//! produce the queue-consumption evidence and the retained final frontiers.
use super::*;
use crate::world::incremental_normal_cognition_expiry::with_current_cognition_io_v1;
use oasis7_formal_protocol::incremental_schedule::SchedulePageWriter;
impl<M, R, E, C, W, F, O> RootModuleExecutionContextV1<'_, '_, '_, '_, M, R, E, C, W, F, O>
where
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    W: ValueNodeReader + ValueNodeWriter + StatePageWriter + SchedulePageWriter,
    F: FnMut(
        &mut dyn crate::world::incremental_body_event::AgentEventOriginalV1,
    ) -> Result<(), BodyMutationError>,
    O: OriginalObjectStreamReader,
{
    #[allow(clippy::too_many_arguments)]
    pub(in crate::world) fn execute_current_provider_bootstrap_v1<'a, 'f, 'p, PE, PC, ME>(
        &mut self,
        input: BoundCurrentProviderBootstrapInputV1<'a, 'f>,
        normal: &'p BoundNormalTickParentV1<'p>,
        sources: &CurrentBootstrapSourcesV1<'_, '_>,
        authorization:&mut crate::world::incremental_authorization_root_current::CurrentAuthorizationProjectionV1<'_>,
        revocation: &mut RuntimeRevocationCurrentV1,
        work: &mut CurrentBootstrapWorkV1<'_, '_>,
        parent: &mut WorldFieldReadContextV1<'_, M, R, PE, PC>,
        materials:&mut crate::world::incremental_body_event::materials::CorePolicyMaterialContextV1<'_,M,ME>,
        common: &mut FormalBoundedContextV1<'_>,
        live_owner: &mut dyn FnMut() -> ExecutionFence,
    ) -> Result<CommittedCurrentBootstrapBatchV1<'a, 'f>, RootModuleExecutionErrorV1<O::Error>>
    where
        PE: FnMut() -> Option<ValueTreeError>,
        PC: FnMut() -> bool,
        ME: FnMut() -> Option<ValueTreeError>,
    {
        let ordinary_now = self.now;
        let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            input.validate(normal, self.ctx.transaction, common, &live_owner())?;
            normal
                .validate_module_execution_families_v1(&sources.lifecycle.families, common)
                .map_err(|e| BodyMutationError::Original(Box::new(e)))?;
            let count = input.inputs().len();
            if count == 0 {
                return Err(
                    BodyMutationError::Invalid("Bootstrap actual nonempty reserved input").into(),
                );
            }
            let capacity = count.checked_mul(4).ok_or(BodyMutationError::Invalid(
                "Bootstrap publication slot count",
            ))?;
            let bytes = capacity
                .checked_mul(std::mem::size_of::<CommittedCurrentBootstrapAuthorizationV1>())
                .ok_or(BodyMutationError::Invalid(
                    "Bootstrap publication slot layout",
                ))?;
            common
                .charge(1, bytes as u64)
                .map_err(|e| BodyMutationError::Original(Box::new(e)))?;
            let slots = common
                .memory()
                .reserve(bytes, MemoryClass::Ordinary)
                .map_err(ValueTreeError::Memory)?;
            let mut publications = Vec::new();
            publications
                .try_reserve_exact(capacity)
                .map_err(|_| BodyMutationError::Invalid("Bootstrap publication allocation"))?;
            if publications.capacity() != capacity {
                return Err(BodyMutationError::Invalid("Bootstrap publication capacity").into());
            }
            let outer = std::mem::size_of::<CommittedCurrentBootstrapBatchV1<'a, 'f>>();
            common
                .charge(1, outer as u64)
                .map_err(|e| BodyMutationError::Original(Box::new(e)))?;
            let fee = common
                .memory()
                .reserve(outer, MemoryClass::Ordinary)
                .map_err(ValueTreeError::Memory)?;
            let environment = self.read_bootstrap_environment_v1(
                normal,
                sources,
                parent,
                work,
                common,
                &live_owner(),
            )?;
            // Original Bootstrap observes the existing World clock. Node height is
            // restored before the ordinary FIFO/tick driver continues, even on unwind.
            self.now = environment.tick;
            let first = input
                .inputs()
                .first()
                .ok_or(BodyMutationError::Invalid("Bootstrap first input"))?;
            let first_expired = self.prepare_bootstrap_input_prefix_v1(
                first,
                normal,
                &environment,
                sources,
                authorization,
                revocation,
                parent,
                materials,
                work,
                &mut publications,
                common,
                live_owner,
            )?;
            let (mut before, economy) =
                self.open_current_bootstrap_economy_v1(normal, sources, work, common, live_owner)?;
            let economy = economy
                .map_move::<super::super::economy::EconomySessionWrapV1>(common)
                .map_err(|e| BodyMutationError::Original(Box::new(e)))?;
            let frontiers = economy
                .with_data(
                    common,
                    |mut session, common| -> Result<_, RootModuleExecutionErrorV1<O::Error>> {
                        for (index, value) in input.inputs().iter().enumerate() {
                            common
                                .consume_steps(1)
                                .map_err(|e| BodyMutationError::Original(Box::new(e)))?;
                            let expired = if index == 0 {
                                first_expired
                            } else {
                                self.prepare_bootstrap_input_prefix_v1(
                                    value,
                                    normal,
                                    &environment,
                                    sources,
                                    authorization,
                                    revocation,
                                    parent,
                                    materials,
                                    work,
                                    &mut publications,
                                    common,
                                    live_owner,
                                )?
                            };
                            if expired {
                                super::super::economy::replay_expired_bootstrap_economy_v1(
                                    value, &session, work, common,
                                )
                                .map_err(|e| {
                                    catalogue_error_v1(e, common)
                                        .unwrap_or_else(RootModuleExecutionErrorV1::Body)
                                })?;
                            } else {
                                self.provision_bootstrap_economy_v1(
                                    value,
                                    &mut session,
                                    &mut before,
                                    sources,
                                    environment.tick,
                                    work,
                                    common,
                                    live_owner,
                                )?;
                            }
                        }
                        // The original staged World persists only after every input succeeds.
                        // Keep that atomic visibility while preserving each real new Original.
                        self.commit_bootstrap_economy_v1(
                            &mut before,
                            &session,
                            sources,
                            work,
                            common,
                            live_owner,
                        )?;
                        let (journal, provision) = with_current_cognition_io_v1(
                            self.ctx,
                            self.members,
                            work.cognition,
                            common,
                            |ctx, _, cog| {
                                before
                                    .frontiers
                                    .finish_v1(
                                        ctx.transaction,
                                        &mut cog.hash,
                                        &live_owner(),
                                        &mut ctx.cancelled,
                                    )
                                    .map_err(Into::into)
                            },
                        )?;
                        retain_economy_frontiers_v1(
                            journal,
                            provision,
                            self.ctx.transaction,
                            common,
                            &live_owner(),
                        )
                        .map_err(Into::into)
                    },
                )
                .map_err(|e| BodyMutationError::Original(Box::new(e)))??;
            common
                .consume_steps(0)
                .map_err(|e| BodyMutationError::Original(Box::new(e)))?;
            self.ctx
                .transaction
                .validate_guard(&self.ctx.transaction.guard(), &live_owner())?;
            Ok(CommittedCurrentBootstrapBatchV1 {
                input,
                authorization: publications,
                frontiers,
                guard: self.ctx.transaction.guard(),
                core: common.work().identity(),
                _slots: slots,
                _fee: fee,
            })
        }));
        self.now = ordinary_now;
        match outcome {
            Ok(result) => {
                if result.is_err() {
                    let _: Result<
                        (),
                        oasis7_formal_protocol::incremental_execution::ExecutionError,
                    > = self.ctx.transaction.contribute(|_| {
                        Err(oasis7_formal_protocol::incremental_execution::ExecutionError::Aborted)
                    });
                }
                result
            }
            Err(payload) => {
                let _: Result<(), oasis7_formal_protocol::incremental_execution::ExecutionError> =
                    self.ctx.transaction.contribute(|_| {
                        Err(oasis7_formal_protocol::incremental_execution::ExecutionError::Aborted)
                    });
                std::panic::resume_unwind(payload)
            }
        }
    }
}
