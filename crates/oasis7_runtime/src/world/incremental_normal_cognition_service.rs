//! Fixed post-expiry scheduler service. The service keeps one real Journal
//! frontier across expiry, selection, evaluations and durable Current writes.
//! It does not complete a tick or promote any accepted source.
use crate::world::{
    incremental_body::BodyMutationError,
    incremental_body_event::AgentEventOriginalV1,
    incremental_module_invocation::*,
    incremental_normal_cognition_expiry::{
        CurrentCognitionExpiryOutcomeV1, CurrentCognitionExpiryWorkV1,
    },
    incremental_normal_cognition_service_evidence::{
        AcceptedCognitionWakeEvidenceV2, CurrentEvidenceErrorV2,
    },
    incremental_normal_sources::BoundNormalTickParentV1,
    incremental_normal_tick_finalize::consensus::HistoryWrittenIwsCommitmentV1,
};
use oasis7_consensus::normal_tick_input::PaidBoundNormalInputDataV1;
use oasis7_formal_protocol::{
    controlled_authority::formal::bounded_context::{FormalBoundedContextV1, WorkIdentityV1},
    incremental_execution::{ExecutionError, ExecutionFence, ExecutionTransaction, PreparedGuard},
    incremental_material::OriginalObjectStreamReader,
    incremental_state::{MerkleNodeReader, StatePageWriter},
    incremental_value::{ValueNodeReader, ValueNodeWriter, ValueTreeError},
};
use oasis7_proto::memory_budget::{MemoryClass, ReservedValue};
use oasis7_world_state::{
    incremental_agent_authority_records::AgentAuthorityWorkBudgetV1,
    incremental_cognition_current_complete_v3::CompleteCognitionCurrentQualificationV3,
    incremental_cognition_history::PendingCurrentCognitionJournalV1,
    incremental_cognition_proof_ledger::{
        AcceptedCognitionProofLedgerV1, CurrentCognitionProofLedgerV1,
    },
    incremental_field_read::WorldFieldReadContextV1,
};
fn bad() -> ValueTreeError {
    ValueTreeError::Invalid("Current Cognition scheduler service source")
}
fn formal(e: impl std::error::Error + Send + Sync + 'static) -> BodyMutationError {
    BodyMutationError::Original(Box::new(e))
}
/// Mandatory genuine sources; these borrows are DATA until the fixed driver
/// checks their actual parent/Core/pool and Current owners. No source is optional.
pub(in crate::world) struct CurrentCognitionServiceSourcesV2<'a, 'p> {
    pub parent: &'p ReservedValue<BoundNormalTickParentV1<'p>>,
    pub written: &'a ReservedValue<HistoryWrittenIwsCommitmentV1>,
    pub input: &'a PaidBoundNormalInputDataV1<'a>,
    pub current: &'a ReservedValue<CompleteCognitionCurrentQualificationV3>,
    pub ledger: &'a CurrentCognitionProofLedgerV1,
    pub accepted_ledger: &'a ReservedValue<AcceptedCognitionProofLedgerV1>,
    pub evidence: &'a ReservedValue<AcceptedCognitionWakeEvidenceV2>,
}
/// Every lane is borrowed from its original continuous caller. The service
/// never constructs/refills an independent allowance after expiry.
pub(in crate::world) struct CurrentCognitionServiceWorkV2<'a> {
    pub expiry: &'a mut CurrentCognitionExpiryWorkV1,
    pub authority: &'a mut AgentAuthorityWorkBudgetV1,
    pub authority_cells: &'a mut u64,
    pub pending_entries: &'a mut usize,
    pub path: &'a mut oasis7_world_state::incremental_json_records::JsonScalarPathReadWorkV1,
    pub max_schema_depth: usize,
}
/// Opaque result of the fixed producer. Selected wakes remain data, rather
/// than RawModuleCall/Sandbox permissions; provider handoff requires its real
/// typed admission/lease and the Host bound worker independently.
pub(in crate::world) struct CurrentCognitionServiceOutcomeV2<'p> {
    pending: Option<Box<PendingCurrentCognitionJournalV1<'p>>>,
    state: Box<CurrentCognitionServiceStateV2>,
    _fee: ReservedValue<()>,
}
struct CurrentCognitionServiceStateV2 {
    selected: selection::PaidSelectedCognitionWakesV2,
    written: Option<journal::WrittenCurrentCognitionServiceV2>,
    guard: PreparedGuard,
    core: WorkIdentityV1,
}
impl<'p> CurrentCognitionServiceOutcomeV2<'p> {
    fn admission_bytes() -> usize {
        // Reserve the pending allocation even on the empty path: final capture
        // may install its genuine frontier while this same fee is still held.
        std::mem::size_of::<Self>()
            + std::mem::size_of::<PendingCurrentCognitionJournalV1<'p>>()
            + std::mem::size_of::<CurrentCognitionServiceStateV2>()
    }

