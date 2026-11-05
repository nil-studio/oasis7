//! Authenticated selection of a pending action in the sole execution overlay.
use super::incremental_body::BodyMutationError;
use oasis7_formal_protocol::{
    incremental_execution::{ExecutionFence, ExecutionTransaction},
    incremental_material::OriginalObjectStreamReader,
    incremental_state::MerkleNodeReader,
    incremental_value::{ValueNodeReader, ValueTreeError},
};
use oasis7_proto::memory_budget::ReservedValue;
use oasis7_world_state::{
    incremental_field_read::WorldFieldReadContextV1,
    incremental_pending_action_family::PendingActionFamilyWorkV1,
    incremental_pending_action_original_binding::verify_pending_action_original_v1,
    incremental_pending_action_overlay::{
        PendingActionCurrentV1, peek_pending_action_v1, pop_pending_action_v1,
    },
    incremental_runtime_pending_action_record::{
        RecoveredPendingActionRecordV1, recover_current_pending_action_record_v1,
    },
};

fn overlay_error(
    error: oasis7_world_state::incremental_field_overlay::WorldFieldOverlayErrorV1,
) -> BodyMutationError {
    use oasis7_world_state::incremental_field_overlay::WorldFieldOverlayErrorV1 as Overlay;
    match error {
        Overlay::Execution(error) => BodyMutationError::Execution(error),
        Overlay::Value(error) => BodyMutationError::Value(error),
        Overlay::Member(error) => BodyMutationError::Original(Box::new(error)),
    }
}

/// Select and remove exactly the authenticated FIFO head. Recovery and original
/// verification finish before the record can reach business dispatch. Any error
/// poisons the same transaction, including errors after provisional queue edits.
#[allow(clippy::too_many_arguments)]
pub(super) fn take_pending_action<M, R, V, E, C, O>(
    current: &mut ReservedValue<PendingActionCurrentV1>,
    tx: &mut ExecutionTransaction<'_, M>,
    read: &mut WorldFieldReadContextV1<'_, R, V, E, C>,
    work: &mut PendingActionFamilyWorkV1,
    originals: &mut O,
    live: &ExecutionFence,
) -> Result<Option<SelectedPendingActionV1>, BodyMutationError>
where
    M: MerkleNodeReader,
    R: MerkleNodeReader,
    V: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    O: OriginalObjectStreamReader,
    O::Error: std::error::Error + Send + Sync + 'static,
{
    tx.contribute(|tx| {
        let selected = peek_pending_action_v1(
            current,
            tx,
            read.values.memory,
            live,
            &mut work.index,
            &mut read.cancelled,
        )
        .map_err(overlay_error)?;
        let Some(selected) = selected else {
            return Ok(None);
        };
        let record = recover_current_pending_action_record_v1(
            selected.position,
            tx,
            live,
            read,
            &mut work.selected_scan,
            &mut work.typed,
        )
        .map_err(overlay_error)?
        .ok_or(BodyMutationError::Invalid(
            "pending action selected record absent",
        ))?;
        let checked = verify_pending_action_original_v1(
            &record,
            originals,
            read.values.memory,
            &mut work.original,
            &mut read.cancelled,
        )
        .map_err(|error| {
            use oasis7_world_state::incremental_pending_action_original_binding::PendingActionOriginalErrorV1 as Original;
            use oasis7_wasm_abi::CanonicalCborStreamError as Encoding;
            match error {
                Original::Value(error) => BodyMutationError::Value(error),
                Original::Encoding(Encoding::Cancelled) => BodyMutationError::Value(ValueTreeError::Cancelled),
                Original::Encoding(Encoding::ResourceLimited(reason)) => BodyMutationError::Value(ValueTreeError::ResourceLimited(reason)),
                Original::Encoding(error) => BodyMutationError::Original(Box::new(error)),
                Original::Source(error) => BodyMutationError::Original(Box::new(error)),
                Original::Invalid(reason) => BodyMutationError::Invalid(reason),
            }
        })?;
        if checked.original() != selected.original
            || record.value().envelope().id != selected.action_id
        {
            return Err(BodyMutationError::Invalid(
                "pending action selected original binding",
            ));
        }
        let guard = tx.guard();
        let removed = pop_pending_action_v1(
            current,
            tx,
            read.values.memory,
            &guard,
            live,
            &mut work.index,
            read.key_work,
            &mut read.cancelled,
        )
        .map_err(overlay_error)?
        .ok_or(BodyMutationError::Invalid(
            "pending action head disappeared",
        ))?;
        if removed.position != selected.position || removed.value_root != selected.value_root {
            return Err(BodyMutationError::Invalid("pending action head changed"));
        }
        tx.append_fact(
            oasis7_formal_protocol::incremental_execution::ExecutionFactRefV1 {
                kind: oasis7_formal_protocol::incremental_execution::ExecutionFactKindV1::Input,
                id: checked.original().id,
            },
        )?;
        Ok(Some(SelectedPendingActionV1 {
            record,
            identity: tx.guard(),
        }))
    })
}

