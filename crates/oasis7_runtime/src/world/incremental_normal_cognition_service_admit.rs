//! V2 continuation admission. This fixed producer writes DATA, not Node input
//! authority; its caller must retain the genuine Host/Node execution boundary.
use super::*;
use crate::cognition_scheduler::SchedulerWakeV1;
use crate::cognition_wake::{AgentContinuation, ContinuationStatusV1};
use crate::world::incremental_normal_cognition_expiry::{
    event, values, with_current_cognition_io_v1,
};
use crate::world::incremental_normal_cognition_owned as owned;
use crate::world::incremental_normal_cognition_registered_turn::CurrentRegisteredCognitionTurnV2;
use crate::world::incremental_normal_tick_finalize::commitment::PaidIwsCurrentCommitmentV1;
use oasis7_agent_api::iws_scheduler_v2::{
    IWS_CURRENT_COMMITMENT_SCHEME_V1, IwsContinuationProposalV2, IwsJournalReferenceV2,
    IwsWakeConditionV2, IwsWakeConditionValidatorV2,
};
use oasis7_world_state::cognition_wake::{
    IWS_CONTINUATION_SCHEMA_V2, IWS_CONTINUATION_STATUS_DOMAIN_V2, IwsAgentContinuationV2,
};
use oasis7_world_state::incremental_cognition_control_overlay::{
    CognitionControlValueV1, change_cognition_control_v1, check_cognition_control_overlay_v1,
};
use oasis7_world_state::incremental_cognition_fields::CognitionControlFieldV1;
use oasis7_world_state::incremental_cognition_json::CognitionJsonFieldV1;
use oasis7_world_state::incremental_cognition_proof_ledger::CognitionProofLedgerFieldV1;
use oasis7_world_state::incremental_field_overlay::WorldFieldOverlayErrorV1;
use oasis7_world_state::incremental_iws_tick_consensus::IwsCurrentCommitmentFrameV1;
use oasis7_world_state::incremental_normal_cognition_current::{
    CurrentCognitionJsonRecordV1, create_current_cognition_continuations_v2,
    create_current_cognition_service_map_v2, read_current_cognition_json_record_v1,
    replace_current_cognition_service_batch_v1,
};
use serde::{Serialize, ser::SerializeMap};

