//! Fixed normal-input queue mutation. Its result is DATA, never input admission.
use super::incremental_factory_phase::io::{FactoryPhaseMeterLaneV1, FactoryPhaseMeterLanesV1};
use super::{
    World,
    incremental_body::BodyMutationError,
    incremental_body_event::{AgentEventOriginalV1, original},
    incremental_module_invocation::*,
    incremental_normal_sources::BoundNormalTickParentV1,
};
use oasis7_agent_api::world_action::GovernanceAuthorizationV1;
use oasis7_formal_protocol::{
    controlled_authority::formal::bounded_context::FormalBoundedContextV1,
    incremental_artifacts::OriginalArtifactRefV1,
    incremental_execution::{ExecutionError, ExecutionFence},
    incremental_material::OriginalObjectStreamReader,
    incremental_state::{MerkleNodeReader, StatePageWriter},
    incremental_value::{ValueNodeReader, ValueNodeWriter, ValueTreeError},
};
use oasis7_proto::memory_budget::{MemoryClass, ReservedValue};
use oasis7_world_state::{Action, CommittedRecipeOrigin};
use oasis7_world_state::{
    incremental_pending_action_family::PendingActionFamilyWorkV1,
    incremental_pending_action_overlay::*,
    incremental_runtime_pending_action_record::{
        PendingActionPositionV1, PendingActionWriteWorkV1,
    },
    incremental_runtime_scheduler_controls::*,
};