/// Only the authenticated FIFO consumer can construct this source capability.
/// It proves the queued original, never permission to execute its business.
pub(super) struct SelectedPendingActionV1 {
    record: RecoveredPendingActionRecordV1,
    identity: oasis7_formal_protocol::incremental_execution::PreparedGuard,
}
impl SelectedPendingActionV1 {
    pub(super) fn validate<R: MerkleNodeReader>(
        &self,
        tx: &ExecutionTransaction<'_, R>,
    ) -> Result<(), oasis7_formal_protocol::incremental_execution::ExecutionError> {
        tx.validate_transaction_identity(&self.identity)
    }

    pub(super) fn envelope(&self) -> &oasis7_world_state::ActionEnvelope {
        self.record.value().envelope()
    }
}

impl<R, V, W, C> super::incremental_step::IwsStepContext<'_, '_, '_, '_, R, V, W, C>
where
    R: MerkleNodeReader,
    V: ValueNodeReader,
    W: oasis7_formal_protocol::incremental_value::ValueNodeWriter,
    C: FnMut() -> bool,
{
    /// Persist a rejection correlated to a genuinely selected queued original.
    /// Rejection has no mailbox recipient in the original event routing contract.
    pub(super) fn append_selected_rejection<F>(
        &mut self,
        selected: &SelectedPendingActionV1,
        reason: crate::RejectReason,
        max_event_bytes: usize,
        mut persist_original: F,
    ) -> Result<(), BodyMutationError>
    where
        F: FnMut(
            &mut dyn super::incremental_body_event::AgentEventOriginalV1,
        ) -> Result<(), BodyMutationError>,
    {
        use super::incremental_allocators::RuntimeAllocatorV1;
        use super::incremental_body_event::original;
        use oasis7_formal_protocol::incremental_execution::{
            ExecutionFactKindV1, ExecutionFactRefV1,
        };
        let before = self.begin_world_event()?;
        let time = self.time();
        let sequence = self.read_allocator(RuntimeAllocatorV1::Event)?;
        let (id, _) = self.allocate(RuntimeAllocatorV1::Event, sequence)?;
        let event = crate::WorldEvent {
            id,
            time,
            caused_by: Some(crate::CausedBy::Action(selected.envelope().id)),
            body: crate::WorldEventBody::Domain(crate::DomainEvent::ActionRejected {
                action_id: selected.envelope().id,
                reason,
            }),
        };
        self.transaction
            .contribute(|tx| -> Result<(), BodyMutationError> {
                let measured = original::measure(
                    &event,
                    self.mutation.memory,
                    self.original_work,
                    &mut self.mutation.cancelled,
                    max_event_bytes,
                )?;
                let id = measured.id();
                let mut producer = original::Producer::new(
                    measured,
                    self.mutation.memory,
                    self.original_work,
                    &mut self.mutation.cancelled,
                );
                persist_original(&mut producer)?;
                producer.check_complete()?;
                drop(producer);
                if (self.mutation.cancelled)() {
                    return Err(ValueTreeError::Cancelled.into());
                }
                tx.append_fact(ExecutionFactRefV1 {
                    kind: ExecutionFactKindV1::Event,
                    id,
                })?;
                Ok(())
            })?;
        self.finish_world_event(before)
    }
}