/// The one admission core encoding excludes the not-yet-produced Journal
/// locator. Both persistence and readback hash exactly this borrowed payload.
#[derive(Serialize)]
pub(in crate::world) struct IwsContinuationAdmissionCoreV2<'a> {
    schema_version: &'static str,
    commitment_scheme: &'a str,
    proposal: &'a IwsContinuationProposalV2,
    continuation_id: &'a str,
    wake_id: &'a str,
    admission_commitment: &'a str,
    admission_frame: &'a IwsCurrentCommitmentFrameV1,
}
pub(in crate::world) fn admission_core_v2<'a>(
    proposal: &'a IwsContinuationProposalV2,
    continuation_id: &'a str,
    wake_id: &'a str,
    admission_commitment: &'a str,
    admission_frame: &'a IwsCurrentCommitmentFrameV1,
) -> IwsContinuationAdmissionCoreV2<'a> {
    IwsContinuationAdmissionCoreV2 {
        schema_version: "iws-continuation-admission.v2",
        commitment_scheme: IWS_CURRENT_COMMITMENT_SCHEME_V1,
        proposal,
        continuation_id,
        wake_id,
        admission_commitment,
        admission_frame,
    }
}
struct ContextPayload<'a> {
    core: IwsContinuationAdmissionCoreV2<'a>,
    admission: &'a IwsJournalReferenceV2,
    digest: &'a str,
}
impl Serialize for ContextPayload<'_> {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        let mut m = s.serialize_map(Some(9))?;
        m.serialize_entry("schema_version", "iws-continuation-context.v2")?;
        m.serialize_entry("commitment_scheme", self.core.commitment_scheme)?;
        m.serialize_entry("proposal", self.core.proposal)?;
        m.serialize_entry("continuation_id", self.core.continuation_id)?;
        m.serialize_entry("wake_id", self.core.wake_id)?;
        m.serialize_entry("admission_commitment", self.core.admission_commitment)?;
        m.serialize_entry("admission_frame", self.core.admission_frame)?;
        m.serialize_entry("admission", self.admission)?;
        m.serialize_entry("context_core_digest", self.digest)?;
        m.end()
    }
}
struct MapInsertion<'a> {
    before: Option<&'a serde_json::Value>,
    key: &'a str,
    value: &'a serde_json::Value,
}
impl Serialize for MapInsertion<'_> {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        let before = self.before.and_then(serde_json::Value::as_object);
        let mut m = s.serialize_map(Some(before.map_or(0, |m| m.len()) + 1))?;
        if let Some(before) = before {
            for (k, v) in before {
                if k != self.key {
                    m.serialize_entry(k, v)?;
                }
            }
        }
        m.serialize_entry(self.key, self.value)?;
        m.end()
    }
}
/// Admission retains the actual registration, prewrite cut, original reference
/// and selected records until the caller consumes the successful outcome.
pub(in crate::world) struct CurrentCognitionAdmissionOutcomeV2<'p> {
    pending: PendingCurrentCognitionJournalV1<'p>,
    continuation: values::OwnedContinuations,
    context: values::OwnedJson,
    registration: ReservedValue<CurrentRegisteredCognitionTurnV2>,
    cut: ReservedValue<PaidIwsCurrentCommitmentV1>,
    reference: IwsJournalReferenceV2,
    guard: PreparedGuard,
    identity: WorkIdentityV1,
    now: u64,
    _fee: ReservedValue<()>,
    _proposal_graph: values::OwnedJson,
    _vector_fee: oasis7_proto::memory_budget::MemoryReservation,
    _id_fee: oasis7_proto::memory_budget::MemoryReservation,
}
impl<'p> CurrentCognitionAdmissionOutcomeV2<'p> {
    pub(in crate::world) fn continuation(&self) -> &AgentContinuation {
        self.continuation
            .value
            .last()
            .expect("private successful admission has appended continuation")
    }
    pub(in crate::world) fn reference(&self) -> &IwsJournalReferenceV2 {
        &self.reference
    }
    pub(in crate::world) fn into_pending(self) -> PendingCurrentCognitionJournalV1<'p> {
        self.pending
    }
    pub(in crate::world) fn validate_evidence<M: MerkleNodeReader>(
        &self,
        tx: &ExecutionTransaction<'_, M>,
        common: &mut FormalBoundedContextV1<'_>,
        now: u64,
        live: &ExecutionFence,
    ) -> Result<(), BodyMutationError> {
        tx.validate_transaction_identity(&self.guard)?;
        tx.validate_guard(&tx.guard(), live)?;
        self.pending.validate_evidence(tx, common.memory(), live)?;
        self.registration.validate(tx, common, live)?;
        common.charge(1, 128).map_err(formal)?;
        if self.identity != common.work().identity()
            || self.now != now
            || !self._fee.uses_budget(common.memory())
            || !self.registration.uses_budget(common.memory())
            || !self.cut.uses_budget(common.memory())
        {
            return Err(bad().into());
        }
        Ok(())
    }
}
fn overlay(e: WorldFieldOverlayErrorV1) -> BodyMutationError {
    match e {
        WorldFieldOverlayErrorV1::Execution(e) => e.into(),
        WorldFieldOverlayErrorV1::Value(e) => e.into(),
        WorldFieldOverlayErrorV1::Member(e) => formal(e),
    }
}
fn invalid<OE>(
    code: &str,
    common: &mut FormalBoundedContextV1<'_>,
) -> RootModuleExecutionErrorV1<OE> {
    failure("cognition validation failed: ", code, common)
}
fn scheduler_error<OE>(
    code: &str,
    common: &mut FormalBoundedContextV1<'_>,
) -> RootModuleExecutionErrorV1<OE> {
    failure("cognition scheduler failed: ", code, common)
}
fn failure<OE>(
    prefix: &str,
    code: &str,
    common: &mut FormalBoundedContextV1<'_>,
) -> RootModuleExecutionErrorV1<OE> {
    if let Err(e) = common.charge(1, (prefix.len() + code.len()) as u64) {
        return RootModuleExecutionErrorV1::Body(formal(e));
    }
    let fee = match common.memory().reserve(
        std::mem::size_of::<crate::WorldError>() + prefix.len() + code.len(),
        MemoryClass::Ordinary,
    ) {
        Ok(f) => f,
        Err(e) => return ValueTreeError::Memory(e).into(),
    };
    let mut reason = String::with_capacity(prefix.len() + code.len());
    reason.push_str(prefix);
    reason.push_str(code);
    RootModuleExecutionErrorV1::Business(
        fee.retain(crate::WorldError::DistributedValidationFailed { reason }),
    )
}
fn status(
    value: &mut AgentContinuation,
    work: &mut CurrentCognitionExpiryWorkV1,
    common: &mut FormalBoundedContextV1<'_>,
) -> Result<(), BodyMutationError> {
    common.charge(32768, 256 << 10).map_err(formal)?;
    let _phase = common
        .memory()
        .reserve(256 << 10, MemoryClass::Ordinary)
        .map_err(ValueTreeError::Memory)?;
    oasis7_world_state::cognition_wake::validate_iws_continuation_fields_v2(value)
        .map_err(formal)?;
    let hash = values::digest(
        IWS_CONTINUATION_STATUS_DOMAIN_V2,
        &oasis7_world_state::cognition_wake::iws_continuation_status_digest_input_v2(value),
        &mut work.hashes,
        common,
    )?;
    common.charge(1, hash.len() as u64).map_err(formal)?;
    value.continuation_status_digest = Some(hash.to_string());
    Ok(())
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
    /// The proposal is DATA. Genuine registration and Current facts are read by
    /// this method; neither the proposal nor this outcome grants execution input.
    pub(in crate::world) fn admit_current_cognition_continuation_v2<'p, P, PE, PC>(
        &mut self,
        sources: &CurrentCognitionServiceSourcesV2<'_, 'p>,
        proposal: IwsContinuationProposalV2,
        mut pending: PendingCurrentCognitionJournalV1<'p>,
        work: &mut CurrentCognitionServiceWorkV2<'_>,
        parent_read: &mut WorldFieldReadContextV1<'_, P, R, PE, PC>,
        common: &mut FormalBoundedContextV1<'_>,
        live_owner: &mut dyn FnMut() -> ExecutionFence,
    ) -> Result<CurrentCognitionAdmissionOutcomeV2<'p>, RootModuleExecutionErrorV1<O::Error>>
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
                .ledger
                .validate_context(self.ctx.transaction, common, &live_owner())?;
            pending.validate_evidence(self.ctx.transaction, common.memory(), &live_owner())?;
            let roots = sources.parent.refs().roots;
            let fee = common
                .memory()
                .reserve(
                    std::mem::size_of::<CurrentCognitionAdmissionOutcomeV2>(),
                    MemoryClass::Ordinary,
                )
                .map_err(ValueTreeError::Memory)?
                .retain(());
            // Bounded proposal graph is retained through all clones made by the fixed
            // admission kernels, including the internally tagged condition representation.
            let proposal_graph = values::json(&proposal, work.expiry.max_selected_heap, common)
                .map_err(RootModuleExecutionErrorV1::Body)?;
            common
                .charge(32768, 256 << 10)
                .map_err(|e| RootModuleExecutionErrorV1::Body(formal(e)))?;
            let _phase = common
                .memory()
                .reserve(256 << 10, MemoryClass::Ordinary)
                .map_err(ValueTreeError::Memory)?;
            proposal
                .validate_data()
                .map_err(|e| invalid::<O::Error>(e.code(), common))?;
            let raw = proposal.as_data();
            // Runtime binding is a genuine Current ledger field, not the historical
            // admission frame or a caller-supplied policy view.
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
            .ok_or_else(|| invalid::<O::Error>("runtime_binding_required", common))?;
            if binding.value().is_null() {
                return Err(invalid("runtime_binding_required", common));
            }
            let bind = binding.value();
            common
                .charge(
                    7,
                    raw.world_id.len() as u64
                        + raw.branch_id.len() as u64
                        + raw.finality_status.len() as u64
                        + raw.runtime_manifest_hash.len() as u64
                        + 256,
                )
                .map_err(|e| RootModuleExecutionErrorV1::Body(formal(e)))?;
            if bind["world_id"].as_str() != Some(raw.world_id.as_str())
                || bind["branch_id"].as_str() != Some(raw.branch_id.as_str())
                || bind["finality_epoch"].as_u64() != Some(raw.finality_epoch)
                || bind["finality_status"].as_str() != Some(raw.finality_status.as_str())
                || bind["reorg_epoch"].as_u64() != Some(raw.reorg_epoch)
            {
                return Err(invalid("foreign_continuation_proposal", common));
            }
            let block = match bind.get("finality_block_hash") {
                None | Some(serde_json::Value::Null) => None,
                Some(serde_json::Value::String(s)) => Some(s.as_str()),
                _ => return Err(invalid("recovery_pending", common)),
            };
            if block != raw.finality_block_hash.as_deref() {
                return Err(invalid("foreign_continuation_proposal", common));
            }
            // Actual Current membership uses the same parent qualified count plus only
            // genuine coalesced Body creations/deletions, never a parent agent list.
            let q = sources.parent.refs().storage.current().raw().agents();
            let present = inputs::io::with_intent_aggregate_v2(self.ctx, work, common, |ctx, w| {
                use oasis7_formal_protocol::incremental_state::StateNamespaceV1;
                use oasis7_world_state::incremental_agent_subtree::{
                    AgentSubtreeKeyV1, CurrentAgentPresenceV1, parse_agent_subtree_key_v1,
                    read_current_agent_presence_v1,
                };
                let guard = ctx.transaction.guard();
                let present = read_current_agent_presence_v1(
                    &raw.agent_id,
                    q,
                    roots,
                    ctx,
                    parent_read,
                    w.pending_entries,
                    &guard,
                    &live_owner(),
                )
                .map_err(overlay)?;
                let mut count = i128::from(q.agent_count());
                ctx.transaction.visit_pending_writes::<BodyMutationError>(
                    StateNamespaceV1::Business,
                    w.pending_entries,
                    || {
                        if (ctx.cancelled)() {
                            Err(ValueTreeError::Cancelled.into())
                        } else {
                            Ok(())
                        }
                    },
                    |key, before, after| {
                        ctx.key_work.bytes_remaining = ctx
                            .key_work
                            .bytes_remaining
                            .checked_sub(key.len() as u64)
                            .ok_or(ValueTreeError::ResourceLimited(
                                "IWS admission pending agent key",
                            ))?;
                        if matches!(
                            parse_agent_subtree_key_v1(StateNamespaceV1::Business, key)?,
                            Some(AgentSubtreeKeyV1::Body { .. })
                        ) {
                            count += i128::from(after.is_some()) - i128::from(before.is_some());
                        }
                        Ok(())
                    },
                )?;
                if count < 0 {
                    return Err(bad().into());
                }
                Ok((count != 0, present == CurrentAgentPresenceV1::Existing))
            })
            .map_err(RootModuleExecutionErrorV1::Body)?;
            if present.0 && !present.1 {
                return Err(invalid("continuation_agent_missing", common));
            }
            let conditions = IwsWakeConditionValidatorV2::canonicalize(
                raw.wake_conditions
                    .iter()
                    .cloned()
                    .map(IwsWakeConditionV2::try_from)
                    .collect::<Result<Vec<_>, _>>()
                    .map_err(|e| invalid::<O::Error>(e.code(), common))?,
            )
            .map_err(|e| invalid::<O::Error>(e.code(), common))?;
            let next = IwsWakeConditionValidatorV2::next_wake_tick_at(&conditions, self.now)
                .map_err(|e| invalid::<O::Error>(e.code(), common))?;
            if raw.next_wake_tick.is_some_and(|n| Some(n) != next)
                || raw.valid_until_tick.is_some_and(|n| n < self.now)
            {
                return Err(invalid("continuation_proposal_invalid", common));
            }
            let scheduler_owner = state::oasis7_current_read(
                self,
                sources,
                CognitionJsonFieldV1::SchedulerState,
                work,
                common,
                live_owner,
            )?;
            let continuations_owner = state::oasis7_current_read(
                self,
                sources,
                CognitionJsonFieldV1::Continuations,
                work,
                common,
                live_owner,
            )?;
            let contexts_owner = state::oasis7_current_read(
                self,
                sources,
                CognitionJsonFieldV1::ContinuationContexts,
                work,
                common,
                live_owner,
            )?;
            let empty = serde_json::Value::Array(Vec::new());
            let mut continuations = values::continuations(
                continuations_owner.as_ref().map_or(&empty, |o| o.value()),
                work.expiry.max_selected_heap,
                common,
            )
            .map_err(RootModuleExecutionErrorV1::Body)?;
            let registration = self.prepare_current_registered_cognition_turn_v2(
                sources.parent,
                &proposal,
                Some(&pending),
                work.expiry,
                common,
                live_owner,
            )?;
            common
                .charge(
                    continuations.value.len() as u64,
                    continuations
                        .value
                        .iter()
                        .map(|c| {
                            c.continuation_proposal_id.len()
                                + c.agent_id.len()
                                + c.agent_session_id.len()
                                + c.agent_turn_id.len()
                                + c.decision_request_id.len()
                        })
                        .sum::<usize>() as u64,
                )
                .map_err(|e| RootModuleExecutionErrorV1::Body(formal(e)))?;
            if continuations
                .value
                .iter()
                .any(|c| c.continuation_proposal_id == raw.continuation_proposal_id)
            {
                return Err(invalid("continuation_proposal_conflict", common));
            }
            if continuations.value.iter().any(|c| {
                c.agent_id == raw.agent_id
                    && c.agent_session_id == raw.agent_session_id
                    && c.agent_turn_id == raw.agent_turn_id
                    && c.decision_request_id == raw.decision_request_id
            }) {
                return Err(invalid("continuation_budget_chain_conflict", common));
            }
            // Finish all reads before preparing this strict Current cut. It describes
            // admission at this point, not the earlier tick HistoryWritten frame.
            let cut = self.prepare_iws_current_commitment_v1(
                sources.parent,
                sources.input,
                common,
                live_owner,
            )?;
            if raw.runtime_manifest_hash != cut.manifest_hash() {
                return Err(invalid("runtime_manifest_mismatch", common));
            }
            cut.validate(
                self.ctx.transaction,
                sources.parent,
                sources.input,
                common,
                &live_owner(),
            )
            .map_err(RootModuleExecutionErrorV1::Body)?;
            let allocation = with_current_cognition_io_v1(
                self.ctx,
                self.members,
                work.expiry,
                common,
                |ctx, write, _| {
                    use crate::world::incremental_allocators::RuntimeAllocatorV1;
                    let mut mutation = crate::world::incremental_body::BodyMutationContext {
                        sink: &mut *write.sink,
                        memory: write.memory,
                        io: &mut *write.io,
                        cancelled: &mut write.cancelled,
                    };
                    let before = crate::world::World::read_iws_allocator(
                        RuntimeAllocatorV1::Proposal,
                        ctx.transaction,
                        ctx.values.source,
                        &mut mutation,
                    )?;
                    crate::world::World::contribute_iws_allocate(
                        RuntimeAllocatorV1::Proposal,
                        before,
                        ctx.transaction,
                        ctx.values.source,
                        &mut mutation,
                    )
                    .map(|(id, _)| id)
                },
            )
            .map_err(RootModuleExecutionErrorV1::Body)?;
            common
                .charge(2, 128)
                .map_err(|e| RootModuleExecutionErrorV1::Body(formal(e)))?;
            let idfee = common
                .memory()
                .reserve(256, MemoryClass::Ordinary)
                .map_err(ValueTreeError::Memory)?;
            let continuation_id = format!("continuation:{allocation}");
            let wake_id = format!("wake:{allocation}");
            let seq = continuations
                .value
                .iter()
                .filter(|c| c.agent_id == raw.agent_id)
                .map(|c| c.wake_seq)
                .max()
                .unwrap_or(0)
                .saturating_add(1);
            let mut continuation = AgentContinuation {
                schema_version: IWS_CONTINUATION_SCHEMA_V2.into(),
                continuation_id,
                wake_id,
                world_id: raw.world_id.clone(),
                branch_id: raw.branch_id.clone(),
                finality_epoch: raw.finality_epoch,
                finality_block_hash: raw.finality_block_hash.clone(),
                finality_status: raw.finality_status.clone(),
                reorg_epoch: raw.reorg_epoch,
                runtime_manifest_hash: raw.runtime_manifest_hash.clone(),
                agent_id: raw.agent_id.clone(),
                agent_session_id: raw.agent_session_id.clone(),
                agent_turn_id: raw.agent_turn_id.clone(),
                decision_request_id: raw.decision_request_id.clone(),
                origin_turn_id: raw.origin_turn_id.clone(),
                origin_request_digest: raw.origin_request_digest.clone(),
                continuation_proposal_id: raw.continuation_proposal_id.clone(),
                proposal_digest: raw.proposal_digest.clone(),
                action_or_envelope_digest: raw.action_or_envelope_digest.clone(),
                wake_conditions: conditions.iter().map(|c| c.as_data().clone()).collect(),
                next_wake_tick: next,
                remaining_budget: raw.remaining_budget.clone(),
                valid_until_tick: raw.valid_until_tick,
                precondition_digest: raw.precondition_digest.clone(),
                wake_seq: seq,
                logical_tick: self.now,
                status: ContinuationStatusV1::Scheduled,
                continuation_status_digest: None,
                terminal_disposition: None,
            };
            status(&mut continuation, work.expiry, common)
                .map_err(RootModuleExecutionErrorV1::Body)?;
            let scheduler_owner = scheduler_owner
                .ok_or_else(|| invalid::<O::Error>("scheduler_unconfigured", common))?;
            if scheduler_owner.value().is_null() {
                return Err(invalid("scheduler_unconfigured", common));
            }
            let mut scheduler =
                owned::decode_scheduler_v1(scheduler_owner.value(), work.max_schema_depth, common)
                    .map_err(|e| match e {
                        owned::SchedulerOwnedErrorV1::Body(e) => {
                            RootModuleExecutionErrorV1::Body(e)
                        }
                        owned::SchedulerOwnedErrorV1::Scheduler(e) => {
                            scheduler_error(e.code(), common)
                        }
                    })?;
            scheduler.value.advance_logical_tick(self.now);
            let wake = SchedulerWakeV1 {
                schema_version: SchedulerWakeV1::SCHEMA_VERSION.into(),
                wake_id: continuation.wake_id.clone(),
                continuation_id: continuation.continuation_id.clone(),
                world_id: continuation.world_id.clone(),
                branch_id: continuation.branch_id.clone(),
                finality_epoch: continuation.finality_epoch,
                finality_block_hash: continuation.finality_block_hash.clone(),
                finality_status: continuation.finality_status.clone(),
                reorg_epoch: continuation.reorg_epoch,
                runtime_manifest_hash: continuation.runtime_manifest_hash.clone(),
                agent_id: continuation.agent_id.clone(),
                agent_session_id: continuation.agent_session_id.clone(),
                agent_turn_id: continuation.agent_turn_id.clone(),
                decision_request_id: continuation.decision_request_id.clone(),
                request_digest: continuation.origin_request_digest.clone(),
                next_wake_tick: next.unwrap_or(u64::MAX),
                eligible_since_tick: self.now,
                starvation_deadline_tick: self
                    .now
                    .saturating_add(scheduler.value.policy().max_starvation_ticks),
                initial_priority: scheduler.value.policy().initial_priority,
                wake_seq: seq,
                retry_seq: 0,
                status: "pending".into(),
                pending_reason: "capacity_available".into(),
            };
            let n = scheduler
                .value
                .snapshot_order_wakes_v2()
                .count()
                .checked_add(1)
                .ok_or_else(bad)?;
            let keybytes = scheduler
                .value
                .snapshot_order_wakes_v2()
                .map(|w| w.wake_id.len())
                .try_fold(continuation.wake_id.len(), |n, m| {
                    n.checked_add(m).ok_or_else(bad)
                })?;
            let peak = n
                .checked_mul(std::mem::size_of::<SchedulerWakeV1>() * 2)
                .and_then(|n| n.checked_add(4096))
                .ok_or_else(bad)?;
            common
                .charge(n as u64, (keybytes + peak) as u64)
                .map_err(|e| RootModuleExecutionErrorV1::Body(formal(e)))?;
            let _enqueue_peak = common
                .memory()
                .reserve(peak, MemoryClass::Ordinary)
                .map_err(ValueTreeError::Memory)?;
            let enqueue = scheduler
                .value
                .try_enqueue(wake)
                .map_err(|e| scheduler_error::<O::Error>(e.code(), common))?;
            if enqueue.disposition == "pending" {
                crate::cognition_wake::ContinuationTransition::apply_at_tick_state_v1(
                    &mut continuation,
                    ContinuationStatusV1::Pending,
                    self.now,
                )
                .map_err(|e| invalid::<O::Error>(e.code(), common))?;
                status(&mut continuation, work.expiry, common)
                    .map_err(RootModuleExecutionErrorV1::Body)?;
            }
            // Keep the clone/growth admission live with the resulting typed vector.
            let vector_peak = continuations
                .value
                .len()
                .checked_add(1)
                .and_then(|n| n.checked_mul(std::mem::size_of::<AgentContinuation>() * 2))
                .ok_or_else(bad)?;
            common
                .charge(continuations.value.len() as u64, vector_peak as u64)
                .map_err(|e| RootModuleExecutionErrorV1::Body(formal(e)))?;
            let vector_fee = common
                .memory()
                .reserve(vector_peak, MemoryClass::Ordinary)
                .map_err(ValueTreeError::Memory)?;
            continuations
                .value
                .try_reserve_exact(1)
                .map_err(|_| ValueTreeError::ResourceLimited("IWS admitted continuation slot"))?;
            // Save borrowed event fields before moving the complete continuation.
            let commitment =
                commitment_text(&cut, common).map_err(RootModuleExecutionErrorV1::Body)?;
            let core = admission_core_v2(
                &proposal,
                &continuation.continuation_id,
                &continuation.wake_id,
                &commitment,
                cut.frame(),
            );
            let core_digest = values::digest(
                "oasis7.cognition.iws-continuation-admission.v2",
                &core,
                &mut work.expiry.hashes,
                common,
            )
            .map_err(RootModuleExecutionErrorV1::Body)?;
            let old_hash = with_current_cognition_io_v1(
                self.ctx,
                self.members,
                work.expiry,
                common,
                |ctx, _, w| {
                    pending
                        .current_digest(ctx.values.memory, &mut w.hash, &mut ctx.cancelled)
                        .map_err(BodyMutationError::from)
                },
            )
            .map_err(RootModuleExecutionErrorV1::Body)?;
            let old_sequence = pending.sequence();
            let old_digest =
                event::head_text(old_hash, common).map_err(RootModuleExecutionErrorV1::Body)?;
            with_current_cognition_io_v1(
                self.ctx,
                self.members,
                work.expiry,
                common,
                |ctx, _, _| {
                    check_cognition_control_overlay_v1(
                        CognitionControlFieldV1::JournalHeadSeq,
                        CognitionControlValueV1::Unsigned(old_sequence),
                        ctx,
                    )
                    .map_err(overlay)?;
                    check_cognition_control_overlay_v1(
                        CognitionControlFieldV1::JournalHeadDigest,
                        CognitionControlValueV1::Text(&old_digest),
                        ctx,
                    )
                    .map_err(overlay)
                },
            )
            .map_err(RootModuleExecutionErrorV1::Body)?;
            let scheduled = event::prepare_continuation_scheduled_v2(
                &continuation,
                &scheduler.value,
                &raw.proposal_digest,
                &core_digest,
                Some(binding.value()),
                old_sequence,
                old_hash,
                work.expiry,
                common,
            )
            .map_err(RootModuleExecutionErrorV1::Body)?;
            // The reference is emitted by the one actual Original/fact/index/cache append.
            // There is no provisional/zero locator in either the hash core or Context.
            let reference = self.append_current_cognition_journal_original_v2(
                roots,
                &scheduled.value.value,
                &mut pending,
                work.expiry,
                common,
                live_owner,
            )?;
            let context = values::json(
                &ContextPayload {
                    core,
                    admission: &reference,
                    digest: &core_digest,
                },
                work.expiry.max_selected_heap,
                common,
            )
            .map_err(RootModuleExecutionErrorV1::Body)?;
            let contexts_after = values::json(
                &MapInsertion {
                    before: contexts_owner.as_ref().map(|o| o.value()),
                    key: &continuation.continuation_id,
                    value: &context.value,
                },
                work.expiry.max_selected_heap,
                common,
            )
            .map_err(RootModuleExecutionErrorV1::Body)?;
            continuations.value.push(continuation);
            let continuations_after =
                values::json(&continuations.value, work.expiry.max_selected_heap, common)
                    .map_err(RootModuleExecutionErrorV1::Body)?;
            let mut scheduler_after =
                values::json(&scheduler.value, work.expiry.max_selected_heap, common)
                    .map_err(RootModuleExecutionErrorV1::Body)?;
            // Match the original snapshot_json observability field. Hold the
            // fixed map-node/key split peak until the encoded CAS has finished.
            common
                .charge(1, 64)
                .map_err(|e| RootModuleExecutionErrorV1::Body(formal(e)))?;
            let _snapshot_count_fee = common
                .memory()
                .reserve(2048, MemoryClass::Ordinary)
                .map_err(ValueTreeError::Memory)?;
            scheduler_after
                .value
                .as_object_mut()
                .ok_or_else(bad)?
                .insert(
                    "backpressure_count".into(),
                    serde_json::json!(scheduler.value.pending_backpressure_count()),
                );
            with_current_cognition_io_v1(
                self.ctx,
                self.members,
                work.expiry,
                common,
                |ctx, write, w| {
                    if continuations_owner.is_none() {
                        let guard = ctx.transaction.guard();
                        create_current_cognition_continuations_v2(
                            sources.current,
                            roots,
                            &continuations_after.value,
                            ctx,
                            write,
                            &mut w.write,
                            &guard,
                            &live_owner(),
                        )
                        .map_err(overlay)?;
                    }
                    if contexts_owner.is_none() {
                        let guard = ctx.transaction.guard();
                        create_current_cognition_service_map_v2(
                            CognitionJsonFieldV1::ContinuationContexts,
                            sources.current,
                            roots,
                            &contexts_after.value,
                            ctx,
                            write,
                            &mut w.write,
                            &guard,
                            &live_owner(),
                        )
                        .map_err(overlay)?;
                    }
                    let slots = [
                        continuations_owner
                            .as_ref()
                            .map(|o| (o, &continuations_after.value)),
                        contexts_owner.as_ref().map(|o| (o, &contexts_after.value)),
                        Some((&scheduler_owner, &scheduler_after.value)),
                    ];
                    // Finite three-record stack inventory; each fixed match passes an actual
                    // slice without allocating an unmetered batch Vec.
                    let guard = ctx.transaction.guard();
                    match slots {
                        [Some(a), Some(b), Some(c)] => replace_current_cognition_service_batch_v1(
                            &[a, b, c],
                            ctx,
                            write,
                            &mut w.write,
                            &guard,
                            &live_owner(),
                        ),
                        [Some(a), None, Some(c)] | [None, Some(a), Some(c)] => {
                            replace_current_cognition_service_batch_v1(
                                &[a, c],
                                ctx,
                                write,
                                &mut w.write,
                                &guard,
                                &live_owner(),
                            )
                        }
                        [None, None, Some(c)] => replace_current_cognition_service_batch_v1(
                            &[c],
                            ctx,
                            write,
                            &mut w.write,
                            &guard,
                            &live_owner(),
                        ),
                        _ => return Err(bad().into()),
                    }
                    .map_err(overlay)
                },
            )
            .map_err(RootModuleExecutionErrorV1::Body)?;
            let hash = with_current_cognition_io_v1(
                self.ctx,
                self.members,
                work.expiry,
                common,
                |ctx, _, w| {
                    pending
                        .current_digest(ctx.values.memory, &mut w.hash, &mut ctx.cancelled)
                        .map_err(BodyMutationError::from)
                },
            )
            .map_err(RootModuleExecutionErrorV1::Body)?;
            let digest =
                event::head_text(hash, common).map_err(RootModuleExecutionErrorV1::Body)?;
            with_current_cognition_io_v1(
                self.ctx,
                self.members,
                work.expiry,
                common,
                |ctx, write, _| {
                    let guard = ctx.transaction.guard();
                    change_cognition_control_v1(
                        CognitionControlFieldV1::JournalHeadSeq,
                        (
                            CognitionControlValueV1::Unsigned(old_sequence),
                            CognitionControlValueV1::Unsigned(pending.sequence()),
                        ),
                        ctx,
                        write,
                        &guard,
                        &live_owner(),
                    )
                    .map_err(overlay)?;
                    let guard = ctx.transaction.guard();
                    change_cognition_control_v1(
                        CognitionControlFieldV1::JournalHeadDigest,
                        (
                            CognitionControlValueV1::Text(&old_digest),
                            CognitionControlValueV1::Text(&digest),
                        ),
                        ctx,
                        write,
                        &guard,
                        &live_owner(),
                    )
                    .map_err(overlay)
                },
            )
            .map_err(RootModuleExecutionErrorV1::Body)?;
            let outcome = CurrentCognitionAdmissionOutcomeV2 {
                pending,
                continuation: continuations,
                context,
                registration,
                cut,
                reference,
                guard: self.ctx.transaction.guard(),
                identity: common.work().identity(),
                now: self.now,
                _fee: fee,
                _proposal_graph: proposal_graph,
                _vector_fee: vector_fee,
                _id_fee: idfee,
            };
            outcome
                .validate_evidence(self.ctx.transaction, common, self.now, &live_owner())
                .map_err(RootModuleExecutionErrorV1::Body)?;
            Ok(outcome)
        }));
        match result {
            Ok(Ok(outcome)) => Ok(outcome),
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
fn commitment_text(
    cut: &PaidIwsCurrentCommitmentV1,
    common: &mut FormalBoundedContextV1<'_>,
) -> Result<ReservedValue<String>, BodyMutationError> {
    common.charge(1, 71).map_err(formal)?;
    let fee = common
        .memory()
        .reserve(std::mem::size_of::<String>() + 71, MemoryClass::Ordinary)
        .map_err(ValueTreeError::Memory)?;
    let mut out = String::with_capacity(71);
    out.push_str("sha256:");
    const HEX: &[u8; 16] = b"0123456789abcdef";
    for b in cut.digest() {
        out.push(HEX[(b >> 4) as usize] as char);
        out.push(HEX[(b & 15) as usize] as char)
    }
    Ok(fee.retain(out))
}
