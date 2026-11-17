//! Private fixed reborrow across Tail's exclusive actual transaction scope.
use super::*;
use crate::world::{
    incremental_body::BodyMutationContext,
    incremental_body_event::materials::CorePolicyMaterialContextV1,
    incremental_factory_phase::{FactoryPhaseAcceptedCachesV1, FactoryPhaseFamiliesV1},
    incremental_module_execution_event::ModuleOutputEventContextV1,
    incremental_module_fee_contribution::RootModuleFeeWriteContextV1,
    incremental_normal_host_work::NormalHostDriverWorkV1,
    incremental_normal_industry_due::delivery::CurrentProductValidationDeliveryIndexV1,
    incremental_normal_tail_sources::BoundNormalTickTailSourcesV1,
};
use oasis7_formal_protocol::incremental_value::ValueReadContext;
use oasis7_wasm_abi::CanonicalCborWorkV1;
use oasis7_wasm_executor::bounded_worker::{WorkerExecutableV1, WorkerLimitsV1};
use oasis7_world_state::{
    incremental_factory_records::FactoryTransitionReadContextV1,
    incremental_field_member::MemberRecordWriteContext,
    incremental_field_overlay::WorldFieldOverlayContextV1,
    incremental_field_read::WorldFieldReadContextV1,
    incremental_industry_jobs::{IndustryFamilyReadContextV1, IndustryTransitionReadContextV1},
    incremental_pending_action_overlay::PendingActionCurrentV1,
    incremental_runtime_revocation::RuntimeRevocationCurrentV1,
    incremental_value_mutation::ValueMutationContextV1,
};
impl<'a, 't, 's, 'v, M, R, E, C, W, F, O>
    RootModuleExecutionContextV1<'a, 't, 's, 'v, M, R, E, C, W, F, O>
