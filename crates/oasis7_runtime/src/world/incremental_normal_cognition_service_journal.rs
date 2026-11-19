//! Actual fixed service Journal production and final Current control CAS.
//! The expiry frontier moves through this phase; no historical writer or
//! caller original/locator is accepted as the appended event.
use super::*;
use crate::world::incremental_normal_cognition_expiry::{event, with_current_cognition_io_v1};
use oasis7_formal_protocol::incremental_execution::ExecutionFactKindV1;
use oasis7_world_state::incremental_cognition_control_overlay::{
    CognitionControlValueV1, change_cognition_control_v1, check_cognition_control_overlay_v1,
};
use oasis7_world_state::incremental_cognition_fields::CognitionControlFieldV1;
use oasis7_world_state::incremental_field_overlay::WorldFieldOverlayErrorV1;

pub(super) struct WrittenCurrentCognitionServiceV2 {
    _prepared: commit::PreparedCurrentCognitionServiceStateV2,
    guard: PreparedGuard,
    identity: WorkIdentityV1,
    first_fact: usize,
    fact_count: usize,
    last: Option<(u64, [u8; 32])>,
    sequence: u64,
    _digest: ReservedValue<String>,
    _fee: ReservedValue<()>,
}
impl WrittenCurrentCognitionServiceV2 {
    pub(super) fn validate_evidence<M: MerkleNodeReader>(
        &self,
        tx: &ExecutionTransaction<'_, M>,
        common: &mut FormalBoundedContextV1<'_>,
        live: &ExecutionFence,
    ) -> Result<(), BodyMutationError> {
        common.charge(1, 128).map_err(formal)?;
        tx.validate_transaction_identity(&self.guard)?;
        tx.validate_guard(&tx.guard(), live)?;
        if self.identity != common.work().identity()
            || !self._fee.uses_budget(common.memory())
            || !self._digest.uses_budget(common.memory())
            || !tx.uses_budget(common.memory())
            || self
                .first_fact
                .checked_add(self.fact_count)
                .is_none_or(|n| n > tx.ordered_fact_count())
        {
            return Err(bad().into());
        }
        if let Some((ordinal, id)) = self.last {
            if !tx
                .ordered_fact(usize::try_from(ordinal).map_err(|_| bad())?)
                .is_some_and(|fact| fact.kind == ExecutionFactKindV1::Event && fact.id == id)
            {
                return Err(bad().into());
            }
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
    pub(super) fn begin_current_cognition_service_journal_v2<'p>(
        &mut self,
        sources: &CurrentCognitionServiceSourcesV2<'_, 'p>,
        work: &mut CurrentCognitionServiceWorkV2<'_>,
        common: &mut FormalBoundedContextV1<'_>,
        live_owner: &mut dyn FnMut() -> ExecutionFence,
    ) -> Result<PendingCurrentCognitionJournalV1<'p>, RootModuleExecutionErrorV1<O::Error>> {
        sources
            .parent
            .validate_context(common)
            .map_err(|e| RootModuleExecutionErrorV1::Body(formal(e)))?;
        with_current_cognition_io_v1(self.ctx, self.members, work.expiry, common, |ctx, _, w| {
            PendingCurrentCognitionJournalV1::from_accepted_at_head_v1(
                sources.parent.refs().cognition,
                sources.parent.refs().head,
                ctx.transaction,
                ctx.values.memory,
                &mut w.hash,
                &live_owner(),
                &mut ctx.cancelled,
            )
            .map_err(BodyMutationError::from)
        })
        .map_err(RootModuleExecutionErrorV1::Body)
    }
    #[allow(clippy::too_many_arguments)]
    pub(super) fn append_current_cognition_service_journal_v2<P, PE, PC>(
        &mut self,
        sources: &CurrentCognitionServiceSourcesV2<'_, '_>,
        plan: &selection::CurrentCognitionSelectionPlanV2,
        prepared: commit::PreparedCurrentCognitionServiceStateV2,
        journal: &mut PendingCurrentCognitionJournalV1<'_>,
        work: &mut CurrentCognitionServiceWorkV2<'_>,
        _parent_read: &mut WorldFieldReadContextV1<'_, P, R, PE, PC>,
        common: &mut FormalBoundedContextV1<'_>,
        live_owner: &mut dyn FnMut() -> ExecutionFence,
    ) -> Result<WrittenCurrentCognitionServiceV2, RootModuleExecutionErrorV1<O::Error>>
    where
        P: MerkleNodeReader,
        PE: FnMut() -> Option<ValueTreeError>,
        PC: FnMut() -> bool,
    {
        prepared
            .validate_before_append(self.ctx.transaction, common, self.now, &live_owner())
            .map_err(RootModuleExecutionErrorV1::Body)?;
        journal.validate_evidence(self.ctx.transaction, common.memory(), &live_owner())?;
        let fee = common
            .memory()
            .reserve(
                std::mem::size_of::<WrittenCurrentCognitionServiceV2>(),
                MemoryClass::Ordinary,
            )
            .map_err(ValueTreeError::Memory)?
            .retain(());
        let old_hash = with_current_cognition_io_v1(
            self.ctx,
            self.members,
            work.expiry,
            common,
            |ctx, _, w| {
                journal
                    .current_digest(ctx.values.memory, &mut w.hash, &mut ctx.cancelled)
                    .map_err(BodyMutationError::from)
            },
        )
        .map_err(RootModuleExecutionErrorV1::Body)?;
        let old_digest =
            event::head_text(old_hash, common).map_err(RootModuleExecutionErrorV1::Body)?;
        common
            .charge(1, (old_digest.len() + prepared.old_digest().len()) as u64)
            .map_err(|e| RootModuleExecutionErrorV1::Body(formal(e)))?;
        if journal.sequence() != prepared.old_sequence()
            || old_digest.as_str() != prepared.old_digest()
        {
            return Err(bad().into());
        }
        with_current_cognition_io_v1(self.ctx, self.members, work.expiry, common, |ctx, _, _| {
            check_cognition_control_overlay_v1(
                CognitionControlFieldV1::JournalHeadSeq,
                CognitionControlValueV1::Unsigned(prepared.old_sequence()),
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
        let first_fact = self.ctx.transaction.ordered_fact_count();
        let mut last = None;
        for row in plan.journal_events() {
            let (kind, wake) = match row {
                selection::CurrentServiceJournalEventV2::Deactivated(wake) => {
                    ("SchedulerWakeDeactivated", Some(wake))
                }
                selection::CurrentServiceJournalEventV2::Selected(wake) => {
                    ("SchedulerWakeSelected", wake)
                }
                selection::CurrentServiceJournalEventV2::Woken(wake) => {
                    ("ContinuationWoken", Some(wake))
                }
            };
            let event = event::prepare_scheduler_v2(
                kind,
                plan.scheduler(),
                wake,
                plan.binding(),
                journal.sequence(),
                journal.last_digest(),
                work.expiry,
                common,
            )
            .map_err(RootModuleExecutionErrorV1::Body)?;
            let appended = self.append_current_cognition_journal_original_v2(
                sources.parent.refs().roots,
                &event.value.value,
                journal,
                work.expiry,
                common,
                live_owner,
            )?;
            last = Some((appended.fact_ordinal, appended.original));
        }
        let hash = with_current_cognition_io_v1(
            self.ctx,
            self.members,
            work.expiry,
            common,
            |ctx, _, w| {
                journal
                    .current_digest(ctx.values.memory, &mut w.hash, &mut ctx.cancelled)
                    .map_err(BodyMutationError::from)
            },
        )
        .map_err(RootModuleExecutionErrorV1::Body)?;
        let digest = event::head_text(hash, common).map_err(RootModuleExecutionErrorV1::Body)?;
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
                        CognitionControlValueV1::Unsigned(prepared.old_sequence()),
                        CognitionControlValueV1::Unsigned(journal.sequence()),
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
                .map_err(overlay)?;
                Ok(())
            },
        )
        .map_err(RootModuleExecutionErrorV1::Body)?;
        let fact_count = self
            .ctx
            .transaction
            .ordered_fact_count()
            .checked_sub(first_fact)
            .ok_or_else(bad)?;
        let written = WrittenCurrentCognitionServiceV2 {
            _prepared: prepared,
            guard: self.ctx.transaction.guard(),
            identity: common.work().identity(),
            first_fact,
            fact_count,
            last,
            sequence: journal.sequence(),
            _digest: digest,
            _fee: fee,
        };
        written
            .validate_evidence(self.ctx.transaction, common, &live_owner())
            .map_err(RootModuleExecutionErrorV1::Body)?;
        Ok(written)
    }
}
