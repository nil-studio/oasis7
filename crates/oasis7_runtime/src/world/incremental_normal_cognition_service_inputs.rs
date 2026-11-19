//! Fixed actual source collection before the service Current cut.
use super::*;
use crate::world::{
    incremental_normal_cognition_expiry::{values, with_current_cognition_io_v1},
    incremental_normal_cognition_owned as owned,
    incremental_normal_cognition_registered_turn::CurrentRegisteredCognitionTurnV2,
};
use oasis7_agent_api::iws_scheduler_v2::{
    IwsContinuationProposalV2, IwsJournalReferenceV2, IwsWakeConditionV2,
};
use oasis7_world_state::{
    incremental_agent_authority_records::{
        CurrentIntentReorgAggregateV2, open_current_intent_reorg_aggregate_v2,
    },
    incremental_cognition_control_overlay::read_current_cognition_service_control_v2,
    incremental_cognition_fields::CognitionControlFieldV1,
    incremental_cognition_proof_ledger::{CognitionProofLedgerFieldV1, ProofLedgerRecoveredV1},
    incremental_cognition_read::CognitionControlReadV1,
    incremental_field_overlay::WorldFieldOverlayErrorV1,
};
use serde::{Deserialize, Serialize};
/// Durable V2 context is an explicit source reference, never an admission bool.
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(in crate::world) struct IwsContinuationContextV2 {
    pub(in crate::world) schema_version: String,
    pub(in crate::world) commitment_scheme: String,
    pub(in crate::world) proposal: IwsContinuationProposalV2,
    pub(in crate::world) continuation_id: String,
    pub(in crate::world) wake_id: String,
    pub(in crate::world) admission_commitment: String,
    pub(in crate::world) admission_frame:
        oasis7_world_state::incremental_iws_tick_consensus::IwsCurrentCommitmentFrameV1,
    pub(in crate::world) admission: IwsJournalReferenceV2,
    pub(in crate::world) context_core_digest: String,
}
impl crate::world::incremental_normal_cognition_registered_turn::IwsContinuationAdmissionContextDataV2 for IwsContinuationContextV2{
 fn admission_reference(&self)->&IwsJournalReferenceV2{&self.admission}
 fn admission_values(&self)->[&str;4]{[&self.continuation_id,&self.wake_id,&self.proposal.as_data().proposal_digest,&self.context_core_digest]}
}
impl IwsContinuationContextV2 {
    pub(in crate::world) fn context_core_input(&self) -> impl Serialize + '_ {
        super::admit::admission_core_v2(
            &self.proposal,
            &self.continuation_id,
            &self.wake_id,
            &self.admission_commitment,
            &self.admission_frame,
        )
    }
}

