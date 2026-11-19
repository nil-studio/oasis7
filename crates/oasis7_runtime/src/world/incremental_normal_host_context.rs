//! Private fixed Host construction scope. All results remain provisional DATA.
use super::{
    incremental_body::{BodyMutationContext, BodyMutationError},
    incremental_body_event::{AgentEventOriginalV1, materials::CorePolicyMaterialContextV1},
    incremental_factory_phase::io::{FactoryPhaseMeterLaneV1, FactoryPhaseMeterLanesV1},
    incremental_module_invocation::*,
    incremental_normal_host_work::{NormalHostDriverWorkV1, NormalHostWorkV1},
    incremental_normal_industry_due::delivery::{
        CurrentProductValidationDeliveryIndexV1, ProductValidationDeliveryIndexV1,
    },
    incremental_normal_sources::BoundNormalTickParentV1,
    incremental_world_event_counter::{WorldEventCounterV1, WorldJournalRetentionV1},
};
use super::{
    incremental_module_execution_event::ModuleOutputEventContextV1,
    incremental_module_execution_worker::RootModuleWorkerAllocatorV1,
    incremental_module_fee_contribution::RootModuleFeeWriteContextV1,
};
use oasis7_formal_protocol::{
    controlled_authority::formal::bounded_context::FormalBoundedContextV1,
    incremental_execution::{ExecutionError, ExecutionFence, ExecutionTransaction},
    incremental_material::OriginalObjectStreamReader,
    incremental_state::{MerkleNodeReader, StateNamespaceV1, StatePageWriter, StateScanBudget},
    incremental_value::{ValueNodeReader, ValueNodeWriter, ValueReadContext, ValueTreeError},
};
use oasis7_proto::memory_budget::{MemoryClass, ReservedValue};
use oasis7_wasm_abi::CanonicalCborWorkV1;
use oasis7_wasm_executor::bounded_worker::{
    BoundedWorkerSandboxV1, WorkerExecutableV1, WorkerLimitsV1,
};
use oasis7_world_state::{
    incremental_factory_records::FactoryTransitionReadContextV1,
    incremental_field_overlay::{WorldFieldOverlayContextV1, WorldFieldOverlayErrorV1},
    incremental_field_read::WorldFieldReadContextV1,
    incremental_industry_jobs::{IndustryFamilyReadContextV1, IndustryTransitionReadContextV1},
    incremental_pending_action_family::PendingActionIndexWorkV1,
    incremental_pending_action_overlay::{PendingActionCurrentV1, begin_pending_action_current_v1},
    incremental_runtime_revocation::{
        RevocationReadWorkV1, RuntimeRevocationCurrentV1, begin_runtime_revocation_current_v1,
    },
    incremental_value_mutation::ValueMutationContextV1,
};

#[derive(Debug)]
struct CurrentClockSourceError(super::incremental_authorization_root_current::SourceError);
impl std::fmt::Display for CurrentClockSourceError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "actual Current clock: {:?}", self.0)
    }
}
impl std::error::Error for CurrentClockSourceError {}
/// World time is read from its actual registered Current leaf. Node commit
/// height remains an independent input/consensus identity, never a clock.
pub(in crate::world) fn read_current_normal_clock_v1<M, R, E, C, P, V, PE, PC>(
    roots: &oasis7_formal_protocol::incremental_execution::RootSetV1,
    overlay: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    read: &mut WorldFieldReadContextV1<'_, P, V, PE, PC>,
    work: &mut super::incremental_authorization_root_current::AuthorizationRootSourceReadWorkV1,
    common: &mut FormalBoundedContextV1<'_>,
) -> Result<u64, BodyMutationError>
where
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    P: MerkleNodeReader,
    V: ValueNodeReader,
    PE: FnMut() -> Option<ValueTreeError>,
    PC: FnMut() -> bool,
{
    super::incremental_authorization_root_current::with_authorization_source_read_work_v1(
        roots,
        overlay,
        read,
        work,
        common,
        |overlay, _, _| {
            oasis7_world_state::incremental_world_u64::read_world_u64_overlay_v1(
                oasis7_world_state::incremental_world_u64::WorldU64FieldV1::Time,
                overlay,
            )
            .map_err(super::incremental_authorization_root_current::SourceError::from)
        },
    )
    .map_err(|error| match error {
        super::incremental_authorization_root_current::SourceError::Execution(e) => e.into(),
        super::incremental_authorization_root_current::SourceError::Cache(
            super::incremental_authorization_root_current::AuthorizationRootCacheErrorV1::Value(e),
        ) => e.into(),
        error => BodyMutationError::Original(Box::new(CurrentClockSourceError(error))),
    })
}

