//! Genuine due-index/lifecycle owner calls the concrete executor internally.
//! The public caller cannot substitute an execution callback.
use super::*;
use oasis7_formal_protocol::incremental_schedule::SchedulePageWriter;
use oasis7_world_state::incremental_value_mutation::ValueMutationContextV1;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RootModuleTickOutcomeV1 {
    due_consumed: bool,
    invoked: bool,
}
impl RootModuleTickOutcomeV1 {
    pub fn due_consumed(&self) -> bool {
        self.due_consumed
    }
    pub fn invoked(&self) -> bool {
        self.invoked
    }
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
        + oasis7_formal_protocol::incremental_state::StatePageWriter
        + SchedulePageWriter,
    F: FnMut(
        &mut dyn crate::world::incremental_body_event::AgentEventOriginalV1,
    ) -> Result<(), BodyMutationError>,
    O: OriginalObjectStreamReader,
{
    /// The original due owner removes the prior wake before executing and uses
    /// the actual successful VM lifecycle directive to stage the successor.
    /// Every failure poisons that same transaction, including a failed fence
    /// reread after the real sandbox and after final lifecycle contribution.
    pub fn execute_scheduled_tick_v1(
        &mut self,
        instance: &str,
        live_owner: &mut dyn FnMut() -> ExecutionFence,
    ) -> Result<RootModuleTickOutcomeV1, RootModuleExecutionErrorV1<O::Error>> {
        let Self {
            families,
            now,
            state,
            ctx,
            members,
            fees,
            events,
            originals,
            worker,
            manifest_cache,
            manifest_work,
            work,
        } = self;
        let WorldFieldOverlayContextV1 {
            transaction,
            values,
            key_work,
            semantic_bytes,
            record_bytes,
            take_state_error,
            cancelled,
        } = &mut **ctx;
        let ModuleOutputEventContextV1 {
            values: event_values,
            world_events,
            mutation,
            persist_original,
            max_event_bytes,
            original_work,
        } = &mut **events;
        let mut actual_error = None;
        let mut invoked = false;
        let result = crate::world::World::contribute_iws_due_module_tick_at(
            *now,
            instance,
            transaction,
            *event_values,
            mutation,
            |instance, actual_now, tx, body| {
                let overlay_cancel: &mut dyn FnMut() -> bool = &mut *cancelled;
                let event_cancel: &mut dyn FnMut() -> bool = &mut body.cancelled;
                let member_cancel: &mut dyn FnMut() -> bool = &mut members.cancelled;
                let fee_cancel: &mut dyn FnMut() -> bool = &mut fees.values.cancelled;
                let mut overlay = WorldFieldOverlayContextV1 {
                    transaction: tx,
                    values: ValueReadContext {
                        source: values.source,
                        memory: values.memory,
                        class: values.class,
                        io: &mut *values.io,
                    },
                    key_work: &mut **key_work,
                    semantic_bytes: *semantic_bytes,
                    record_bytes: *record_bytes,
                    take_state_error: &mut *take_state_error,
                    cancelled: overlay_cancel,
                };
                let mut member = MemberRecordWriteContext {
                    sink: &mut *members.sink,
                    memory: members.memory,
                    io: &mut *members.io,
                    key_work: &mut *members.key_work,
                    limits: members.limits,
                    cancelled: member_cancel,
                };
                let mut fee = RootModuleFeeWriteContextV1 {
                    values: ValueMutationContextV1 {
                        sink: &mut *fees.values.sink,
                        memory: fees.values.memory,
                        io: &mut *fees.values.io,
                        cancelled: fee_cancel,
                    },
                    key_work: &mut *fees.key_work,
                    member_limits: fees.member_limits,
                };
                let placeholder = oasis7_wasm_abi::CanonicalCborWorkV1 {
                    visits_remaining: 0,
                    bytes_remaining: 0,
                    max_depth: original_work.max_depth,
                    max_map_keys: original_work.max_map_keys,
                    max_key_bytes: original_work.max_key_bytes,
                };
                let mut event = ModuleOutputEventContextV1 {
                    values: *event_values,
                    world_events: *world_events,
                    mutation: BodyMutationContext {
                        sink: &mut *body.sink,
                        memory: body.memory,
                        io: &mut *body.io,
                        cancelled: event_cancel,
                    },
                    persist_original: &mut *persist_original,
                    max_event_bytes: *max_event_bytes,
                    original_work: std::mem::replace(original_work, placeholder),
                };
                // A thin second handle has the SAME actual sealed executable,
                // pool allocator, cancellation and finite limits; no process,
                // cache or separate budget is created by copying these refs.
                let executable = match &worker.executable {
                    WorkerExecutableV1::SealedFd(file) => WorkerExecutableV1::SealedFd(*file),
                    WorkerExecutableV1::FixturePath(path) => WorkerExecutableV1::FixturePath(*path),
                };
                let mut backend = BoundedWorkerSandboxV1 {
                    executable,
                    allocator: worker.allocator,
                    cancelled: worker.cancelled,
                    limits: WorkerLimitsV1 {
                        address_space_bytes: worker.limits.address_space_bytes,
                        max_request_bytes: worker.limits.max_request_bytes,
                        max_output_bytes: worker.limits.max_output_bytes,
                        wall_time_ms: worker.limits.wall_time_ms,
                    },
                };
                let result = {
                    let mut actual = RootModuleExecutionContextV1 {
                        families: *families,
                        now: actual_now,
                        state: *state,
                        ctx: &mut overlay,
                        members: &mut member,
                        fees: &mut fee,
                        events: &mut event,
                        originals: &mut **originals,
                        worker: &mut backend,
                        manifest_cache: *manifest_cache,
                        manifest_work: &mut **manifest_work,
                        work: &mut **work,
                    };
                    actual.execute_tick_invocation_v1(instance, live_owner)
                };
                *original_work = event.original_work;
                match result {
                    Ok(output) => {
                        invoked = output.is_some();
                        Ok(output.and_then(|output| output.tick_lifecycle.clone()))
                    }
                    Err(error) => {
                        actual_error = Some(error);
                        Err(BodyMutationError::Invalid(
                            "actual root module executor rejected",
                        ))
                    }
                }
            },
        );
        if let Some(error) = actual_error {
            return Err(error);
        }
        let due_consumed = result.map_err(RootModuleExecutionErrorV1::Body)?;
        transaction.contribute(|tx| {
            crate::world::incremental_clock::check_clock(*now, tx, mutation)
                .map_err(RootModuleExecutionErrorV1::Body)?;
            let live = live_owner();
            tx.validate_guard(&tx.guard(), &live)?;
            Ok(RootModuleTickOutcomeV1 {
                due_consumed,
                invoked,
            })
        })
    }
}