pub(super) struct PreparedIwsWakeConditionsV2{
 pub(super) conditions:Vec<IwsWakeConditionV2>,pub(super) predicates:predicates::PaidCurrentPredicateValuesV2,
 pub(super) events:Vec<String>,pub(super) receipts:Vec<String>,pub(super) registration:IwsJournalReferenceV2,
 pub(super) valid_until:Option<u64>,pub(super) _registration:ReservedValue<CurrentRegisteredCognitionTurnV2>,
 pub(super) _admitted:ReservedValue<crate::world::incremental_normal_cognition_registered_turn::CurrentAdmittedIwsContinuationV2>,
 _context:values::OwnedJson,_fee:ReservedValue<()>,
}
pub(super) struct PreparedWakeInputV2 {
    wake_id: String,
    continuation_id: String,
    keep: bool,
    pub(super) evaluation: Option<PreparedIwsWakeConditionsV2>,
    _fee: ReservedValue<()>,
}
impl PreparedWakeInputV2 {
    pub(super) fn wake_id(&self) -> &str {
        &self.wake_id
    }
    pub(super) fn continuation_id(&self) -> &str {
        &self.continuation_id
    }
    pub(super) fn keep(&self) -> bool {
        self.keep
    }
}
pub(super) struct CurrentServiceInputsV2 {
    pub(super) scheduler: owned::OwnedCognitionSchedulerV1,
    pub(super) continuations: values::OwnedContinuations,
    pub(super) hydrated: values::OwnedJson,
    pub(super) binding: ProofLedgerRecoveredV1<serde_json::Value>,
    pub(super) epoch: CurrentIntentReorgAggregateV2,
    pub(super) reorg_epoch: u64,
    pub(super) old_sequence: u64,
    pub(super) old_digest: ReservedValue<String>,
    rows: rows::PreparedServiceRowsV2,
    guard: PreparedGuard,
    core: WorkIdentityV1,
    _fee: ReservedValue<()>,
}
impl CurrentServiceInputsV2 {
    pub(super) fn split_policy_rows_v2(
        &mut self,
    ) -> (
        &mut crate::cognition_scheduler::CognitionScheduler,
        &[PreparedWakeInputV2],
    ) {
        (&mut self.scheduler.value, self.rows.rows())
    }
    pub(super) fn validate_evidence_identity<M: MerkleNodeReader>(
        &self,
        tx: &ExecutionTransaction<'_, M>,
        common: &mut FormalBoundedContextV1<'_>,
        live: &ExecutionFence,
    ) -> Result<(), BodyMutationError> {
        tx.validate_transaction_identity(&self.guard)?;
        tx.validate_guard(&tx.guard(), live)?;
        if self.core != common.work().identity() || !self._fee.uses_budget(common.memory()) {
            return Err(bad().into());
        }
        Ok(())
    }
    pub(super) fn rows(&self) -> &[PreparedWakeInputV2] {
        self.rows.rows()
    }
    pub(super) fn binding(&self) -> Option<&serde_json::Value> {
        Some(self.binding.value())
    }
}
fn overlay(e: WorldFieldOverlayErrorV1) -> BodyMutationError {
    match e {
        WorldFieldOverlayErrorV1::Execution(e) => e.into(),
        WorldFieldOverlayErrorV1::Value(e) => e.into(),
        WorldFieldOverlayErrorV1::Member(e) => formal(e),
    }
}
fn business<OE>(
    reason: &str,
    common: &mut FormalBoundedContextV1<'_>,
) -> RootModuleExecutionErrorV1<OE> {
    let fee = common.memory().reserve(
        std::mem::size_of::<crate::WorldError>() + reason.len(),
        MemoryClass::Ordinary,
    );
    match fee {
        Ok(fee) => RootModuleExecutionErrorV1::Business(fee.retain(
            crate::WorldError::DistributedValidationFailed {
                reason: reason.to_owned(),
            },
        )),
        Err(e) => ValueTreeError::Memory(e).into(),
    }
}
#[path = "incremental_normal_cognition_service_inputs_io.rs"]
pub(super) mod io;
#[path = "incremental_normal_cognition_service_predicates.rs"]
pub(super) mod predicates;
#[path = "incremental_normal_cognition_service_inputs_rows.rs"]
mod rows;
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
    pub(super) fn prepare_current_cognition_service_inputs_v2<P, PE, PC>(
        &mut self,
        sources: &CurrentCognitionServiceSourcesV2<'_, '_>,
        state: &state::CurrentServiceStateV2,
        pending: &PendingCurrentCognitionJournalV1<'_>,
        work: &mut CurrentCognitionServiceWorkV2<'_>,
        parent_read: &mut WorldFieldReadContextV1<'_, P, R, PE, PC>,
        common: &mut FormalBoundedContextV1<'_>,
        live_owner: &mut dyn FnMut() -> ExecutionFence,
    ) -> Result<CurrentServiceInputsV2, RootModuleExecutionErrorV1<O::Error>>
    where
        P: MerkleNodeReader,
        PE: FnMut() -> Option<ValueTreeError>,
        PC: FnMut() -> bool,
    {
        let roots = sources.parent.refs().roots;
        sources
            .ledger
            .validate_context(self.ctx.transaction, common, &live_owner())?;
        pending.validate_evidence(self.ctx.transaction, common.memory(), &live_owner())?;
        let raw = state.scheduler.as_ref().ok_or_else(bad)?;
        let empty = serde_json::Value::Array(Vec::new());
        let mut continuations = values::continuations(
            state
                .continuations
                .as_ref()
                .map_or(&empty, |owner| owner.value()),
            work.expiry.max_selected_heap,
            common,
        )
        .map_err(RootModuleExecutionErrorV1::Body)?;
        rows::normalize(&mut continuations.value, work.expiry, common)
            .map_err(RootModuleExecutionErrorV1::Body)?;
        // The original hydration pass only uses active continuations. The source
        // vector itself retains all terminal rows for exact binding/prune semantics.
        let mut hydrated = values::json(raw.value(), work.expiry.max_selected_heap, common)
            .map_err(RootModuleExecutionErrorV1::Body)?;
        rows::hydrate(&mut hydrated.value, &continuations.value, common)
            .map_err(RootModuleExecutionErrorV1::Body)?;
        let scheduler = owned::decode_scheduler_v1(&hydrated.value, work.max_schema_depth, common)
            .map_err(|e| match e {
                owned::SchedulerOwnedErrorV1::Body(e) => RootModuleExecutionErrorV1::Body(e),
                owned::SchedulerOwnedErrorV1::Scheduler(e) => business(e.code(), common),
            })?;
        let binding = {
            let ctx = &mut self.ctx;
            let mut read = WorldFieldReadContextV1 {
                root: roots.state_root(
                    oasis7_formal_protocol::incremental_state::StateNamespaceV1::Artifacts,
                ),
                state: parent_read.state,
                values: oasis7_formal_protocol::incremental_value::ValueReadContext {
                    source: ctx.values.source,
                    memory: ctx.values.memory,
                    class: ctx.values.class,
                    io: &mut *ctx.values.io,
                },
                key_work: &mut *ctx.key_work,
                semantic_bytes: ctx.semantic_bytes,
                record_bytes: ctx.record_bytes,
                take_state_error: &mut ctx.take_state_error,
                cancelled: &mut ctx.cancelled,
            };
            sources.ledger.read_field(
                ctx.transaction,
                CognitionProofLedgerFieldV1::RuntimeBinding,
                &mut read,
                &mut work.expiry.ledger_scan,
                &mut work.expiry.ledger,
                common,
                &live_owner(),
            )?
        }
        .ok_or_else(|| business::<O::Error>("runtime_binding_required", common))?;
        // Capture the actual Current controls, never the accepted parent head.
        let invalidation = with_current_cognition_io_v1(
            self.ctx,
            self.members,
            work.expiry,
            common,
            |ctx, _, _| {
                let guard = ctx.transaction.guard();
                match read_current_cognition_service_control_v2(
                    CognitionControlFieldV1::ReorgInvalidationEpoch,
                    sources.current,
                    roots,
                    ctx,
                    &mut std::io::sink(),
                    0,
                    &guard,
                    &live_owner(),
                )
                .map_err(overlay)?
                {
                    CognitionControlReadV1::Unsigned(value) => Ok(value),
                    CognitionControlReadV1::Absent => Ok(0),
                    _ => Err(bad().into()),
                }
            },
        )
        .map_err(RootModuleExecutionErrorV1::Body)?;
        let old_sequence = with_current_cognition_io_v1(
            self.ctx,
            self.members,
            work.expiry,
            common,
            |ctx, _, _| {
                let guard = ctx.transaction.guard();
                match read_current_cognition_service_control_v2(
                    CognitionControlFieldV1::JournalHeadSeq,
                    sources.current,
                    roots,
                    ctx,
                    &mut std::io::sink(),
                    0,
                    &guard,
                    &live_owner(),
                )
                .map_err(overlay)?
                {
                    CognitionControlReadV1::Unsigned(value) => Ok(value),
                    _ => Err(bad().into()),
                }
            },
        )
        .map_err(RootModuleExecutionErrorV1::Body)?;
        let mut text = [0u8; 71];
        let mut output = std::io::Cursor::new(text.as_mut_slice());
        let size = with_current_cognition_io_v1(
            self.ctx,
            self.members,
            work.expiry,
            common,
            |ctx, _, _| {
                let guard = ctx.transaction.guard();
                match read_current_cognition_service_control_v2(
                    CognitionControlFieldV1::JournalHeadDigest,
                    sources.current,
                    roots,
                    ctx,
                    &mut output,
                    71,
                    &guard,
                    &live_owner(),
                )
                .map_err(overlay)?
                {
                    CognitionControlReadV1::Text { bytes } => Ok(bytes),
                    _ => Err(bad().into()),
                }
            },
        )
        .map_err(RootModuleExecutionErrorV1::Body)?;
        drop(output);
        let size = usize::try_from(size).map_err(|_| bad())?;
        let value = std::str::from_utf8(text.get(..size).ok_or_else(bad)?).map_err(|_| bad())?;
        common
            .charge(1, size as u64)
            .map_err(|e| RootModuleExecutionErrorV1::Body(formal(e)))?;
        let fee = common
            .memory()
            .reserve(std::mem::size_of::<String>() + size, MemoryClass::Ordinary)
            .map_err(ValueTreeError::Memory)?;
        let old_digest = fee.retain(value.to_owned());
        if old_sequence != pending.sequence() {
            return Err(bad().into());
        }
        let prepared = self.prepare_current_cognition_service_rows_v2(
            sources,
            state,
            pending,
            &scheduler.value,
            &continuations.value,
            binding.value(),
            work,
            parent_read,
            common,
            live_owner,
        )?;
        // This is the ALL-ledger source, not only agents that happen to have wakes.
        let epoch = io::with_intent_aggregate_v2(self.ctx, work, common, |ctx, work| {
            let guard = ctx.transaction.guard();
            open_current_intent_reorg_aggregate_v2(
                sources
                    .parent
                    .refs()
                    .storage
                    .current()
                    .native()
                    .agent_authority(),
                roots,
                parent_read.state,
                ctx,
                &mut work.authority.scan,
                work.authority_cells,
                work.authority.max_pending_node_bytes as u64,
                work.pending_entries,
                &guard,
                &live_owner(),
            )
            .map_err(overlay)
        })
        .map_err(RootModuleExecutionErrorV1::Body)?;
        epoch.validate(self.ctx.transaction, common.memory(), &live_owner())?;
        let reorg_epoch = binding
            .value()
            .get("reorg_epoch")
            .and_then(serde_json::Value::as_u64)
            .unwrap_or(0)
            .max(invalidation)
            .max(epoch.max_reorg_epoch());
        common
            .charge(1, 24)
            .map_err(|e| RootModuleExecutionErrorV1::Body(formal(e)))?;
        let fee = common
            .memory()
            .reserve(
                std::mem::size_of::<CurrentServiceInputsV2>(),
                MemoryClass::Ordinary,
            )
            .map_err(ValueTreeError::Memory)?
            .retain(());
        Ok(CurrentServiceInputsV2 {
            scheduler,
            continuations,
            hydrated,
            binding,
            epoch,
            reorg_epoch,
            old_sequence,
            old_digest,
            rows: prepared,
            guard: self.ctx.transaction.guard(),
            core: common.work().identity(),
            _fee: fee,
        })
    }
}
