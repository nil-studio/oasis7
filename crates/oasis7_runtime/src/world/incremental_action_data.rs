//! Selected CollectData resource and event owners in the caller's one IWS txn.
//! The plain entry excludes authentication; the dedicated authenticated child
//! requires real Current claim and nonce sources before any business publication.
use super::{
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

#[path = "incremental_action_data_event.rs"]
mod event;

pub(super) struct CollectDataWorkV1 {
    pub steps_remaining: u64,
    pub bytes_remaining: u64,
    pub pending_entries_remaining: usize,
    pub max_scalar_bytes: u64,
}
impl CollectDataWorkV1 {
    fn spend(&mut self, bytes: usize) -> Result<(), BodyMutationError> {
        self.steps_remaining = self
            .steps_remaining
            .checked_sub(1)
            .ok_or(ValueTreeError::ResourceLimited("collect data steps"))?;
        self.bytes_remaining = self
            .bytes_remaining
            .checked_sub(
                u64::try_from(bytes)
                    .map_err(|_| ValueTreeError::ResourceLimited("collect data byte bound"))?,
            )
            .ok_or(ValueTreeError::ResourceLimited("collect data byte work"))?;
        Ok(())
    }
}

/// Only this module's selected-action reducer constructs a prepared owner.
/// Its fields bind the genuine selected/rule pair and the exact Current
/// resource values used to calculate the two checked resource writes.
pub(super) struct PreparedCollectDataV1 {
    event: ReservedValue<crate::WorldEvent>,
    action_id: u64,
    now: u64,
    electricity_before: Option<i64>,
    data_before: Option<i64>,
    electricity_after: i64,
    data_after: i64,
    identity: PreparedGuard,
}
impl PreparedCollectDataV1 {
    fn validate<M: MerkleNodeReader>(
        &self,
        tx: &ExecutionTransaction<'_, M>,
        memory: &oasis7_proto::memory_budget::MemoryBudget,
        now: u64,
    ) -> Result<(), BodyMutationError> {
        tx.validate_transaction_identity(&self.identity)?;
        if !tx.uses_budget(memory) || !self.event.uses_budget(memory) || self.now != now {
            return Err(BodyMutationError::Invalid("prepared CollectData pool/time"));
        }
        let crate::WorldEvent {
            id: _,
            time,
            caused_by,
            body: _,
        } = &*self.event;
        let (_, electricity_cost, data_amount) = self.route()?;
        let electricity_before = self.electricity_before.unwrap_or(0);
        let data_before = self.data_before.unwrap_or(0);
        if *time != now
            || *caused_by != Some(crate::CausedBy::Action(self.action_id))
            || electricity_cost <= 0
            || data_amount <= 0
            || electricity_before < electricity_cost
            || self.electricity_after != electricity_before - electricity_cost
            || self.data_after
                != data_before
                    .checked_add(data_amount)
                    .ok_or(BodyMutationError::Invalid(
                        "prepared CollectData data overflow",
                    ))?
            || self.electricity_after < 0
            || self.data_after < 0
        {
            return Err(BodyMutationError::Invalid("prepared CollectData binding"));
        }
        Ok(())
    }

    fn event(&self) -> &crate::WorldEvent {
        &self.event
    }

    fn route(&self) -> Result<(&str, i64, i64), BodyMutationError> {
        match &self.event.body {
            crate::WorldEventBody::Domain(crate::DomainEvent::DataCollected {
                collector_agent_id,
                electricity_cost,
                data_amount,
            })
            | crate::WorldEventBody::Domain(crate::DomainEvent::DataCollectedAuthenticated {
                collector_agent_id,
                electricity_cost,
                data_amount,
                ..
            }) => Ok((collector_agent_id, *electricity_cost, *data_amount)),
            _ => Err(BodyMutationError::Invalid(
                "prepared CollectData event kind",
            )),
        }
    }
    fn collector(&self) -> &str {
        self.route().expect("validated CollectData event").0
    }
}

/// Genuine result of the selected action's real business and event sinks.
/// The contained receipt is required before the common PostAction router can
/// consume the result event.
pub(super) struct CompletedCollectDataV1 {
    owner: CompletedCollectDataOwnerV1,
    _fee: MemoryReservation,
}
enum CompletedCollectDataOwnerV1 {
    Rejection(SelectedActionAuditV1),
    Collected {
        event: ReservedValue<crate::WorldEvent>,
        identity: PreparedGuard,
    },
}
impl CompletedCollectDataV1 {
    pub(super) fn from_rejection<M: MerkleNodeReader>(
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
                "CollectData rejection receipt binding",
            ));
        }
        let fee = memory
            .reserve(std::mem::size_of::<Self>(), MemoryClass::Ordinary)
            .map_err(ExecutionError::Memory)?;
        Ok(Self {
            owner: CompletedCollectDataOwnerV1::Rejection(owner),
            _fee: fee,
        })
    }
    pub(super) fn event(&self) -> &crate::WorldEvent {
        match &self.owner {
            CompletedCollectDataOwnerV1::Rejection(owner) => owner.event(),
            CompletedCollectDataOwnerV1::Collected { event, .. } => event,
        }
    }

    pub(super) fn validate<M: MerkleNodeReader>(
        &self,
        tx: &ExecutionTransaction<'_, M>,
    ) -> Result<(), ExecutionError> {
        match &self.owner {
            CompletedCollectDataOwnerV1::Rejection(owner) => owner.validate(tx),
            CompletedCollectDataOwnerV1::Collected { identity, .. } => {
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

fn read_resource<M, P, V, W, E, C, D>(
    step: &mut IwsStepContext<'_, '_, '_, '_, M, V, W, D>,
    agent: &str,
    agents: &ReservedValue<AgentSubtreeStructureQualificationV1>,
    read: &mut WorldFieldReadContextV1<'_, P, V, E, C>,
    work: &mut CollectDataWorkV1,
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
                "collect data resource read bytes",
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
                field: AgentFieldV1::ResourceAmount,
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
        Some(AgentSubtreeValueV1::I64(amount)) if amount >= 0 => Ok(Some(amount)),
        Some(AgentSubtreeValueV1::I64(_)) => {
            Err(BodyMutationError::Invalid("CollectData resource negative"))
        }
        _ => Err(BodyMutationError::Invalid(
            "CollectData resource scalar type",
        )),
    }
}

/// Execute only the non-authenticated CollectData result for a genuine FIFO
/// selection and this transaction's real Rule outcome. Admission, Accepted,
/// PostAction, and later promotion remain with the shared driver.
#[expect(
    clippy::too_many_arguments,
    reason = "The Current action, scopes, shared work and original sink stay explicit."
)]
pub(super) fn execute_selected_collect_data_v1<M, P, V, W, E, C, PE, PC, D, F>(
    step: &mut IwsStepContext<'_, '_, '_, '_, M, V, W, D>,
    materials: &mut crate::world::incremental_body_event::materials::CorePolicyMaterialContextV1<
        '_,
        M,
        impl FnMut() -> Option<ValueTreeError>,
    >,
    selected: &SelectedPendingActionV1,
    outcome: &ReservedValue<super::incremental_module_invocation::RootRulePhaseOutcomeV1>,
    agents: &ReservedValue<AgentSubtreeStructureQualificationV1>,
    roots: &RootSetV1,
    read: &mut WorldFieldReadContextV1<'_, P, V, E, C>,
    parent_read: &mut WorldFieldReadContextV1<'_, P, V, PE, PC>,
    work: &mut CollectDataWorkV1,
    audit_work: &mut WorldEventDecodeWorkV1,
    max_event_bytes: usize,
    guard: &PreparedGuard,
    live: &ExecutionFence,
    mut persist_original: F,
) -> Result<CompletedCollectDataV1, BodyMutationError>
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
                "CollectData selected-action pool/class",
            ));
        }
        read.check()?;
        parent_read.check()?;

        if matches!(
            &selected.envelope().action,
            crate::Action::CollectDataAuthenticated { .. }
        ) {
            return Err(BodyMutationError::Invalid(
                "authenticated CollectData Current authority unsupported",
            ));
        }
        let replacement = outcome.override_action_for_selected_v1(selected, step.transaction)?;
        if outcome.decision.verdict == crate::RuleVerdict::Deny {
            return Err(BodyMutationError::Invalid(
                "CollectData denied by Rule outcome",
            ));
        }
        let action = replacement.unwrap_or(&selected.envelope().action);
        let crate::Action::CollectData {
            collector_agent_id,
            electricity_cost,
            data_amount,
        } = action
        else {
            return Err(BodyMutationError::Invalid(
                "selected CollectData action kind",
            ));
        };
        work.spend(collector_agent_id.len())?;

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
                collector_agent_id,
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
            let owner_fee = reserve_completed_fee(step.mutation.memory, collector_agent_id.len())?;
            let audit = step.append_selected_rejection_owned(
                materials,
                live,
                selected,
                crate::RejectReason::AgentNotFound {
                    agent_id: collector_agent_id.clone(),
                },
                max_event_bytes,
                audit_work,
                &mut persist_original,
            )?;
            return Ok(CompletedCollectDataV1 {
                owner: CompletedCollectDataOwnerV1::Rejection(audit),
                _fee: owner_fee,
            });
        }
        if *electricity_cost <= 0 {
            return reject_invalid_amount(
                step,
                materials,
                live,
                selected,
                *electricity_cost,
                max_event_bytes,
                audit_work,
                &mut persist_original,
            );
        }
        if *data_amount <= 0 {
            return reject_invalid_amount(
                step,
                materials,
                live,
                selected,
                *data_amount,
                max_event_bytes,
                audit_work,
                &mut persist_original,
            );
        }

        let electricity_before =
            read_resource(step, collector_agent_id, agents, read, work, 0, live)?;
        let electricity = electricity_before.unwrap_or(0);
        if electricity < *electricity_cost {
            let owner_fee = reserve_completed_fee(step.mutation.memory, collector_agent_id.len())?;
            let audit = step.append_selected_rejection_owned(
                materials,
                live,
                selected,
                crate::RejectReason::InsufficientResource {
                    agent_id: collector_agent_id.clone(),
                    kind: oasis7_agent_api::world_action::ResourceKind::Electricity,
                    requested: *electricity_cost,
                    available: electricity,
                },
                max_event_bytes,
                audit_work,
                &mut persist_original,
            )?;
            return Ok(CompletedCollectDataV1 {
                owner: CompletedCollectDataOwnerV1::Rejection(audit),
                _fee: owner_fee,
            });
        }

        let data_before = read_resource(step, collector_agent_id, agents, read, work, 1, live)?;
        let (electricity_after, data_after) = collection_balances(
            electricity,
            *electricity_cost,
            data_before.unwrap_or(0),
            *data_amount,
            step.mutation.memory,
        )?;

        let owner_fee = reserve_completed_fee(step.mutation.memory, collector_agent_id.len())?;
        let sequence =
            step.read_allocator(super::incremental_allocators::RuntimeAllocatorV1::Event)?;
        let (event_id, _, _) = World::preview_next_event_id(sequence.next, sequence.era);
        let event_bytes = std::mem::size_of::<crate::WorldEvent>()
            .checked_add(collector_agent_id.len())
            .ok_or(ValueTreeError::ResourceLimited(
                "CollectData event allocation",
            ))?;
        let event_build_fee = step
            .mutation
            .memory
            .reserve(event_bytes, MemoryClass::Ordinary)
            .map_err(ValueTreeError::Memory)?;
        let collector_copy = collector_agent_id.clone();
        if collector_copy.capacity() > collector_agent_id.len() {
            return Err(ValueTreeError::ResourceLimited("CollectData collector capacity").into());
        }
        let event = crate::WorldEvent {
            id: event_id,
            time: step.time(),
            caused_by: Some(crate::CausedBy::Action(selected.envelope().id)),
            body: crate::WorldEventBody::Domain(crate::DomainEvent::DataCollected {
                collector_agent_id: collector_copy,
                electricity_cost: *electricity_cost,
                data_amount: *data_amount,
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
        drop(event_build_fee);
        let prepared = PreparedCollectDataV1 {
            event,
            action_id: selected.envelope().id,
            now: step.time(),
            electricity_before,
            data_before,
            electricity_after,
            data_after,
            identity: step.transaction.guard(),
        };
        prepared.validate(step.transaction, step.mutation.memory, step.time())?;
        let before = step.begin_world_event()?;
        event::sink_prepared_collect_data_v1(
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
        Ok(CompletedCollectDataV1 {
            owner: CompletedCollectDataOwnerV1::Collected {
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
    let bytes = std::mem::size_of::<CompletedCollectDataV1>()
        .checked_add(std::mem::size_of::<PreparedCollectDataV1>())
        .and_then(|bytes| bytes.checked_add(text_bytes))
        .and_then(|bytes| bytes.checked_add(256))
        .ok_or(ValueTreeError::ResourceLimited(
            "CollectData owner allocation",
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
fn reject_invalid_amount<M, V, W, D, F>(
    step: &mut IwsStepContext<'_, '_, '_, '_, M, V, W, D>,
    materials: &mut crate::world::incremental_body_event::materials::CorePolicyMaterialContextV1<
        '_,
        M,
        impl FnMut() -> Option<ValueTreeError>,
    >,
    live: &ExecutionFence,
    selected: &SelectedPendingActionV1,
    amount: i64,
    max_event_bytes: usize,
    audit_work: &mut WorldEventDecodeWorkV1,
    persist_original: &mut F,
) -> Result<CompletedCollectDataV1, BodyMutationError>
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
        crate::RejectReason::InvalidAmount { amount },
        max_event_bytes,
        audit_work,
        persist_original,
    )?;
    Ok(CompletedCollectDataV1 {
        owner: CompletedCollectDataOwnerV1::Rejection(audit),
        _fee: owner_fee,
    })
}

#[path = "incremental_action_transfer_resource.rs"]
pub(in crate::world) mod transfer_resource;

#[path = "incremental_action_data_access.rs"]
pub(in crate::world) mod data_access;

#[path = "incremental_collect_data_auth_signature.rs"]
pub(in crate::world) mod auth_signature;

#[path = "incremental_collect_data_current_authorization.rs"]
pub(in crate::world) mod auth_current;

#[path = "incremental_collect_data_authenticated.rs"]
pub(in crate::world) mod authenticated;

/// Original closed stock failure, retaining its exact typed WorldError and fee.
#[derive(Debug)]
pub(in crate::world) struct CollectionResourceStateErrorV1 {
    pub(in crate::world) cause: crate::WorldError,
    _fee: MemoryReservation,
}
impl std::fmt::Display for CollectionResourceStateErrorV1 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}", self.cause)
    }
}
impl std::error::Error for CollectionResourceStateErrorV1 {}
fn collection_stock_error(
    error: oasis7_agent_api::world_resource::StockError,
    operation: &str,
    memory: &oasis7_proto::memory_budget::MemoryBudget,
) -> BodyMutationError {
    let fee = match memory.reserve(
        std::mem::size_of::<CollectionResourceStateErrorV1>() + 256,
        MemoryClass::Ordinary,
    ) {
        Ok(fee) => fee,
        Err(error) => return ValueTreeError::Memory(error).into(),
    };
    let reason = format!("data collection {operation} failed: {error:?}");
    if reason.capacity() > 256 {
        return ValueTreeError::ResourceLimited("collection stock error capacity").into();
    }
    BodyMutationError::Original(Box::new(CollectionResourceStateErrorV1 {
        cause: crate::WorldError::ResourceBalanceInvalid { reason },
        _fee: fee,
    }))
}
fn collection_balances(
    electricity: i64,
    cost: i64,
    data: i64,
    amount: i64,
    memory: &oasis7_proto::memory_budget::MemoryBudget,
) -> Result<(i64, i64), BodyMutationError> {
    use oasis7_agent_api::{
        world_action::ResourceKind,
        world_resource::{checked_resource_add_v1, checked_resource_remove_v1},
    };
    let electricity_after =
        checked_resource_remove_v1(ResourceKind::Electricity, electricity, cost)
            .map_err(|error| collection_stock_error(error, "electricity debit", memory))?;
    let data_after = checked_resource_add_v1(ResourceKind::Data, data, amount)
        .map_err(|error| collection_stock_error(error, "data credit", memory))?;
    Ok((electricity_after, data_after))
}
