//! Genuine selected transfer results in one transaction. A native TransferResource
//! still requires a Rule module. Successful EmitResourceTransfer is taken only
//! from the genuine effective Rule action; admission remains the driver's duty.
use crate::world::{
    World,
    incremental_action_execution::{SelectedActionAuditV1, SelectedPendingActionV1},
    incremental_body::BodyMutationError,
    incremental_body_event::AgentEventOriginalV1,
    incremental_step::IwsStepContext,
};
use oasis7_formal_protocol::{
    incremental_execution::{
        ExecutionError, ExecutionFence, ExecutionTransaction, PreparedGuard, RootSetV1,
    },
    incremental_state::MerkleNodeReader,
    incremental_value::{ValueNodeReader, ValueNodeWriter, ValueReadContext, ValueTreeError},
};
use oasis7_proto::memory_budget::{MemoryClass, MemoryReservation, ReservedValue};
use oasis7_world_state::{
    incremental_agent_fields::AgentFieldV1,
    incremental_agent_subtree::{
        AgentSubtreeQualifiedRequestV1, AgentSubtreeScalarRequestV1,
        AgentSubtreeStructureQualificationV1, AgentSubtreeValueV1, CurrentAgentPresenceV1,
        read_agent_subtree_scalar_overlay_v1, read_current_agent_presence_v1,
    },
    incremental_field_overlay::{WorldFieldOverlayContextV1, WorldFieldOverlayErrorV1},
    incremental_field_read::WorldFieldReadContextV1,
    incremental_world_event_decode::WorldEventDecodeWorkV1,
};

#[path = "incremental_action_transfer_resource_event.rs"]
mod event;

pub(in crate::world) struct TransferResourceWorkV1 {
    pub steps_remaining: u64,
    pub bytes_remaining: u64,
    pub pending_entries_remaining: usize,
    pub max_scalar_bytes: u64,
}
impl TransferResourceWorkV1 {
    fn spend(&mut self, bytes: usize) -> Result<(), BodyMutationError> {
        self.steps_remaining = self
            .steps_remaining
            .checked_sub(1)
            .ok_or(ValueTreeError::ResourceLimited("transfer resource steps"))?;
        self.bytes_remaining = self
            .bytes_remaining
            .checked_sub(
                u64::try_from(bytes)
                    .map_err(|_| ValueTreeError::ResourceLimited("transfer resource byte bound"))?,
            )
            .ok_or(ValueTreeError::ResourceLimited(
                "transfer resource byte work",
            ))?;
        Ok(())
    }
}

