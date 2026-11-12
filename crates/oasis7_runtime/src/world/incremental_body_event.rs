//! Original agent-event fact, allocator and mailbox contribution in one overlay.
//! Acceptance, consensus and cache installation remain outer driver operations.
use super::body::PreparedBodyAttributesUpdate;
use super::{
    World,
    incremental_body::{BodyMutationContext, BodyMutationError, write_field_bytes},
};
use crate::{DomainEvent, WorldEvent, WorldEventBody};
use oasis7_formal_protocol::{
    incremental_execution::{
        ExecutionError, ExecutionFactKindV1, ExecutionFactRefV1, ExecutionTransaction,
    },
    incremental_state::{MerkleNodeReader, StateNamespaceV1 as Ns},
    incremental_value::{ValueNodeReader, ValueNodeWriter, ValueReadContext},
};
use oasis7_proto::memory_budget::MemoryClass;
use oasis7_wasm_abi::CanonicalCborWorkV1;
#[path = "incremental_agent_event_original.rs"]
pub(super) mod original;
use oasis7_world_state::incremental_agent_body::{
    AGENT_BODY_VALUE_BYTES, AgentBodyKeyV1, AgentBodyValueV1,
};
use oasis7_world_state::incremental_event_fields::*;
pub use original::{AgentEventOriginalSinkV1, AgentEventOriginalStreamV1, AgentEventOriginalV1};
#[path = "incremental_core_policy_materials.rs"]
pub mod materials;
use materials::CorePolicyMaterialContextV1;

pub struct AgentEventContext<'a, V, W, C, F> {
    pub values: &'a V,
    pub mutation: BodyMutationContext<'a, W, C>,
    /// Consume the one-shot borrowed producer and finish its sink. Callback Ok
    /// alone cannot append a fact; original persistence does not grant acceptance.
    pub persist_original: F,
    pub max_event_bytes: usize,
    /// Both measurement and actual output consume this same continuous work.
    pub original_work: CanonicalCborWorkV1,
}

pub(super) fn read_fixed<const N: usize, V: ValueNodeReader, R: MerkleNodeReader, W, C>(
    txn: &mut ExecutionTransaction<'_, R>,
    namespace: Ns,
    key: &[u8],
    values: &V,
    context: &mut BodyMutationContext<'_, W, C>,
) -> Result<[u8; N], BodyMutationError>
where
    C: FnMut() -> bool,
{
    let id = txn
        .read(namespace, key)?
        .ok_or(BodyMutationError::Invalid("missing event control leaf"))?;
    let mut reader = ValueReadContext {
        source: values,
        memory: context.memory,
        class: MemoryClass::Ordinary,
        io: context.io,
    };
    let root = reader.load_root(&id)?;
    if root.0.logical_len != N as u64 {
        return Err(BodyMutationError::Invalid("event control value length"));
    }
    let mut bytes = [0; N];
    reader.read_range(
        &root,
        0,
        N as u64,
        &mut std::io::Cursor::new(bytes.as_mut_slice()),
        &mut context.cancelled,
    )?;
    Ok(bytes)
}

impl World {
    #[allow(clippy::too_many_arguments)]
    pub(super) fn contribute_iws_factory_schedule_event_at<R, V, W, C, F, ME>(
        now: u64,
        cap: &super::incremental_normal_action_factory_schedule::PreparedFactoryScheduleEventV1,
        identity:oasis7_formal_protocol::controlled_authority::formal::bounded_context::WorkIdentityV1,
        txn: &mut ExecutionTransaction<'_, R>,
        context: &mut AgentEventContext<'_, V, W, C, F>,
        materials: &mut CorePolicyMaterialContextV1<'_, R, ME>,
        live: &oasis7_formal_protocol::incremental_execution::ExecutionFence,
    ) -> Result<(), BodyMutationError>
    where
        R: MerkleNodeReader,
        V: ValueNodeReader,
        W: ValueNodeWriter,
        C: FnMut() -> bool,
        F: FnMut(&mut dyn AgentEventOriginalV1) -> Result<(), BodyMutationError>,
        ME: FnMut() -> Option<oasis7_formal_protocol::incremental_value::ValueTreeError>,
    {
        Self::contribute_iws_agent_event_qualified_core(
            now,
            cap.event(),
            txn,
            context,
            materials,
            live,
            false,
            None,
            None,
            None,
            None,
            None,
            Some((cap, identity)),
        )
    }