    pub(in crate::world) fn pending_for_final_source(
        &mut self,
    ) -> &mut Option<Box<PendingCurrentCognitionJournalV1<'p>>> {
        &mut self.pending
    }

    pub(in crate::world) fn into_detached_after_capture(
        self,
    ) -> Result<CurrentCognitionServiceOutcomeV2<'static>, Self> {
        if self.pending.is_some() {
            return Err(self);
        }
        Ok(CurrentCognitionServiceOutcomeV2 {
            pending: None,
            state: self.state,
            _fee: self._fee,
        })
    }

    pub(in crate::world) fn selected(&self) -> &[crate::cognition_scheduler::SchedulerWakeV1] {
        self.state.selected.wakes()
    }
    pub(in crate::world) fn into_pending(self) -> Option<PendingCurrentCognitionJournalV1<'p>> {
        // The closure frees the pending Box before releasing its storage fee;
        // the original Journal owner keeps its own reservations on return.
        let pending = self.pending.map(|pending| *pending);
        drop(self.state);
        drop(self._fee);
        pending
    }
    pub(in crate::world) fn validate_evidence<M: MerkleNodeReader>(
        &self,
        tx: &ExecutionTransaction<'_, M>,
        common: &mut FormalBoundedContextV1<'_>,
        live: &ExecutionFence,
    ) -> Result<(), BodyMutationError> {
        tx.validate_transaction_identity(&self.state.guard)?;
        tx.validate_guard(&tx.guard(), live)?;
        if self.state.core != common.work().identity() || !self._fee.uses_budget(common.memory()) {
            return Err(bad().into());
        }
        if let Some(pending) = &self.pending {
            pending.validate_evidence(tx, common.memory(), live)?;
        }
        if let Some(written) = &self.state.written {
            written.validate_evidence(tx, common, live)?;
        }
        Ok(())
    }
}
#[path = "incremental_normal_cognition_service_commit.rs"]
mod commit;
#[path = "incremental_normal_cognition_service_inputs.rs"]
pub(in crate::world) mod inputs;
#[path = "incremental_normal_cognition_service_journal.rs"]
mod journal;
#[path = "incremental_normal_cognition_service_selection.rs"]
mod selection;
#[path = "incremental_normal_cognition_service_state.rs"]
mod state;
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
    #[allow(clippy::too_many_arguments)]
    pub(in crate::world) fn execute_current_cognition_scheduler_v2<'p, P, PE, PC>(
        &mut self,
        sources: &CurrentCognitionServiceSourcesV2<'_, 'p>,
        expiry: CurrentCognitionExpiryOutcomeV1<'p>,
        work: &mut CurrentCognitionServiceWorkV2<'_>,
        parent_read: &mut WorldFieldReadContextV1<'_, P, R, PE, PC>,
        common: &mut FormalBoundedContextV1<'_>,
        live_owner: &mut dyn FnMut() -> ExecutionFence,
    ) -> Result<CurrentCognitionServiceOutcomeV2<'p>, RootModuleExecutionErrorV1<O::Error>>
    where
        P: MerkleNodeReader,
        PE: FnMut() -> Option<ValueTreeError>,
        PC: FnMut() -> bool,
    {
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            sources
                .parent
                .validate_context(common)
                .map_err(|e| RootModuleExecutionErrorV1::Body(formal(e)))?;
            sources
                .input
                .validate_context(common)
                .map_err(|e| RootModuleExecutionErrorV1::Body(formal(e)))?;
            sources
                .written
                .validate_evidence(self.ctx.transaction, common, &live_owner())
                .map_err(RootModuleExecutionErrorV1::Body)?;
            expiry
                .validate_evidence(self.ctx.transaction, common, &live_owner())
                .map_err(RootModuleExecutionErrorV1::Body)?;
            let fee = common
                .memory()
                .reserve(
                    CurrentCognitionServiceOutcomeV2::admission_bytes(),
                    MemoryClass::Ordinary,
                )
                .map_err(ValueTreeError::Memory)?
                .retain(());
            let mut pending = expiry.into_pending();
            // Preserve the original cheap ready gate before any hydration/validation.
            // This gate reads the actual Current Scheduler record, never a caller flag.
            let current =
                self.read_current_cognition_service_state_v2(sources, work, common, live_owner)?;
            if !current.has_active_or_pending() {
                let selected = selection::PaidSelectedCognitionWakesV2::empty(common)
                    .map_err(RootModuleExecutionErrorV1::Body)?;
                return Ok(CurrentCognitionServiceOutcomeV2 {
                    pending: pending.map(Box::new),
                    state: Box::new(CurrentCognitionServiceStateV2 {
                        selected,
                        written: None,
                        guard: self.ctx.transaction.guard(),
                        core: common.work().identity(),
                    }),
                    _fee: fee,
                });
            }
            if pending.is_none() {
                pending = Some(self.begin_current_cognition_service_journal_v2(
                    sources, work, common, live_owner,
                )?);
            }
            // Full persisted wake binding, continuation/context and actual-state/evidence
            // reads precede the new cut. ALL-intent max is a genuine Current aggregate.
            let inputs = self.prepare_current_cognition_service_inputs_v2(
                sources,
                &current,
                pending.as_ref().ok_or_else(bad)?,
                work,
                parent_read,
                common,
                live_owner,
            )?;
            let cut = self.prepare_iws_current_commitment_v1(
                sources.parent,
                sources.input,
                common,
                live_owner,
            )?;
            cut.validate(
                self.ctx.transaction,
                sources.parent,
                sources.input,
                common,
                &live_owner(),
            )
            .map_err(RootModuleExecutionErrorV1::Body)?;
            // Fixed old prune/select policy and quotas consume only those private actual
            // inputs. The V2 evaluation stores an explicit sha256 Current scheme.
            let plan =
                selection::prepare_selection_v2(current, inputs, cut, self.now, work, common)
                    .map_err(selection::root_error)?;
            plan.validate_before_write(self.ctx.transaction, sources, common, &live_owner())
                .map_err(RootModuleExecutionErrorV1::Body)?;
            // Evaluations retain all previous in-flight entries. Scheduler writes use
            // the real selected before-record owners and original Current batch CAS.
            let prepared = self.commit_current_cognition_service_state_v2(
                sources, &plan, work, common, live_owner,
            )?;
            let frontier = pending.as_mut().ok_or_else(bad)?;
            // Original order: stale deactivations, then each selected/Woken pair, or the
            // one empty-selection event. Every append uses the same Pending owner.
            let written = self.append_current_cognition_service_journal_v2(
                sources,
                &plan,
                prepared,
                frontier,
                work,
                parent_read,
                common,
                live_owner,
            )?;
            let selected = plan.into_selected();
            Ok(CurrentCognitionServiceOutcomeV2 {
                pending: pending.map(Box::new),
                state: Box::new(CurrentCognitionServiceStateV2 {
                    selected,
                    written: Some(written),
                    guard: self.ctx.transaction.guard(),
                    core: common.work().identity(),
                }),
                _fee: fee,
            })
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
}

