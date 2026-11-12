//! Exact outcome projection and Current member CAS. This is an intermediate
//! owner: the fixed driver must still publish OutcomeRecorded and recompute the
//! original authorization root before it can finish the transaction.
use super::*;
use crate::world::incremental_module_execution_event::PersistedWorldEventV1;
use crate::world::incremental_safe_alternative_outcome::{
    PreparedSafeAlternativeOutcomeDataV1, prepare_outcome_data_v1,
};
use oasis7_formal_protocol::{
    incremental_state::{StatePageWriter, StateScanBudget},
    incremental_value::{ValueNodeReader, ValueNodeWriter, ValueTreeError},
};
use oasis7_world_state::{
    incremental_field_member::MemberRecordWriteContext,
    incremental_field_overlay::{WorldFieldOverlayContextV1, WorldFieldOverlayErrorV1},
    incremental_field_read::WorldFieldReadContextV1,
    incremental_runtime_revocation::{
        RevocationReadWorkV1, RevocationWriteWorkV1, RuntimeRevocationCurrentV1,
        RuntimeRevocationQualificationV1, SafeAlternativeRecordWriteReceiptV1,
        replace_current_safe_alternative_record_with_receipt_v1,
    },
};

pub(in crate::world) enum SafeAlternativeResultOwnerV1<'selected, 'source> {
    Matched(PaidMatchingSafeAlternativeV1<'selected, 'source>),
    Rejected(super::rejection::CompletedSafeAlternativeRejectionV1<'selected, 'source>),
}
impl SafeAlternativeResultOwnerV1<'_, '_> {
    fn input(&self) -> &PaidSignedSafeAlternativeRecordV1<'_, '_> {
        match self {
            Self::Matched(m) => &m.input,
            Self::Rejected(r) => &r.input,
        }
    }
}

pub(in crate::world) struct AppliedSafeAlternativeOutcomeV1<'selected, 'source> {
    input: SafeAlternativeResultOwnerV1<'selected, 'source>,
    outcome: PreparedSafeAlternativeOutcomeDataV1,
    receipt: ReservedValue<SafeAlternativeRecordWriteReceiptV1>,
    guard: PreparedGuard,
    _outer: ReservedValue<()>,
}
impl AppliedSafeAlternativeOutcomeV1<'_, '_> {
    pub(in crate::world) fn key(&self) -> &str {
        self.input.input().record.lineage()
    }
    pub(in crate::world) fn action_id(&self) -> u64 {
        self.outcome.action_id()
    }
    pub(in crate::world) fn result_event_id(&self) -> u64 {
        self.outcome.event_id()
    }
    pub(in crate::world) fn validate<M: MerkleNodeReader>(
        &self,
        tx: &ExecutionTransaction<'_, M>,
        common: &mut FormalBoundedContextV1<'_>,
        live: &ExecutionFence,
    ) -> Result<(), BodyMutationError> {
        self.input.input().validate_after_result(tx, common)?;
        if !self._outer.uses_budget(common.memory()) {
            return Err(BodyMutationError::Invalid(
                "SafeAlternative outcome owner fee",
            ));
        }
        self.outcome.validate(tx, common)?;
        self.receipt
            .validate_transaction(tx)
            .map_err(overlay_error)?;
        if !self.receipt.uses_budget(common.memory()) {
            return Err(BodyMutationError::Invalid(
                "SafeAlternative CAS receipt pool",
            ));
        }
        tx.validate_guard(&self.guard, live)?;
        Ok(())
    }
}
fn overlay_error(e: WorldFieldOverlayErrorV1) -> BodyMutationError {
    match e {
        WorldFieldOverlayErrorV1::Execution(e) => BodyMutationError::Execution(e),
        WorldFieldOverlayErrorV1::Value(e) => BodyMutationError::Value(e),
        WorldFieldOverlayErrorV1::Member(e) => BodyMutationError::Original(Box::new(e)),
    }
}

#[allow(clippy::too_many_arguments)]
pub(in crate::world) fn apply_safe_alternative_outcome_v1<
    'selected,
    'source,
    M,
    R,
    E,
    C,
    P,
    V,
    ER,
    CR,
    W,
    D,