    /// Stage body-attribute or move state changes and the exact original fact.
    /// No hot journal scan/copy, tick consensus reconstruction or state hash.
    /// The caller supplies a borrowed original envelope and the common txn.
    pub fn contribute_iws_agent_event<
        R: MerkleNodeReader,
        V: ValueNodeReader,
        W: ValueNodeWriter,
        C: FnMut() -> bool,
        F: FnMut(&mut dyn AgentEventOriginalV1) -> Result<(), BodyMutationError>,
        ME: FnMut() -> Option<oasis7_formal_protocol::incremental_value::ValueTreeError>,
    >(
        &self,
        event: &WorldEvent,
        txn: &mut ExecutionTransaction<'_, R>,
        context: &mut AgentEventContext<'_, V, W, C, F>,
        materials: &mut CorePolicyMaterialContextV1<'_, R, ME>,
        live: &oasis7_formal_protocol::incremental_execution::ExecutionFence,
    ) -> Result<(), BodyMutationError> {
        Self::contribute_iws_agent_event_at(self.state.time, event, txn, context, materials, live)
    }

    pub(super) fn contribute_iws_agent_event_at<
        R: MerkleNodeReader,
        V: ValueNodeReader,
        W: ValueNodeWriter,
        C: FnMut() -> bool,
        F: FnMut(&mut dyn AgentEventOriginalV1) -> Result<(), BodyMutationError>,
        ME: FnMut() -> Option<oasis7_formal_protocol::incremental_value::ValueTreeError>,
    >(
        now: u64,
        event: &WorldEvent,
        txn: &mut ExecutionTransaction<'_, R>,
        context: &mut AgentEventContext<'_, V, W, C, F>,
        materials: &mut CorePolicyMaterialContextV1<'_, R, ME>,
        live: &oasis7_formal_protocol::incremental_execution::ExecutionFence,
    ) -> Result<(), BodyMutationError> {
        Self::contribute_iws_agent_event_core(now, event, txn, context, materials, live, false)
    }

    pub(super) fn contribute_iws_accepted_event_at<
        R: MerkleNodeReader,
        V: ValueNodeReader,
        W: ValueNodeWriter,
        C: FnMut() -> bool,
        F: FnMut(&mut dyn AgentEventOriginalV1) -> Result<(), BodyMutationError>,
        ME: FnMut() -> Option<oasis7_formal_protocol::incremental_value::ValueTreeError>,
    >(
        now: u64,
        event: &WorldEvent,
        txn: &mut ExecutionTransaction<'_, R>,
        context: &mut AgentEventContext<'_, V, W, C, F>,
        materials: &mut CorePolicyMaterialContextV1<'_, R, ME>,
        live: &oasis7_formal_protocol::incremental_execution::ExecutionFence,
        scope: &super::incremental_action_execution::AcceptedActorScopeV1<'_>,
    ) -> Result<(), BodyMutationError> {
        Self::contribute_iws_agent_event_qualified_core(
            now,
            event,
            txn,
            context,
            materials,
            live,
            false,
            Some(scope),
            None,
            None,
            None,
            None,
            None,
        )
    }

    pub(super) fn contribute_iws_body_interface_event_at<
        R: MerkleNodeReader,
        V: ValueNodeReader,
        W: ValueNodeWriter,
        C: FnMut() -> bool,
        F: FnMut(&mut dyn AgentEventOriginalV1) -> Result<(), BodyMutationError>,
        ME: FnMut() -> Option<oasis7_formal_protocol::incremental_value::ValueTreeError>,
    >(
        now: u64,
        cap: &super::incremental_normal_action_body_interface::PreparedBodyInterfaceEventV1,
        txn: &mut ExecutionTransaction<'_, R>,
        context: &mut AgentEventContext<'_, V, W, C, F>,
        materials: &mut CorePolicyMaterialContextV1<'_, R, ME>,
        live: &oasis7_formal_protocol::incremental_execution::ExecutionFence,
    ) -> Result<(), BodyMutationError> {
        Self::contribute_iws_agent_event_qualified_core(
            now,
            cap.event(),
            txn,
            context,
            materials,
            live,
            false,
            None,
            Some(cap),
            None,
            None,
            None,
            None,
        )
    }

