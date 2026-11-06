//! Qualified registration and original event contribution in the same transaction.
use super::*;
use oasis7_formal_protocol::incremental_execution::{ExecutionFence, PreparedGuard};
use oasis7_formal_protocol::incremental_value::ValueTreeError;
use oasis7_proto::memory_budget::ReservedValue;
use oasis7_world_state::{
    AgentActivityV1, AgentCell, AgentState,
    incremental_agent_subtree::{
        AgentSubtreeCreateV1, AgentSubtreeStructureQualificationV1, create_agent_subtree_v1,
    },
    incremental_field_overlay::WorldFieldOverlayContextV1,
};

impl World {
    /// The step supplies its authenticated reducer clock. Only qualified absent
    /// cells can be created; all errors poison the caller's entire transaction.
    #[expect(
        clippy::too_many_arguments,
        reason = "The authenticated reducer clock, qualified source, bounded IO and original event writer are independent obligations."
    )]
    pub(in crate::world) fn contribute_iws_agent_registration_at<
        M: MerkleNodeReader,
        V: ValueNodeReader,
        E: FnMut() -> Option<ValueTreeError>,
        C: FnMut() -> bool,
        W: ValueNodeWriter,
        CW: FnMut() -> bool,
        F: FnMut(&mut dyn AgentEventOriginalV1) -> Result<(), BodyMutationError>,
    >(
        now: u64,
        event: &WorldEvent,
        qualification: &ReservedValue<AgentSubtreeStructureQualificationV1>,
        overlay: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, V, E, C>,
        write: &mut BodyMutationContext<'_, W, CW>,
        guard: &PreparedGuard,
        live: &ExecutionFence,
        original_work: &mut CanonicalCborWorkV1,
        max_event_bytes: usize,
        persist_original: F,
    ) -> Result<(), BodyMutationError> {
        let WorldFieldOverlayContextV1 {
            transaction,
            values,
            key_work,
            semantic_bytes,
            record_bytes,
            take_state_error,
            cancelled,
        } = overlay;
        transaction.contribute(|transaction| {
            transaction.validate_guard(guard, live)?;
            crate::world::incremental_clock::check_clock(now, transaction, write)?;
            if event.time != now {
                return Err(BodyMutationError::Invalid(
                    "event time differs from reducer clock",
                ));
            }
            let WorldEventBody::Domain(DomainEvent::AgentRegistered {
                agent_id,
                pos,
                registration_contract_version,
            }) = &event.body
            else {
                return Err(BodyMutationError::Invalid("expected agent registration"));
            };
            if *registration_contract_version > 1 {
                return Err(BodyMutationError::Invalid(
                    "unsupported_registration_contract",
                ));
            }
            if *registration_contract_version == 1 && agent_id.starts_with("industrial:") {
                return Err(BodyMutationError::Invalid(
                    "industrial_identity_requires_delivery",
                ));
            }
            let _cell_charge = write
                .memory
                .reserve(
                    agent_id.len().saturating_add(32 << 10),
                    MemoryClass::Ordinary,
                )
                .map_err(ExecutionError::Memory)?;
            let mut cell = AgentCell::new(AgentState::new(agent_id, *pos), now);
            cell.activity = Some(AgentActivityV1::idle(now));
            let creation_guard = transaction.guard();
            create_agent_subtree_v1(
                AgentSubtreeCreateV1 {
                    agent: agent_id,
                    cell: &cell,
                    qualification,
                    max_scalar_bytes: *record_bytes,
                },
                &mut WorldFieldOverlayContextV1 {
                    transaction,
                    values: ValueReadContext {
                        source: values.source,
                        memory: values.memory,
                        class: values.class,
                        io: &mut *values.io,
                    },
                    key_work,
                    semantic_bytes: *semantic_bytes,
                    record_bytes: *record_bytes,
                    take_state_error: &mut *take_state_error,
                    cancelled: &mut *cancelled,
                },
                write,
                &mut |_, _| {
                    Err(BodyMutationError::Invalid(
                        "new registration mailbox must be empty",
                    ))
                },
                &creation_guard,
                live,
            )?;
            let placeholder = CanonicalCborWorkV1 {
                visits_remaining: 0,
                bytes_remaining: 0,
                max_depth: original_work.max_depth,
                max_map_keys: original_work.max_map_keys,
                max_key_bytes: original_work.max_key_bytes,
            };
            let mut context = AgentEventContext {
                values: values.source,
                mutation: BodyMutationContext {
                    sink: &mut *write.sink,
                    memory: write.memory,
                    io: &mut *write.io,
                    cancelled: || cancelled() || (write.cancelled)(),
                },
                persist_original,
                max_event_bytes,
                original_work: std::mem::replace(original_work, placeholder),
            };
            let result =
                Self::contribute_iws_agent_event_core(now, event, transaction, &mut context, true);
            *original_work = context.original_work;
            result
        })
    }
}