/// Caller keeps the paid decoded input graph alive through this consuming call.
/// These correlation fields do not establish authorization or input identity.
pub(in crate::world) enum CommittedActionMetadataV1 {
    Ordinary,
    Governance(GovernanceAuthorizationV1),
    Recipe(CommittedRecipeOrigin),
}
pub(in crate::world) struct CommittedInputQueueWorkV1<'a> {
    pub pending: &'a mut PendingActionFamilyWorkV1,
    pub write: &'a mut PendingActionWriteWorkV1,
}
impl FactoryPhaseMeterLanesV1 for CommittedInputQueueWorkV1<'_> {
    fn visit_lanes(&mut self, v: &mut dyn for<'a> FnMut(FactoryPhaseMeterLaneV1<'a>)) {
        use FactoryPhaseMeterLaneV1::{Bytes as B, Steps as S};
        let p = &mut *self.pending;
        v(S(&mut p.index.steps_remaining));
        v(S(&mut p.typed.json.cells_remaining));
        v(S(&mut p.typed.json.json_nodes_remaining));
        v(S(&mut p.original.objects_remaining));
        v(B(&mut p.original.original_bytes_remaining));
        v(S(&mut p.original.canonical.visits_remaining));
        v(B(&mut p.original.canonical.bytes_remaining));
        v(S(&mut p.selected_scan.nodes_remaining));
        v(B(&mut p.selected_scan.read_bytes_remaining));
        v(S(&mut p.selected_scan.leaves_remaining));
        v(S(&mut self.write.cells_remaining));
        v(S(&mut self.write.json.cells_remaining));
        v(S(&mut self.write.json.json_nodes_remaining));
    }
}
#[derive(Debug)]
pub(in crate::world) enum CommittedInputQueueMutationErrorV1<E> {
    Recipe(ReservedValue<crate::WorldError>),
    Runtime(RootModuleExecutionErrorV1<E>),
}
/// The exact provisional mutation report; neither this type nor its fields can
/// be substituted for a bound input, durable Original, or completed tick.
pub(in crate::world) struct QueueWriteDataV1 {
    action_id: u64,
    position: PendingActionPositionV1,
    original: OriginalArtifactRefV1,
    evicted: u64,
}
/// Genuine prepare-time allocator issuance. It must remain held through the Cognition
/// submit-with-id path; reading its number alone never authorizes an enqueue.
pub(in crate::world) struct AllocatedCognitionActionIdV1 {
    id: u64,
    parent_record: [u8; 32],
    guard: oasis7_formal_protocol::incremental_execution::PreparedGuard,
    core: oasis7_formal_protocol::controlled_authority::formal::bounded_context::WorkIdentityV1,
    memory: oasis7_proto::memory_budget::MemoryBudget,
    _fee: oasis7_proto::memory_budget::MemoryReservation,
}
impl AllocatedCognitionActionIdV1 {
    pub(in crate::world) fn id(&self) -> u64 {
        self.id
    }
    pub(in crate::world) fn validate<M: MerkleNodeReader>(
        &self,
        parent: &ReservedValue<BoundNormalTickParentV1<'_>>,
        tx: &oasis7_formal_protocol::incremental_execution::ExecutionTransaction<'_, M>,
        common: &mut FormalBoundedContextV1<'_>,
        live: &ExecutionFence,
    ) -> Result<(), BodyMutationError> {
        parent
            .validate_context(common)
            .map_err(|e| BodyMutationError::Original(Box::new(e)))?;
        common
            .charge(4, 32)
            .map_err(|e| BodyMutationError::Original(Box::new(e)))?;
        tx.validate_exact_transaction_identity(&self.guard)?;
        tx.validate_guard(&tx.guard(), live)?;
        if self.core != common.work().identity()
            || !self.memory.same_pool(common.memory())
            || !self._fee.uses_budget(common.memory())
            || !tx.uses_budget(common.memory())
            || self.parent_record != parent.refs().head.verified_record_id()
        {
            return Err(BodyMutationError::Invalid(
                "Cognition allocated action identity",
            ));
        }
        Ok(())
    }
}
/// The exact allocation transition consumes a validated before cut. Its
/// retained inputs are historical before-DATA after the allocator write; they
/// are never relabelled as a fresh Current snapshot.
pub(in crate::world) struct AllocatedCurrentCognitionPreparationV1<'a> {
    inputs: super::incremental_normal_cognition_commit::HistoricalCurrentCognitionInputV1<'a>,
    allocation: AllocatedCognitionActionIdV1,
    _fee: oasis7_proto::memory_budget::MemoryReservation,
}
impl<'a> AllocatedCurrentCognitionPreparationV1<'a> {
    pub(in crate::world) fn before_inputs(
        &self,
    ) -> &super::incremental_normal_cognition_commit::CurrentCognitionCommitInputV1<'a> {
        self.inputs.before_inputs()
    }
    pub(in crate::world) fn action_id(&self) -> u64 {
        self.allocation.id()
    }
    pub(in crate::world) fn validate<M: MerkleNodeReader>(
        &self,
        parent: &ReservedValue<BoundNormalTickParentV1<'_>>,
        tx: &oasis7_formal_protocol::incremental_execution::ExecutionTransaction<'_, M>,
        common: &mut FormalBoundedContextV1<'_>,
        live: &ExecutionFence,
    ) -> Result<(), BodyMutationError> {
        self.inputs.validate_historical(parent, tx, common, live)?;
        self.allocation.validate(parent, tx, common, live)?;
        if !self._fee.uses_budget(common.memory()) {
            return Err(BodyMutationError::Invalid(
                "Cognition allocated preparation pool",
            ));
        }
        Ok(())
    }
}
impl QueueWriteDataV1 {
    pub(in crate::world) fn action_id(&self) -> u64 {
        self.action_id
    }
    pub(in crate::world) fn position(&self) -> PendingActionPositionV1 {
        self.position
    }
    pub(in crate::world) fn original(&self) -> OriginalArtifactRefV1 {
        self.original
    }
    pub(in crate::world) fn evicted(&self) -> u64 {
        self.evicted
    }
}
fn overlay(
    e: oasis7_world_state::incremental_field_overlay::WorldFieldOverlayErrorV1,
) -> BodyMutationError {
    use oasis7_world_state::incremental_field_overlay::WorldFieldOverlayErrorV1 as E;
    match e {
        E::Execution(e) => e.into(),
        E::Value(e) => e.into(),
        E::Member(e) => BodyMutationError::Original(Box::new(e)),
    }
}
fn invalid<E>(s: &'static str) -> RootModuleExecutionErrorV1<E> {
    RootModuleExecutionErrorV1::Body(BodyMutationError::Invalid(s))
}
/// Charge the actual strings visited by the unchanged pre-allocation recipe
/// predicate, including whitespace scans and comparison operands.
fn recipe_validation_bytes(
    action: &Action,
    o: &CommittedRecipeOrigin,
) -> Result<u64, ValueTreeError> {
    let s = &o.submission;
    let mut bytes = 0usize;
    for n in [
        s.verified_player_id.len(),
        s.public_key.len(),
        s.hosted_registration_nonce.as_ref().map_or(0, String::len),
        s.requester_agent_id.len(),
        s.factory_id.len(),
        s.recipe_id.len(),
        o.consensus_submitter_player_id.len(),
        o.action_payload_hash.len(),
        o.action_root.len(),
    ] {
        bytes = bytes.checked_add(n).ok_or(ValueTreeError::ResourceLimited(
            "recipe input validation bytes",
        ))?;
    }
    if let Action::ScheduleRecipe {
        requester_agent_id,
        factory_id,
        recipe_id,
        ..
    } = action
    {
        for n in [requester_agent_id.len(), factory_id.len(), recipe_id.len()] {
            bytes = bytes.checked_add(n).ok_or(ValueTreeError::ResourceLimited(
                "recipe input validation bytes",
            ))?;
        }
    }
    u64::try_from(bytes)
        .map_err(|_| ValueTreeError::ResourceLimited("recipe input validation bytes"))
}
impl<'a, 't, 's, 'v, M, R, E, C, W, F, O>
    RootModuleExecutionContextV1<'a, 't, 's, 'v, M, R, E, C, W, F, O>