where
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError> + Copy,
    C: FnMut() -> bool + Copy,
    W: ValueNodeReader + ValueNodeWriter + StatePageWriter + SchedulePageWriter,
    F: FnMut(&mut dyn AgentEventOriginalV1) -> Result<(), BodyMutationError>,
    O: OriginalObjectStreamReader
        + crate::world::PaidProjectionRetainedOriginalReaderV1<RetainedError = BodyMutationError>,
    O::Error: std::error::Error + Send + Sync + 'static,
{
    #[allow(clippy::too_many_arguments)]
    pub(super) fn execute_normal_host_with_bound_tail_v1<'p, S>(
        &mut self,
        parent: &'p ReservedValue<BoundNormalTickParentV1<'p>>,
        input: &PaidBoundNormalInputDataV1<'_>,
        sources: &NormalHostContextSourcesV1<'p>,
        authorization: &mut crate::world::incremental_authorization_root_current::CurrentAuthorizationProjectionV1<'p>,
        bound: &ReservedValue<BoundNormalTickTailSourcesV1<'_, 'p>>,
        pending: &mut ReservedValue<PendingActionCurrentV1>,
        revocation: &mut RuntimeRevocationCurrentV1,
        delivery: &mut CurrentProductValidationDeliveryIndexV1<'p>,
        current: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
        parent_read: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
        admission: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
        factory: &mut FactoryTransitionReadContextV1<'_, R, C>,
        recipe: &mut IndustryTransitionReadContextV1<'_, R, C>,
        industry: &mut IndustryFamilyReadContextV1<'_, M, R, E, C>,
        materials: &mut CorePolicyMaterialContextV1<'_, M, E>,
        driver: &mut NormalHostDriverWorkV1,
        input_limits: &CborDataLimitsV1,
        max_action_bytes: usize,
        max_grant_payload_bytes: u64,
        sink: &mut S,
        continued_cognition: Option<PendingCurrentCognitionJournalV1<'p>>,
        common: &mut FormalBoundedContextV1<'_>,
        live_owner: &mut dyn FnMut() -> ExecutionFence,
    ) -> Result<FinishedNormalTickDirectoriesV1<'p>, NormalHostRuntimeErrorV1<O::Error>>
    where
        S: ObservationJsonOriginalSinkV2,
        S::Error: std::fmt::Debug + Send + Sync + 'static,
    {
        let RootModuleExecutionContextV1 {
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
            ..
        } = self;
        let executable = match &worker.executable {
            WorkerExecutableV1::NativeBoundSealedFd { file, descriptor } => {
                WorkerExecutableV1::NativeBoundSealedFd { file, descriptor }
            }
            _ => {
                return Err(NormalHostRuntimeErrorV1::Root(
                    BodyMutationError::Invalid("normal fixed tail NativeBound worker").into(),
                ));
            }
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
        let WorldFieldOverlayContextV1 {
            transaction,
            values,
            key_work,
            semantic_bytes,
            record_bytes,
            take_state_error,
            cancelled,
        } = &mut **ctx;
        // The actual Original allowance moves, rather than being copied into the
        // nested context. The occupied outer slot carries zero until rejoined.
        let original_slot = &mut events.original_work;
        let placeholder = CanonicalCborWorkV1 {
            visits_remaining: 0,
            bytes_remaining: 0,
            max_depth: original_slot.max_depth,
            max_map_keys: original_slot.max_map_keys,
            max_key_bytes: original_slot.max_key_bytes,
        };
        let mut event = ModuleOutputEventContextV1 {
            values: events.values,
            world_events: events.world_events,
            mutation: BodyMutationContext {
                sink: &mut *events.mutation.sink,
                memory: events.mutation.memory,
                io: &mut *events.mutation.io,
                cancelled: events.mutation.cancelled,
            },
            persist_original: &mut events.persist_original,
            max_event_bytes: events.max_event_bytes,
            original_work: std::mem::replace(original_slot, placeholder),
        };
        let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let live = live_owner();
            bound
                .with_sources_v1(&mut **transaction, common, &live, |tail, tx, common| {
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
                        take_state_error: *take_state_error,
                        cancelled: *cancelled,
                    };
                    let mut member = MemberRecordWriteContext {
                        sink: &mut *members.sink,
                        memory: members.memory,
                        io: &mut *members.io,
                        key_work: &mut *members.key_work,
                        limits: members.limits,
                        cancelled: members.cancelled,
                    };
                    let mut fee = RootModuleFeeWriteContextV1 {
                        values: ValueMutationContextV1 {
                            sink: &mut *fees.values.sink,
                            memory: fees.values.memory,
                            io: &mut *fees.values.io,
                            cancelled: fees.values.cancelled,
                        },
                        key_work: &mut *fees.key_work,
                        member_limits: fees.member_limits,
                    };
                    let mut root = RootModuleExecutionContextV1 {
                        families: *families,
                        now: *now,
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
                        #[cfg(any(test, feature = "test-support"))]
                        input_observation: None,
                    };
                    let accepted = sources.caches.borrow();
                    let native = parent.refs().storage.current().native();
                    let runtime = parent.refs().storage.current().runtime();
                    let factory_caches = FactoryPhaseAcceptedCachesV1 {
                        orders: accepted.orders,
                        records: tail.recipes,
                        materials: sources.materials,
                        invocations: accepted.invocations,
                    };
                    let factory_families = FactoryPhaseFamiliesV1 {
                        orders: native.rules().orders(),
                        native: native.native(),
                        factories: native.factory(),
                        recipes: native.industry(),
                        sites: native.factory_authority(),
                        anchors: native.agent_authority(),
                        revocation: runtime.revocation(),
                        artifacts: runtime.resources(),
                    };
                    // The actual counted counter is already begun. Apply the
                    // reserved Bootstrap batch before enqueuing ordinary input,
                    // then reborrow the same updated authorization/revocation.
                    let bootstrap_input = crate::world::incremental_normal_bootstrap_input::bind_current_provider_bootstrap_input_v1(input,input_limits,common)
                        .map_err(|e|NormalHostRuntimeErrorV1::Root(e.into()))?;
                    let bootstrap = if let Some(input_owner) = bootstrap_input {
                        let ledger = tail.cognition.as_ref().ok_or_else(||NormalHostRuntimeErrorV1::Root(BodyMutationError::Invalid("Bootstrap genuine Current proof ledger missing").into()))?;
                        common.charge(1,2048).map_err(|e|NormalHostRuntimeErrorV1::Root(BodyMutationError::Original(Box::new(e)).into()))?;
                        let _layout=common.memory().reserve(2048,oasis7_proto::memory_budget::MemoryClass::Ordinary).map_err(|e|NormalHostRuntimeErrorV1::Root(BodyMutationError::from(ValueTreeError::Memory(e)).into()))?;
                        let template=crate::world::normal_bootstrap_current::BootstrapLifecycleSourcesV1 {
                            controls:native.controls(),proposals:runtime.proposals(),families:root.families,
                            factory:tail.economic.factory,governance:runtime.governance(),compiler:sources.compiler,
                            typed:runtime,native:native.native(),primitive:native.primitive(),
                        };
                        let finality=root.prepare_current_bootstrap_finality_v1(parent,&mut driver.bootstrap.finality,common,live_owner).map_err(NormalHostRuntimeErrorV1::Root)?;
                        let bootstrap_sources=crate::world::normal_bootstrap_current::CurrentBootstrapSourcesV1 {
                            finality:&finality,artifacts:runtime.resources(),accepted_orders:tail.economic.orders,lifecycle:&template,cognition:parent.refs().storage.current().cognition(),ledger:ledger.ledger,process:&**sources.process,
                        };
                        Some(root.execute_current_provider_bootstrap_v1(input_owner,parent,&bootstrap_sources,authorization,revocation,&mut driver.bootstrap_loan(input_limits),parent_read,materials,common,live_owner).map_err(NormalHostRuntimeErrorV1::Root)?)
                    } else { None };
                    root.execute_normal_tick_with_host_work_v1(
                        driver.loan(),
                        parent,
                        input,
                        bootstrap.as_ref(),
                        input_limits,
                        max_action_bytes,
                        max_grant_payload_bytes,
                        &factory_caches,
                        &factory_families,
                        factory,
                        recipe,
                        pending,
                        runtime.revocation(),
                        revocation,
                        admission,
                        sources.caches,
                        authorization,
                        sources.business,
                        sources.projection_cache,
                        sources.compiler,
                        &**sources.process,
                        tail,
                        delivery,
                        industry,
                        sink,
                        continued_cognition,
                        materials,
                        current,
                        parent_read,
                        accepted.invocations,
                        common,
                        live_owner,
                    )
                    .map_err(NormalHostRuntimeErrorV1::Tick)
                })
                .map_err(NormalHostRuntimeErrorV1::Tail)?
        }));
        original_slot.visits_remaining = event.original_work.visits_remaining;
        original_slot.bytes_remaining = event.original_work.bytes_remaining;
        match outcome {
            Ok(result) => result,
            Err(payload) => std::panic::resume_unwind(payload),
        }
    }
}
