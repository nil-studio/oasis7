//! One fixed Runtime-only Host consumer: real contexts, bound sources,
//! committed originals/FIFO, full normal tick and terminal directory capture.
//! Bootstrap/service side input remains rejected by its actual input consumer.
use super::{
    incremental_body::BodyMutationError,
    incremental_body_event::AgentEventOriginalV1,
    incremental_module_execution_worker::RootModuleWorkerAllocatorV1,
    incremental_module_invocation::*,
    incremental_normal_host_context::{
        NormalHostContextErrorV1, NormalHostContextSourcesV1, with_begun_normal_root_v1,
    },
    incremental_normal_host_finish::FinishedNormalTickDirectoriesV1,
    incremental_normal_host_work::{NormalHostTickErrorV1, NormalHostWorkV1},
    incremental_normal_sources::BoundNormalTickParentV1,
    incremental_normal_tail_sources::{
        NormalTailCognitionInputsV1, NormalTailDirectoryInputsV1, bind_normal_tick_tail_sources_v1,
    },
    incremental_normal_world_rule_observation_v2::ObservationJsonOriginalSinkV2,
};
use oasis7_consensus::normal_tick_input::PaidBoundNormalInputDataV1;
use oasis7_formal_protocol::{
    controlled_authority::formal::{
        bounded_cbor_data::CborDataLimitsV1, bounded_context::FormalBoundedContextV1,
    },
    incremental_execution::{ExecutionError, ExecutionFence, ExecutionTransaction},
    incremental_material::OriginalObjectStreamReader,
    incremental_schedule::SchedulePageWriter,
    incremental_state::{MerkleNodeReader, StatePageWriter},
    incremental_value::{ValueNodeReader, ValueNodeWriter, ValueTreeError},
};
use oasis7_proto::memory_budget::ReservedValue;
use oasis7_wasm_executor::bounded_worker::BoundedWorkerSandboxV1;
use oasis7_world_state::incremental_cognition_history::PendingCurrentCognitionJournalV1;
#[path = "incremental_normal_host_driver_tail.rs"]
mod tail;