/// Mandatory genuine resident owners, not a bag of caller-selected roots/Qs.
/// Their respective consumers retain their original full source validation.
pub(in crate::world) struct NormalHostContextSourcesV1<'a> {
    pub projection_cache: &'a ReservedValue<
        super::incremental_normal_projection_commitment::AcceptedNormalProjectionCacheV1,
    >,
    pub business: &'a ReservedValue<
        super::incremental_normal_business_commitment::NormalBusinessMetadataParentSourceV1,
    >,
    pub process: &'a std::sync::Arc<
        ReservedValue<super::incremental_normal_process_source::NormalProcessSourceV1>,
    >,
    pub manifest: &'a ReservedValue<
        super::incremental_module_execution_manifest_hash::AcceptedWorldManifestHashV1,
    >,
    pub delivery: &'a ReservedValue<ProductValidationDeliveryIndexV1>,
    pub retention: &'a ReservedValue<WorldJournalRetentionV1>,
    pub materials: &'a ReservedValue<
        oasis7_world_state::incremental_inventory::ParentInventoryMaterialScopeV1,
    >,
    pub caches: &'a ReservedValue<super::incremental_normal_parent_caches::NormalParentCachesV1>,
    pub compiler: &'a crate::module_source_compiler::RuntimeSourceCompiler,
}
/// Exact read-only construction inputs used before the projection cache can
/// exist. This private view borrows genuine accepted-source owners; ordinary
/// execution still requires the full NormalHostContextSourcesV1 carrier.
pub(in crate::world) struct NormalHostRootReadSourcesV1<'a> {
    pub manifest: &'a ReservedValue<
        super::incremental_module_execution_manifest_hash::AcceptedWorldManifestHashV1,
    >,
    pub delivery: &'a ReservedValue<ProductValidationDeliveryIndexV1>,
    pub retention: &'a ReservedValue<WorldJournalRetentionV1>,
    pub materials: &'a ReservedValue<
        oasis7_world_state::incremental_inventory::ParentInventoryMaterialScopeV1,
    >,
}
impl<'a> NormalHostContextSourcesV1<'a> {
    pub(in crate::world) fn root_read_sources_v1(&self) -> NormalHostRootReadSourcesV1<'a> {
        NormalHostRootReadSourcesV1 {
            manifest: self.manifest,
            delivery: self.delivery,
            retention: self.retention,
            materials: self.materials,
        }
    }
}
fn overlay(e: WorldFieldOverlayErrorV1) -> BodyMutationError {
    match e {
        WorldFieldOverlayErrorV1::Execution(e) => e.into(),
        WorldFieldOverlayErrorV1::Value(e) => e.into(),
        WorldFieldOverlayErrorV1::Member(e) => BodyMutationError::Original(Box::new(e)),
    }
}
struct BeginLoans<'a, 'read, M, R, E, C> {
    pending: &'a mut PendingActionIndexWorkV1,
    policy: &'a mut RevocationReadWorkV1,
    scan: &'a mut StateScanBudget,
    admission: &'a mut WorldFieldReadContextV1<'read, M, R, E, C>,
}
impl<M, R, E, C> FactoryPhaseMeterLanesV1 for BeginLoans<'_, '_, M, R, E, C> {
    fn visit_lanes(&mut self, v: &mut dyn for<'a> FnMut(FactoryPhaseMeterLaneV1<'a>)) {
        use FactoryPhaseMeterLaneV1::{Bytes as B, Steps as S};
        v(S(&mut self.pending.steps_remaining));
        v(S(&mut self.policy.json.cells_remaining));
        v(S(&mut self.policy.json.json_nodes_remaining));
        v(S(&mut self.scan.nodes_remaining));
        v(B(&mut self.scan.read_bytes_remaining));
        v(S(&mut self.scan.leaves_remaining));
        v(S(&mut self.admission.values.io.nodes_remaining));
        v(B(&mut self.admission.values.io.read_bytes_remaining));
        v(B(&mut self.admission.values.io.write_bytes_remaining));
        v(B(&mut self.admission.key_work.bytes_remaining));
    }
}
/// The callback is private Runtime composition. It receives one ongoing FIFO,
/// revocation view, delivery view and the exact original remaining work loans.
/// No Root/worker/context can escape this scope; T may retain genuine parent
/// sources, but conveys no completed/accepted-input or completed-tick status.
#[allow(clippy::too_many_arguments)]
#[derive(Debug)]
pub enum NormalHostContextErrorV1<OE, CE> {
    Root(RootModuleExecutionErrorV1<OE>),
    Operation(CE),
}