    pub(super) fn contribute_iws_logistics_registration_event_at<
        R: MerkleNodeReader,
        V: ValueNodeReader,
        W: ValueNodeWriter,
        C: FnMut() -> bool,
        F: FnMut(&mut dyn AgentEventOriginalV1) -> Result<(), BodyMutationError>,
        ME: FnMut() -> Option<oasis7_formal_protocol::incremental_value::ValueTreeError>,
    >(
        now: u64,
        cap: &super::incremental_normal_action_logistics::PreparedLogisticsRegistrationEventV1,
        txn: &mut ExecutionTransaction<'_, R>,
        context: &mut AgentEventContext<'_, V, W, C, F>,
        materials: &mut CorePolicyMaterialContextV1<'_, R, ME>,
        live: &oasis7_formal_protocol::incremental_execution::ExecutionFence,
    ) -> Result<(), BodyMutationError> {
        Self::contribute_iws_agent_event_qualified_core(
            now,
            cap.event(),
            txn,
            context,
            materials,
            live,
            false,
            None,
            None,
            Some(cap),
            None,
            None,
            None,
        )
    }

    pub(super) fn contribute_iws_alliance_event_at<
        R: MerkleNodeReader,
        V: ValueNodeReader,
        W: ValueNodeWriter,
        C: FnMut() -> bool,
        F: FnMut(&mut dyn AgentEventOriginalV1) -> Result<(), BodyMutationError>,
        ME: FnMut() -> Option<oasis7_formal_protocol::incremental_value::ValueTreeError>,
    >(
        now: u64,
        cap: &super::incremental_normal_action_diplomacy::PreparedAllianceEventV1,
        txn: &mut ExecutionTransaction<'_, R>,
        context: &mut AgentEventContext<'_, V, W, C, F>,
        materials: &mut CorePolicyMaterialContextV1<'_, R, ME>,
        live: &oasis7_formal_protocol::incremental_execution::ExecutionFence,
    ) -> Result<(), BodyMutationError> {
        Self::contribute_iws_agent_event_qualified_core(
            now,
            cap.event(),
            txn,
            context,
            materials,
            live,
            false,
            None,
            None,
            None,
            Some(cap),
            None,
            None,
        )
    }

    pub(super) fn contribute_iws_normal_system_event_at<
        R: MerkleNodeReader,
        V: ValueNodeReader,
        W: ValueNodeWriter,
        C: FnMut() -> bool,
        F: FnMut(&mut dyn AgentEventOriginalV1) -> Result<(), BodyMutationError>,
        ME: FnMut() -> Option<oasis7_formal_protocol::incremental_value::ValueTreeError>,
    >(
        now: u64,
        cap: &super::incremental_normal_system_event::PreparedNormalSystemEventV1<'_>,
        txn: &mut ExecutionTransaction<'_, R>,
        context: &mut AgentEventContext<'_, V, W, C, F>,
        materials: &mut CorePolicyMaterialContextV1<'_, R, ME>,
        live: &oasis7_formal_protocol::incremental_execution::ExecutionFence,
    ) -> Result<(), BodyMutationError> {
        Self::contribute_iws_agent_event_qualified_core(
            now,
            cap.event(),
            txn,
            context,
            materials,
            live,
            false,
            None,
            None,
            None,
            None,
            Some(cap),
            None,
        )
    }