where
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    W: ValueNodeReader + ValueNodeWriter + StatePageWriter,
    F: FnMut(&mut dyn AgentEventOriginalV1) -> Result<(), BodyMutationError>,
    O: OriginalObjectStreamReader,
{
    /// Consume the before source captured before binding/prefix writes. Validate
    /// its historical transaction identity, then perform the real allocation;
    /// the retained snapshot is never relabelled as Current after those writes.
    pub(in crate::world) fn allocate_current_cognition_preparation_v1<'i>(
        &mut self,
        inputs: super::incremental_normal_cognition_commit::HistoricalCurrentCognitionInputV1<'i>,
        parent: &ReservedValue<BoundNormalTickParentV1<'_>>,
        common: &mut FormalBoundedContextV1<'_>,
        live_owner: &mut dyn FnMut() -> ExecutionFence,
    ) -> Result<AllocatedCurrentCognitionPreparationV1<'i>, RootModuleExecutionErrorV1<O::Error>>
    {
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            inputs.validate_historical(parent, self.ctx.transaction, common, &live_owner())?;
            let bytes = std::mem::size_of::<AllocatedCurrentCognitionPreparationV1<'_>>();
            common
                .charge(1, bytes as u64)
                .map_err(|e| BodyMutationError::Original(Box::new(e)))?;
            let fee = common
                .memory()
                .reserve(bytes, MemoryClass::Ordinary)
                .map_err(ValueTreeError::Memory)?;
            let allocation =
                self.reserve_current_cognition_action_id_v1(parent, common, live_owner)?;
            let result = AllocatedCurrentCognitionPreparationV1 {
                inputs,
                allocation,
                _fee: fee,
            };
            result.validate(parent, self.ctx.transaction, common, &live_owner())?;
            Ok(result)
        }));
        match result {
            Ok(Ok(value)) => Ok(value),
            Ok(Err(error)) => {
                let _: Result<(), ExecutionError> = self
                    .ctx
                    .transaction
                    .contribute(|_| Err(ExecutionError::Aborted));
                Err(error)
            }
            Err(panic) => {
                let _: Result<(), ExecutionError> = self
                    .ctx
                    .transaction
                    .contribute(|_| Err(ExecutionError::Aborted));
                std::panic::resume_unwind(panic)
            }
        }
    }
    /// Original Cognition prepare allocates once before building its marker.
    /// Use the same Current allocator and the same ongoing Common/IO lanes as
    /// ordinary enqueue; the later Cognition submit must retain this owner.
    pub(in crate::world) fn reserve_current_cognition_action_id_v1(
        &mut self,
        parent: &ReservedValue<BoundNormalTickParentV1<'_>>,
        common: &mut FormalBoundedContextV1<'_>,
        live_owner: &mut dyn FnMut() -> ExecutionFence,
    ) -> Result<AllocatedCognitionActionIdV1, RootModuleExecutionErrorV1<O::Error>> {
        let attempt = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            parent
                .validate_context(common)
                .map_err(|e| BodyMutationError::Original(Box::new(e)))?;
            if self.ctx.transaction.parent_fence() != parent.fence() {
                return Err(invalid("Cognition allocator actual parent"));
            }
            let bytes = std::mem::size_of::<AllocatedCognitionActionIdV1>();
            common
                .charge(1, bytes as u64)
                .map_err(|e| BodyMutationError::Original(Box::new(e)))?;
            let fee = common
                .memory()
                .reserve(bytes, MemoryClass::Ordinary)
                .map_err(ValueTreeError::Memory)?;
            let id = with_current_root_module_meter_v1(self, common, live_owner, |root, _| {
                use super::incremental_allocators::RuntimeAllocatorV1;
                let expected = World::read_iws_allocator(
                    RuntimeAllocatorV1::Action,
                    root.ctx.transaction,
                    root.events.values,
                    &mut root.events.mutation,
                )?;
                let (id, _) = World::contribute_iws_allocate(
                    RuntimeAllocatorV1::Action,
                    expected,
                    root.ctx.transaction,
                    root.events.values,
                    &mut root.events.mutation,
                )?;
                Ok(id)
            })?;
            let value = AllocatedCognitionActionIdV1 {
                id,
                parent_record: parent.refs().head.verified_record_id(),
                guard: self.ctx.transaction.guard(),
                core: common.work().identity(),
                memory: common.memory().clone(),
                _fee: fee,
            };
            value.validate(parent, self.ctx.transaction, common, &live_owner())?;
            Ok(value)
        }));
        match attempt {
            Ok(Ok(value)) => Ok(value),
            Ok(Err(error)) => {
                let _: Result<(), ExecutionError> = self
                    .ctx
                    .transaction
                    .contribute(|_| Err(ExecutionError::Aborted));
                Err(error)
            }
            Err(panic) => {
                let _: Result<(), ExecutionError> = self
                    .ctx
                    .transaction
                    .contribute(|_| Err(ExecutionError::Aborted));
                std::panic::resume_unwind(panic)
            }
        }
    }
    /// The caller supplies the one ongoing Current FIFO owner; reopening the
    /// parent FIFO here would lose earlier appends in the same transaction.
    pub(in crate::world) fn enqueue_committed_action_data_v1(
        &mut self,
        parent: &ReservedValue<BoundNormalTickParentV1<'_>>,
        pending: &mut ReservedValue<PendingActionCurrentV1>,
        action: Action,
        metadata: CommittedActionMetadataV1,
        work: &mut CommittedInputQueueWorkV1<'_>,
        max_action_bytes: usize,
        common: &mut FormalBoundedContextV1<'_>,
        live_owner: &mut dyn FnMut() -> ExecutionFence,
    ) -> Result<ReservedValue<QueueWriteDataV1>, CommittedInputQueueMutationErrorV1<O::Error>> {
        use CommittedInputQueueMutationErrorV1::{Recipe, Runtime};
        let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            // Preserve the legacy recipe rejection before allocation or queue mutation.
            if let CommittedActionMetadataV1::Recipe(origin) = &metadata {
                let bytes = recipe_validation_bytes(&action, origin)
                    .map_err(|e| Runtime(RootModuleExecutionErrorV1::Body(e.into())))?;
                common.charge(16, bytes).map_err(|e| {
                    Runtime(RootModuleExecutionErrorV1::Body(
                        BodyMutationError::Original(Box::new(e)),
                    ))
                })?;
                let fee = common
                    .memory()
                    .reserve(
                        std::mem::size_of::<crate::WorldError>()
                            + "committed recipe origin action mismatch".len(),
                        MemoryClass::Ordinary,
                    )
                    .map_err(|e| {
                        Runtime(RootModuleExecutionErrorV1::Value(ValueTreeError::Memory(e)))
                    })?;
                super::actions::validate_committed_recipe_submission(&action, origin)
                    .map_err(|error| Recipe(fee.retain(error)))?;
            }
            let result = (|| {
                parent.validate_context(common).map_err(|e| {
                    RootModuleExecutionErrorV1::Body(BodyMutationError::Original(Box::new(e)))
                })?;
                parent
                    .validate_module_execution_families_v1(&self.families, common)
                    .map_err(|e| {
                        RootModuleExecutionErrorV1::Body(BodyMutationError::Original(Box::new(e)))
                    })?;
                if self.ctx.transaction.parent_fence() != parent.fence()
                    || !self.ctx.transaction.uses_budget(common.memory())
                {
                    return Err(invalid("committed queue parent/pool"));
                }
                let live = live_owner();
                self.ctx
                    .transaction
                    .validate_guard(&self.ctx.transaction.guard(), &live)
                    .map_err(BodyMutationError::from)?;
                let output = common
                    .memory()
                    .reserve(
                        std::mem::size_of::<QueueWriteDataV1>(),
                        MemoryClass::Ordinary,
                    )
                    .map_err(ValueTreeError::Memory)?;
                // The Root IO/canonical lanes and all borrowed pending lanes share exactly
                // one Common child. Its mapper reconciles real deltas on Err and unwind.
                self.with_factory_prologue_external_meter_v1(
                    common,
                    live_owner,
                    work,
                    |root, live_owner, work| {
                        use super::incremental_allocators::RuntimeAllocatorV1;
                        let expected = World::read_iws_allocator(
                            RuntimeAllocatorV1::Action,
                            root.ctx.transaction,
                            root.events.values,
                            &mut root.events.mutation,
                        )?;
                        let (action_id, _) = World::contribute_iws_allocate(
                            RuntimeAllocatorV1::Action,
                            expected,
                            root.ctx.transaction,
                            root.events.values,
                            &mut root.events.mutation,
                        )?;
                        let (governance, recipe) = match metadata {
                            CommittedActionMetadataV1::Ordinary => (None, None),
                            CommittedActionMetadataV1::Governance(a) => (Some(a), None),
                            CommittedActionMetadataV1::Recipe(o) => (None, Some(o)),
                        };
                        let envelope = super::actions::submission_envelope(
                            action_id, action, governance, recipe,
                        );
                        let reference = {
                            let measured = original::measure_serializable(
                                &envelope,
                                root.events.mutation.memory,
                                &mut root.events.original_work,
                                &mut root.events.mutation.cancelled,
                                max_action_bytes,
                            )?;
                            let mut producer = original::Producer::new(
                                measured,
                                root.events.mutation.memory,
                                &mut root.events.original_work,
                                &mut root.events.mutation.cancelled,
                            );
                            let reference = OriginalArtifactRefV1 {
                                id: producer.original_id(),
                                byte_len: producer.original_bytes(),
                            };
                            (root.events.persist_original)(&mut producer)?;
                            producer.check_complete()?;
                            reference
                        };
                        let guard = root.ctx.transaction.guard();
                        let live = live_owner();
                        let position = append_pending_action_v1(
                            pending,
                            &envelope,
                            reference,
                            root.ctx.transaction,
                            root.members,
                            &guard,
                            &live,
                            &mut work.pending.index,
                            work.write,
                        )
                        .map_err(overlay)?;
                        // Limits are read at the same point as the legacy post-enqueue limiter.
                        let q = parent.refs().storage.current().runtime().scheduler();
                        let guard = root.ctx.transaction.guard();
                        let live = live_owner();
                        let max = read_qualified_runtime_scheduler_scalar_overlay_v1(
                            RuntimeSchedulerScalarRequestV1 {
                                field: RuntimeSchedulerScalarV1::MaxPendingActions,
                                qualification: q,
                            },
                            root.ctx,
                            8,
                            &mut std::io::sink(),
                            &guard,
                            &live,
                        )
                        .map_err(overlay)?;
                        let RuntimeSchedulerControlValueV1::Unsigned(max) = max else {
                            return Err(invalid("pending action limit scalar"));
                        };
                        let max = max.max(1);
                        let mut evicted = 0u64;
                        loop {
                            let guard = root.ctx.transaction.guard();
                            let live = live_owner();
                            let len = current_pending_action_len_v1(
                                pending,
                                root.ctx.transaction,
                                root.members.memory,
                                &guard,
                                &live,
                            )
                            .map_err(overlay)?;
                            if u64::try_from(len).map_err(|_| {
                                ValueTreeError::ResourceLimited("pending FIFO length")
                            })? <= max
                            {
                                break;
                            }
                            let guard = root.ctx.transaction.guard();
                            let live = live_owner();
                            let removed = pop_pending_action_v1(
                                pending,
                                root.ctx.transaction,
                                root.members.memory,
                                &guard,
                                &live,
                                &mut work.pending.index,
                                root.members.key_work,
                                &mut root.members.cancelled,
                            )
                            .map_err(overlay)?;
                            if removed.is_none() {
                                return Err(invalid("pending FIFO eviction head"));
                            }
                            evicted = evicted.saturating_add(1);
                        }
                        if evicted != 0 {
                            let guard = root.ctx.transaction.guard();
                            let live = live_owner();
                            let old = read_qualified_runtime_scheduler_scalar_overlay_v1(
                                RuntimeSchedulerScalarRequestV1 {
                                    field: RuntimeSchedulerScalarV1::PendingActionsEvicted,
                                    qualification: q,
                                },
                                root.ctx,
                                8,
                                &mut std::io::sink(),
                                &guard,
                                &live,
                            )
                            .map_err(overlay)?;
                            let RuntimeSchedulerControlValueV1::Unsigned(old) = old else {
                                return Err(invalid("pending eviction stats scalar"));
                            };
                            let guard = root.ctx.transaction.guard();
                            let live = live_owner();
                            change_qualified_runtime_scheduler_scalar_v1(
                                RuntimeSchedulerScalarChangeV1 {
                                    field: RuntimeSchedulerScalarV1::PendingActionsEvicted,
                                    qualification: q,
                                    expected: RuntimeSchedulerControlValueRefV1::U64(old),
                                    after: RuntimeSchedulerControlValueRefV1::U64(
                                        old.saturating_add(evicted),
                                    ),
                                    max_current_payload_bytes: 8,
                                },
                                root.ctx,
                                root.members,
                                &guard,
                                &live,
                            )
                            .map_err(overlay)?;
                        }
                        Ok(output.retain(QueueWriteDataV1 {
                            action_id,
                            position,
                            original: reference,
                            evicted,
                        }))
                    },
                )
            })();
            result.map_err(Runtime)
        }));
        match outcome {
            Ok(result) => {
                if matches!(&result, Err(Runtime(_))) {
                    let _: Result<(), ExecutionError> = self
                        .ctx
                        .transaction
                        .contribute(|_| Err(ExecutionError::Aborted));
                }
                result
            }
            Err(payload) => {
                let _: Result<(), ExecutionError> = self
                    .ctx
                    .transaction
                    .contribute(|_| Err(ExecutionError::Aborted));
                std::panic::resume_unwind(payload)
            }
        }
    }
}

