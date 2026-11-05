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

/// Private runtime owner minted only by an actual qualified Current query.
pub(super) struct AcceptedActorScopeV1<'actor> {
    actor: &'actor str,
    identity: oasis7_formal_protocol::incremental_execution::PreparedGuard,
    existing: bool,
}
impl AcceptedActorScopeV1<'_> {
    pub(super) fn validate<M: MerkleNodeReader>(
        &self,
        tx: &ExecutionTransaction<'_, M>,
        actor: &str,
    ) -> Result<(), BodyMutationError> {
        tx.validate_transaction_identity(&self.identity)?;
        if actor != self.actor {
            return Err(BodyMutationError::Invalid("accepted actor scope binding"));
        }
        Ok(())
    }
    pub(super) fn existing(&self) -> bool {
        self.existing
    }
}

pub(super) struct AcceptedActorReadContextV1<
    'a,
    'cur,
    'parent,
    P: MerkleNodeReader,
    V: ValueNodeReader,
    E,
    D,
    PE,
    PD,
> {
    pub roots: &'a oasis7_formal_protocol::incremental_execution::RootSetV1,
    pub agents: &'a ReservedValue<
        oasis7_world_state::incremental_agent_subtree::AgentSubtreeStructureQualificationV1,
    >,
    pub current: &'a mut WorldFieldReadContextV1<'cur, P, V, E, D>,
    pub parent: &'a mut WorldFieldReadContextV1<'parent, P, V, PE, PD>,
    pub pending_entries: &'a mut usize,
    pub live: &'a ExecutionFence,
}

/// Owns the exact audit original only after persistence and counted fact success.
/// Fields and construction remain private to the authenticated action consumer.
pub(super) struct SelectedActionAuditV1 {
    event: ReservedValue<crate::WorldEvent>,
    identity: oasis7_formal_protocol::incremental_execution::PreparedGuard,
}
impl SelectedActionAuditV1 {
    pub(super) fn event(&self) -> &crate::WorldEvent {
        &self.event
    }
    pub(super) fn validate<M: MerkleNodeReader>(
        &self,
        tx: &ExecutionTransaction<'_, M>,
    ) -> Result<(), oasis7_formal_protocol::incremental_execution::ExecutionError> {
        tx.validate_transaction_identity(&self.identity)
    }
}