#[allow(clippy::too_many_arguments)]
pub(in crate::world) fn with_begun_normal_root_v1<
    's,
    'p,
    M,
    R,
    E,
    C,
    W,
    F,
    O,
    T,
    CE,
    Run,
>(
    parent: &'p ReservedValue<BoundNormalTickParentV1<'p>>,
    transaction: &mut ExecutionTransaction<'s, M>,
    state: &M,
    values: &R,
    member_sink: &mut W,
    fee_sink: &mut W,
    event_sink: &mut W,
    originals: &mut O,
    persist_original: F,
    worker: &BoundedWorkerSandboxV1<'_, RootModuleWorkerAllocatorV1>,
    sources: &NormalHostRootReadSourcesV1<'p>,
    work: &mut NormalHostWorkV1,
    take_state_error: E,
    cancelled: C,
    common: &mut FormalBoundedContextV1<'_>,
    live_owner: &mut dyn FnMut() -> ExecutionFence,
    run: Run,
) -> Result<T, NormalHostContextErrorV1<O::Error, CE>>
where
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError> + Copy,
    C: FnMut() -> bool + Copy,
    W: ValueNodeReader + ValueNodeWriter + StatePageWriter,
    F: FnMut(&mut dyn AgentEventOriginalV1) -> Result<(), BodyMutationError>,
    O: OriginalObjectStreamReader,
    Run: FnOnce(
        &mut RootModuleExecutionContextV1<'_, '_, 's, '_, M, R, E, C, W, F, O>,
        &mut ReservedValue<PendingActionCurrentV1>,
        &mut RuntimeRevocationCurrentV1,
        &mut CurrentProductValidationDeliveryIndexV1<'p>,
        &mut WorldFieldReadContextV1<'_, M, R, E, C>,
        &mut WorldFieldReadContextV1<'_, M, R, E, C>,
        &mut WorldFieldReadContextV1<'_, M, R, E, C>,
        &mut FactoryTransitionReadContextV1<'_, R, C>,
        &mut IndustryTransitionReadContextV1<'_, R, C>,
        &mut IndustryFamilyReadContextV1<'_, M, R, E, C>,
        &mut CorePolicyMaterialContextV1<'_, M, E>,
        &mut NormalHostDriverWorkV1,
        &mut FormalBoundedContextV1<'_>,
        &mut dyn FnMut() -> ExecutionFence,
    ) -> Result<T, CE>,
{
    let result: std::thread::Result<Result<Result<T, CE>, RootModuleExecutionErrorV1<O::Error>>> =
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            parent
                .validate_context(common)
                .map_err(|e| BodyMutationError::Original(Box::new(e)))?;
            let live = live_owner();
            transaction.validate_guard(&transaction.guard(), &live)?;
            if transaction.parent_fence() != parent.fence()
                || !transaction.uses_budget(common.memory())
            {
                return Err(BodyMutationError::Invalid("normal Host actual parent/TX/pool").into());
            }
            // Native descriptor/file/allocator are borrowed from the real Host worker;
            // fixture executables cannot enter this production composition.
            let executable = match &worker.executable {
                WorkerExecutableV1::NativeBoundSealedFd { file, descriptor } => {
                    WorkerExecutableV1::NativeBoundSealedFd { file, descriptor }
                }
                _ => {
                    return Err(BodyMutationError::Invalid("normal Host NativeBound worker").into());
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
            let families = parent
                .module_execution_families_v1(common)
                .map_err(|e| BodyMutationError::Original(Box::new(e)))?;
            common
                .consume_steps(1)
                .map_err(|e| BodyMutationError::Original(Box::new(e)))?;
            // This constructor rejects any already-contributed Input/Event/Result fact.
            let counter = WorldEventCounterV1::begin(
                parent.refs().runtime,
                parent.refs().head,
                transaction,
                common.memory(),
            )?;
            let mut delivery=sources.delivery.begin_current(sources.delivery,parent.refs().head,sources.retention,&counter,transaction,common,&live_owner()).map_err(|e|RootModuleExecutionErrorV1::ProductDelivery(super::incremental_normal_industry_due::delivery::DeliverySourceErrorV1::Data(e)))?;
            let memory_pool = common.memory().clone();
            let memory = &memory_pool;
            let io = &mut work.io;
            let mut current = io.current.field(
                families.roots.state_root(StateNamespaceV1::Business),
                state,
                values,
                memory,
                take_state_error,
                cancelled,
            );
            let mut parent_read = io.parent.field(
                families.roots.state_root(StateNamespaceV1::Business),
                state,
                values,
                memory,
                take_state_error,
                cancelled,
            );
            let mut admission = io.admission.field(
                families.roots.state_root(StateNamespaceV1::Authorization),
                state,
                values,
                memory,
                take_state_error,
                cancelled,
            );
            let mut factory = io.factory.factory(values, memory, cancelled);
            let mut recipe = io.recipe.recipe(values, memory, cancelled);
            let mut industry = IndustryFamilyReadContextV1 {
                state,
                values: ValueReadContext {
                    source: values,
                    memory,
                    class: MemoryClass::Ordinary,
                    io: &mut io.industry.io,
                },
                key_work: &mut io.industry.keys,
                semantic_bytes: io.industry.semantic_bytes,
                record_bytes: io.industry.record_bytes,
                max_payload_bytes: io.industry.max_payload_bytes,
                outer_scan: &mut io.industry_scan,
                take_state_error,
                cancelled,
            };
            let mut materials =
                work.materials
                    .context(sources.materials, families.roots, state, take_state_error);
            let mut overlay = WorldFieldOverlayContextV1 {
                transaction: &mut *transaction,
                values: ValueReadContext {
                    source: values,
                    memory,
                    class: MemoryClass::Ordinary,
                    io: &mut io.overlay.io,
                },
                key_work: &mut io.overlay.keys,
                semantic_bytes: io.overlay.semantic_bytes,
                record_bytes: io.overlay.record_bytes,
                take_state_error,
                cancelled,
            };
            let mut members = io.members.members(member_sink, memory, cancelled);
            let mut fees = RootModuleFeeWriteContextV1 {
                values: ValueMutationContextV1 {
                    sink: fee_sink,
                    memory,
                    io: &mut io.fees.io,
                    cancelled,
                },
                key_work: &mut io.fees.keys,
                member_limits: io.fees.limits,
            };
            let now = read_current_normal_clock_v1(
                families.roots,
                &mut overlay,
                &mut parent_read,
                &mut work.driver.final_sources.projection.aux_authorization,
                common,
            )?;
            let original_slot = &mut work.invocation.event_original;
            // Zero is an inert occupied slot, not a fresh allowance. On every exit only
            // the real remaining counters owned by `events` return to this same slot.
            let placeholder = CanonicalCborWorkV1 {
                visits_remaining: 0,
                bytes_remaining: 0,
                max_depth: original_slot.max_depth,
                max_map_keys: original_slot.max_map_keys,
                max_key_bytes: original_slot.max_key_bytes,
            };
            let mut events = ModuleOutputEventContextV1 {
                values,
                world_events: &counter,
                mutation: BodyMutationContext {
                    sink: event_sink,
                    memory,
                    io: &mut io.events.io,
                    cancelled,
                },
                persist_original,
                max_event_bytes: work.invocation.max_event_bytes,
                original_work: std::mem::replace(original_slot, placeholder),
            };
            let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                let mut root = RootModuleExecutionContextV1 {
                    families: *families,
                    now,
                    state,
                    ctx: &mut overlay,
                    members: &mut members,
                    fees: &mut fees,
                    events: &mut events,
                    originals,
                    worker: &mut backend,
                    manifest_cache: sources.manifest,
                    manifest_work: &mut work.invocation.manifest,
                    work: &mut work.invocation.invocation,
                    #[cfg(any(test, feature = "test-support"))]
                    input_observation: None,
                };
                let mut loans = BeginLoans {
                    pending: &mut work.driver.pending.index,
                    policy: &mut work.driver.policy,
                    scan: &mut work.driver.policy_scan,
                    admission: &mut admission,
                };
                let (mut pending, mut revocation) = root.with_factory_prologue_external_meter_v1(
                    common,
                    live_owner,
                    &mut loans,
                    |root, live, loans| {
                        let guard = root.ctx.transaction.guard();
                        let fence = live();
                        let pending = begin_pending_action_current_v1(
                            parent.refs().storage.current().runtime().pending(),
                            families.roots,
                            root.ctx.transaction,
                            root.members.memory,
                            &guard,
                            &fence,
                            loans.pending,
                            &mut root.ctx.cancelled,
                        )
                        .map_err(overlay_error)?;
                        let guard = root.ctx.transaction.guard();
                        let fence = live();
                        let revocation = begin_runtime_revocation_current_v1(
                            parent.refs().storage.current().runtime().revocation(),
                            root.ctx,
                            loans.admission,
                            loans.scan,
                            loans.policy,
                            root.members,
                            &guard,
                            &fence,
                        )
                        .map_err(overlay_error)?;
                        Ok((pending, revocation))
                    },
                )?;
                drop(loans);
                Ok::<_, RootModuleExecutionErrorV1<O::Error>>(run(
                    &mut root,
                    &mut pending,
                    &mut revocation,
                    &mut delivery,
                    &mut current,
                    &mut parent_read,
                    &mut admission,
                    &mut factory,
                    &mut recipe,
                    &mut industry,
                    &mut materials,
                    &mut work.driver,
                    common,
                    live_owner,
                ))
            }));
            original_slot.visits_remaining = events.original_work.visits_remaining;
            original_slot.bytes_remaining = events.original_work.bytes_remaining;
            match outcome {
                Ok(result) => result,
                Err(payload) => std::panic::resume_unwind(payload),
            }
        }));
    match result {
        Ok(Ok(Ok(value))) => Ok(value),
        Ok(Ok(Err(error))) => {
            let _: Result<(), ExecutionError> =
                transaction.contribute(|_| Err(ExecutionError::Aborted));
            Err(NormalHostContextErrorV1::Operation(error))
        }
        Ok(Err(error)) => {
            let _: Result<(), ExecutionError> =
                transaction.contribute(|_| Err(ExecutionError::Aborted));
            Err(NormalHostContextErrorV1::Root(error))
        }
        Err(payload) => {
            let _: Result<(), ExecutionError> =
                transaction.contribute(|_| Err(ExecutionError::Aborted));
            std::panic::resume_unwind(payload)
        }
    }
}
fn overlay_error<E>(e: WorldFieldOverlayErrorV1) -> RootModuleExecutionErrorV1<E> {
    RootModuleExecutionErrorV1::Body(overlay(e))
}