>(
    owner: SafeAlternativeResultOwnerV1<'selected, 'source>,
    selected: &SelectedPendingActionV1,
    event: Option<&PersistedWorldEventV1<'_>>,
    qualification: &ReservedValue<RuntimeRevocationQualificationV1>,
    current: &mut RuntimeRevocationCurrentV1,
    overlay: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    read: &mut WorldFieldReadContextV1<'_, P, V, ER, CR>,
    scan: &mut StateScanBudget,
    work: &mut RevocationReadWorkV1,
    write: &mut MemberRecordWriteContext<'_, W, D>,
    write_work: &mut RevocationWriteWorkV1,
    common: &mut FormalBoundedContextV1<'_>,
    live: &ExecutionFence,
) -> Result<AppliedSafeAlternativeOutcomeV1<'selected, 'source>, BodyMutationError>
where
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    P: MerkleNodeReader,
    V: ValueNodeReader,
    ER: FnMut() -> Option<ValueTreeError>,
    CR: FnMut() -> bool,
    W: ValueNodeReader + ValueNodeWriter + StatePageWriter,
    D: FnMut() -> bool,
{
    let result = (|| {
        let input = owner.input();
        input.validate_after_result(overlay.transaction, common)?;
        selected.validate(overlay.transaction)?;
        let rejected_event = match &owner {
            SafeAlternativeResultOwnerV1::Rejected(rejected) => {
                Some(PersistedWorldEventV1::from_safe_alternative_rejection(
                    rejected,
                    overlay.transaction,
                )?)
            }
            SafeAlternativeResultOwnerV1::Matched(_) => None,
        };
        let event = rejected_event
            .as_ref()
            .or(event)
            .ok_or(BodyMutationError::Invalid(
                "SafeAlternative Core result missing",
            ))?;
        event.validate(overlay.transaction)?;
        if !std::ptr::eq(input.selected, selected)
            || !overlay.values.memory.same_pool(common.memory())
            || !read.values.memory.same_pool(common.memory())
            || !write.memory.same_pool(common.memory())
        {
            return Err(BodyMutationError::Invalid(
                "SafeAlternative outcome source binding",
            ));
        }
        let outer = common
            .memory()
            .reserve(
                std::mem::size_of::<AppliedSafeAlternativeOutcomeV1<'selected, 'source>>(),
                oasis7_proto::memory_budget::MemoryClass::Ordinary,
            )
            .map_err(ValueTreeError::Memory)?;
        let outcome = prepare_outcome_data_v1(
            input.record.record(),
            selected,
            event,
            overlay.transaction,
            common,
        )?;
        // The exclusive child is completed below on both success and returned
        // errors. Every legacy lane is lowered, never raised or replenished.
        let (_, meter, stop) = common.parts();
        let child = meter
            .begin_child()
            .map_err(|e| BodyMutationError::Original(Box::new(e)))?;
        let visits = child.remaining_steps() / 10;
        let bytes = child.remaining_read_bytes() / 8;
        macro_rules! lanes {
            ($m:ident) => {
                $m!(
                    work.json.cells_remaining,
                    work.json.json_nodes_remaining,
                    scan.nodes_remaining,
                    scan.leaves_remaining,
                    write_work.cells_remaining,
                    write_work.json.cells_remaining,
                    write_work.json.json_nodes_remaining,
                    overlay.values.io.nodes_remaining,
                    read.values.io.nodes_remaining,
                    write.io.nodes_remaining
                )
            };
        }
        macro_rules! byte_lanes {
            ($m:ident) => {
                $m!(
                    scan.read_bytes_remaining,
                    overlay.values.io.read_bytes_remaining,
                    read.values.io.read_bytes_remaining,
                    write.io.read_bytes_remaining,
                    write.io.write_bytes_remaining,
                    overlay.key_work.bytes_remaining,
                    read.key_work.bytes_remaining,
                    write.key_work.bytes_remaining
                )
            };
        }
        macro_rules! clamp_steps {($($lane:expr),*)=>{$($lane=$lane.min(visits);)*};}
        macro_rules! clamp_bytes {($($lane:expr),*)=>{$($lane=$lane.min(bytes);)*};}
        macro_rules! snapshot {($($lane:expr),*)=>{[$($lane),*]};}
        lanes!(clamp_steps);
        byte_lanes!(clamp_bytes);
        let before_steps = lanes!(snapshot);
        let before_bytes = byte_lanes!(snapshot);
        let changed = {
            let stop = std::cell::RefCell::new(stop);
            let mut current_context = WorldFieldOverlayContextV1 {
                transaction: &mut *overlay.transaction,
                values: oasis7_formal_protocol::incremental_value::ValueReadContext {
                    source: overlay.values.source,
                    memory: overlay.values.memory,
                    class: overlay.values.class,
                    io: &mut *overlay.values.io,
                },
                key_work: &mut *overlay.key_work,
                semantic_bytes: overlay.semantic_bytes,
                record_bytes: overlay.record_bytes,
                take_state_error: &mut overlay.take_state_error,
                cancelled: || (overlay.cancelled)() || (stop.borrow_mut())(),
            };
            let mut parent_context = WorldFieldReadContextV1 {
                root: read.root,
                state: read.state,
                values: oasis7_formal_protocol::incremental_value::ValueReadContext {
                    source: read.values.source,
                    memory: read.values.memory,
                    class: read.values.class,
                    io: &mut *read.values.io,
                },
                key_work: &mut *read.key_work,
                semantic_bytes: read.semantic_bytes,
                record_bytes: read.record_bytes,
                take_state_error: &mut read.take_state_error,
                cancelled: || (read.cancelled)() || (stop.borrow_mut())(),
            };
            let mut writes = MemberRecordWriteContext {
                sink: &mut *write.sink,
                memory: write.memory,
                io: &mut *write.io,
                key_work: &mut *write.key_work,
                limits: write.limits,
                cancelled: || (write.cancelled)() || (stop.borrow_mut())(),
            };
            replace_current_safe_alternative_record_with_receipt_v1(
                &input.record,
                outcome.record(),
                qualification,
                current,
                &mut current_context,
                &mut parent_context,
                scan,
                work,
                &mut writes,
                write_work,
                live,
            )
            .map_err(overlay_error)
        };
        let after_steps = lanes!(snapshot);
        let after_bytes = byte_lanes!(snapshot);
        let debit = |before: &[u64], after: &[u64]| {
            before
                .iter()
                .zip(after)
                .try_fold(0u64, |n, (b, a)| n.checked_add(b.checked_sub(*a)?))
        };
        let Some(steps) = debit(&before_steps, &after_steps) else {
            return changed.and(Err(BodyMutationError::Invalid(
                "SafeAlternative CAS visits accounting",
            )));
        };
        let Some(bytes) = debit(&before_bytes, &after_bytes) else {
            return changed.and(Err(BodyMutationError::Invalid(
                "SafeAlternative CAS bytes accounting",
            )));
        };
        let paid = child.consume(steps, bytes);
        child.finish();
        let (_, receipt) = changed?;
        paid.map_err(|e| BodyMutationError::Original(Box::new(e)))?;
        common
            .consume_steps(0)
            .map_err(|e| BodyMutationError::Original(Box::new(e)))?;
        Ok(AppliedSafeAlternativeOutcomeV1 {
            input: owner,
            outcome,
            receipt,
            guard: overlay.transaction.guard(),
            _outer: outer.retain(()),
        })
    })();
    if result.is_err() {
        let _: Result<(), oasis7_formal_protocol::incremental_execution::ExecutionError> =
            overlay.transaction.contribute(|_| {
                Err(oasis7_formal_protocol::incremental_execution::ExecutionError::Aborted)
            });
    }
    result
}

#[path = "incremental_safe_alternative_publication.rs"]
mod publication;
pub(in crate::world) use publication::PreparedSafeAlternativeOutcomeRecordedV1;