/// Errors retain deferred/refused payloads, including their original paid
/// graph. A deferred service/bootstrap input must be handled by its own fixed
/// consumer; it can never be substituted for an enqueued Runtime action.
#[derive(Debug)]
pub(in crate::world) enum BoundRuntimeInputQueueErrorV1<E> {
    Resource(oasis7_formal_protocol::controlled_authority::formal::bounded_cbor_data::BoundedCborDataErrorV1),
    Payload(super::incremental_normal_input_decode::RuntimePayloadConsumeErrorV1),
    Queue(CommittedInputQueueMutationErrorV1<E>),
    Invalid(&'static str),
    Reserved(ReservedValue<String>),
}
impl<'a, 't, 's, 'v, M, R, E, C, W, F, O>
    RootModuleExecutionContextV1<'a, 't, 's, 'v, M, R, E, C, W, F, O>
where
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    W: ValueNodeReader + ValueNodeWriter + StatePageWriter,
    F: FnMut(&mut dyn AgentEventOriginalV1) -> Result<(), BodyMutationError>,
    O: OriginalObjectStreamReader,
{
    /// Fixed DATA bridge from an actual Frozen-bound Node input. The index
    /// selects the original committed action, never a caller-supplied payload,
    /// submitter, governance proof or recipe correlation value. The enclosing
    /// normal driver still owns whole-list validation and side-input dispatch.
    pub(in crate::world) fn enqueue_bound_runtime_input_data_v1(
        &mut self,
        parent: &ReservedValue<BoundNormalTickParentV1<'_>>,
        input: &oasis7_consensus::normal_tick_input::PaidBoundNormalInputDataV1<'_>,
        index: usize,
        pending: &mut ReservedValue<PendingActionCurrentV1>,
        work: &mut CommittedInputQueueWorkV1<'_>,
        limits:&oasis7_formal_protocol::controlled_authority::formal::bounded_cbor_data::CborDataLimitsV1,
        max_action_bytes: usize,
        common: &mut FormalBoundedContextV1<'_>,
        live_owner: &mut dyn FnMut() -> ExecutionFence,
    ) -> Result<ReservedValue<QueueWriteDataV1>, BoundRuntimeInputQueueErrorV1<O::Error>> {
        use BoundRuntimeInputQueueErrorV1 as Error;
        use oasis7_client_api::consensus_action_wire::{
            ConsensusActionAuthEnvelope as Auth, ConsensusActionPayloadBody as Body,
        };
        use oasis7_consensus::replicated_execution_input::{
            REPLICATED_EXECUTION_INPUT_ACTION_ID, REPLICATED_EXECUTION_INPUT_SUBMITTER,
        };
        parent
            .validate_context(common)
            .map_err(|e| Error::Resource(e.into()))?;
        input
            .validate_context(common)
            .map_err(|e| Error::Resource(e.into()))?;
        common
            .charge(1, 128)
            .map_err(|e| Error::Resource(e.into()))?;
        if input.checked().parent_record_id() != parent.refs().core.candidate_record_id()
            || self.ctx.transaction.parent_fence() != parent.fence()
        {
            return Err(Error::Invalid("committed input actual parent"));
        }
        let context = input.context();
        let source = context
            .committed_actions
            .get(index)
            .ok_or(Error::Invalid("committed input original index"))?;
        common
            .charge(1, source.submitter_player_id.len() as u64)
            .map_err(|e| Error::Resource(e.into()))?;
        if source.action_id == REPLICATED_EXECUTION_INPUT_ACTION_ID
            || source.submitter_player_id == REPLICATED_EXECUTION_INPUT_SUBMITTER
        {
            return Err(Error::Invalid(
                "reserved execution input requires fixed side consumer",
            ));
        }
        let payload = super::incremental_normal_input_decode::decode_runtime_input_payload_data_v1(
            &source.payload_cbor,
            limits,
            common,
        )
        .map_err(Error::Resource)?;
        payload
            .with_runtime_data(common, |envelope, common| {
                // Copies come only from the authenticated original context; their
                // reservation remains live through Original encoding and FIFO IO.
                let bytes = if envelope.gameplay_submission_origin.is_some() {
                    source
                        .submitter_player_id
                        .len()
                        .checked_add(source.payload_hash.len())
                        .and_then(|n| n.checked_add(context.action_root.len()))
                        .and_then(|n| n.checked_add(std::mem::size_of::<CommittedRecipeOrigin>()))
                        .ok_or(Error::Invalid("committed origin layout"))?
                } else {
                    0
                };
                common
                    .charge(3, bytes as u64)
                    .map_err(|e| Error::Resource(e.into()))?;
                let _copies = common
                    .memory()
                    .reserve(bytes, MemoryClass::Ordinary)
                    .map_err(|e| Error::Resource(e.into()))?;
                fn copy(value: &str) -> Result<String, &'static str> {
                    let mut result = String::new();
                    result
                        .try_reserve_exact(value.len())
                        .map_err(|_| "committed origin allocation")?;
                    if result.capacity() > value.len() {
                        return Err("committed origin capacity");
                    }
                    result.push_str(value);
                    Ok(result)
                }
                let metadata = match (envelope.gameplay_submission_origin, envelope.auth) {
                    (Some(submission), _) => {
                        CommittedActionMetadataV1::Recipe(CommittedRecipeOrigin {
                            submission,
                            consensus_action_id: source.action_id,
                            consensus_submitter_player_id: copy(&source.submitter_player_id)
                                .map_err(Error::Invalid)?,
                            action_payload_hash: copy(&source.payload_hash)
                                .map_err(Error::Invalid)?,
                            committed_height: context.height,
                            action_root: copy(&context.action_root).map_err(Error::Invalid)?,
                        })
                    }
                    (None, Some(Auth::GovernanceAction(proof))) => {
                        CommittedActionMetadataV1::Governance(proof)
                    }
                    _ => CommittedActionMetadataV1::Ordinary,
                };
                let Body::RuntimeAction { action } = envelope.body else {
                    return Err(Error::Invalid("private Runtime input classification"));
                };
                self.enqueue_committed_action_data_v1(
                    parent,
                    pending,
                    action,
                    metadata,
                    work,
                    max_action_bytes,
                    common,
                    live_owner,
                )
                .map_err(Error::Queue)
            })
            .map_err(Error::Payload)?
    }
}