    fn contribute_iws_agent_event_core<
        R: MerkleNodeReader,
        V: ValueNodeReader,
        W: ValueNodeWriter,
        C: FnMut() -> bool,
        F: FnMut(&mut dyn AgentEventOriginalV1) -> Result<(), BodyMutationError>,
        ME: FnMut() -> Option<oasis7_formal_protocol::incremental_value::ValueTreeError>,
    >(
        now: u64,
        event: &WorldEvent,
        txn: &mut ExecutionTransaction<'_, R>,
        context: &mut AgentEventContext<'_, V, W, C, F>,
        materials: &mut CorePolicyMaterialContextV1<'_, R, ME>,
        live: &oasis7_formal_protocol::incremental_execution::ExecutionFence,
        registration_created: bool,
    ) -> Result<(), BodyMutationError> {
        Self::contribute_iws_agent_event_qualified_core(
            now,
            event,
            txn,
            context,
            materials,
            live,
            registration_created,
            None,
            None,
            None,
            None,
            None,
            None,
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn contribute_iws_agent_event_qualified_core<
        R: MerkleNodeReader,
        V: ValueNodeReader,
        W: ValueNodeWriter,
        C: FnMut() -> bool,
        F: FnMut(&mut dyn AgentEventOriginalV1) -> Result<(), BodyMutationError>,
        ME: FnMut() -> Option<oasis7_formal_protocol::incremental_value::ValueTreeError>,
    >(
        now: u64,
        event: &WorldEvent,
        txn: &mut ExecutionTransaction<'_, R>,
        context: &mut AgentEventContext<'_, V, W, C, F>,
        materials: &mut CorePolicyMaterialContextV1<'_, R, ME>,
        live: &oasis7_formal_protocol::incremental_execution::ExecutionFence,
        registration_created: bool,
        accepted_scope: Option<&super::incremental_action_execution::AcceptedActorScopeV1<'_>>,
        body_interface: Option<
            &super::incremental_normal_action_body_interface::PreparedBodyInterfaceEventV1,
        >,
        logistics_registration: Option<
            &super::incremental_normal_action_logistics::PreparedLogisticsRegistrationEventV1,
        >,
        alliance: Option<&super::incremental_normal_action_diplomacy::PreparedAllianceEventV1>,
        system_event: Option<
            &super::incremental_normal_system_event::PreparedNormalSystemEventV1<'_>,
        >,
        factory_schedule: Option<(
            &super::incremental_normal_action_factory_schedule::PreparedFactoryScheduleEventV1,
            oasis7_formal_protocol::controlled_authority::formal::bounded_context::WorkIdentityV1,
        )>,
    ) -> Result<(), BodyMutationError> {
        txn.contribute(|txn| {
            let WorldEvent {
                id: _,
                time: _,
                caused_by: _,
                body: _,
            } = event;
            let WorldEventBody::Domain(domain) = &event.body else {
                return Err(BodyMutationError::Invalid(
                    "unsupported incremental agent event",
                ));
            };
            if let DomainEvent::ActionAccepted { actor_id, .. } = domain {
                accepted_scope
                    .ok_or(BodyMutationError::Invalid(
                        "accepted event requires actual actor scope",
                    ))?
                    .validate(txn, actor_id)?;
            }
            if let Some(cap) = body_interface {
                cap.validate(txn, context.mutation.memory, now)?;
                if !std::ptr::eq(cap.event(), event) {
                    return Err(BodyMutationError::Invalid("body interface event source"));
                }
            }
            if let Some(cap) = logistics_registration {
                cap.validate(txn, context.mutation.memory, now)?;
                if !std::ptr::eq(cap.event(), event) {
                    return Err(BodyMutationError::Invalid(
                        "logistics registration event source",
                    ));
                }
            }
            if let Some(cap) = alliance {
                cap.validate(txn, context.mutation.memory, now)?;
                if !std::ptr::eq(cap.event(), event) {
                    return Err(BodyMutationError::Invalid("Alliance event source"));
                }
            }
            if let Some(cap) = system_event {
                cap.validate(txn, context.mutation.memory, now)?;
                if !std::ptr::eq(cap.event(), event) {
                    return Err(BodyMutationError::Invalid("normal system event source"));
                }
            }
            if let Some((cap, core)) = &factory_schedule {
                cap.validate(txn, context.mutation.memory, now, *core)?;
                if !std::ptr::eq(cap.event(), event) {
                    return Err(BodyMutationError::Invalid("industrial event source"));
                }
            }
            let agent_id = match domain {
                DomainEvent::FactoryBuildStarted {
                    builder_agent_id, ..
                } if factory_schedule.is_some() => builder_agent_id,
                DomainEvent::RecipeStarted {
                    requester_agent_id, ..
                } if factory_schedule.is_some() => requester_agent_id,

                DomainEvent::FactoryBuilt {
                    builder_agent_id, ..
                } if system_event.is_some() => builder_agent_id,

                DomainEvent::MaterialTransitCompleted {
                    requester_agent_id, ..
                } if system_event.is_some() => requester_agent_id,
                DomainEvent::AllianceFormed {
                    proposer_agent_id, ..
                } if alliance.is_some() => proposer_agent_id,
                DomainEvent::WarDeclared {
                    initiator_agent_id, ..
                } if alliance.is_some() => initiator_agent_id,
                DomainEvent::EconomicContractOpened {
                    creator_agent_id, ..
                } if alliance.is_some() => creator_agent_id,
                DomainEvent::EconomicContractSettled {
                    operator_agent_id, ..
                } if alliance.is_some() => operator_agent_id,
                DomainEvent::AllianceJoined {
                    operator_agent_id, ..
                }
                | DomainEvent::AllianceLeft {
                    operator_agent_id, ..
                }
                | DomainEvent::AllianceDissolved {
                    operator_agent_id, ..
                } if alliance.is_some() => operator_agent_id,
                DomainEvent::LogisticsRouteRegistered {
                    requester_agent_id, ..
                } if logistics_registration.is_some() => requester_agent_id,
                DomainEvent::BodyInterfaceExpanded { agent_id, .. }
                | DomainEvent::BodyInterfaceExpandRejected { agent_id, .. }
                    if body_interface.is_some() =>
                {
                    agent_id
                }
                DomainEvent::Observation { observation } => &observation.agent_id,
                DomainEvent::ActionAccepted {
                    actor_id: agent_id, ..
                }
                | DomainEvent::BodyAttributesUpdated { agent_id, .. }
                | DomainEvent::BodyAttributesRejected { agent_id, .. }
                | DomainEvent::AgentMoved { agent_id, .. } => agent_id,
                DomainEvent::AgentRegistered { agent_id, .. } if registration_created => agent_id,
                _ => {
                    return Err(BodyMutationError::Invalid(
                        "unsupported incremental agent event",
                    ));
                }
            };
            if !txn.uses_budget(context.mutation.memory) {
                return Err(BodyMutationError::Invalid("transaction memory pool"));
            }
            if event.time != now {
                return Err(BodyMutationError::Invalid(
                    "event time differs from reducer clock",
                ));
            }
            // Every supported event uses the original CorePolicy two-map
            // normalization, including rejected/absent Accepted actors and
            // registration. Its genuine parent cache is opened against the
            // actual Current transaction, not this borrowed event or a World.
            let material_guard = txn.guard();
            materials.contribute(
                txn,
                context.values,
                &mut context.mutation,
                &material_guard,
                live,
            )?;
            // AgentIntent receipt validation remains independent. No journal,
            // cache hydration or borrowed World state is read.
            let sequence = EventSequenceV1::from_canonical_bytes(&read_fixed::<
                EVENT_SEQUENCE_BYTES,
                _,
                _,
                _,
                _,
            >(
                txn,
                Ns::Allocator,
                EVENT_SEQUENCE_KEY_V1,
                context.values,
                &mut context.mutation,
            )?)
            .map_err(BodyMutationError::Invalid)?;
            let (allocated, next, era) = Self::preview_next_event_id(sequence.next, sequence.era);
            if event.id != allocated {
                return Err(BodyMutationError::Invalid("event allocator precondition"));
            }
            // Legacy ActionAccepted routes only to an existing actor. A missing
            // actor still produces the audit original and allocator successor.
            if matches!(
                domain,
                DomainEvent::ActionAccepted { .. } | DomainEvent::Observation { .. }
            ) || system_event.is_some()
            {
                let body_key = AgentBodyKeyV1::new(agent_id).map_err(BodyMutationError::Invalid)?;
                let body_exists = txn.read(Ns::Business, body_key.as_bytes())?.is_some();
                if accepted_scope.is_some_and(|scope| body_exists != scope.existing()) {
                    return Err(BodyMutationError::Invalid("accepted actor scope changed"));
                }
                if !body_exists {
                    let frontier_key =
                        MailboxKeyV1::new(agent_id, None).map_err(BodyMutationError::Invalid)?;
                    if txn.read(Ns::Scheduler, frontier_key.as_bytes())?.is_some() {
                        return Err(BodyMutationError::Invalid(
                            "absent accepted actor has mailbox",
                        ));
                    }
                    let measured = original::measure(
                        event,
                        context.mutation.memory,
                        &mut context.original_work,
                        &mut context.mutation.cancelled,
                        context.max_event_bytes,
                    )?;
                    let original_id = measured.id();
                    let next_ref = write_field_bytes(
                        &EventSequenceV1 { next, era }.canonical_bytes(),
                        &mut context.mutation,
                    )?;
                    txn.write(Ns::Allocator, EVENT_SEQUENCE_KEY_V1, Some(next_ref))?;
                    let mut producer = original::Producer::new(
                        measured,
                        context.mutation.memory,
                        &mut context.original_work,
                        &mut context.mutation.cancelled,
                    );
                    (context.persist_original)(&mut producer)?;
                    producer.check_complete()?;
                    drop(producer);
                    if (context.mutation.cancelled)() {
                        return Err(
                            oasis7_formal_protocol::incremental_value::ValueTreeError::Cancelled
                                .into(),
                        );
                    }
                    txn.append_fact(ExecutionFactRefV1 {
                        kind: ExecutionFactKindV1::Event,
                        id: original_id,
                    })?;
                    if (context.mutation.cancelled)() {
                        return Err(
                            oasis7_formal_protocol::incremental_value::ValueTreeError::Cancelled
                                .into(),
                        );
                    }
                    return Ok(());
                }
            }
            let frontier_key =
                MailboxKeyV1::new(agent_id, None).map_err(BodyMutationError::Invalid)?;
            let mut frontier = MailboxFrontierV1::from_canonical_bytes(&read_fixed::<
                MAILBOX_FRONTIER_BYTES,
                _,
                _,
                _,
                _,
            >(
                txn,
                Ns::Scheduler,
                frontier_key.as_bytes(),
                context.values,
                &mut context.mutation,
            )?)
            .map_err(BodyMutationError::Invalid)?;
            let item_key = MailboxKeyV1::new(agent_id, Some(frontier.tail))
                .map_err(BodyMutationError::Invalid)?;
            frontier.tail = frontier
                .tail
                .checked_add(1)
                .ok_or(BodyMutationError::Invalid("mailbox ordinal overflow"))?;
            if txn.read(Ns::Scheduler, item_key.as_bytes())?.is_some() {
                return Err(BodyMutationError::Invalid(
                    "mailbox append position occupied",
                ));
            }

            let original = original::measure(
                event,
                context.mutation.memory,
                &mut context.original_work,
                &mut context.mutation.cancelled,
                context.max_event_bytes,
            )?;
            let id = original.id();

            match domain {
                DomainEvent::BodyAttributesUpdated { .. }
                | DomainEvent::BodyAttributesRejected { .. } => {
                    let key = AgentBodyKeyV1::new(agent_id).map_err(BodyMutationError::Invalid)?;
                    let current = AgentBodyValueV1::from_canonical_bytes(&read_fixed::<
                        AGENT_BODY_VALUE_BYTES,
                        _,
                        _,
                        _,
                        _,
                    >(
                        txn,
                        Ns::Business,
                        key.as_bytes(),
                        context.values,
                        &mut context.mutation,
                    )?)
                    .map_err(BodyMutationError::Invalid)?;
                    let view = match domain {
                        DomainEvent::BodyAttributesUpdated { view, .. } => {
                            super::body::validate_body_kernel_view(&current.body_view, view)
                                .map_err(BodyMutationError::Rejected)?;
                            view
                        }
                        DomainEvent::BodyAttributesRejected { .. } => &current.body_view,
                        _ => unreachable!("matched body attribute event"),
                    };
                    let _prepared_charge = context
                        .mutation
                        .memory
                        .reserve(agent_id.len() + 512, MemoryClass::Ordinary)
                        .map_err(ExecutionError::Memory)?;
                    let prepared = PreparedBodyAttributesUpdate::new(
                        agent_id.clone(),
                        view.clone(),
                        event.time,
                    );
                    prepared.contribute_iws(
                        &current.body_view,
                        current.last_active,
                        txn,
                        &mut context.mutation,
                    )?;
                }
                DomainEvent::AgentMoved { to, .. } => {
                    super::incremental_agent_move::contribute_move(
                        agent_id,
                        to,
                        event.time,
                        txn,
                        context.values,
                        &mut context.mutation,
                    )?;
                }
                DomainEvent::FactoryBuildStarted { .. } | DomainEvent::RecipeStarted { .. }
                    if factory_schedule.is_some() => {}
                DomainEvent::FactoryBuilt { .. } if system_event.is_some() => {}
                DomainEvent::MaterialTransitCompleted { .. } if system_event.is_some() => {}
                DomainEvent::AllianceFormed { .. }
                | DomainEvent::AllianceJoined { .. }
                | DomainEvent::AllianceLeft { .. }
                | DomainEvent::AllianceDissolved { .. }
                | DomainEvent::WarDeclared { .. }
                | DomainEvent::EconomicContractOpened { .. }
                | DomainEvent::EconomicContractSettled { .. }
                    if alliance.is_some() => {}
                DomainEvent::LogisticsRouteRegistered { .. }
                    if logistics_registration.is_some() => {}
                DomainEvent::BodyInterfaceExpanded { .. }
                | DomainEvent::BodyInterfaceExpandRejected { .. }
                    if body_interface.is_some() => {}
                DomainEvent::ActionAccepted { .. } | DomainEvent::Observation { .. } => {}
                DomainEvent::AgentRegistered { .. } if registration_created => {}
                _ => unreachable!("event kind was checked before any contribution"),
            }
            let next_ref = write_field_bytes(
                &EventSequenceV1 { next, era }.canonical_bytes(),
                &mut context.mutation,
            )?;
            txn.write(Ns::Allocator, EVENT_SEQUENCE_KEY_V1, Some(next_ref))?;
            let item_ref = write_field_bytes(&mailbox_event_ref_v1(id), &mut context.mutation)?;
            txn.write(Ns::Scheduler, item_key.as_bytes(), Some(item_ref))?;
            let frontier_ref = write_field_bytes(
                &frontier
                    .canonical_bytes()
                    .map_err(BodyMutationError::Invalid)?,
                &mut context.mutation,
            )?;
            txn.write(Ns::Scheduler, frontier_key.as_bytes(), Some(frontier_ref))?;
            let mut producer = original::Producer::new(
                original,
                context.mutation.memory,
                &mut context.original_work,
                &mut context.mutation.cancelled,
            );
            (context.persist_original)(&mut producer)?;
            producer.check_complete()?;
            drop(producer);
            if (context.mutation.cancelled)() {
                return Err(
                    oasis7_formal_protocol::incremental_value::ValueTreeError::Cancelled.into(),
                );
            }
            txn.append_fact(ExecutionFactRefV1 {
                kind: ExecutionFactKindV1::Event,
                id,
            })?;
            if (context.mutation.cancelled)() {
                return Err(
                    oasis7_formal_protocol::incremental_value::ValueTreeError::Cancelled.into(),
                );
            }
            Ok(())
        })
    }
}
#[cfg(test)]
#[path = "incremental_body_event_tests.rs"]
mod tests;

#[path = "incremental_agent_registration.rs"]
mod registration;
