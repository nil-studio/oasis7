//! Fixed continuation expiry over genuine selected Current and accepted Journal.
//! This stage never mints a complete tick, a Node source or accepted history.
use crate::world::{
    incremental_body::BodyMutationError, incremental_body_event::AgentEventOriginalV1,
    incremental_module_invocation::*, incremental_normal_cognition_owned as owned,
    incremental_normal_sources::BoundNormalTickParentV1,
    incremental_normal_tick_finalize::consensus::HistoryWrittenIwsCommitmentV1,
};
use oasis7_formal_protocol::{
    controlled_authority::formal::bounded_context::FormalBoundedContextV1,
    incremental_execution::{
        ExecutionError, ExecutionFactKindV1, ExecutionFactRefV1, ExecutionFence,
        ExecutionTransaction, PreparedGuard,
    },
    incremental_material::{FactLocationV1, OriginalObjectReader, OriginalObjectStreamReader},
    incremental_state::{MerkleNodeReader, StateNamespaceV1, StatePageWriter, StateScanBudget},
    incremental_value::{ValueNodeReader, ValueNodeWriter, ValueReadContext, ValueTreeError},
};
use oasis7_proto::memory_budget::{MemoryClass, MemoryReservation, ReservedValue};
use oasis7_world_state::{
    incremental_cognition_control_overlay::{
        CognitionControlValueV1, change_cognition_control_v1, check_cognition_control_overlay_v1,
    },
    incremental_cognition_current_complete_v3::CompleteCognitionCurrentQualificationV3,
    incremental_cognition_fields::CognitionControlFieldV1,
    incremental_cognition_history::*,
    incremental_cognition_history_cbor::CognitionHistoryCborWorkV1,
    incremental_cognition_history_hash_outboard::CognitionHeadHashWorkV1,
    incremental_cognition_json::CognitionJsonFieldV1,
    incremental_cognition_proof_ledger::{
        CognitionProofLedgerFieldV1, CurrentCognitionProofLedgerV1, ProofLedgerReadWorkV1,
    },
    incremental_field_member::MemberRecordWriteContext,
    incremental_field_overlay::{WorldFieldOverlayContextV1, WorldFieldOverlayErrorV1},
    incremental_field_read::WorldFieldReadContextV1,
    incremental_json_records::{JsonReadWorkV1, JsonWriteWorkV1},
    incremental_normal_cognition_current::*,
};
fn bad() -> ValueTreeError {
    ValueTreeError::Invalid("Current Cognition expiry source")
}
fn formal(e: impl std::error::Error + Send + Sync + 'static) -> BodyMutationError {
    BodyMutationError::Original(Box::new(e))
}
fn overlay(e: WorldFieldOverlayErrorV1) -> BodyMutationError {
    match e {
        WorldFieldOverlayErrorV1::Execution(e) => e.into(),
        WorldFieldOverlayErrorV1::Value(e) => e.into(),
        WorldFieldOverlayErrorV1::Member(e) => formal(e),
    }
}
pub(in crate::world) struct CurrentCognitionExpiryWorkV1 {
    pub read: JsonReadWorkV1,
    pub write: JsonWriteWorkV1,
    pub index: CognitionHistoryIndexWorkV1,
    pub cbor: CognitionHistoryCborWorkV1,
    pub hash: CognitionHeadHashWorkV1,
    pub original: oasis7_wasm_abi::CanonicalCborWorkV1,
    pub hashes: oasis7_wasm_abi::CanonicalCborWorkV1,
    pub ledger: ProofLedgerReadWorkV1,
    pub ledger_scan: StateScanBudget,
    pub max_selected_heap: usize,
}
/// Constructed only by the actual fixed executor, including legitimate no-op
/// paths. Even a no-op retains any earlier same-tick Journal frontier.
pub(in crate::world) struct CurrentCognitionExpiryOutcomeV1<'p> {
    kind: ExpiryKindV1<'p>,
    guard: PreparedGuard,
    identity: oasis7_formal_protocol::controlled_authority::formal::bounded_context::WorkIdentityV1,
    _fee: ReservedValue<()>,
}
enum ExpiryKindV1<'p> {
    Unchanged {
        continued: Option<PendingCurrentCognitionJournalV1<'p>>,
    },
    Written(PreparedCurrentCognitionExpiryV1<'p>),
}
impl<'p> CurrentCognitionExpiryOutcomeV1<'p> {
    pub(in crate::world) fn expired(&self) -> usize {
        match &self.kind {
            ExpiryKindV1::Unchanged { .. } => 0,
            ExpiryKindV1::Written(source) => source.expired,
        }
    }
    pub(in crate::world) fn journal(&self) -> Option<&PendingCurrentCognitionJournalV1<'p>> {
        match &self.kind {
            ExpiryKindV1::Unchanged { continued } => continued.as_ref(),
            ExpiryKindV1::Written(source) => Some(&source.journal),
        }
    }
    /// Move the complete opaque pending owner into the next scheduler phase.
    /// No cache state, root or qualification is detached from its fees/source.
    pub(in crate::world) fn into_pending(self) -> Option<PendingCurrentCognitionJournalV1<'p>> {
        match self.kind {
            ExpiryKindV1::Unchanged { continued } => continued,
            ExpiryKindV1::Written(source) => Some(source.journal),
        }
    }
    pub(in crate::world) fn validate_evidence<M: MerkleNodeReader>(
        &self,
        tx: &ExecutionTransaction<'_, M>,
        common: &mut FormalBoundedContextV1<'_>,
        live: &ExecutionFence,
    ) -> Result<(), BodyMutationError> {
        common.charge(1, 128).map_err(formal)?;
        tx.validate_transaction_identity(&self.guard)?;
        tx.validate_guard(&tx.guard(), live)?;
        if self.identity != common.work().identity() || !self._fee.uses_budget(common.memory()) {
            return Err(bad().into());
        }
        if let Some(journal) = self.journal() {
            journal.validate_evidence(tx, common.memory(), live)?
        }
        Ok(())
    }
}
pub(in crate::world) struct PreparedCurrentCognitionExpiryV1<'p> {
    journal: PendingCurrentCognitionJournalV1<'p>,
    expired: usize,
    sequence: u64,
    digest: [u8; 32],
    _digest_text: ReservedValue<String>,
    _scheduler: values::OwnedJson,
    _continuations: values::OwnedJson,
    _before_scheduler: ReservedValue<CurrentCognitionJsonRecordV1>,
    _before_continuations: ReservedValue<CurrentCognitionJsonRecordV1>,
}
fn unchanged<'p, M: MerkleNodeReader>(
    continued: Option<PendingCurrentCognitionJournalV1<'p>>,
    tx: &ExecutionTransaction<'_, M>,
    common: &mut FormalBoundedContextV1<'_>,
    live: &ExecutionFence,
    fee: ReservedValue<()>,
) -> Result<CurrentCognitionExpiryOutcomeV1<'p>, BodyMutationError> {
    let value = CurrentCognitionExpiryOutcomeV1 {
        kind: ExpiryKindV1::Unchanged { continued },
        guard: tx.guard(),
        identity: common.work().identity(),
        _fee: fee,
    };
    value.validate_evidence(tx, common, live)?;
    Ok(value)
}
#[path = "incremental_normal_cognition_expiry_event.rs"]
pub(in crate::world) mod event;
#[path = "incremental_normal_cognition_expiry_io.rs"]
mod io;
#[path = "incremental_normal_cognition_expiry_values.rs"]
pub(in crate::world) mod values;
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
    pub(in crate::world) fn execute_current_cognition_expiry_v1<
        'p,
        P: MerkleNodeReader,
        PE: FnMut() -> Option<ValueTreeError>,
        PC: FnMut() -> bool,
    >(
        &mut self,
        parent: &'p ReservedValue<BoundNormalTickParentV1<'p>>,
        written: &ReservedValue<HistoryWrittenIwsCommitmentV1>,
        current_q: &ReservedValue<CompleteCognitionCurrentQualificationV3>,
        ledger: &CurrentCognitionProofLedgerV1,
        continued: Option<PendingCurrentCognitionJournalV1<'p>>,
        work: &mut CurrentCognitionExpiryWorkV1,
        parent_read: &mut WorldFieldReadContextV1<'_, P, R, PE, PC>,
        common: &mut FormalBoundedContextV1<'_>,
        live_owner: &mut dyn FnMut() -> ExecutionFence,
    ) -> Result<CurrentCognitionExpiryOutcomeV1<'p>, RootModuleExecutionErrorV1<O::Error>> {
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            parent
                .validate_context(common)
                .map_err(|e| RootModuleExecutionErrorV1::Body(formal(e)))?;
            written
                .validate_evidence(self.ctx.transaction, common, &live_owner())
                .map_err(RootModuleExecutionErrorV1::Body)?;
            let outcome_fee = common
                .memory()
                .reserve(
                    std::mem::size_of::<CurrentCognitionExpiryOutcomeV1>(),
                    MemoryClass::Ordinary,
                )
                .map_err(ValueTreeError::Memory)?
                .retain(());
            let roots = parent.refs().roots;
            let before_scheduler =
                io::paid(self.ctx, self.members, work, common, |ctx, _, work| {
                    let guard = ctx.transaction.guard();
                    read_current_cognition_json_record_v1(
                        CognitionJsonFieldV1::SchedulerState,
                        current_q,
                        roots,
                        ctx,
                        &mut work.read,
                        work.max_selected_heap,
                        &guard,
                        &live_owner(),
                    )
                    .map_err(overlay)
                })
                .map_err(RootModuleExecutionErrorV1::Body)?;
            let Some(before_scheduler) = before_scheduler else {
                return unchanged(
                    continued,
                    self.ctx.transaction,
                    common,
                    &live_owner(),
                    outcome_fee,
                )
                .map_err(RootModuleExecutionErrorV1::Body);
            };
            if before_scheduler.value().is_null() {
                return unchanged(
                    continued,
                    self.ctx.transaction,
                    common,
                    &live_owner(),
                    outcome_fee,
                )
                .map_err(RootModuleExecutionErrorV1::Body);
            }
            let before_continuations =
                io::paid(self.ctx, self.members, work, common, |ctx, _, work| {
                    let guard = ctx.transaction.guard();
                    read_current_cognition_json_record_v1(
                        CognitionJsonFieldV1::Continuations,
                        current_q,
                        roots,
                        ctx,
                        &mut work.read,
                        work.max_selected_heap,
                        &guard,
                        &live_owner(),
                    )
                    .map_err(overlay)
                })
                .map_err(RootModuleExecutionErrorV1::Body)?;
            let empty = serde_json::Value::Array(Vec::new());
            let raw = before_continuations.as_ref().map_or(&empty, |v| v.value());
            // Exact legacy hydration uses normalized ACTIVE continuations before parsing
            // Scheduler, while the expiry working vector retains the original raw rows.
            let mut active = values::continuations(raw, work.max_selected_heap, common)
                .map_err(RootModuleExecutionErrorV1::Body)?;
            active
                .value
                .retain(|c| !crate::world::World::is_terminal_continuation_status(c.status));
            for c in &mut active.value {
                if c.schema_version
                    == oasis7_world_state::cognition_wake::IWS_CONTINUATION_SCHEMA_V2
                {
                    // V2 is an explicit profile, never hydrated with a V1 hash.
                    // Selected-shape admission retains the working clone; the
                    // finite V2 SDK decoder has no unbounded collection hints.
                    common
                        .charge(32768, 256 << 10)
                        .map_err(|e| RootModuleExecutionErrorV1::Body(formal(e)))?;
                    let _phase = common
                        .memory()
                        .reserve(256 << 10, MemoryClass::Ordinary)
                        .map_err(ValueTreeError::Memory)?;
                    oasis7_world_state::cognition_wake::validate_iws_continuation_fields_v2(c)
                        .map_err(|e| {
                            business_error::<O::Error>(
                                "cognition validation failed: ",
                                e.code(),
                                common,
                            )
                        })?;
                    let digest = values::digest(
                        oasis7_world_state::cognition_wake::IWS_CONTINUATION_STATUS_DOMAIN_V2,
                        &oasis7_world_state::cognition_wake::iws_continuation_status_digest_input_v2(c),
                        &mut work.hashes,
                        common,
                    )
                    .map_err(RootModuleExecutionErrorV1::Body)?;
                    common
                        .charge(1, digest.len() as u64)
                        .map_err(|e| RootModuleExecutionErrorV1::Body(formal(e)))?;
                    if c.continuation_status_digest.as_deref() != Some(digest.as_str()) {
                        return Err(business_error::<O::Error>(
                            "cognition validation failed: ",
                            "recovery_pending",
                            common,
                        ));
                    }
                    continue;
                }
                if c.continuation_status_digest.is_none() {
                    let digest = values::digest(
                        "oasis7.cognition.continuation-status.v1",
                        &c.status_digest_input_v1(),
                        &mut work.hashes,
                        common,
                    )
                    .map_err(RootModuleExecutionErrorV1::Body)?;
                    common
                        .charge(1, digest.len() as u64)
                        .map_err(|e| RootModuleExecutionErrorV1::Body(formal(e)))?;
                    c.continuation_status_digest = Some(digest.to_string());
                }
                // The original validator's status hash is statically bounded after its
                // fixed identifier guards; all variable wake-condition traversal was paid
                // by the actual selected graph admission above.
                common
                    .charge(8192, 32768)
                    .map_err(|e| RootModuleExecutionErrorV1::Body(formal(e)))?;
                let _phase = common
                    .memory()
                    .reserve(32 << 10, MemoryClass::Ordinary)
                    .map_err(ValueTreeError::Memory)?;
                c.validate_authoritative().map_err(|e| {
                    business_error::<O::Error>("cognition validation failed: ", e.code(), common)
                })?;
            }
            let mut hydrated =
                values::json(before_scheduler.value(), work.max_selected_heap, common)
                    .map_err(RootModuleExecutionErrorV1::Body)?;
            for bucket in ["active", "backpressure", "in_flight"] {
                let Some(entries) = hydrated.value.get_mut(bucket) else {
                    continue;
                };
                let mut hydrate = |entry: &mut serde_json::Value| -> Result<(), BodyMutationError> {
                    let n = active.value.iter().try_fold(0usize, |n, c| {
                        n.checked_add(c.continuation_id.len() + c.origin_request_digest.len())
                            .ok_or_else(|| BodyMutationError::from(bad()))
                    })?;
                    common
                        .charge(active.value.len() as u64, n as u64)
                        .map_err(formal)?;
                    crate::world::cognition_orchestration::hydrate_wake_digest(
                        entry,
                        &active.value,
                    );
                    Ok(())
                };
                match entries {
                    serde_json::Value::Array(v) => {
                        for e in v {
                            hydrate(e).map_err(RootModuleExecutionErrorV1::Body)?
                        }
                    }
                    serde_json::Value::Object(v) => {
                        for e in v.values_mut() {
                            hydrate(e).map_err(RootModuleExecutionErrorV1::Body)?
                        }
                    }
                    _ => {}
                }
            }
            let mut scheduler =
                owned::decode_scheduler_v1(&hydrated.value, work.read.max_depth, common)
                    .map_err(|e| scheduler_error::<O::Error>(e, common))?;
            let mut continuations = values::continuations(raw, work.max_selected_heap, common)
                .map_err(RootModuleExecutionErrorV1::Body)?;
            let count = continuations.value.len();
            let _expired_fee = common
                .memory()
                .reserve(
                    count
                        .checked_mul(std::mem::size_of::<(
                            usize,
                            Option<crate::cognition_scheduler::SchedulerWakeV1>,
                        )>())
                        .ok_or_else(bad)?,
                    MemoryClass::Ordinary,
                )
                .map_err(ValueTreeError::Memory)?;
            let mut expired = Vec::new();
            expired.try_reserve_exact(count).map_err(|_| bad())?;
            if expired.capacity() > count {
                return Err(bad().into());
            }
            for (index, c) in continuations.value.iter_mut().enumerate() {
                if let Some(wake) = owned::expire_selected_continuation_v1(
                    c,
                    &mut scheduler,
                    self.now,
                    &mut work.hashes,
                    common,
                )
                .map_err(|e| scheduler_error::<O::Error>(e, common))?
                {
                    expired.push((index, wake));
                }
            }
            if expired.is_empty() {
                return unchanged(
                    continued,
                    self.ctx.transaction,
                    common,
                    &live_owner(),
                    outcome_fee,
                )
                .map_err(RootModuleExecutionErrorV1::Body);
            }
            let before_continuations = before_continuations.ok_or_else(bad)?;
            let binding = ledger.read_field(
                self.ctx.transaction,
                CognitionProofLedgerFieldV1::RuntimeBinding,
                parent_read,
                &mut work.ledger_scan,
                &mut work.ledger,
                common,
                &live_owner(),
            )?;
            let mut journal = match continued {
                Some(source) => source,
                None => io::paid(self.ctx, self.members, work, common, |ctx, _, w| {
                    PendingCurrentCognitionJournalV1::from_accepted_at_head_v1(
                        parent.refs().cognition,
                        parent.refs().head,
                        ctx.transaction,
                        ctx.values.memory,
                        &mut w.hash,
                        &live_owner(),
                        &mut ctx.cancelled,
                    )
                    .map_err(BodyMutationError::from)
                })
                .map_err(RootModuleExecutionErrorV1::Body)?,
            };
            common
                .charge(1, 128)
                .map_err(|e| RootModuleExecutionErrorV1::Body(formal(e)))?;
            journal.validate_evidence(self.ctx.transaction, common.memory(), &live_owner())?;
            let old_sequence = journal.sequence();
            let old_hash = io::paid(self.ctx, self.members, work, common, |ctx, _, w| {
                journal
                    .current_digest(ctx.values.memory, &mut w.hash, &mut ctx.cancelled)
                    .map_err(BodyMutationError::from)
            })
            .map_err(RootModuleExecutionErrorV1::Body)?;
            let old_digest =
                event::head_text(old_hash, common).map_err(RootModuleExecutionErrorV1::Body)?;
            io::paid(self.ctx, self.members, work, common, |ctx, _, _| {
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
            })
            .map_err(RootModuleExecutionErrorV1::Body)?;
            let scheduler_after = values::json(&scheduler.value, work.max_selected_heap, common)
                .map_err(RootModuleExecutionErrorV1::Body)?;
            let mut scheduler_after = scheduler_after;
            let _count_fee = common
                .memory()
                .reserve(2048, MemoryClass::Ordinary)
                .map_err(ValueTreeError::Memory)?;
            common
                .charge(1, 64)
                .map_err(|e| RootModuleExecutionErrorV1::Body(formal(e)))?;
            if let Some(object) = scheduler_after.value.as_object_mut() {
                object.insert(
                    "backpressure_count".into(),
                    serde_json::json!(scheduler.value.pending_backpressure_count()),
                );
            }
            let continuations_after =
                values::json(&continuations.value, work.max_selected_heap, common)
                    .map_err(RootModuleExecutionErrorV1::Body)?;
            for (index, wake) in &expired {
                let event = event::prepare(
                    &continuations.value[*index],
                    &scheduler.value,
                    wake.as_ref(),
                    binding.as_ref().map(|v| v.value()),
                    journal.sequence(),
                    journal.last_digest(),
                    work,
                    common,
                )
                .map_err(RootModuleExecutionErrorV1::Body)?;
                self.append_current_cognition_journal_original_v2(
                    roots,
                    &event.value.value,
                    &mut journal,
                    work,
                    common,
                    live_owner,
                )?;
            }
            let new_hash = io::paid(self.ctx, self.members, work, common, |ctx, _, w| {
                journal
                    .current_digest(ctx.values.memory, &mut w.hash, &mut ctx.cancelled)
                    .map_err(BodyMutationError::from)
            })
            .map_err(RootModuleExecutionErrorV1::Body)?
            .ok_or_else(bad)?;
            let new_digest = event::head_text(Some(new_hash), common)
                .map_err(RootModuleExecutionErrorV1::Body)?;
            io::paid(self.ctx, self.members, work, common, |ctx, write, w| {
                let live = live_owner();
                let guard = ctx.transaction.guard();
                replace_current_cognition_service_batch_v1(
                    &[
                        (&before_scheduler, &scheduler_after.value),
                        (&before_continuations, &continuations_after.value),
                    ],
                    ctx,
                    write,
                    &mut w.write,
                    &guard,
                    &live,
                )
                .map_err(overlay)?;
                let guard = ctx.transaction.guard();
                change_cognition_control_v1(
                    CognitionControlFieldV1::JournalHeadSeq,
                    (
                        CognitionControlValueV1::Unsigned(old_sequence),
                        CognitionControlValueV1::Unsigned(journal.sequence()),
                    ),
                    ctx,
                    write,
                    &guard,
                    &live,
                )
                .map_err(overlay)?;
                let guard = ctx.transaction.guard();
                change_cognition_control_v1(
                    CognitionControlFieldV1::JournalHeadDigest,
                    (
                        CognitionControlValueV1::Text(&old_digest),
                        CognitionControlValueV1::Text(&new_digest),
                    ),
                    ctx,
                    write,
                    &guard,
                    &live,
                )
                .map_err(overlay)?;
                Ok(())
            })
            .map_err(RootModuleExecutionErrorV1::Body)?;
            let prepared = PreparedCurrentCognitionExpiryV1 {
                sequence: journal.sequence(),
                digest: new_hash,
                journal,
                expired: expired.len(),
                _digest_text: new_digest,
                _scheduler: scheduler_after,
                _continuations: continuations_after,
                _before_scheduler: before_scheduler,
                _before_continuations: before_continuations,
            };
            let outcome = CurrentCognitionExpiryOutcomeV1 {
                kind: ExpiryKindV1::Written(prepared),
                guard: self.ctx.transaction.guard(),
                identity: common.work().identity(),
                _fee: outcome_fee,
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
    /// One physical Original/Event fact and the original authenticated Journal
    /// index/outboard append. Expiry and scheduler service share this kernel.
    #[allow(clippy::too_many_arguments)]
    pub(in crate::world) fn append_current_cognition_journal_original_v2(
        &mut self,
        roots: &oasis7_formal_protocol::incremental_execution::RootSetV1,
        event: &serde_json::Value,
        journal: &mut PendingCurrentCognitionJournalV1<'_>,
        work: &mut CurrentCognitionExpiryWorkV1,
        common: &mut FormalBoundedContextV1<'_>,
        live_owner: &mut dyn FnMut() -> ExecutionFence,
    ) -> Result<
        oasis7_agent_api::iws_scheduler_v2::IwsJournalReferenceV2,
        RootModuleExecutionErrorV1<O::Error>,
    > {
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let (id, bytes, ordinal) =
                io::paid(self.ctx, self.members, work, common, |ctx, _, w| {
                    use oasis7_world_state::incremental_original_producer as original;
                    let max = work_limit(w);
                    let measured = original::measure_serializable(
                        event,
                        ctx.values.memory,
                        &mut w.original,
                        &mut ctx.cancelled,
                        max,
                    )?;
                    let id = measured.id();
                    let mut producer = original::Producer::new(
                        measured,
                        ctx.values.memory,
                        &mut w.original,
                        &mut ctx.cancelled,
                    );
                    let bytes = producer.original_bytes();
                    (self.events.persist_original)(&mut producer)?;
                    producer.check_complete()?;
                    drop(producer);
                    let ordinal =
                        u64::try_from(ctx.transaction.ordered_fact_count()).map_err(|_| bad())?;
                    ctx.transaction.append_fact(ExecutionFactRefV1 {
                        kind: ExecutionFactKindV1::Event,
                        id,
                    })?;
                    Ok((id, bytes, ordinal))
                })
                .map_err(RootModuleExecutionErrorV1::Body)?;
            let raw = with_current_root_module_meter_v1(self, common, live_owner, |root, _| {
                if root.originals.allocation_class() != MemoryClass::Ordinary
                    || !root
                        .originals
                        .allocation_budget()
                        .same_pool(root.ctx.values.memory)
                {
                    return Err(bad().into());
                }
                root.work.input.bytes_remaining =
                    root.work.input.bytes_remaining.checked_sub(bytes).ok_or(
                        ValueTreeError::ResourceLimited("Cognition expiry Original read"),
                    )?;
                let raw=root.originals.read_original(&id,usize::try_from(bytes).map_err(|_|bad())?,root.ctx.values.memory).map_err(|e|RootModuleExecutionErrorV1::Invocation(ModuleInvocationPreparationErrorV1::Original(oasis7_world_state::incremental_runtime_artifact_content::RuntimeArtifactOriginalErrorV1::Source(e))))?;
                if !raw.uses_budget(root.ctx.values.memory)
                    || raw.reserved_bytes() < raw.capacity()
                    || raw.len() as u64 != bytes
                {
                    return Err(bad().into());
                }
                Ok(raw)
            })?;
            io::paid(self.ctx, self.members, work, common, |ctx, write, w| {
                let mut read = WorldFieldReadContextV1 {
                    root: roots.state_root(StateNamespaceV1::Artifacts),
                    state: self.state,
                    values: ValueReadContext {
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
                journal
                    .append(
                        &raw,
                        id,
                        FactLocationV1 {
                            original: id,
                            height: roots.height.checked_add(1).ok_or_else(bad)?,
                            ordinal,
                        },
                        ctx.transaction,
                        &mut read,
                        write,
                        &mut w.index,
                        &mut w.cbor,
                        &mut w.hash,
                        &live_owner(),
                    )
                    .map_err(formal)
            })
            .map_err(RootModuleExecutionErrorV1::Body)?;
            Ok(oasis7_agent_api::iws_scheduler_v2::IwsJournalReferenceV2 {
                journal_ordinal: journal.count().checked_sub(1).ok_or_else(bad)?,
                fact_height: roots.height.checked_add(1).ok_or_else(bad)?,
                fact_ordinal: ordinal,
                original: id,
                encoded_bytes: bytes,
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

fn work_limit(w: &CurrentCognitionExpiryWorkV1) -> usize {
    w.max_selected_heap
}
fn business_error<OE>(
    prefix: &str,
    code: &str,
    common: &mut FormalBoundedContextV1<'_>,
) -> RootModuleExecutionErrorV1<OE> {
    let Some(n) = prefix.len().checked_add(code.len()) else {
        return bad().into();
    };
    if let Err(e) = common.charge(1, n as u64) {
        return RootModuleExecutionErrorV1::Body(formal(e));
    }
    let fee = match common.memory().reserve(
        std::mem::size_of::<crate::WorldError>() + n,
        MemoryClass::Ordinary,
    ) {
        Ok(v) => v,
        Err(e) => return ValueTreeError::Memory(e).into(),
    };
    let mut text = String::with_capacity(n);
    text.push_str(prefix);
    text.push_str(code);
    RootModuleExecutionErrorV1::Business(
        fee.retain(crate::WorldError::DistributedValidationFailed { reason: text }),
    )
}
fn scheduler_error<OE>(
    e: owned::SchedulerOwnedErrorV1,
    common: &mut FormalBoundedContextV1<'_>,
) -> RootModuleExecutionErrorV1<OE> {
    match e {
        owned::SchedulerOwnedErrorV1::Body(BodyMutationError::Original(error)) => {
            if let Some(error) = error.downcast_ref::<crate::cognition_wake::WakeConditionError>() {
                business_error("cognition validation failed: ", error.code(), common)
            } else {
                RootModuleExecutionErrorV1::Body(BodyMutationError::Original(error))
            }
        }
        owned::SchedulerOwnedErrorV1::Body(e) => RootModuleExecutionErrorV1::Body(e),
        owned::SchedulerOwnedErrorV1::Scheduler(e) => {
            business_error("cognition scheduler failed: ", e.code(), common)
        }
    }
}

#[cfg(test)]
#[path = "incremental_normal_cognition_expiry_tests.rs"]
mod tests;

pub(in crate::world) use io::paid as with_current_cognition_io_v1;

#[path = "incremental_normal_cognition_disabled.rs"]
mod disabled;
pub(in crate::world) use disabled::{
    NormalCognitionExpirySourceV1, PaidDisabledNormalCognitionSourceV1,
};