/// Exact queue-write DATA from this one fixed input application. It is not a
/// complete tick and cannot admit a side input or a durable commit.
pub(in crate::world) struct BoundRuntimeQueueDataV1 {
    writes: Vec<ReservedValue<QueueWriteDataV1>>,
    bootstrap_index: Option<usize>,
    material: [u8; 32],
    identity: oasis7_formal_protocol::incremental_execution::PreparedGuard,
    core: oasis7_formal_protocol::controlled_authority::formal::bounded_context::WorkIdentityV1,
    _fee: oasis7_proto::memory_budget::MemoryReservation,
}
impl BoundRuntimeQueueDataV1 {
    pub(in crate::world) fn validate<M: MerkleNodeReader>(
        &self,
        tx: &oasis7_formal_protocol::incremental_execution::ExecutionTransaction<'_, M>,
        input: &oasis7_consensus::normal_tick_input::PaidBoundNormalInputDataV1<'_>,
        common: &mut FormalBoundedContextV1<'_>,
    ) -> Result<(), BodyMutationError> {
        input
            .validate_context(common)
            .map_err(|e| BodyMutationError::Original(Box::new(e)))?;
        tx.validate_exact_transaction_identity(&self.identity)?;
        common
            .consume_steps(1)
            .map_err(|e| BodyMutationError::Original(Box::new(e)))?;
        if self.core != common.work().identity()
            || self.material != input.checked().material_id()
            || !self._fee.uses_budget(common.memory())
            || !tx.uses_budget(common.memory())
            || self
                .writes
                .len()
                .checked_add(usize::from(self.bootstrap_index.is_some()))
                != Some(input.context().committed_actions.len())
            || self
                .bootstrap_index
                .is_some_and(|i| i >= input.context().committed_actions.len())
        {
            return Err(BodyMutationError::Invalid(
                "normal actual queue input lineage",
            ));
        }
        for write in &self.writes {
            common
                .consume_steps(1)
                .map_err(|e| BodyMutationError::Original(Box::new(e)))?;
            if !write.uses_budget(common.memory()) {
                return Err(BodyMutationError::Invalid("normal actual queue input pool"));
            }
        }
        Ok(())
    }
    pub(in crate::world) fn writes(&self) -> &[ReservedValue<QueueWriteDataV1>] {
        &self.writes
    }
    pub(in crate::world) fn material(&self) -> [u8; 32] {
        self.material
    }
}
impl<'a, 't, 's, 'v, M, R, E, C, W, F, O>
    RootModuleExecutionContextV1<'a, 't, 's, 'v, M, R, E, C, W, F, O>