/// Only this module's selected-action reducer constructs a prepared owner.
/// Its fields bind the genuine selected/rule pair and the exact Current
/// resource values used to calculate the two checked resource writes.
pub(in crate::world) struct PreparedTransferResourceV1 {
    event: ReservedValue<crate::WorldEvent>,
    action_id: u64,
    now: u64,
    from_before: Option<i64>,
    to_before: Option<i64>,
    from_after: i64,
    to_after: i64,
    identity: PreparedGuard,
}
impl PreparedTransferResourceV1 {
    fn validate<M: MerkleNodeReader>(
        &self,
        tx: &ExecutionTransaction<'_, M>,
        memory: &oasis7_proto::memory_budget::MemoryBudget,
        now: u64,
    ) -> Result<(), BodyMutationError> {
        tx.validate_transaction_identity(&self.identity)?;
        if !tx.uses_budget(memory) || !self.event.uses_budget(memory) || self.now != now {
            return Err(BodyMutationError::Invalid("prepared transfer pool/time"));
        }
        let crate::WorldEventBody::Domain(crate::DomainEvent::ResourceTransferred {
            from_agent_id,
            to_agent_id,
            amount,
            kind,
        }) = &self.event.body
        else {
            return Err(BodyMutationError::Invalid("prepared transfer event"));
        };
        if self.event.time != now
            || self.event.caused_by != Some(crate::CausedBy::Action(self.action_id))
            || *amount <= 0
            || self.from_before.unwrap_or(0) < *amount
        {
            return Err(BodyMutationError::Invalid("prepared transfer binding"));
        }
        if from_agent_id != to_agent_id
            && (self.from_after
                != oasis7_agent_api::world_resource::checked_resource_remove_v1(
                    *kind,
                    self.from_before.unwrap_or(0),
                    *amount,
                )
                .map_err(|error| stock_error(error, "remove", memory))?
                || self.to_after
                    != oasis7_agent_api::world_resource::checked_resource_add_v1(
                        *kind,
                        self.to_before.unwrap_or(0),
                        *amount,
                    )
                    .map_err(|error| stock_error(error, "add", memory))?)
        {
            return Err(BodyMutationError::Invalid("prepared transfer balances"));
        }
        Ok(())
    }
    fn event(&self) -> &crate::WorldEvent {
        &self.event
    }
    fn route(
        &self,
    ) -> (
        &str,
        &str,
        oasis7_agent_api::world_action::ResourceKind,
        i64,
    ) {
        let crate::WorldEventBody::Domain(crate::DomainEvent::ResourceTransferred {
            from_agent_id,
            to_agent_id,
            kind,
            amount,
        }) = &self.event.body
        else {
            unreachable!("validated transfer event")
        };
        (from_agent_id, to_agent_id, *kind, *amount)
    }
}
/// Retains the original typed state error and its closed bounded text allocation.
/// Formatting this error does not reinterpret an overflow as an action rejection.
#[derive(Debug)]
pub(in crate::world) struct TransferResourceStateErrorV1 {
    pub(in crate::world) cause: crate::WorldError,
    _fee: MemoryReservation,
}
impl std::fmt::Display for TransferResourceStateErrorV1 {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "{:?}", self.cause)
    }
}
impl std::error::Error for TransferResourceStateErrorV1 {}
fn stock_error(
    error: oasis7_agent_api::world_resource::StockError,
    operation: &str,
    memory: &oasis7_proto::memory_budget::MemoryBudget,
) -> BodyMutationError {
    // StockError is closed: one finite resource tag and at most three i64
    // decimal fields. All original reason variants fit this 256-byte buffer.
    let fee = match memory.reserve(
        std::mem::size_of::<TransferResourceStateErrorV1>() + 256,
        MemoryClass::Ordinary,
    ) {
        Ok(fee) => fee,
        Err(error) => return BodyMutationError::Value(ValueTreeError::Memory(error)),
    };
    let reason = format!("transfer {operation} failed: {error:?}");
    if reason.capacity() > 256 {
        return BodyMutationError::Value(ValueTreeError::ResourceLimited(
            "transfer state error capacity",
        ));
    }
    BodyMutationError::Original(Box::new(TransferResourceStateErrorV1 {
        cause: crate::WorldError::ResourceBalanceInvalid { reason },
        _fee: fee,
    }))
}

/// Genuine result of the selected action's real business and event sinks.
/// The contained receipt is required before the common PostAction router can
/// consume the result event.
pub(in crate::world) struct CompletedTransferResourceV1 {
    owner: CompletedTransferResourceOwnerV1,
    _fee: MemoryReservation,
}
enum CompletedTransferResourceOwnerV1 {
    Rejection(SelectedActionAuditV1),
    Transferred {
        event: ReservedValue<crate::WorldEvent>,
        identity: PreparedGuard,
    },
}
impl CompletedTransferResourceV1 {
    pub(in crate::world) fn from_rejection<M: MerkleNodeReader>(
        owner: SelectedActionAuditV1,
        selected: &SelectedPendingActionV1,
        tx: &ExecutionTransaction<'_, M>,
        memory: &oasis7_proto::memory_budget::MemoryBudget,
    ) -> Result<Self, BodyMutationError> {
        selected.validate(tx)?;
        owner.validate(tx)?;
        if !tx.uses_budget(memory)
            || owner.event().caused_by != Some(crate::CausedBy::Action(selected.envelope().id))
            || !matches!(&owner.event().body,
                crate::WorldEventBody::Domain(crate::DomainEvent::ActionRejected { action_id, .. })
                if *action_id == selected.envelope().id)
        {
            return Err(BodyMutationError::Invalid(
                "TransferResource rejection receipt binding",
            ));
        }
        let fee = memory
            .reserve(std::mem::size_of::<Self>(), MemoryClass::Ordinary)
            .map_err(ExecutionError::Memory)?;
        Ok(Self {
            owner: CompletedTransferResourceOwnerV1::Rejection(owner),
            _fee: fee,
        })
    }
    pub(in crate::world) fn event(&self) -> &crate::WorldEvent {
        match &self.owner {
            CompletedTransferResourceOwnerV1::Rejection(owner) => owner.event(),
            CompletedTransferResourceOwnerV1::Transferred { event, .. } => event,
        }
    }