#[derive(Debug)]
pub(in crate::world) enum NormalHostRuntimeErrorV1<OE> {
    Root(RootModuleExecutionErrorV1<OE>),
    Tail(BodyMutationError),
    Tick(NormalHostTickErrorV1<OE>),
}
/// No caller callback, qualification, roots, completion flag or replacement
/// work can select a different dispatch path. All qualifications come from the
/// same genuine BoundParent; all work returns through the original Host scope.
#[allow(clippy::too_many_arguments)]
pub(in crate::world) fn execute_normal_host_runtime_v1<'s, 'p, M, R, E, C, W, F, O, S>(
    parent: &'p ReservedValue<BoundNormalTickParentV1<'p>>,
    input: &PaidBoundNormalInputDataV1<'_>,
    transaction: &mut ExecutionTransaction<'s, M>,
    state: &M,
    values: &R,
    member_sink: &mut W,
    fee_sink: &mut W,
    event_sink: &mut W,
    originals: &mut O,
    persist_original: F,
    worker: &BoundedWorkerSandboxV1<'_, RootModuleWorkerAllocatorV1>,
    sources: &NormalHostContextSourcesV1<'p>,
    authorization: &mut super::incremental_authorization_root_current::CurrentAuthorizationProjectionV1<'p>,
    directories: NormalTailDirectoryInputsV1<'p>,
    cognition: NormalTailCognitionInputsV1<'p>,
    work: &mut NormalHostWorkV1,
    take_state_error: E,
    cancelled: C,
    input_limits: &CborDataLimitsV1,
    max_action_bytes: usize,
    max_grant_payload_bytes: u64,
    sink: &mut S,
    continued_cognition: Option<PendingCurrentCognitionJournalV1<'p>>,
    common: &mut FormalBoundedContextV1<'_>,
    live_owner: &mut dyn FnMut() -> ExecutionFence,
) -> Result<FinishedNormalTickDirectoriesV1<'p>, NormalHostRuntimeErrorV1<O::Error>>
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
    S: ObservationJsonOriginalSinkV2,
    S::Error: std::fmt::Debug + Send + Sync + 'static,
{
    let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        input.validate_context(common).map_err(|e| {
            NormalHostRuntimeErrorV1::Root(BodyMutationError::Original(Box::new(e)).into())
        })?;
        if input.checked().parent_record_id() != parent.refs().core.candidate_record_id() {
            return Err(NormalHostRuntimeErrorV1::Root(
                BodyMutationError::Invalid("normal fixed input actual parent").into(),
            ));
        }
        let outcome = with_begun_normal_root_v1(
            parent,
            transaction,
            state,
            values,
            member_sink,
            fee_sink,
            event_sink,
            originals,
            persist_original,
            worker,
            &sources.root_read_sources_v1(),
            work,
            take_state_error,
            cancelled,
            common,
            live_owner,
            |root,
             pending,
             revocation,
             delivery,
             current,
             parent_read,
             admission,
             factory,
             recipe,
             industry,
             materials,
             driver,
             common,
             live| {
                // The original step advances the actual World clock once
                // before its prologue. Node height is an independent identity.
                root.now = root.now.checked_add(1).ok_or_else(|| {
                    NormalHostRuntimeErrorV1::Root(
                        BodyMutationError::Invalid("normal World clock overflow").into(),
                    )
                })?;
                let bound = bind_normal_tick_tail_sources_v1(
                    parent,
                    directories,
                    sources.retention,
                    cognition,
                    root.ctx.transaction,
                    common,
                    &live(),
                )
                .map_err(NormalHostRuntimeErrorV1::Tail)?;
                root.execute_normal_host_with_bound_tail_v1(
                    parent,
                    input,
                    sources,
                    authorization,
                    &bound,
                    pending,
                    revocation,
                    delivery,
                    current,
                    parent_read,
                    admission,
                    factory,
                    recipe,
                    industry,
                    materials,
                    driver,
                    input_limits,
                    max_action_bytes,
                    max_grant_payload_bytes,
                    sink,
                    continued_cognition,
                    common,
                    live,
                )
            },
        );
        match outcome {
            Ok(value) => Ok(value),
            Err(NormalHostContextErrorV1::Root(error)) => {
                Err(NormalHostRuntimeErrorV1::Root(error))
            }
            Err(NormalHostContextErrorV1::Operation(error)) => Err(error),
        }
    }));
    match outcome {
        Ok(result) => {
            if result.is_err() {
                let _: Result<(), ExecutionError> =
                    transaction.contribute(|_| Err(ExecutionError::Aborted));
            }
            result
        }
        Err(payload) => {
            let _: Result<(), ExecutionError> =
                transaction.contribute(|_| Err(ExecutionError::Aborted));
            std::panic::resume_unwind(payload)
        }
    }
}