where
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    W: ValueNodeReader + ValueNodeWriter + StatePageWriter,
    F: FnMut(&mut dyn AgentEventOriginalV1) -> Result<(), BodyMutationError>,
    O: OriginalObjectStreamReader,
{
    /// Applies the original ordered Runtime inputs to one ongoing FIFO. Every
    /// failure aborts the same transaction, including a later deferred packet;
    /// no successful prefix can escape as a fully applied input block.
    pub(in crate::world) fn enqueue_bound_runtime_block_data_v1(
        &mut self,
        parent: &ReservedValue<BoundNormalTickParentV1<'_>>,
        input: &oasis7_consensus::normal_tick_input::PaidBoundNormalInputDataV1<'_>,
        bootstrap: Option<
            &crate::world::normal_bootstrap_current::CommittedCurrentBootstrapBatchV1<'_, '_>,
        >,
        pending: &mut ReservedValue<PendingActionCurrentV1>,
        work: &mut CommittedInputQueueWorkV1<'_>,
        limits:&oasis7_formal_protocol::controlled_authority::formal::bounded_cbor_data::CborDataLimitsV1,
        max_action_bytes: usize,
        common: &mut FormalBoundedContextV1<'_>,
        live_owner: &mut dyn FnMut() -> ExecutionFence,
    ) -> Result<BoundRuntimeQueueDataV1, BoundRuntimeInputQueueErrorV1<O::Error>> {
        use BoundRuntimeInputQueueErrorV1 as Error;
        let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            // Validate the actual source even for an empty block; the loop's
            // per-action checks cannot stand in for the zero-action case.
            parent
                .validate_context(common)
                .map_err(|e| Error::Resource(e.into()))?;
            input
                .validate_context(common)
                .map_err(|e| Error::Resource(e.into()))?;
            common
                .charge(1, input.context().world_id.len() as u64 + 128)
                .map_err(|e| Error::Resource(e.into()))?;
            if input.checked().parent_record_id() != parent.refs().core.candidate_record_id()
                || self.ctx.transaction.parent_fence() != parent.fence()
                || input.context().world_id != parent.refs().roots.world_id
            {
                return Err(Error::Invalid("committed block actual source"));
            }
            super::incremental_normal_input_decode::validate_bound_reserved_input_data_v1(
                input, limits, common,
            )
            .map_err(Error::Resource)?
            .map_err(Error::Reserved)?;
            let count = input.context().committed_actions.len();
            let bytes = count
                .checked_mul(std::mem::size_of::<ReservedValue<QueueWriteDataV1>>())
                .and_then(|n| n.checked_add(std::mem::size_of::<BoundRuntimeQueueDataV1>()))
                .ok_or(Error::Invalid("committed input queue report layout"))?;
            common
                .charge(1, bytes as u64)
                .map_err(|e| Error::Resource(e.into()))?;
            let fee = common
                .memory()
                .reserve(bytes, MemoryClass::Ordinary)
                .map_err(|e| Error::Resource(e.into()))?;
            let mut writes = Vec::new();
            writes
                .try_reserve_exact(count)
                .map_err(|_| Error::Invalid("committed input queue report allocation"))?;
            if writes.capacity() > count {
                return Err(Error::Invalid("committed input queue report capacity"));
            }
            let mut bootstrap_index = None;
            for index in 0..count {
                if let Some(batch) = bootstrap {
                    if batch
                        .validate_enqueued_source_v1(
                            input,
                            index,
                            self.ctx.transaction,
                            common,
                            &live_owner(),
                        )
                        .map_err(|e| {
                            Error::Queue(CommittedInputQueueMutationErrorV1::Runtime(e.into()))
                        })?
                    {
                        if bootstrap_index.replace(index).is_some() {
                            return Err(Error::Invalid("duplicate completed Bootstrap input"));
                        }
                        continue;
                    }
                }
                let write = self.enqueue_bound_runtime_input_data_v1(
                    parent,
                    input,
                    index,
                    pending,
                    work,
                    limits,
                    max_action_bytes,
                    common,
                    live_owner,
                )?;
                writes.push(write);
            }
            common
                .consume_steps(0)
                .map_err(|e| Error::Resource(e.into()))?;
            Ok(BoundRuntimeQueueDataV1 {
                writes,
                bootstrap_index,
                material: input.checked().material_id(),
                identity: self.ctx.transaction.guard(),
                core: common.work().identity(),
                _fee: fee,
            })
        }));
        match outcome {
            Ok(Ok(data)) => Ok(data),
            Ok(Err(error)) => {
                let _: Result<(), ExecutionError> = self
                    .ctx
                    .transaction
                    .contribute(|_| Err(ExecutionError::Aborted));
                Err(error)
            }
            Err(payload) => {
                let _: Result<(), ExecutionError> = self
                    .ctx
                    .transaction
                    .contribute(|_| Err(ExecutionError::Aborted));
                std::panic::resume_unwind(payload)
            }
        }
    }
}