    pub(in crate::world) fn validate<M: MerkleNodeReader>(
        &self,
        tx: &ExecutionTransaction<'_, M>,
    ) -> Result<(), ExecutionError> {
        match &self.owner {
            CompletedTransferResourceOwnerV1::Rejection(owner) => owner.validate(tx),
            CompletedTransferResourceOwnerV1::Transferred { identity, .. } => {
                tx.validate_transaction_identity(identity)
            }
        }
    }
}

fn overlay_error(error: WorldFieldOverlayErrorV1) -> BodyMutationError {
    match error {
        WorldFieldOverlayErrorV1::Execution(error) => BodyMutationError::Execution(error),
        WorldFieldOverlayErrorV1::Value(error) => BodyMutationError::Value(error),
        WorldFieldOverlayErrorV1::Member(error) => BodyMutationError::Original(Box::new(error)),
    }
}

fn read_point<M, P, V, W, E, C, D>(
    step: &mut IwsStepContext<'_, '_, '_, '_, M, V, W, D>,
    agent: &str,
    agents: &ReservedValue<AgentSubtreeStructureQualificationV1>,
    read: &mut WorldFieldReadContextV1<'_, P, V, E, C>,
    work: &mut TransferResourceWorkV1,
    field: AgentFieldV1,
    ordinal: u64,
    live: &ExecutionFence,
) -> Result<Option<i64>, BodyMutationError>
where
    M: MerkleNodeReader,
    P: MerkleNodeReader,
    V: ValueNodeReader,
    W: ValueNodeWriter,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    D: FnMut() -> bool,
{
    work.spend(
        agent
            .len()
            .checked_add(1)
            .ok_or(ValueTreeError::ResourceLimited(
                "transfer resource resource read bytes",
            ))?,
    )?;
    let mut overlay = WorldFieldOverlayContextV1 {
        transaction: step.transaction,
        values: ValueReadContext {
            source: read.values.source,
            memory: read.values.memory,
            class: read.values.class,
            io: &mut *read.values.io,
        },
        key_work: &mut *read.key_work,
        semantic_bytes: read.semantic_bytes,
        record_bytes: read.record_bytes,
        take_state_error: &mut read.take_state_error,
        cancelled: || (read.cancelled)() || (step.mutation.cancelled)(),
    };
    let guard = overlay.transaction.guard();
    let value = read_agent_subtree_scalar_overlay_v1(
        AgentSubtreeQualifiedRequestV1 {
            cell: AgentSubtreeScalarRequestV1 {
                agent,
                field,
                ordinal,
            },
            qualification: agents,
        },
        &mut overlay,
        work.max_scalar_bytes,
        &mut std::io::sink(),
        &guard,
        live,
    )
    .map_err(overlay_error)?;
    match value {
        None => Ok(None),
        Some(AgentSubtreeValueV1::I64(amount))
            if field != AgentFieldV1::ResourceAmount || amount >= 0 =>
        {
            Ok(Some(amount))
        }
        Some(AgentSubtreeValueV1::I64(_)) => {
            Err(BodyMutationError::Invalid("transfer negative balance"))
        }
        _ => Err(BodyMutationError::Invalid(
            "TransferResource resource scalar type",
        )),
    }
}

