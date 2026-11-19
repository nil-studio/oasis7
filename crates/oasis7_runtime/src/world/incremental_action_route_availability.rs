//! Genuine selected owner for the complete SetLogisticsRouteAvailability action.
use crate::world::{
    World,
    incremental_action_execution::{SelectedActionAuditV1, SelectedPendingActionV1},
    incremental_body::BodyMutationError,
    incremental_body_event::{AgentEventOriginalV1, materials::CorePolicyMaterialContextV1},
    incremental_module_invocation::RootRulePhaseOutcomeV1,
    incremental_step::IwsStepContext,
};
use oasis7_formal_protocol::{
    incremental_execution::{
        ExecutionError, ExecutionFence, ExecutionTransaction, PreparedGuard, RootSetV1,
    },
    incremental_state::{MerkleNodeReader, StatePageWriter},
    incremental_value::{ValueNodeReader, ValueNodeWriter, ValueReadContext, ValueTreeError},
};
use oasis7_proto::memory_budget::{MemoryBudget, MemoryClass, MemoryReservation, ReservedValue};
use oasis7_world_state::{
    incremental_agent_subtree::{
        AgentSubtreeStructureQualificationV1, CurrentAgentPresenceV1,
        read_current_agent_presence_v1,
    },
    incremental_field_member::MemberRecordWriteContext,
    incremental_field_overlay::{WorldFieldOverlayContextV1, WorldFieldOverlayErrorV1},
    incremental_field_read::WorldFieldReadContextV1,
    incremental_logistics_records::{
        LogisticsCellReadV1, LogisticsCellV1, LogisticsColumnV1, LogisticsFamilyQualificationV1,
        LogisticsRecordKeyV1, LogisticsRecordKindV1, LogisticsValueV1, LogisticsWorkBudgetV1,
        read_logistics_cell_overlay_v1,
    },
    incremental_world_event_decode::WorldEventDecodeWorkV1,
};
#[path = "incremental_action_route_availability_event.rs"]
mod event;
pub(in crate::world) struct RouteAvailabilityWorkV1 {
    pub steps_remaining: u64,
    pub bytes_remaining: u64,
    pub pending_entries_remaining: usize,
    pub max_scalar_bytes: u64,
    pub logistics: LogisticsWorkBudgetV1,
}
impl RouteAvailabilityWorkV1 {
    fn spend(&mut self, n: usize) -> Result<(), BodyMutationError> {
        self.steps_remaining = self
            .steps_remaining
            .checked_sub(1)
            .ok_or(ValueTreeError::ResourceLimited("route availability steps"))?;
        self.bytes_remaining = self
            .bytes_remaining
            .checked_sub(
                u64::try_from(n)
                    .map_err(|_| ValueTreeError::ResourceLimited("route availability bytes"))?,
            )
            .ok_or(ValueTreeError::ResourceLimited("route availability bytes"))?;
        Ok(())
    }
}
struct PreparedRouteAvailabilityV1 {
    event: ReservedValue<crate::WorldEvent>,
    action_id: u64,
    now: u64,
    expected: bool,
    identity: PreparedGuard,
}
impl PreparedRouteAvailabilityV1 {
    fn validate<M: MerkleNodeReader>(
        &self,
        tx: &ExecutionTransaction<'_, M>,
        memory: &MemoryBudget,
        now: u64,
    ) -> Result<(), BodyMutationError> {
        tx.validate_transaction_identity(&self.identity)?;
        if !tx.uses_budget(memory)
            || !self.event.uses_budget(memory)
            || self.now != now
            || self.event.time != now
            || self.event.caused_by != Some(crate::CausedBy::Action(self.action_id))
        {
            return Err(BodyMutationError::Invalid(
                "prepared route availability binding",
            ));
        }
        let crate::WorldEventBody::Domain(crate::DomainEvent::LogisticsRouteAvailabilityChanged {
            requester_agent_id,
            owner_agent_id,
            ..
        }) = &self.event.body
        else {
            return Err(BodyMutationError::Invalid(
                "prepared route availability kind",
            ));
        };
        if requester_agent_id != owner_agent_id {
            return Err(BodyMutationError::Invalid(
                "prepared route availability owner",
            ));
        }
        Ok(())
    }
    fn event(&self) -> &crate::WorldEvent {
        &self.event
    }
    fn route(&self) -> (&str, &str, bool) {
        let crate::WorldEventBody::Domain(crate::DomainEvent::LogisticsRouteAvailabilityChanged {
            requester_agent_id,
            route_id,
            available,
            ..
        }) = &self.event.body
        else {
            unreachable!("private checked route event")
        };
        (requester_agent_id, route_id, *available)
    }
    fn actor(&self) -> &str {
        self.route().0
    }
}
pub(in crate::world) struct RouteRejectionReservationV1 {
    memory: MemoryBudget,
    fee: MemoryReservation,
}
pub(in crate::world) struct CompletedRouteAvailabilityV1 {
    owner: CompletedOwner,
    _fee: MemoryReservation,
}
enum CompletedOwner {
    Rejected(SelectedActionAuditV1),
    Changed {
        event: ReservedValue<crate::WorldEvent>,
        identity: PreparedGuard,
    },
}
impl CompletedRouteAvailabilityV1 {
    pub(in crate::world) fn reserve_rejection(
        memory: &MemoryBudget,
    ) -> Result<RouteRejectionReservationV1, BodyMutationError> {
        Ok(RouteRejectionReservationV1 {
            memory: memory.clone(),
            fee: memory
                .reserve(std::mem::size_of::<Self>(), MemoryClass::Ordinary)
                .map_err(ValueTreeError::Memory)?,
        })
    }
    pub(in crate::world) fn event(&self) -> &crate::WorldEvent {
        match &self.owner {
            CompletedOwner::Rejected(v) => v.event(),
            CompletedOwner::Changed { event, .. } => event,
        }
    }
    pub(in crate::world) fn validate<M: MerkleNodeReader>(
        &self,
        tx: &ExecutionTransaction<'_, M>,
    ) -> Result<(), ExecutionError> {
        match &self.owner {
            CompletedOwner::Rejected(v) => v.validate(tx),
            CompletedOwner::Changed { identity, .. } => tx.validate_transaction_identity(identity),
        }
    }
    pub(in crate::world) fn from_rejection<M: MerkleNodeReader>(
        owner: SelectedActionAuditV1,
        selected: &SelectedPendingActionV1,
        tx: &ExecutionTransaction<'_, M>,
        memory: &MemoryBudget,
        prepaid: RouteRejectionReservationV1,
    ) -> Result<Self, BodyMutationError> {
        selected.validate(tx)?;
        owner.validate(tx)?;
        if !tx.uses_budget(memory)
            || !prepaid.memory.same_pool(memory)
            || owner.event().caused_by != Some(crate::CausedBy::Action(selected.envelope().id))
            || !matches!(&owner.event().body,crate::WorldEventBody::Domain(crate::DomainEvent::ActionRejected {action_id,..}) if *action_id==selected.envelope().id)
        {
            return Err(BodyMutationError::Invalid(
                "route availability rejection receipt",
            ));
        }
        Ok(Self {
            owner: CompletedOwner::Rejected(owner),
            _fee: prepaid.fee,
        })
    }
}
fn overlay_error(e: WorldFieldOverlayErrorV1) -> BodyMutationError {
    match e {
        WorldFieldOverlayErrorV1::Execution(e) => e.into(),
        WorldFieldOverlayErrorV1::Value(e) => e.into(),
        WorldFieldOverlayErrorV1::Member(e) => BodyMutationError::Original(Box::new(e)),
    }
}
struct OwnerCompare<'a> {
    remaining: &'a [u8],
    equal: bool,
}
impl std::io::Write for OwnerCompare<'_> {
    fn write(&mut self, b: &[u8]) -> std::io::Result<usize> {
        self.equal &= self.remaining.get(..b.len()) == Some(b);
        self.remaining = &self.remaining[b.len().min(self.remaining.len())..];
        Ok(b.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}
#[allow(clippy::too_many_arguments)]
pub(in crate::world) fn execute_selected_route_availability_v1<
    M,
    P,
    V,
    W,
    E,
    C,
    PE,
    PC,
    ME,
    CW,
    D,
    F,
>(
    step: &mut IwsStepContext<'_, '_, '_, '_, M, V, W, D>,
    materials: &mut CorePolicyMaterialContextV1<'_, M, ME>,
    selected: &SelectedPendingActionV1,
    outcome: &ReservedValue<RootRulePhaseOutcomeV1>,
    agents: &ReservedValue<AgentSubtreeStructureQualificationV1>,
    roots: &RootSetV1,
    logistics: &ReservedValue<LogisticsFamilyQualificationV1>,
    writer: &mut MemberRecordWriteContext<'_, W, CW>,
    read: &mut WorldFieldReadContextV1<'_, P, V, E, C>,
    parent_read: &mut WorldFieldReadContextV1<'_, P, V, PE, PC>,
    work: &mut RouteAvailabilityWorkV1,
    audit: &mut WorldEventDecodeWorkV1,
    max_event_bytes: usize,
    guard: &PreparedGuard,
    live: &ExecutionFence,
    mut persist: F,
) -> Result<CompletedRouteAvailabilityV1, BodyMutationError>
where
    M: MerkleNodeReader,
    P: MerkleNodeReader,
    V: ValueNodeReader,
    W: ValueNodeWriter + ValueNodeReader + StatePageWriter,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    PE: FnMut() -> Option<ValueTreeError>,
    PC: FnMut() -> bool,
    ME: FnMut() -> Option<ValueTreeError>,
    CW: FnMut() -> bool,
    D: FnMut() -> bool,
    F: FnMut(&mut dyn AgentEventOriginalV1) -> Result<(), BodyMutationError>,
{
    let result = (|| {
        step.transaction.validate_guard(guard, live)?;
        selected.validate(step.transaction)?;
        read.check()?;
        parent_read.check()?;
        if (writer.cancelled)() || (step.mutation.cancelled)() {
            return Err(ValueTreeError::Cancelled.into());
        }
        let memory = step.mutation.memory;
        if read.values.class != MemoryClass::Ordinary
            || parent_read.values.class != MemoryClass::Ordinary
            || logistics.allocation_class() != MemoryClass::Ordinary
            || !read.values.memory.same_pool(memory)
            || !parent_read.values.memory.same_pool(memory)
            || !writer.memory.same_pool(memory)
            || !logistics.uses_budget(memory)
            || !outcome.uses_budget(memory)
        {
            return Err(BodyMutationError::Invalid("route availability pool/class"));
        }
        logistics.validate_binding(&step.transaction.parent_fence().parent_roots)?;
        if outcome.decision.verdict == crate::RuleVerdict::Deny {
            return Err(BodyMutationError::Invalid(
                "route availability requires settled non-denied Rule",
            ));
        }
        let action = outcome
            .override_action_for_selected_v1(selected, step.transaction)?
            .unwrap_or(&selected.envelope().action);
        let crate::Action::SetLogisticsRouteAvailability {
            requester_agent_id,
            route_id,
            available,
        } = action
        else {
            return Err(BodyMutationError::Invalid(
                "route availability action family",
            ));
        };
        let text = requester_agent_id
            .len()
            .checked_mul(2)
            .and_then(|n| n.checked_add(route_id.len()))
            .ok_or(ValueTreeError::ResourceLimited("route availability text"))?;
        work.spend(text)?;
        let _phase = memory
            .reserve(
                text.checked_mul(2)
                    .and_then(|n| n.checked_add(8192))
                    .ok_or(ValueTreeError::ResourceLimited(
                        "route availability prepayment",
                    ))?,
                MemoryClass::Ordinary,
            )
            .map_err(ValueTreeError::Memory)?;
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
                cancelled: &mut read.cancelled,
            };
            let guard = overlay.transaction.guard();
            read_current_agent_presence_v1(
                requester_agent_id,
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
        let mut rejection = None;
        if presence != CurrentAgentPresenceV1::Existing {
            rejection = Some(crate::RejectReason::AgentNotFound {
                agent_id: requester_agent_id.clone(),
            });
        }
        let mut expected = false;
        if rejection.is_none() {
            let mut compare = OwnerCompare {
                remaining: requester_agent_id.as_bytes(),
                equal: true,
            };
            let owner = read_cell(
                step,
                read,
                route_id,
                LogisticsColumnV1::OwnerAgentId,
                work,
                &mut compare,
            )?;
            match owner {
                LogisticsCellReadV1::RecordAbsent => {
                    rejection = Some(crate::RejectReason::RuleDenied {
                        notes: vec![format!("logistics route not found: route_id={route_id}")],
                    })
                }
                LogisticsCellReadV1::Value(LogisticsValueV1::Text { .. }) => {
                    if !compare.equal || !compare.remaining.is_empty() {
                        rejection = Some(crate::RejectReason::RuleDenied {
                            notes: vec![format!(
                                "logistics route availability requires owner: route_id={route_id}"
                            )],
                        });
                    }
                }
                _ => return Err(BodyMutationError::Invalid("route owner typed cell")),
            }
            if rejection.is_none() {
                expected = match read_cell(
                    step,
                    read,
                    route_id,
                    LogisticsColumnV1::Available,
                    work,
                    &mut std::io::sink(),
                )? {
                    LogisticsCellReadV1::Value(LogisticsValueV1::Bool(v)) => v,
                    _ => return Err(BodyMutationError::Invalid("route availability typed cell")),
                };
            }
        }
        if let Some(reason) = rejection {
            let prepaid = CompletedRouteAvailabilityV1::reserve_rejection(memory)?;
            let owner = step.append_selected_rejection_owned(
                materials,
                live,
                selected,
                reason,
                max_event_bytes,
                audit,
                &mut persist,
            )?;
            return CompletedRouteAvailabilityV1::from_rejection(
                owner,
                selected,
                step.transaction,
                memory,
                prepaid,
            );
        }
        let _owner_fee = memory
            .reserve(
                std::mem::size_of::<CompletedRouteAvailabilityV1>(),
                MemoryClass::Ordinary,
            )
            .map_err(ValueTreeError::Memory)?;
        let sequence =
            step.read_allocator(crate::world::incremental_allocators::RuntimeAllocatorV1::Event)?;
        let (id, _, _) = World::preview_next_event_id(sequence.next, sequence.era);
        let event = crate::WorldEvent {
            id,
            time: step.time(),
            caused_by: Some(crate::CausedBy::Action(selected.envelope().id)),
            body: crate::WorldEventBody::Domain(
                crate::DomainEvent::LogisticsRouteAvailabilityChanged {
                    requester_agent_id: requester_agent_id.clone(),
                    route_id: route_id.clone(),
                    available: *available,
                    owner_agent_id: requester_agent_id.clone(),
                },
            ),
        };
        let event = oasis7_world_state::incremental_world_event_owned::retain_world_event_owned_v1(
            event,
            memory,
            MemoryClass::Ordinary,
            audit,
            &mut step.mutation.cancelled,
        )?;
        let prepared = PreparedRouteAvailabilityV1 {
            event,
            action_id: selected.envelope().id,
            now: step.time(),
            expected,
            identity: step.transaction.guard(),
        };
        let before = step.begin_world_event()?;
        event::sink_prepared_route_availability_v1(
            step,
            materials,
            &prepared,
            agents,
            writer,
            read,
            work,
            max_event_bytes,
            live,
            &mut persist,
        )?;
        step.finish_world_event(before)?;
        Ok(CompletedRouteAvailabilityV1 {
            owner: CompletedOwner::Changed {
                event: prepared.event,
                identity: step.transaction.guard(),
            },
            _fee: _owner_fee,
        })
    })();
    let result = result.and_then(|value| {
        if (step.mutation.cancelled)()
            || (writer.cancelled)()
            || (read.cancelled)()
            || (parent_read.cancelled)()
        {
            return Err(ValueTreeError::Cancelled.into());
        }
        read.check()?;
        parent_read.check()?;
        step.transaction
            .validate_guard(&step.transaction.guard(), live)?;
        Ok(value)
    });
    if result.is_err() {
        let _: Result<(), ExecutionError> = step
            .transaction
            .contribute(|_| Err(ExecutionError::Aborted));
    }
    result
}
fn read_cell<M, P, V, W, E, C, D>(
    step: &mut IwsStepContext<'_, '_, '_, '_, M, V, W, D>,
    read: &mut WorldFieldReadContextV1<'_, P, V, E, C>,
    id: &str,
    column: LogisticsColumnV1,
    work: &mut RouteAvailabilityWorkV1,
    output: &mut impl std::io::Write,
) -> Result<LogisticsCellReadV1, BodyMutationError>
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
        usize::try_from(work.max_scalar_bytes)
            .map_err(|_| ValueTreeError::ResourceLimited("route scalar work"))?,
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
        cancelled: &mut read.cancelled,
    };
    read_logistics_cell_overlay_v1(
        LogisticsRecordKeyV1 {
            kind: LogisticsRecordKindV1::Route,
            id,
        },
        LogisticsCellV1::Column(column),
        &mut overlay,
        work.max_scalar_bytes,
        output,
    )
    .map_err(overlay_error)
}