impl<R, V, W, C> super::incremental_step::IwsStepContext<'_, '_, '_, '_, R, V, W, C>
where
    R: MerkleNodeReader,
    V: ValueNodeReader,
    W: oasis7_formal_protocol::incremental_value::ValueNodeWriter,
    C: FnMut() -> bool,
{
    fn retain_audit_event(
        &mut self,
        event: crate::WorldEvent,
        audit_work: &mut oasis7_world_state::incremental_world_event_decode::WorldEventDecodeWorkV1,
    ) -> Result<ReservedValue<crate::WorldEvent>, BodyMutationError> {
        self.transaction.contribute(|tx| {
            if !tx.uses_budget(self.mutation.memory) {
                return Err(BodyMutationError::Invalid("selected audit memory pool"));
            }
            oasis7_world_state::incremental_world_event_owned::retain_world_event_owned_v1(
                event,
                self.mutation.memory,
                oasis7_proto::memory_budget::MemoryClass::Ordinary,
                audit_work,
                &mut self.mutation.cancelled,
            )
            .map_err(BodyMutationError::Value)
        })
    }

    /// Emit the original selective Accepted audit and its actual actor mailbox.
    pub(super) fn append_selected_accepted<P, E, D, PE, PD, F>(
        &mut self,
        selected: &SelectedPendingActionV1,
        actor_read: &mut AcceptedActorReadContextV1<'_, '_, '_, P, V, E, D, PE, PD>,
        max_event_bytes: usize,
        audit_work: &mut oasis7_world_state::incremental_world_event_decode::WorldEventDecodeWorkV1,
        persist_original: F,
    ) -> Result<Option<SelectedActionAuditV1>, BodyMutationError>
    where
        P: MerkleNodeReader,
        E: FnMut() -> Option<ValueTreeError>,
        D: FnMut() -> bool,
        PE: FnMut() -> Option<ValueTreeError>,
        PD: FnMut() -> bool,
        F: FnMut(
            &mut dyn super::incremental_body_event::AgentEventOriginalV1,
        ) -> Result<(), BodyMutationError>,
    {
        self.transaction
            .contribute(|tx| selected.validate(tx).map_err(BodyMutationError::from))?;
        let envelope = selected.envelope();
        if !super::World::should_emit_action_accepted(&envelope.action) {
            return Ok(None);
        }
        let actor = envelope.action.actor_id().unwrap_or("system");
        let presence = {
            let current = &mut *actor_read.current;
            let mut overlay =
                oasis7_world_state::incremental_field_overlay::WorldFieldOverlayContextV1 {
                    transaction: self.transaction,
                    values: oasis7_formal_protocol::incremental_value::ValueReadContext {
                        source: current.values.source,
                        memory: current.values.memory,
                        class: current.values.class,
                        io: &mut *current.values.io,
                    },
                    key_work: &mut *current.key_work,
                    semantic_bytes: current.semantic_bytes,
                    record_bytes: current.record_bytes,
                    take_state_error: &mut current.take_state_error,
                    cancelled: &mut current.cancelled,
                };
            let guard = overlay.transaction.guard();
            oasis7_world_state::incremental_agent_subtree::read_current_agent_presence_v1(
                actor,
                actor_read.agents,
                actor_read.roots,
                &mut overlay,
                actor_read.parent,
                actor_read.pending_entries,
                &guard,
                actor_read.live,
            )
            .map_err(overlay_error)?
        };
        let scope = AcceptedActorScopeV1 {
            actor,
            identity: self.transaction.guard(),
            existing: presence
                == oasis7_world_state::incremental_agent_subtree::CurrentAgentPresenceV1::Existing,
        };
        let kind = super::module_runtime_labels::action_kind_label(&envelope.action);
        let fee = self.transaction.contribute(|_| {
            let bytes = std::mem::size_of::<crate::WorldEvent>()
                .checked_add(actor.len())
                .and_then(|n| n.checked_add(kind.len()))
                .and_then(|n| n.checked_add(1024))
                .ok_or(BodyMutationError::Invalid("selected accepted owner bound"))?;
            self.mutation
                .memory
                .reserve(bytes, oasis7_proto::memory_budget::MemoryClass::Ordinary)
                .map_err(|e| BodyMutationError::Value(ValueTreeError::Memory(e)))
        })?;
        let sequence =
            self.read_allocator(super::incremental_allocators::RuntimeAllocatorV1::Event)?;
        let (id, _, _) = super::World::preview_next_event_id(sequence.next, sequence.era);
        let event = crate::WorldEvent {
            id,
            time: self.time(),
            caused_by: Some(crate::CausedBy::Action(envelope.id)),
            body: crate::WorldEventBody::Domain(crate::DomainEvent::ActionAccepted {
                action_id: envelope.id,
                action_kind: kind.into(),
                actor_id: actor.into(),
                eta_ticks: 0,
                notes: vec!["accepted_for_gameplay_processing".into()],
            }),
        };
        let time = self.time();
        let before = self.begin_world_event()?;
        let placeholder = oasis7_wasm_abi::CanonicalCborWorkV1 {
            visits_remaining: 0,
            bytes_remaining: 0,
            max_depth: self.original_work.max_depth,
            max_map_keys: self.original_work.max_map_keys,
            max_key_bytes: self.original_work.max_key_bytes,
        };
        let mut context = super::incremental_body_event::AgentEventContext {
            values: self.values,
            mutation: super::incremental_body::BodyMutationContext {
                sink: &mut *self.mutation.sink,
                memory: self.mutation.memory,
                io: &mut *self.mutation.io,
                cancelled: &mut self.mutation.cancelled,
            },
            persist_original,
            max_event_bytes,
            original_work: std::mem::replace(self.original_work, placeholder),
        };
        let result = super::World::contribute_iws_accepted_event_at(
            time,
            &event,
            self.transaction,
            &mut context,
            &scope,
        );
        *self.original_work = context.original_work;
        result?;
        self.finish_world_event(before)?;
        let event = self.retain_audit_event(event, audit_work)?;
        drop(fee);
        Ok(Some(SelectedActionAuditV1 {
            event,
            identity: self.transaction.guard(),
        }))
    }

    /// Persist a rejection correlated to a genuinely selected queued original.
    /// Rejection has no mailbox recipient in the original event routing contract.
    pub(super) fn append_selected_rejection<F>(
        &mut self,
        selected: &SelectedPendingActionV1,
        reason: crate::RejectReason,
        max_event_bytes: usize,
        persist_original: F,
    ) -> Result<(), BodyMutationError>
    where
        F: FnMut(
            &mut dyn super::incremental_body_event::AgentEventOriginalV1,
        ) -> Result<(), BodyMutationError>,
    {
        self.append_selected_rejection_event(selected, reason, max_event_bytes, persist_original)
            .map(|_| ())
    }

    pub(super) fn append_selected_rejection_owned<F>(
        &mut self,
        selected: &SelectedPendingActionV1,
        reason: crate::RejectReason,
        max_event_bytes: usize,
        audit_work: &mut oasis7_world_state::incremental_world_event_decode::WorldEventDecodeWorkV1,
        persist_original: F,
    ) -> Result<SelectedActionAuditV1, BodyMutationError>
    where
        F: FnMut(
            &mut dyn super::incremental_body_event::AgentEventOriginalV1,
        ) -> Result<(), BodyMutationError>,
    {
        let event = self.append_selected_rejection_event(
            selected,
            reason,
            max_event_bytes,
            persist_original,
        )?;
        let event = self.retain_audit_event(event, audit_work)?;
        Ok(SelectedActionAuditV1 {
            event,
            identity: self.transaction.guard(),
        })
    }

    fn append_selected_rejection_event<F>(
        &mut self,
        selected: &SelectedPendingActionV1,
        reason: crate::RejectReason,
        max_event_bytes: usize,
        mut persist_original: F,
    ) -> Result<crate::WorldEvent, BodyMutationError>
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
        self.transaction
            .contribute(|tx| selected.validate(tx).map_err(BodyMutationError::from))?;
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
        self.finish_world_event(before)?;
        Ok(event)
    }
}