/// Execute the effective transfer result for a genuine FIFO
/// selection and this transaction's real Rule outcome. Admission, Accepted,
/// PostAction, and later promotion remain with the shared driver.
#[expect(
    clippy::too_many_arguments,
    reason = "The Current action, scopes, shared work and original sink stay explicit."
)]
pub(in crate::world) fn execute_selected_transfer_resource_v1<M, P, V, W, E, C, PE, PC, D, F>(
    step: &mut IwsStepContext<'_, '_, '_, '_, M, V, W, D>,
    materials: &mut crate::world::incremental_body_event::materials::CorePolicyMaterialContextV1<
        '_,
        M,
        impl FnMut() -> Option<ValueTreeError>,
    >,
    selected: &SelectedPendingActionV1,
    outcome: &ReservedValue<crate::world::incremental_module_invocation::RootRulePhaseOutcomeV1>,
    agents: &ReservedValue<AgentSubtreeStructureQualificationV1>,
    roots: &RootSetV1,
    read: &mut WorldFieldReadContextV1<'_, P, V, E, C>,
    parent_read: &mut WorldFieldReadContextV1<'_, P, V, PE, PC>,
    work: &mut TransferResourceWorkV1,
    audit_work: &mut WorldEventDecodeWorkV1,
    max_event_bytes: usize,
    guard: &PreparedGuard,
    live: &ExecutionFence,
    mut persist_original: F,
) -> Result<CompletedTransferResourceV1, BodyMutationError>
where
    M: MerkleNodeReader,
    P: MerkleNodeReader,
    V: ValueNodeReader,
    W: ValueNodeWriter,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    PE: FnMut() -> Option<ValueTreeError>,
    PC: FnMut() -> bool,
    D: FnMut() -> bool,
    F: FnMut(&mut dyn AgentEventOriginalV1) -> Result<(), BodyMutationError>,
{
    let result = (|| {
        step.transaction.validate_guard(guard, live)?;
        selected.validate(step.transaction)?;
        if !outcome.uses_budget(step.mutation.memory)
            || read.values.class != MemoryClass::Ordinary
            || parent_read.values.class != MemoryClass::Ordinary
            || !read.values.memory.same_pool(step.mutation.memory)
            || !parent_read.values.memory.same_pool(step.mutation.memory)
            || !step.transaction.uses_budget(step.mutation.memory)
            || !agents.uses_budget(step.mutation.memory)
        {
            return Err(BodyMutationError::Invalid(
                "TransferResource selected-action pool/class",
            ));
        }
        read.check()?;
        parent_read.check()?;

        let replacement = outcome.override_action_for_selected_v1(selected, step.transaction)?;
        if outcome.decision.verdict == crate::RuleVerdict::Deny {
            return Err(BodyMutationError::Invalid(
                "transfer denied by Rule outcome",
            ));
        }
        let action = replacement.unwrap_or(&selected.envelope().action);
        let (from, to, kind, amount, native_rejection) = match action {
            crate::Action::TransferResource {
                from_agent_id,
                to_agent_id,
                kind,
                amount,
            } => (from_agent_id, to_agent_id, *kind, *amount, true),
            crate::Action::EmitResourceTransfer {
                from_agent_id,
                to_agent_id,
                kind,
                amount,
            } => (from_agent_id, to_agent_id, *kind, *amount, false),
            _ => return Err(BodyMutationError::Invalid("selected transfer action kind")),
        };
        work.spend(
            from.len()
                .checked_add(to.len())
                .ok_or(ValueTreeError::ResourceLimited("transfer identifiers"))?,
        )?;
        // Cover the closed two-ID event/reason/keys and bounded error formatting
        // through immutable-output admission and the entire reducer/original sink.
        let phase_bytes = from
            .len()
            .checked_add(to.len())
            .and_then(|n| n.checked_mul(8))
            .and_then(|n| n.checked_add(16 << 10))
            .ok_or(ValueTreeError::ResourceLimited("transfer phase allocation"))?;
        let _phase_fee = step
            .mutation
            .memory
            .reserve(phase_bytes, MemoryClass::Ordinary)
            .map_err(ValueTreeError::Memory)?;
        if !native_rejection && amount <= 0 {
            return reject(
                step,
                materials,
                live,
                selected,
                crate::RejectReason::InvalidAmount { amount },
                max_event_bytes,
                audit_work,
                &mut persist_original,
            );
        }
        for agent in [from, to] {
            work.spend(agent.len())?;
            let presence = {
                let mut overlay = WorldFieldOverlayContextV1 {
                    transaction: step.transaction,
                    values: ValueReadContext {
                        source: read.values.source,
                        memory: read.values.memory,
                        class: read.values.class,
                        io: &mut *read.values.io,
                    },
                    key_work: &mut *read.key_work,
                    semantic_bytes: read.semantic_bytes,
                    record_bytes: read.record_bytes,
                    take_state_error: &mut read.take_state_error,
                    cancelled: || (read.cancelled)() || (step.mutation.cancelled)(),
                };
                let guard = overlay.transaction.guard();
                read_current_agent_presence_v1(
                    agent,
                    agents,
                    roots,
                    &mut overlay,
                    parent_read,
                    &mut work.pending_entries_remaining,
                    &guard,
                    live,
                )
                .map_err(overlay_error)?
            };
            if presence != CurrentAgentPresenceV1::Existing {
                return reject(
                    step,
                    materials,
                    live,
                    selected,
                    crate::RejectReason::AgentNotFound {
                        agent_id: agent.clone(),
                    },
                    max_event_bytes,
                    audit_work,
                    &mut persist_original,
                );
            }
        }
        if native_rejection {
            return reject(
                step,
                materials,
                live,
                selected,
                crate::RejectReason::RuleDenied {
                    notes: vec!["transfer requires rule module".into()],
                },
                max_event_bytes,
                audit_work,
                &mut persist_original,
            );
        }
        let mut positions = [[0i64; 3]; 2];
        for (position, agent) in positions.iter_mut().zip([from, to]) {
            for (slot, field) in position.iter_mut().zip([
                AgentFieldV1::PositionX,
                AgentFieldV1::PositionY,
                AgentFieldV1::PositionZ,
            ]) {
                *slot = read_point(step, agent, agents, read, work, field, 0, live)?.ok_or(
                    BodyMutationError::Invalid("transfer current position missing"),
                )?;
            }
        }
        let position = |p: [i64; 3]| oasis7_agent_api::world_action::GeoPos::new(p[0], p[1], p[2]);
        if oasis7_agent_api::world_geometry::space_distance_cm(
            position(positions[0]),
            position(positions[1]),
        ) > 0
        {
            return reject(
                step,
                materials,
                live,
                selected,
                crate::RejectReason::AgentsNotCoLocated {
                    agent_id: from.clone(),
                    other_agent_id: to.clone(),
                },
                max_event_bytes,
                audit_work,
                &mut persist_original,
            );
        }
        use oasis7_agent_api::world_action::ResourceKind;
        if kind == ResourceKind::Data && from != to {
            work.spend(
                from.len()
                    .checked_add(to.len())
                    .ok_or(ValueTreeError::ResourceLimited("transfer permission work"))?,
            )?;
            let mut overlay = WorldFieldOverlayContextV1 {
                transaction: step.transaction,
                values: ValueReadContext {
                    source: read.values.source,
                    memory: read.values.memory,
                    class: read.values.class,
                    io: &mut *read.values.io,
                },
                key_work: &mut *read.key_work,
                semantic_bytes: read.semantic_bytes,
                record_bytes: read.record_bytes,
                take_state_error: &mut read.take_state_error,
                cancelled: || (read.cancelled)() || (step.mutation.cancelled)(),
            };
            let guard = overlay.transaction.guard();
            overlay.transaction.validate_guard(&guard, live)?;
            use oasis7_world_state::incremental_authorization_mapsets::{
                AuthorizationMapSetFieldV1, AuthorizationMembershipV1,
                read_authorization_membership_overlay_v1,
            };
            if read_authorization_membership_overlay_v1(
                AuthorizationMapSetFieldV1::DataAccessPermissions,
                from,
                to,
                &mut overlay,
            )
            .map_err(overlay_error)?
                != AuthorizationMembershipV1::Present
            {
                return reject(
                    step,
                    materials,
                    live,
                    selected,
                    crate::RejectReason::RuleDenied {
                        notes: vec![format!(
                            "data transfer denied: missing access grant owner={} grantee={}",
                            from, to
                        )],
                    },
                    max_event_bytes,
                    audit_work,
                    &mut persist_original,
                );
            }
        }
        let ordinal = match kind {
            ResourceKind::Electricity => 0,
            ResourceKind::Data => 1,
        };
        let from_before = read_point(
            step,
            from,
            agents,
            read,
            work,
            AgentFieldV1::ResourceAmount,
            ordinal,
            live,
        )?;
        if from_before.unwrap_or(0) < amount {
            return reject(
                step,
                materials,
                live,
                selected,
                crate::RejectReason::InsufficientResource {
                    agent_id: from.clone(),
                    kind,
                    requested: amount,
                    available: from_before.unwrap_or(0),
                },
                max_event_bytes,
                audit_work,
                &mut persist_original,
            );
        }
        let to_before = if from == to {
            from_before
        } else {
            read_point(
                step,
                to,
                agents,
                read,
                work,
                AgentFieldV1::ResourceAmount,
                ordinal,
                live,
            )?
        };
        let (from_after, to_after) = if from == to {
            (from_before.unwrap_or(0), to_before.unwrap_or(0))
        } else {
            (
                oasis7_agent_api::world_resource::checked_resource_remove_v1(
                    kind,
                    from_before.unwrap_or(0),
                    amount,
                )
                .map_err(|error| stock_error(error, "remove", step.mutation.memory))?,
                oasis7_agent_api::world_resource::checked_resource_add_v1(
                    kind,
                    to_before.unwrap_or(0),
                    amount,
                )
                .map_err(|error| stock_error(error, "add", step.mutation.memory))?,
            )
        };
        let owner_fee = reserve_completed_fee(
            step.mutation.memory,
            from.len()
                .checked_add(to.len())
                .ok_or(ValueTreeError::ResourceLimited("transfer owner text"))?,
        )?;
        let sequence =
            step.read_allocator(crate::world::incremental_allocators::RuntimeAllocatorV1::Event)?;
        let (event_id, _, _) = World::preview_next_event_id(sequence.next, sequence.era);
        let event = crate::WorldEvent {
            id: event_id,
            time: step.time(),
            caused_by: Some(crate::CausedBy::Action(selected.envelope().id)),
            body: crate::WorldEventBody::Domain(crate::DomainEvent::ResourceTransferred {
                from_agent_id: from.clone(),
                to_agent_id: to.clone(),
                kind,
                amount,
            }),
        };
        let event = oasis7_world_state::incremental_world_event_owned::retain_world_event_owned_v1(
            event,
            step.mutation.memory,
            MemoryClass::Ordinary,
            audit_work,
            &mut step.mutation.cancelled,
        )
        .map_err(BodyMutationError::Value)?;
        let prepared = PreparedTransferResourceV1 {
            event,
            action_id: selected.envelope().id,
            now: step.time(),
            from_before,
            to_before,
            from_after,
            to_after,
            identity: step.transaction.guard(),
        };
        prepared.validate(step.transaction, step.mutation.memory, step.time())?;
        let before = step.begin_world_event()?;
        event::sink_prepared_transfer_resource_v1(
            step,
            materials,
            &prepared,
            agents,
            read,
            work,
            max_event_bytes,
            live,
            &mut persist_original,
        )?;
        step.finish_world_event(before)?;
        if (step.mutation.cancelled)() || (read.cancelled)() || (parent_read.cancelled)() {
            return Err(ValueTreeError::Cancelled.into());
        }
        Ok(CompletedTransferResourceV1 {
            owner: CompletedTransferResourceOwnerV1::Transferred {
                event: prepared.event,
                identity: step.transaction.guard(),
            },
            _fee: owner_fee,
        })
    })();
    let result = result.and_then(|completed| {
        if (step.mutation.cancelled)() || (read.cancelled)() || (parent_read.cancelled)() {
            Err(ValueTreeError::Cancelled.into())
        } else {
            Ok(completed)
        }
    });
    if result.is_err() {
        let _: Result<(), ExecutionError> = step
            .transaction
            .contribute(|_| Err(ExecutionError::Aborted));
    }
    result
}