#[path = "incremental_normal_cognition_service_admit.rs"]
pub(super) mod admit;

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
    /// Exact physical-storage-absence no-op. No provider, scheduler hydration,
    /// side-ledger or synthetic Journal event is introduced on this path.
    pub(in crate::world) fn execute_disabled_current_cognition_scheduler_v2<'p>(
        &mut self,
        parent: &'p ReservedValue<BoundNormalTickParentV1<'p>>,
        input: &PaidBoundNormalInputDataV1<'_>,
        current: &ReservedValue<CompleteCognitionCurrentQualificationV3>,
        source: &ReservedValue<
            crate::world::incremental_normal_cognition_expiry::PaidDisabledNormalCognitionSourceV1,
        >,
        written: &ReservedValue<HistoryWrittenIwsCommitmentV1>,
        expiry: CurrentCognitionExpiryOutcomeV1<'p>,
        work: &mut CurrentCognitionExpiryWorkV1,
        common: &mut FormalBoundedContextV1<'_>,
        live_owner: &mut dyn FnMut() -> ExecutionFence,
    ) -> Result<CurrentCognitionServiceOutcomeV2<'p>, RootModuleExecutionErrorV1<O::Error>> {
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            input
                .validate_context(common)
                .map_err(|e| RootModuleExecutionErrorV1::Body(formal(e)))?;
            written
                .validate_evidence(self.ctx.transaction, common, &live_owner())
                .map_err(RootModuleExecutionErrorV1::Body)?;
            expiry
                .validate_evidence(self.ctx.transaction, common, &live_owner())
                .map_err(RootModuleExecutionErrorV1::Body)?;
            self.validate_disabled_normal_cognition_current_v1(
                parent,
                current,
                source,
                work,
                common,
                &live_owner(),
            )?;
            common
                .charge(
                    1,
                    CurrentCognitionServiceOutcomeV2::admission_bytes() as u64,
                )
                .map_err(|e| RootModuleExecutionErrorV1::Body(formal(e)))?;
            let fee = common
                .memory()
                .reserve(
                    CurrentCognitionServiceOutcomeV2::admission_bytes(),
                    MemoryClass::Ordinary,
                )
                .map_err(ValueTreeError::Memory)?
                .retain(());
            let pending = expiry.into_pending();
            let selected = selection::PaidSelectedCognitionWakesV2::empty(common)
                .map_err(RootModuleExecutionErrorV1::Body)?;
            let outcome = CurrentCognitionServiceOutcomeV2 {
                pending: pending.map(Box::new),
                state: Box::new(CurrentCognitionServiceStateV2 {
                    selected,
                    written: None,
                    guard: self.ctx.transaction.guard(),
                    core: common.work().identity(),
                }),
                _fee: fee,
            };
            outcome
                .validate_evidence(self.ctx.transaction, common, &live_owner())
                .map_err(RootModuleExecutionErrorV1::Body)?;
            Ok(outcome)
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
}