#[derive(Debug)]
pub(in crate::world) enum NormalHostPreparationErrorV1<OE, PE> {
    Execution(NormalHostRuntimeErrorV1<OE>),
    Preparation(super::incremental_commit::NormalCommitPreparationErrorV1<OE, PE>),
}
/// One fixed consuming entry from the original live transaction through the
/// real Runtime tick, terminal directories, real seal and History candidate.
/// This still prepares content: endpoint publication and accepted cache moves
/// require their original separate retained leases and formal commit owners.
#[allow(clippy::too_many_arguments)]
pub(in crate::world) fn prepare_normal_host_runtime_v1<'s, 'p, M, R, E, C, W, F, O, S>(
    parent: &'p ReservedValue<BoundNormalTickParentV1<'p>>,
    input: &PaidBoundNormalInputDataV1<'_>,
    mut transaction: ExecutionTransaction<'s, M>,
    state: &M,
    values: &R,
    member_sink: &mut W,
    fee_sink: &mut W,
    event_sink: &mut W,
    originals: &mut O,
    persist_original: F,
    worker: &BoundedWorkerSandboxV1<'_, RootModuleWorkerAllocatorV1>,
    sources: &NormalHostContextSourcesV1<'p>,
    authorization: &mut super::incremental_authorization_root_current::CurrentAuthorizationProjectionV1<'p>,
    directories: NormalTailDirectoryInputsV1<'p>,
    cognition: NormalTailCognitionInputsV1<'p>,
    work: &mut NormalHostWorkV1,
    take_state_error: E,
    cancelled: C,
    input_limits: &CborDataLimitsV1,
    max_action_bytes: usize,
    max_grant_payload_bytes: u64,
    sink: &mut S,
    continued_cognition: Option<PendingCurrentCognitionJournalV1<'p>>,
    policy: super::incremental_commit::NormalCommitHeaderLimitsV1,
    common: &mut FormalBoundedContextV1<'_>,
    live_owner: &mut dyn FnMut() -> ExecutionFence,
) -> Result<
    super::incremental_commit::PreparedNormalCommitV1<'p>,
    NormalHostPreparationErrorV1<
        O::Error,
        super::incremental_normal_final_sources::FinalNormalOriginalErrorV1<O::Error>,
    >,
>
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
    S: ObservationJsonOriginalSinkV2,
    S::Error: std::fmt::Debug + Send + Sync + 'static,
{
    let finished = execute_normal_host_runtime_v1(
        parent,
        input,
        &mut transaction,
        state,
        values,
        member_sink,
        fee_sink,
        event_sink,
        originals,
        persist_original,
        worker,
        sources,
        authorization,
        directories,
        cognition,
        work,
        take_state_error,
        cancelled,
        input_limits,
        max_action_bytes,
        max_grant_payload_bytes,
        sink,
        continued_cognition,
        common,
        live_owner,
    )
    .map_err(NormalHostPreparationErrorV1::Execution)?;
    // Both closures close over the same real mutable loans. Only the actual
    // full-router profile is selected; no caller callback produces Results.
    let captured = std::sync::Arc::clone(finished.final_sources());
    let (final_work, directory_work) = work.driver.final_preparation_loans();
    let final_io = std::cell::RefCell::new((originals, final_work));
    let pair = |after: &oasis7_formal_protocol::incremental_execution::RootSetV1,
                common: &mut FormalBoundedContextV1<'_>| {
        let mut io = final_io.borrow_mut();
        let (originals, work) = &mut *io;
        captured.produce_pair(after, *originals, *work, common)
    };
    let triple = |after: &oasis7_formal_protocol::incremental_execution::RootSetV1,
                  common: &mut FormalBoundedContextV1<'_>| {
        let mut io = final_io.borrow_mut();
        let (originals, work) = &mut *io;
        captured.produce_three(after, *originals, *work, common)
    };
    let result_producer = if captured.has_ledger() {
        super::incremental_commit::NormalFinalResultProducerV1::ProofLedger(triple)
    } else {
        super::incremental_commit::NormalFinalResultProducerV1::Pair(pair)
    };
    // Both loans point to the same remaining owners used by the completed tick.
    // The zero/remaining handoff never replaces them with a fresh allowance.
    super::incremental_commit::prepare_finished_normal_commit_v1(
        transaction,
        finished,
        parent,
        input,
        state,
        policy,
        &mut work.materials.scope,
        directory_work,
        result_producer,
        common,
        live_owner,
    )
    .map_err(NormalHostPreparationErrorV1::Preparation)
}

#[path = "incremental_normal_host_driver_fixed.rs"]
mod fixed;
pub use fixed::{
    NormalHostCognitionParentSourcesV1, NormalHostFixedFailureV1, PreparedNormalHostRuntimeV1,
    prepare_normal_host_from_resident_v1,
};