fn reserve_completed_fee(
    memory: &oasis7_proto::memory_budget::MemoryBudget,
    text_bytes: usize,
) -> Result<MemoryReservation, BodyMutationError> {
    let bytes = std::mem::size_of::<CompletedTransferResourceV1>()
        .checked_add(std::mem::size_of::<PreparedTransferResourceV1>())
        .and_then(|bytes| bytes.checked_add(text_bytes))
        .and_then(|bytes| bytes.checked_add(256))
        .ok_or(ValueTreeError::ResourceLimited(
            "TransferResource owner allocation",
        ))?;
    memory
        .reserve(bytes, MemoryClass::Ordinary)
        .map_err(ValueTreeError::Memory)
        .map_err(Into::into)
}

#[allow(
    clippy::too_many_arguments,
    reason = "The genuine material owner and live fence remain explicit across the rejection sink."
)]
fn reject<M, V, W, D, F>(
    step: &mut IwsStepContext<'_, '_, '_, '_, M, V, W, D>,
    materials: &mut crate::world::incremental_body_event::materials::CorePolicyMaterialContextV1<
        '_,
        M,
        impl FnMut() -> Option<ValueTreeError>,
    >,
    live: &ExecutionFence,
    selected: &SelectedPendingActionV1,
    reason: crate::RejectReason,
    max_event_bytes: usize,
    audit_work: &mut WorldEventDecodeWorkV1,
    persist_original: &mut F,
) -> Result<CompletedTransferResourceV1, BodyMutationError>
where
    M: MerkleNodeReader,
    V: ValueNodeReader,
    W: ValueNodeWriter,
    D: FnMut() -> bool,
    F: FnMut(&mut dyn AgentEventOriginalV1) -> Result<(), BodyMutationError>,
{
    let owner_fee = reserve_completed_fee(step.mutation.memory, 0)?;
    let audit = step.append_selected_rejection_owned(
        materials,
        live,
        selected,
        reason,
        max_event_bytes,
        audit_work,
        persist_original,
    )?;
    Ok(CompletedTransferResourceV1 {
        owner: CompletedTransferResourceOwnerV1::Rejection(audit),
        _fee: owner_fee,
    })
}
